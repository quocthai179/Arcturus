//! Đọc JSON hook Claude Code (chịu lỗi) và chuẩn hóa thành [`Event`] (`docs/PLAN.md` §3.2).
//!
//! Hình dạng đầu vào đã kiểm chứng bằng fixture thật ở T1 (`docs/HOOK_FINDINGS.md`): trường chung `session_id`, `cwd`,
//! `hook_event_name`; sự kiện tool có `tool_name`, `tool_input`; mọi sự kiện của subagent có `agent_id`, `agent_type`.
//! JSON thiếu trường, sai kiểu, hay sự kiện lạ **không bao giờ** làm lỗi: thành `"unknown"` hoặc [`EventKind::Unknown`].

use serde_json::Value;
use time::OffsetDateTime;

use crate::schema::{
    Activity, AgentKind, AgentRef, Event, EventKind, Source, ToolInfo, SCHEMA_VERSION,
};

/// `session_id` dùng khi hook không gửi trường này.
pub const UNKNOWN_SESSION: &str = "unknown";

/// Độ dài tối đa của [`ToolInfo::summary`] (ký tự Unicode, kể cả dấu `…`).
pub const SUMMARY_MAX_CHARS: usize = 200;

/// JSON hook thô. Mọi accessor trả `None` khi trường thiếu hoặc không phải chuỗi/object.
#[derive(Clone, Debug, PartialEq)]
pub struct HookInput {
    pub raw: Value,
}

impl HookInput {
    pub fn new(raw: Value) -> Self {
        HookInput { raw }
    }

    /// Lỗi chỉ khi body không phải JSON hợp lệ; JSON hợp lệ nhưng sai hình dạng vẫn nhận.
    pub fn from_slice(body: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(body).map(HookInput::new)
    }

    fn str_field(&self, key: &str) -> Option<&str> {
        self.raw.get(key).and_then(Value::as_str)
    }

    /// `hook_event_name`, chuỗi rỗng nếu thiếu.
    pub fn event_name(&self) -> &str {
        self.str_field("hook_event_name").unwrap_or("")
    }

    pub fn session_id(&self) -> Option<&str> {
        self.str_field("session_id").filter(|s| !s.is_empty())
    }

    pub fn cwd(&self) -> Option<&str> {
        self.str_field("cwd")
    }

    pub fn tool_name(&self) -> Option<&str> {
        self.str_field("tool_name").filter(|s| !s.is_empty())
    }

    pub fn tool_input(&self) -> Option<&Value> {
        self.raw.get("tool_input")
    }

    /// Có ở mọi sự kiện phát ra từ subagent (Q4).
    pub fn agent_id(&self) -> Option<&str> {
        self.str_field("agent_id").filter(|s| !s.is_empty())
    }

    /// Ví dụ `"general-purpose"` (Q4).
    pub fn agent_type(&self) -> Option<&str> {
        self.str_field("agent_type").filter(|s| !s.is_empty())
    }
}

/// Chuẩn hóa một hook thành [`Event`]. `approval` luôn `None` (hub điền khi tạo yêu cầu duyệt, T3);
/// `tool.risk` luôn `None` (chỉ policy điền).
pub fn to_event(input: &HookInput, host: &str, now: OffsetDateTime) -> Event {
    let name = input.event_name();
    let session_id = match input.session_id() {
        Some(s) => s.to_owned(),
        None => {
            tracing::warn!(event = name, "hook thiếu session_id, dùng \"unknown\"");
            UNKNOWN_SESSION.to_owned()
        }
    };
    let (kind, activity) = kind_and_activity(name, input.tool_name());
    let tool = input.tool_name().map(|tool_name| ToolInfo {
        name: tool_name.to_owned(),
        summary: tool_summary(tool_name, input.tool_input().unwrap_or(&Value::Null)),
        risk: None,
    });

    Event {
        v: SCHEMA_VERSION,
        id: format!("evt_{}", ulid::Ulid::generate()),
        ts: now,
        host: host.to_owned(),
        source: Source::ClaudeCode,
        agent: agent_ref(&session_id, input.agent_id(), input.agent_type()),
        session_id,
        cwd: input.cwd().filter(|c| !c.is_empty()).map(str::to_owned),
        kind,
        activity,
        tool,
        approval: None,
        metrics: None,
    }
}

/// Agent con khi có `agent_id`: `agt_<session>_<agent_id>`, cha là `agt_<session>`.
fn agent_ref(session_id: &str, agent_id: Option<&str>, agent_type: Option<&str>) -> AgentRef {
    let parent = format!("agt_{session_id}");
    match agent_id {
        Some(a) => AgentRef {
            id: format!("{parent}_{a}"),
            parent_id: Some(parent),
            kind: AgentKind::Subagent,
            agent_type: agent_type.map(str::to_owned),
        },
        None => AgentRef {
            id: parent,
            parent_id: None,
            kind: AgentKind::Session,
            agent_type: None,
        },
    }
}

/// Bảng §3.2: `hook_event_name` → (EventKind, activity).
fn kind_and_activity(name: &str, tool_name: Option<&str>) -> (EventKind, Option<Activity>) {
    match name {
        "SessionStart" => (EventKind::SessionStarted, Some(Activity::Idle)),
        "UserPromptSubmit" => (EventKind::PromptSubmitted, Some(Activity::Thinking)),
        "PreToolUse" => (
            EventKind::ToolPre,
            Some(activity_for_tool(tool_name.unwrap_or(""))),
        ),
        "PostToolUse" => (EventKind::ToolPost, Some(Activity::Thinking)),
        "PostToolUseFailure" => (EventKind::ToolFailed, None),
        "PermissionRequest" => (EventKind::ApprovalRequested, Some(Activity::Waiting)),
        "SubagentStart" => (EventKind::SubagentStarted, None),
        "SubagentStop" => (EventKind::SubagentStopped, None),
        "Notification" => (EventKind::Notification, None),
        "Stop" => (EventKind::TurnStopped, Some(Activity::Idle)),
        "StopFailure" => (EventKind::TurnFailed, None),
        "SessionEnd" => (EventKind::SessionEnded, None),
        _ => (EventKind::Unknown, None),
    }
}

/// Bảng §3.6: activity khi PreToolUse theo tool.
pub fn activity_for_tool(tool_name: &str) -> Activity {
    match tool_name {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => Activity::Typing,
        "Read" | "Grep" | "Glob" | "LS" | "WebFetch" | "WebSearch" => Activity::Reading,
        "Bash" => Activity::Running,
        _ => Activity::Thinking,
    }
}

/// Tóm tắt cho UI: Bash → `command`; Edit/Write/Read/MultiEdit → `file_path`; NotebookEdit → `notebook_path`;
/// Grep/Glob → `pattern`; tool khác hoặc thiếu trường → tên tool. Cắt còn ≤ [`SUMMARY_MAX_CHARS`] ký tự.
pub fn tool_summary(tool_name: &str, tool_input: &Value) -> String {
    let field = match tool_name {
        "Bash" => Some("command"),
        "Edit" | "Write" | "Read" | "MultiEdit" => Some("file_path"),
        "NotebookEdit" => Some("notebook_path"),
        "Grep" | "Glob" => Some("pattern"),
        _ => None,
    };
    let text = field
        .and_then(|f| tool_input.get(f))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(tool_name);
    truncate_chars(text, SUMMARY_MAX_CHARS)
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-10-21 09:15:02 UTC);

    fn ev(raw: Value) -> Event {
        to_event(&HookInput::new(raw), "laptop", NOW)
    }

    #[test]
    fn unknown_event_name_maps_to_unknown_kind() {
        for name in ["PreCompact", "TeammateIdle", "", "  "] {
            let e = ev(json!({ "hook_event_name": name, "session_id": "s1" }));
            assert_eq!(e.kind, EventKind::Unknown, "{name:?}");
            assert_eq!(e.activity, None);
        }
        assert_eq!(ev(json!({ "session_id": "s1" })).kind, EventKind::Unknown);
        assert_eq!(
            ev(json!({ "hook_event_name": 42 })).kind,
            EventKind::Unknown
        );
    }

    #[test]
    fn missing_session_id_uses_unknown() {
        for raw in [
            json!({ "hook_event_name": "Stop" }),
            json!({ "hook_event_name": "Stop", "session_id": "" }),
            json!({ "hook_event_name": "Stop", "session_id": 7 }),
        ] {
            let e = ev(raw);
            assert_eq!(e.session_id, UNKNOWN_SESSION);
            assert_eq!(e.agent.id, "agt_unknown");
            assert_eq!(e.kind, EventKind::TurnStopped);
        }
    }

    #[test]
    fn non_object_json_never_panics() {
        for raw in [json!(null), json!([1, 2]), json!("x"), json!(3)] {
            let e = ev(raw);
            assert_eq!(e.kind, EventKind::Unknown);
            assert_eq!(e.session_id, UNKNOWN_SESSION);
            assert!(e.tool.is_none());
        }
    }

    #[test]
    fn invalid_json_is_the_only_error() {
        assert!(HookInput::from_slice(b"{hong").is_err());
        assert!(HookInput::from_slice(b"[]").is_ok());
    }

    #[test]
    fn tool_summary_bash_uses_command() {
        assert_eq!(
            tool_summary(
                "Bash",
                &json!({ "command": "npm test", "description": "x" })
            ),
            "npm test"
        );
    }

    #[test]
    fn tool_summary_edit_uses_file_path() {
        for t in ["Edit", "Write", "Read", "MultiEdit"] {
            assert_eq!(
                tool_summary(t, &json!({ "file_path": "src/a.rs", "old_string": "x" })),
                "src/a.rs"
            );
        }
        assert_eq!(
            tool_summary("NotebookEdit", &json!({ "notebook_path": "n.ipynb" })),
            "n.ipynb"
        );
    }

    #[test]
    fn tool_summary_grep_glob_use_pattern_others_use_name() {
        assert_eq!(tool_summary("Grep", &json!({ "pattern": "beta" })), "beta");
        assert_eq!(
            tool_summary("Glob", &json!({ "pattern": "*.txt" })),
            "*.txt"
        );
        assert_eq!(
            tool_summary("WebFetch", &json!({ "url": "https://x" })),
            "WebFetch"
        );
        // Thiếu trường hoặc sai kiểu: rơi về tên tool.
        assert_eq!(tool_summary("Bash", &json!({})), "Bash");
        assert_eq!(tool_summary("Bash", &json!({ "command": 1 })), "Bash");
        assert_eq!(tool_summary("Bash", &Value::Null), "Bash");
    }

    #[test]
    fn tool_summary_truncates_by_chars() {
        let long = "ệ".repeat(300);
        let s = tool_summary("Bash", &json!({ "command": long }));
        assert_eq!(s.chars().count(), SUMMARY_MAX_CHARS);
        assert!(s.ends_with('…'));
        let exact = "a".repeat(SUMMARY_MAX_CHARS);
        assert_eq!(
            tool_summary("Bash", &json!({ "command": exact.clone() })),
            exact
        );
    }

    #[test]
    fn event_kind_and_activity_table() {
        let cases = [
            (
                "SessionStart",
                EventKind::SessionStarted,
                Some(Activity::Idle),
            ),
            (
                "UserPromptSubmit",
                EventKind::PromptSubmitted,
                Some(Activity::Thinking),
            ),
            ("PostToolUse", EventKind::ToolPost, Some(Activity::Thinking)),
            ("PostToolUseFailure", EventKind::ToolFailed, None),
            (
                "PermissionRequest",
                EventKind::ApprovalRequested,
                Some(Activity::Waiting),
            ),
            ("SubagentStart", EventKind::SubagentStarted, None),
            ("SubagentStop", EventKind::SubagentStopped, None),
            ("Notification", EventKind::Notification, None),
            ("Stop", EventKind::TurnStopped, Some(Activity::Idle)),
            ("StopFailure", EventKind::TurnFailed, None),
            ("SessionEnd", EventKind::SessionEnded, None),
        ];
        for (name, kind, activity) in cases {
            let e = ev(json!({ "hook_event_name": name, "session_id": "s1" }));
            assert_eq!((e.kind, e.activity), (kind, activity), "{name}");
        }
    }

    #[test]
    fn pre_tool_activity_per_tool() {
        let cases = [
            ("Edit", Activity::Typing),
            ("Write", Activity::Typing),
            ("MultiEdit", Activity::Typing),
            ("NotebookEdit", Activity::Typing),
            ("Read", Activity::Reading),
            ("Grep", Activity::Reading),
            ("Glob", Activity::Reading),
            ("LS", Activity::Reading),
            ("WebFetch", Activity::Reading),
            ("WebSearch", Activity::Reading),
            ("Bash", Activity::Running),
            ("Task", Activity::Thinking),
            ("Agent", Activity::Thinking),
            ("mcp__x__y", Activity::Thinking),
        ];
        for (tool, activity) in cases {
            let e = ev(
                json!({ "hook_event_name": "PreToolUse", "session_id": "s1", "tool_name": tool }),
            );
            assert_eq!(e.kind, EventKind::ToolPre);
            assert_eq!(e.activity, Some(activity), "{tool}");
        }
    }

    #[test]
    fn subagent_event_maps_to_child_agent() {
        let e = ev(json!({
            "hook_event_name": "PermissionRequest", "session_id": "s1",
            "agent_id": "ad7e", "agent_type": "general-purpose",
            "tool_name": "Bash", "tool_input": { "command": "touch x" }
        }));
        assert_eq!(e.agent.id, "agt_s1_ad7e");
        assert_eq!(e.agent.parent_id.as_deref(), Some("agt_s1"));
        assert_eq!(e.agent.kind, AgentKind::Subagent);
        assert_eq!(e.tool.as_ref().map(|t| t.summary.as_str()), Some("touch x"));
    }

    #[test]
    fn main_session_event_has_no_parent() {
        let e = ev(json!({ "hook_event_name": "Stop", "session_id": "s1", "agent_id": "" }));
        assert_eq!(e.agent.id, "agt_s1");
        assert_eq!(e.agent.parent_id, None);
        assert_eq!(e.agent.kind, AgentKind::Session);
    }

    #[test]
    fn ids_are_unique_and_prefixed() {
        let a = ev(json!({ "hook_event_name": "Stop", "session_id": "s1" }));
        let b = ev(json!({ "hook_event_name": "Stop", "session_id": "s1" }));
        assert!(a.id.starts_with("evt_") && b.id.starts_with("evt_"));
        assert_ne!(a.id, b.id);
    }
}
