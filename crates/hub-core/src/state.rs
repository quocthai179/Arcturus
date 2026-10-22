//! Trạng thái agent suy ra từ sự kiện (`docs/PLAN.md` §3.6), 0 token (quy tắc bất biến 5).
//!
//! - Agent được tạo từ sự kiện đầu tiên của một phiên lạ: hook http không bắn `SessionStart` (F1). Sự kiện có
//!   `agent_id` lạ tạo agent con (và agent cha nếu chưa có).
//! - Chỉ gửi `AgentUpdated` khi có thay đổi nhìn thấy được; `last_event_at` vẫn được cập nhật (gửi qua `Snapshot`).
//! - `Stop` → `done` 3 giây rồi `idle` qua [`TeamState::tick`]. Hub chỉ hẹn giờ khi [`TeamState::next_wakeup`] có
//!   giá trị (không timer thường trực, quy tắc bất biến 6).
//! - Tên sao đã gán được giữ suốt phiên và nhả lại khi agent bị xóa.

use std::collections::BTreeMap;

use time::{Duration, OffsetDateTime};

use crate::hook::activity_for_tool;
use crate::persona::{
    default_palette, next_star_name, PersonaBook, DEFAULT_ROLE, DEFAULT_SUBAGENT_ROLE,
};
use crate::protocol::{AgentView, ServerMsg};
use crate::schema::{Activity, AgentId, AgentKind, Event, EventKind, Mood, ToolInfo};

/// Thời gian giữ mood `done` sau `Stop`.
pub const DONE_HOLD: Duration = Duration::seconds(3);

/// Độ dài tối đa của lệnh Bash trong `act_text` (ký tự Unicode, kể cả `…`).
pub const ACT_COMMAND_MAX_CHARS: usize = 60;

/// Tối đa agent giữ trong state (ngân sách RAM, quy tắc bất biến 6; audit T5a finding 3). Đầy thì sự kiện của agent
/// mới bị bỏ qua (log warn); agent đã có vẫn được cập nhật.
pub const MAX_AGENTS: usize = 256;

/// Độ dài tối đa của `cwd` giữ trong `AgentView` (ký tự Unicode, kể cả `…`).
pub const CWD_MAX_CHARS: usize = 4096;

pub const TEXT_IDLE: &str = "Đang rảnh";
pub const TEXT_THINKING: &str = "Đang suy nghĩ";
pub const TEXT_DONE: &str = "Xong";
pub const TEXT_ERROR: &str = "Gặp lỗi";
pub const TEXT_DELEGATING: &str = "Đang giao việc cho trợ lý";

#[derive(Clone, Debug, PartialEq)]
pub struct AgentRuntime {
    pub view: AgentView,
    pub done_until: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, Default)]
pub struct TeamState {
    pub agents: BTreeMap<AgentId, AgentRuntime>,
    /// "Giờ tập trung" (T14). Cờ tạm dừng không nằm ở đây: `Approvals` giữ nó để kiểm đếm ngược dưới cùng một khóa
    /// (quyết định D3 ở T5).
    pub focus: bool,
}

impl TeamState {
    /// Áp một Event, trả danh sách thay đổi để broadcast.
    pub fn apply(
        &mut self,
        ev: &Event,
        personas: &PersonaBook,
        now: OffsetDateTime,
    ) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        match ev.kind {
            EventKind::SessionEnded => {
                let session = session_agent_id(ev);
                let mut ids: Vec<AgentId> = self
                    .agents
                    .values()
                    .filter(|a| a.view.parent_id.as_deref() == Some(session.as_str()))
                    .map(|a| a.view.id.clone())
                    .collect();
                ids.push(session);
                for id in ids {
                    if self.agents.remove(&id).is_some() {
                        out.push(ServerMsg::AgentRemoved { id });
                    }
                }
                return out;
            }
            EventKind::SubagentStopped if ev.agent.kind == AgentKind::Subagent => {
                if self.agents.remove(&ev.agent.id).is_some() {
                    out.push(ServerMsg::AgentRemoved {
                        id: ev.agent.id.clone(),
                    });
                }
                return out;
            }
            _ => {}
        }

        let missing = usize::from(!self.agents.contains_key(&ev.agent.id))
            + ev.agent
                .parent_id
                .as_ref()
                .map_or(0, |p| usize::from(!self.agents.contains_key(p)));
        if missing > 0 && self.agents.len() + missing > MAX_AGENTS {
            tracing::warn!(
                agents = self.agents.len(),
                session = ?ev.session_id,
                "đã đủ {MAX_AGENTS} agent, bỏ qua sự kiện của agent mới"
            );
            return out;
        }

        if let Some(parent) = &ev.agent.parent_id {
            if !self.agents.contains_key(parent) {
                let mut rt = self.new_session_runtime(parent, ev, personas);
                set(
                    &mut rt.view,
                    Mood::Working,
                    Activity::Thinking,
                    TEXT_DELEGATING,
                );
                out.push(ServerMsg::AgentUpdated {
                    agent: rt.view.clone(),
                });
                self.agents.insert(parent.clone(), rt);
            }
        }

        let is_new = !self.agents.contains_key(&ev.agent.id);
        if is_new {
            let rt = match ev.agent.kind {
                AgentKind::Subagent | AgentKind::Teammate => self.new_subagent_runtime(ev),
                AgentKind::Session => self.new_session_runtime(&ev.agent.id, ev, personas),
            };
            self.agents.insert(ev.agent.id.clone(), rt);
        }
        let Some(rt) = self.agents.get_mut(&ev.agent.id) else {
            return out;
        };
        let before = rt.view.clone();
        transition(rt, ev, now);
        rt.view.last_event_at = ev.ts;
        rt.view.host.clone_from(&ev.host);
        if let Some(cwd) = &ev.cwd {
            rt.view.cwd = truncate(cwd, CWD_MAX_CHARS);
        }
        if is_new || !same_visible(&before, &rt.view) {
            out.push(ServerMsg::AgentUpdated {
                agent: rt.view.clone(),
            });
        }
        out
    }

    /// Chuyển `done` → `idle` cho agent đã hết [`DONE_HOLD`]. Chỉ gọi khi [`Self::next_wakeup`] tới hạn.
    pub fn tick(&mut self, now: OffsetDateTime) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        for rt in self.agents.values_mut() {
            if rt.done_until.is_some_and(|t| t <= now) {
                rt.done_until = None;
                set(&mut rt.view, Mood::Idle, Activity::Idle, TEXT_IDLE);
                out.push(ServerMsg::AgentUpdated {
                    agent: rt.view.clone(),
                });
            }
        }
        out
    }

    /// Thời điểm sớm nhất cần gọi [`Self::tick`]; `None` khi không có agent nào đang `done`.
    pub fn next_wakeup(&self) -> Option<OffsetDateTime> {
        self.agents.values().filter_map(|a| a.done_until).min()
    }

    /// Mọi agent, cho `Snapshot` (T5).
    pub fn views(&self) -> Vec<AgentView> {
        self.agents.values().map(|a| a.view.clone()).collect()
    }

    /// Tên hiển thị của agent (cho thẻ duyệt).
    pub fn name_of(&self, id: &str) -> Option<&str> {
        self.agents.get(id).map(|a| a.view.name.as_str())
    }

    /// Yêu cầu duyệt của `id` đã có quyết định (PLAN T3 bước 9): agent đang chờ duyệt quay về suy nghĩ.
    /// Agent đã sang trạng thái khác (sự kiện kế tiếp tới trước) thì giữ nguyên.
    pub fn approval_finished(&mut self, id: &str) -> Vec<ServerMsg> {
        match self.agents.get_mut(id) {
            Some(rt) if rt.view.mood == Mood::Approval => {
                set(
                    &mut rt.view,
                    Mood::Working,
                    Activity::Thinking,
                    TEXT_THINKING,
                );
                vec![ServerMsg::AgentUpdated {
                    agent: rt.view.clone(),
                }]
            }
            _ => Vec::new(),
        }
    }

    fn name_in_use(&self, name: &str) -> bool {
        self.agents.values().any(|a| a.view.name == name)
    }

    /// Agent phiên: persona khớp `cwd`, không thì tên sao + màu theo hash `session_id`.
    fn new_session_runtime(&self, id: &str, ev: &Event, personas: &PersonaBook) -> AgentRuntime {
        let persona = ev.cwd.as_deref().and_then(|c| personas.find(c));
        let (name, role, palette) = match persona {
            Some(p) => (p.name.clone(), p.role.clone(), p.palette.clone()),
            None => (
                next_star_name(|n| self.name_in_use(n), &ev.session_id),
                DEFAULT_ROLE.to_owned(),
                default_palette(&ev.session_id),
            ),
        };
        self.runtime(id, None, AgentKind::Session, name, role, palette, ev)
    }

    /// Agent con: không lấy persona (tránh trùng tên với cha), tên sao riêng, vai trò là `agent_type`.
    fn new_subagent_runtime(&self, ev: &Event) -> AgentRuntime {
        let role = ev
            .agent
            .agent_type
            .clone()
            .unwrap_or_else(|| DEFAULT_SUBAGENT_ROLE.to_owned());
        self.runtime(
            &ev.agent.id,
            ev.agent.parent_id.clone(),
            ev.agent.kind,
            next_star_name(|n| self.name_in_use(n), &ev.session_id),
            role,
            default_palette(&ev.agent.id),
            ev,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn runtime(
        &self,
        id: &str,
        parent_id: Option<AgentId>,
        kind: AgentKind,
        name: String,
        role: String,
        palette: crate::protocol::Palette,
        ev: &Event,
    ) -> AgentRuntime {
        AgentRuntime {
            view: AgentView {
                id: id.to_owned(),
                parent_id,
                kind,
                name,
                role,
                palette,
                mood: Mood::Idle,
                activity: Activity::Idle,
                act_text: TEXT_IDLE.to_owned(),
                host: ev.host.clone(),
                source: ev.source,
                cwd: truncate(ev.cwd.as_deref().unwrap_or_default(), CWD_MAX_CHARS),
                context_pct: None,
                last_event_at: ev.ts,
            },
            done_until: None,
        }
    }
}

fn session_agent_id(ev: &Event) -> AgentId {
    ev.agent
        .parent_id
        .clone()
        .unwrap_or_else(|| ev.agent.id.clone())
}

/// Bảng §3.6 cho một agent đã tồn tại.
fn transition(rt: &mut AgentRuntime, ev: &Event, now: OffsetDateTime) {
    let v = &mut rt.view;
    match ev.kind {
        EventKind::SessionStarted => set(v, Mood::Idle, Activity::Idle, TEXT_IDLE),
        EventKind::PromptSubmitted | EventKind::ToolPost | EventKind::SubagentStarted => {
            set(v, Mood::Working, Activity::Thinking, TEXT_THINKING)
        }
        EventKind::ToolPre => {
            let activity = ev.activity.unwrap_or_else(|| {
                activity_for_tool(ev.tool.as_ref().map_or("", |t| t.name.as_str()))
            });
            set(v, Mood::Working, activity, &tool_act_text(ev.tool.as_ref()));
        }
        EventKind::ApprovalRequested => {
            let tool = ev.tool.as_ref().map_or("tool", |t| t.name.as_str());
            set(
                v,
                Mood::Approval,
                Activity::Waiting,
                &format!("Chờ duyệt: {}", clean(tool)),
            );
        }
        // Giữ activity, mood error tới sự kiện kế tiếp.
        EventKind::ToolFailed | EventKind::TurnFailed => {
            v.mood = Mood::Error;
            TEXT_ERROR.clone_into(&mut v.act_text);
        }
        EventKind::TurnStopped => {
            set(v, Mood::Done, Activity::Idle, TEXT_DONE);
            rt.done_until = Some(now + DONE_HOLD);
            return;
        }
        // Không đổi trạng thái nhìn thấy, không hủy đếm `done`.
        EventKind::Notification
        | EventKind::ApprovalResolved
        | EventKind::Unknown
        | EventKind::SessionEnded
        | EventKind::SubagentStopped => return,
    }
    rt.done_until = None;
}

fn set(v: &mut AgentView, mood: Mood, activity: Activity, text: &str) {
    v.mood = mood;
    v.activity = activity;
    text.clone_into(&mut v.act_text);
}

/// `act_text` cho PreToolUse (§3.6), dựng từ `ToolInfo::summary`.
fn tool_act_text(tool: Option<&ToolInfo>) -> String {
    let Some(tool) = tool else {
        return TEXT_THINKING.to_owned();
    };
    let name = tool.name.as_str();
    // `tool_summary` trả tên tool khi không có trường phù hợp.
    let summary = (tool.summary != tool.name).then_some(tool.summary.as_str());
    match name {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => match summary {
            Some(s) => format!("Đang sửa {}", clean(file_name(s))),
            None => "Đang sửa file".to_owned(),
        },
        "Read" => match summary {
            Some(s) => format!("Đang đọc {}", clean(file_name(s))),
            None => "Đang đọc file".to_owned(),
        },
        "Grep" | "Glob" | "LS" => match summary {
            Some(s) => format!("Đang đọc {}", truncate(&clean(s), ACT_COMMAND_MAX_CHARS)),
            None => "Đang đọc thư mục".to_owned(),
        },
        "Bash" => match summary {
            Some(s) => format!("Đang chạy {}", truncate(&clean(s), ACT_COMMAND_MAX_CHARS)),
            None => "Đang chạy lệnh".to_owned(),
        },
        "WebFetch" | "WebSearch" => "Đang tra cứu".to_owned(),
        "Task" | "Agent" => TEXT_DELEGATING.to_owned(),
        other => format!(
            "Đang dùng {}",
            truncate(&clean(other), ACT_COMMAND_MAX_CHARS)
        ),
    }
}

/// Phần sau `/` hoặc `\` cuối cùng.
fn file_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
}

/// Nội dung từ agent là không tin cậy: thay ký tự điều khiển (xuống dòng, tab…) bằng khoảng trắng.
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// So sánh bỏ qua `last_event_at`.
fn same_visible(a: &AgentView, b: &AgentView) -> bool {
    let mut a = a.clone();
    a.last_event_at = b.last_event_at;
    a == *b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::{to_event, HookInput};
    use crate::persona::PersonaSpec;
    use crate::protocol::Palette;
    use serde_json::{json, Value};
    use time::macros::datetime;

    const T0: OffsetDateTime = datetime!(2026-10-22 09:00:00 UTC);
    const CWD: &str = r"D:\code\Arcturus";

    fn ev(raw: Value, at: OffsetDateTime) -> Event {
        to_event(&HookInput::new(raw), "laptop", at)
    }

    fn hook(name: &str, session: &str) -> Value {
        json!({ "hook_event_name": name, "session_id": session, "cwd": CWD })
    }

    fn pre_tool(session: &str, tool: &str, input: Value) -> Value {
        let mut v = hook("PreToolUse", session);
        v["tool_name"] = json!(tool);
        v["tool_input"] = input;
        v
    }

    fn sub(mut v: Value, agent_id: &str) -> Value {
        v["agent_id"] = json!(agent_id);
        v["agent_type"] = json!("general-purpose");
        v
    }

    fn no_personas() -> PersonaBook {
        PersonaBook::default()
    }

    struct World {
        st: TeamState,
        book: PersonaBook,
    }

    impl World {
        fn new() -> Self {
            World {
                st: TeamState::default(),
                book: no_personas(),
            }
        }

        fn apply(&mut self, raw: Value) -> Vec<ServerMsg> {
            self.apply_at(raw, T0)
        }

        fn apply_at(&mut self, raw: Value, at: OffsetDateTime) -> Vec<ServerMsg> {
            let e = ev(raw, at);
            self.st.apply(&e, &self.book, at)
        }

        fn view(&self, id: &str) -> &AgentView {
            &self.st.agents[id].view
        }
    }

    fn assert_state(w: &World, id: &str, mood: Mood, activity: Activity, text: &str) {
        let v = w.view(id);
        assert_eq!(
            (v.mood, v.activity, v.act_text.as_str()),
            (mood, activity, text)
        );
    }

    // --- Bảng §3.6: mỗi dòng một test -------------------------------------------------------------------------

    #[test]
    fn pre_tool_edit_sets_typing() {
        for tool in ["Edit", "Write", "MultiEdit"] {
            let mut w = World::new();
            w.apply(pre_tool(
                "s1",
                tool,
                json!({ "file_path": r"D:\code\ui\LoginForm.tsx" }),
            ));
            assert_state(
                &w,
                "agt_s1",
                Mood::Working,
                Activity::Typing,
                "Đang sửa LoginForm.tsx",
            );
        }
        let mut w = World::new();
        w.apply(pre_tool(
            "s1",
            "NotebookEdit",
            json!({ "notebook_path": "/n/a.ipynb" }),
        ));
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Typing,
            "Đang sửa a.ipynb",
        );
    }

    #[test]
    fn pre_tool_read_sets_reading() {
        let mut w = World::new();
        w.apply(pre_tool(
            "s1",
            "Read",
            json!({ "file_path": "/repo/src/main.rs" }),
        ));
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Reading,
            "Đang đọc main.rs",
        );
        for (tool, pattern) in [("Grep", "fn apply"), ("Glob", "**/*.rs")] {
            w.apply(pre_tool("s1", tool, json!({ "pattern": pattern })));
            assert_state(
                &w,
                "agt_s1",
                Mood::Working,
                Activity::Reading,
                &format!("Đang đọc {pattern}"),
            );
        }
        w.apply(pre_tool("s1", "LS", json!({ "path": "/repo" })));
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Reading,
            "Đang đọc thư mục",
        );
    }

    #[test]
    fn pre_tool_bash_sets_running() {
        let mut w = World::new();
        w.apply(pre_tool(
            "s1",
            "Bash",
            json!({ "command": "cargo test --workspace" }),
        ));
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Running,
            "Đang chạy cargo test --workspace",
        );

        // Lệnh dài cắt còn 60 ký tự; xuống dòng không lọt vào act_text.
        let long = format!("echo {}\nrm -rf /", "x".repeat(100));
        w.apply(pre_tool("s1", "Bash", json!({ "command": long })));
        let text = &w.view("agt_s1").act_text;
        let cmd = text.strip_prefix("Đang chạy ").unwrap();
        assert_eq!(cmd.chars().count(), ACT_COMMAND_MAX_CHARS);
        assert!(cmd.ends_with('…'));
        assert!(!text.contains('\n'));
    }

    #[test]
    fn pre_tool_web_sets_reading_lookup() {
        for tool in ["WebFetch", "WebSearch"] {
            let mut w = World::new();
            w.apply(pre_tool(
                "s1",
                tool,
                json!({ "url": "https://example.com" }),
            ));
            assert_state(
                &w,
                "agt_s1",
                Mood::Working,
                Activity::Reading,
                "Đang tra cứu",
            );
        }
    }

    #[test]
    fn pre_tool_task_sets_thinking_delegating() {
        for tool in ["Task", "Agent"] {
            let mut w = World::new();
            w.apply(pre_tool("s1", tool, json!({ "prompt": "..." })));
            assert_state(
                &w,
                "agt_s1",
                Mood::Working,
                Activity::Thinking,
                TEXT_DELEGATING,
            );
        }
    }

    #[test]
    fn pre_tool_other_sets_thinking_using() {
        let mut w = World::new();
        w.apply(pre_tool("s1", "mcp__youtrack__get_issue", json!({})));
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Thinking,
            "Đang dùng mcp__youtrack__get_issue",
        );
    }

    // --- Sự kiện khác ----------------------------------------------------------------------------------------

    #[test]
    fn prompt_and_post_tool_set_thinking() {
        let mut w = World::new();
        w.apply(hook("UserPromptSubmit", "s1"));
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Thinking,
            TEXT_THINKING,
        );
        w.apply(pre_tool("s1", "Bash", json!({ "command": "ls" })));
        let mut post = pre_tool("s1", "Bash", json!({ "command": "ls" }));
        post["hook_event_name"] = json!("PostToolUse");
        w.apply(post);
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Thinking,
            TEXT_THINKING,
        );
    }

    #[test]
    fn permission_request_sets_waiting_approval() {
        let mut w = World::new();
        let mut p = pre_tool("s1", "Bash", json!({ "command": "git push" }));
        p["hook_event_name"] = json!("PermissionRequest");
        w.apply(p);
        assert_state(
            &w,
            "agt_s1",
            Mood::Approval,
            Activity::Waiting,
            "Chờ duyệt: Bash",
        );
    }

    #[test]
    fn failure_sets_error_until_next_event() {
        for failure in ["PostToolUseFailure", "StopFailure"] {
            let mut w = World::new();
            w.apply(pre_tool("s1", "Bash", json!({ "command": "cargo build" })));
            let mut f = pre_tool("s1", "Bash", json!({ "command": "cargo build" }));
            f["hook_event_name"] = json!(failure);
            w.apply(f);
            let v = w.view("agt_s1");
            assert_eq!(v.mood, Mood::Error, "{failure}");
            assert_eq!(v.activity, Activity::Running, "{failure}: giữ activity");
            assert_eq!(v.act_text, TEXT_ERROR);

            // Notification không xóa lỗi; sự kiện kế tiếp có trạng thái thì xóa.
            w.apply(hook("Notification", "s1"));
            assert_eq!(w.view("agt_s1").mood, Mood::Error);
            w.apply(hook("UserPromptSubmit", "s1"));
            assert_eq!(w.view("agt_s1").mood, Mood::Working);
        }
    }

    #[test]
    fn stop_sets_done_then_idle_after_3s() {
        let mut w = World::new();
        w.apply(pre_tool("s1", "Bash", json!({ "command": "ls" })));
        assert_eq!(w.st.next_wakeup(), None);

        w.apply_at(hook("Stop", "s1"), T0 + Duration::seconds(1));
        assert_state(&w, "agt_s1", Mood::Done, Activity::Idle, TEXT_DONE);
        let wake = w.st.next_wakeup().unwrap();
        assert_eq!(wake, T0 + Duration::seconds(4));

        assert!(w.st.tick(wake - Duration::milliseconds(1)).is_empty());
        assert_eq!(w.view("agt_s1").mood, Mood::Done);

        let msgs = w.st.tick(wake);
        assert_eq!(msgs.len(), 1);
        assert_state(&w, "agt_s1", Mood::Idle, Activity::Idle, TEXT_IDLE);
        assert_eq!(w.st.next_wakeup(), None);
        assert!(w.st.tick(wake + Duration::seconds(10)).is_empty());
    }

    #[test]
    fn new_event_cancels_pending_done() {
        let mut w = World::new();
        w.apply(hook("Stop", "s1"));
        assert!(w.st.next_wakeup().is_some());
        // Notification không hủy đếm done.
        w.apply(hook("Notification", "s1"));
        assert!(w.st.next_wakeup().is_some());
        w.apply(hook("UserPromptSubmit", "s1"));
        assert_eq!(w.st.next_wakeup(), None);
        assert!(w.st.tick(T0 + Duration::seconds(10)).is_empty());
        assert_eq!(w.view("agt_s1").mood, Mood::Working);
    }

    #[test]
    fn next_wakeup_none_when_no_done_agent() {
        let mut w = World::new();
        assert_eq!(w.st.next_wakeup(), None);
        w.apply(hook("UserPromptSubmit", "s1"));
        w.apply(pre_tool("s2", "Read", json!({ "file_path": "/a" })));
        assert_eq!(w.st.next_wakeup(), None);
    }

    #[test]
    fn first_event_of_unknown_session_creates_agent() {
        // Không có SessionStart (F1): sự kiện đầu tiên tạo agent.
        let mut w = World::new();
        let msgs = w.apply(pre_tool("s1", "Bash", json!({ "command": "ls" })));
        assert_eq!(msgs.len(), 1);
        let v = w.view("agt_s1");
        assert_eq!(v.kind, AgentKind::Session);
        assert_eq!(v.cwd, CWD);
        assert_eq!(v.host, "laptop");
        assert_eq!(v.last_event_at, T0);
        // SessionStart nếu có thì vẫn tạo agent rảnh.
        w.apply(hook("SessionStart", "s2"));
        assert_state(&w, "agt_s2", Mood::Idle, Activity::Idle, TEXT_IDLE);
    }

    #[test]
    fn only_visible_changes_are_broadcast() {
        let mut w = World::new();
        w.apply(hook("UserPromptSubmit", "s1"));
        // Cùng trạng thái: không gửi, nhưng last_event_at vẫn đổi.
        let later = T0 + Duration::seconds(5);
        assert!(w.apply_at(hook("UserPromptSubmit", "s1"), later).is_empty());
        assert_eq!(w.view("agt_s1").last_event_at, later);
        assert!(w.apply(hook("Notification", "s1")).is_empty());
        assert_eq!(
            w.apply(pre_tool("s1", "Read", json!({ "file_path": "/a" })))
                .len(),
            1
        );
    }

    // --- Agent con ------------------------------------------------------------------------------------------

    #[test]
    fn subagent_start_creates_child_with_parent() {
        let mut w = World::new();
        w.apply(pre_tool("s1", "Agent", json!({ "prompt": "x" })));
        let msgs = w.apply(sub(hook("SubagentStart", "s1"), "a1"));
        assert_eq!(msgs.len(), 1);
        let child = w.view("agt_s1_a1");
        assert_eq!(child.kind, AgentKind::Subagent);
        assert_eq!(child.parent_id.as_deref(), Some("agt_s1"));
        assert_eq!(child.role, "general-purpose");
        assert_ne!(child.name, w.view("agt_s1").name, "con có tên riêng");
        assert_state(
            &w,
            "agt_s1_a1",
            Mood::Working,
            Activity::Thinking,
            TEXT_THINKING,
        );
        // Cha giữ nguyên trạng thái giao việc.
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Thinking,
            TEXT_DELEGATING,
        );
    }

    #[test]
    fn unknown_agent_id_creates_child_and_parent() {
        let mut w = World::new();
        let msgs = w.apply(sub(
            pre_tool("s1", "Bash", json!({ "command": "touch x" })),
            "a1",
        ));
        assert_eq!(msgs.len(), 2, "tạo cả cha và con");
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Thinking,
            TEXT_DELEGATING,
        );
        assert_state(
            &w,
            "agt_s1_a1",
            Mood::Working,
            Activity::Running,
            "Đang chạy touch x",
        );
    }

    #[test]
    fn subagent_stop_removes_child() {
        let mut w = World::new();
        w.apply(sub(hook("SubagentStart", "s1"), "a1"));
        let msgs = w.apply(sub(hook("SubagentStop", "s1"), "a1"));
        assert_eq!(
            msgs,
            vec![ServerMsg::AgentRemoved {
                id: "agt_s1_a1".to_owned()
            }]
        );
        assert!(w.st.agents.contains_key("agt_s1"));
        assert!(!w.st.agents.contains_key("agt_s1_a1"));
        // Dừng con không tồn tại: không có gì.
        assert!(w.apply(sub(hook("SubagentStop", "s1"), "a9")).is_empty());
    }

    #[test]
    fn session_end_removes_agent_and_children() {
        let mut w = World::new();
        w.apply(sub(hook("SubagentStart", "s1"), "a1"));
        w.apply(sub(hook("SubagentStart", "s1"), "a2"));
        w.apply(hook("UserPromptSubmit", "s2"));
        let msgs = w.apply(hook("SessionEnd", "s1"));
        let removed: Vec<_> = msgs
            .iter()
            .map(|m| match m {
                ServerMsg::AgentRemoved { id } => id.as_str(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(removed, ["agt_s1_a1", "agt_s1_a2", "agt_s1"]);
        assert_eq!(w.st.agents.keys().collect::<Vec<_>>(), ["agt_s2"]);
        // SessionEnd của phiên lạ không tạo agent.
        assert!(w.apply(hook("SessionEnd", "s9")).is_empty());
        assert!(!w.st.agents.contains_key("agt_s9"));
    }

    // --- Persona và tên -------------------------------------------------------------------------------------

    fn khoa() -> PersonaSpec {
        PersonaSpec {
            match_cwd: "**/.claude/worktrees/hub*".to_owned(),
            name: "Khoa".to_owned(),
            role: "backend".to_owned(),
            palette: Palette {
                hair: "#1F1F24".to_owned(),
                outfit: "#4C8DF6".to_owned(),
                acc: "#9AA3B5".to_owned(),
            },
        }
    }

    #[test]
    fn persona_applied_by_cwd() {
        let mut w = World::new();
        w.book = PersonaBook::with_case(&[khoa()], true).unwrap();
        let mut h = hook("UserPromptSubmit", "s1");
        h["cwd"] = json!(r"D:\code\Arcturus\.claude\worktrees\hub\crates");
        w.apply(h);
        let v = w.view("agt_s1");
        assert_eq!((v.name.as_str(), v.role.as_str()), ("Khoa", "backend"));
        assert_eq!(v.palette, khoa().palette);
        // Subagent trong cùng worktree không lấy persona.
        let mut c = sub(hook("SubagentStart", "s1"), "a1");
        c["cwd"] = json!(r"D:\code\Arcturus\.claude\worktrees\hub");
        w.apply(c);
        assert_eq!(w.view("agt_s1_a1").name, "Muphrid");
    }

    #[test]
    fn unmatched_gets_star_name_in_order() {
        let mut w = World::new();
        for s in ["s1", "s2", "s3"] {
            w.apply(hook("UserPromptSubmit", s));
        }
        assert_eq!(w.view("agt_s1").name, "Muphrid");
        assert_eq!(w.view("agt_s2").name, "Seginus");
        assert_eq!(w.view("agt_s3").name, "Nekkar");
        assert_eq!(w.view("agt_s1").role, DEFAULT_ROLE);
        assert_eq!(w.view("agt_s1").palette, default_palette("s1"));
    }

    #[test]
    fn name_stable_for_session() {
        let mut w = World::new();
        w.apply(hook("UserPromptSubmit", "s1"));
        w.apply(hook("UserPromptSubmit", "s2"));
        for e in [
            pre_tool("s1", "Bash", json!({ "command": "ls" })),
            hook("Stop", "s1"),
            hook("UserPromptSubmit", "s1"),
        ] {
            w.apply(e);
            assert_eq!(w.view("agt_s1").name, "Muphrid");
        }
        // s2 kết thúc không làm s1 đổi tên.
        w.apply(hook("SessionEnd", "s2"));
        assert_eq!(w.view("agt_s1").name, "Muphrid");
    }

    #[test]
    fn star_name_released_after_session_end() {
        let mut w = World::new();
        w.apply(hook("UserPromptSubmit", "s1"));
        w.apply(hook("UserPromptSubmit", "s2"));
        w.apply(hook("SessionEnd", "s1"));
        w.apply(hook("UserPromptSubmit", "s3"));
        assert_eq!(w.view("agt_s3").name, "Muphrid", "tên của s1 được dùng lại");
    }

    #[test]
    fn star_names_exhausted_fall_back_to_session_prefix() {
        let mut w = World::new();
        for i in 0..crate::persona::STAR_NAMES.len() {
            w.apply(hook("UserPromptSubmit", &format!("sess{i:02}")));
        }
        w.apply(hook("UserPromptSubmit", "abcdef12"));
        assert_eq!(w.view("agt_abcdef12").name, "Agent abcd");
    }

    #[test]
    fn agent_count_is_capped() {
        let mut w = World::new();
        for i in 0..MAX_AGENTS {
            w.apply(hook("UserPromptSubmit", &format!("s{i}")));
        }
        assert_eq!(w.st.agents.len(), MAX_AGENTS);
        // Phiên mới bị bỏ qua, không tạo agent, không broadcast.
        assert!(w.apply(hook("UserPromptSubmit", "moi")).is_empty());
        assert!(w.apply(sub(hook("SubagentStart", "s0"), "a1")).is_empty());
        assert_eq!(w.st.agents.len(), MAX_AGENTS);
        // Agent đã có vẫn được cập nhật; xóa bớt thì lại nhận agent mới.
        assert_eq!(w.apply(hook("Stop", "s0")).len(), 1);
        w.apply(hook("SessionEnd", "s1"));
        assert_eq!(w.apply(hook("UserPromptSubmit", "moi")).len(), 1);
    }

    #[test]
    fn long_cwd_is_truncated() {
        let mut w = World::new();
        let mut h = hook("UserPromptSubmit", "s1");
        h["cwd"] = json!(format!("D:/{}", "d".repeat(100_000)));
        w.apply(h.clone());
        assert_eq!(w.view("agt_s1").cwd.chars().count(), CWD_MAX_CHARS);
        w.apply(h);
        assert_eq!(w.view("agt_s1").cwd.chars().count(), CWD_MAX_CHARS);
    }

    #[test]
    fn approval_finished_returns_waiting_agent_to_thinking() {
        let mut w = World::new();
        let mut p = pre_tool("s1", "Bash", json!({ "command": "git push" }));
        p["hook_event_name"] = json!("PermissionRequest");
        w.apply(p);
        assert_eq!(w.st.name_of("agt_s1"), Some("Muphrid"));
        assert_eq!(w.st.approval_finished("agt_s1").len(), 1);
        assert_state(
            &w,
            "agt_s1",
            Mood::Working,
            Activity::Thinking,
            TEXT_THINKING,
        );

        // Đã sang trạng thái khác, hoặc agent không còn: không làm gì.
        w.apply(hook("Stop", "s1"));
        assert!(w.st.approval_finished("agt_s1").is_empty());
        assert_eq!(w.view("agt_s1").mood, Mood::Done);
        assert!(w.st.approval_finished("agt_khong_co").is_empty());
        assert_eq!(w.st.name_of("agt_khong_co"), None);
    }

    #[test]
    fn helpers() {
        assert_eq!(file_name(r"D:\a\b\c.rs"), "c.rs");
        assert_eq!(file_name("/a/b/"), "b");
        assert_eq!(file_name("c.rs"), "c.rs");
        assert_eq!(clean("a\nb\tc\u{7}"), "a b c ");
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("ab", 4), "ab");
    }
}
