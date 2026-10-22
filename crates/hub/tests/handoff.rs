//! Test T5b: lệnh quá dài để duyệt trên thẻ được trả về terminal (quy tắc bất biến 3).
//!
//! Bất biến được kiểm: hub trả `{}` ngay, không giữ, không hỏi policy (kể cả policy AutoAllow), không có id nào để
//! `Decide`; thẻ báo có cờ tính trên lệnh đầy đủ và được xóa theo luật supersede của T3.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hub::approvals::{
    ApprovalCtx, ApprovalRecord, ApprovalStatus, ApprovalStore, ApprovalsConfig, DecideOrigin,
    DecideResult, Policy, PolicyOutcome, StoreAck, MAX_HANDOFFS,
};
use hub::hub::{Hub, HubTasks};
use hub::{hooks, server};
use hub_core::config::Config;
use hub_core::persona::PersonaBook;
use hub_core::protocol::{HandoffView, ServerMsg, Suspicion};
use hub_core::schema::{Decision, Mood, Risk};
use hub_core::state::TEXT_APPROVE_IN_TERMINAL;
use serde_json::{json, Value};
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;

const TOKEN: &str = "test-token-0123456789abcdef-arcturus-t5b";
const WAIT: Duration = Duration::from_secs(5);

/// Policy tự duyệt mọi thứ, và đếm số lần được hỏi.
struct AutoAllowCounting(Arc<Mutex<usize>>);

impl Policy for AutoAllowCounting {
    fn decide(&self, _ctx: &ApprovalCtx<'_>) -> PolicyOutcome {
        *self.0.lock().unwrap() += 1;
        PolicyOutcome::AutoAllow {
            reason: "test".to_owned(),
            risk: Risk::Low,
        }
    }
}

/// Store ack mọi bản ghi (để AutoAllow thật sự có hiệu lực nếu bị gọi) và ghi lại chúng.
struct AckStore(Arc<Mutex<Vec<ApprovalRecord>>>);

impl ApprovalStore for AckStore {
    fn record(&self, rec: ApprovalRecord) -> StoreAck {
        self.0.lock().unwrap().push(rec);
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Ok(()));
        rx
    }
}

struct T {
    addr: SocketAddr,
    core: Arc<Hub>,
    rx: broadcast::Receiver<ServerMsg>,
    client: reqwest::Client,
    policy_calls: Arc<Mutex<usize>>,
    records: Arc<Mutex<Vec<ApprovalRecord>>>,
    stop: Option<oneshot::Sender<()>>,
    server: JoinHandle<()>,
    _tasks: HubTasks,
}

impl T {
    async fn start() -> T {
        Self::start_with(ApprovalsConfig::default()).await
    }

    async fn start_with(acfg: ApprovalsConfig) -> T {
        let policy_calls = Arc::new(Mutex::new(0));
        let records = Arc::new(Mutex::new(Vec::new()));
        let (core, tasks) = Hub::start_with(
            Config::default(),
            PersonaBook::default(),
            "laptop".to_owned(),
            acfg,
            Box::new(AutoAllowCounting(policy_calls.clone())),
            Box::new(AckStore(records.clone())),
        );
        let mut t = Self::serve(core, tasks).await;
        t.policy_calls = policy_calls;
        t.records = records;
        t
    }

    async fn serve(core: Arc<Hub>, tasks: HubTasks) -> T {
        let rx = core.subscribe();
        let app = hooks::router(core.clone(), TOKEN.to_owned());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (stop, stop_rx) = oneshot::channel::<()>();
        let c = core.clone();
        let server = tokio::spawn(async move {
            server::serve_until(listener, app, async move {
                let _ = stop_rx.await;
                c.shutdown();
            })
            .await
            .unwrap();
        });
        T {
            addr,
            core,
            rx,
            client: reqwest::Client::new(),
            policy_calls: Arc::new(Mutex::new(0)),
            records: Arc::new(Mutex::new(Vec::new())),
            stop: Some(stop),
            server,
            _tasks: tasks,
        }
    }

    fn post(&self, path: &str, body: &Value) -> reqwest::RequestBuilder {
        self.client
            .post(format!("http://{}{}", self.addr, path))
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json")
            .body(body.to_string())
    }

    /// `/hooks/permission`, chờ trả lời; trả (JSON, thời gian chờ).
    async fn permission_now(&self, body: &Value) -> (Value, Duration) {
        let started = Instant::now();
        let res = tokio::time::timeout(WAIT, self.post("/hooks/permission", body).send())
            .await
            .expect("hub phải trả lời ngay cho lệnh quá dài")
            .unwrap();
        (res.json().await.unwrap(), started.elapsed())
    }

    async fn event(&self, body: &Value) {
        let res = self.post("/hooks/event", body).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 204);
    }

    async fn next<R>(&mut self, mut pick: impl FnMut(&ServerMsg) -> Option<R>) -> R {
        loop {
            match tokio::time::timeout(WAIT, self.rx.recv()).await {
                Ok(Ok(m)) => {
                    if let Some(r) = pick(&m) {
                        return r;
                    }
                }
                other => panic!("không thấy thông điệp cần chờ: {other:?}"),
            }
        }
    }

    async fn handed_off(&mut self) -> HandoffView {
        self.next(|m| match m {
            ServerMsg::ApprovalHandedOff { handoff } => Some(handoff.clone()),
            ServerMsg::ApprovalRequested { approval } => {
                panic!(
                    "lệnh quá dài không được thành thẻ duyệt: {}",
                    approval.request_id
                )
            }
            _ => None,
        })
        .await
    }

    async fn cleared(&mut self) -> String {
        self.next(|m| match m {
            ServerMsg::HandoffCleared { handoff_id } => Some(handoff_id.clone()),
            _ => None,
        })
        .await
    }

    /// Không có `HandoffCleared` nào trong một khoảng ngắn (sau khi đã chắc vòng sự kiện xử lý xong).
    async fn no_clear_after_barrier(&mut self, session: &str) {
        // Rào: một sự kiện của phiên khác; khi thấy agent của nó cập nhật thì sự kiện trước đã được xử lý.
        self.event(&hook("UserPromptSubmit", session)).await;
        let target = format!("agt_{session}");
        self.next(|m| match m {
            ServerMsg::HandoffCleared { handoff_id } => panic!("không được xóa thẻ {handoff_id}"),
            ServerMsg::AgentUpdated { agent } if agent.id == target => Some(()),
            _ => None,
        })
        .await;
    }

    async fn stop(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        tokio::time::timeout(WAIT, &mut self.server)
            .await
            .expect("server không tắt")
            .unwrap();
    }
}

fn hook(name: &str, session: &str) -> Value {
    json!({ "hook_event_name": name, "session_id": session, "cwd": "D:/code" })
}

fn tool_hook(name: &str, session: &str, tool: &str, input: &Value) -> Value {
    let mut v = hook(name, session);
    v["tool_name"] = json!(tool);
    v["tool_input"] = input.clone();
    v
}

fn bash(cmd: &str) -> Value {
    json!({ "command": cmd, "description": "d" })
}

fn long_cmd() -> String {
    format!("echo {}", "x".repeat(5000))
}

#[tokio::test]
async fn long_command_returns_empty_immediately_never_holds_or_asks_policy() {
    let mut t = T::start().await;
    let input = bash(&long_cmd());
    let (res, took) = t
        .permission_now(&tool_hook("PermissionRequest", "s1", "Bash", &input))
        .await;
    assert_eq!(res, json!({}), "không allow dù policy là AutoAllow");
    assert!(took < Duration::from_secs(1), "không giữ: {took:?}");
    assert_eq!(*t.policy_calls.lock().unwrap(), 0, "không hỏi policy");
    assert!(t.core.approvals.snapshot().is_empty());

    let h = t.handed_off().await;
    assert_eq!(h.agent_id, "agt_s1");
    assert_eq!(h.tool, "Bash");
    assert_eq!(h.bytes, long_cmd().len());
    assert!(h.message.contains("terminal"));
    assert_eq!(t.core.approvals.handoffs(), vec![h.clone()]);

    let agent = t.core.agent_views().into_iter().next().unwrap();
    assert_eq!(agent.mood, Mood::Approval);
    assert_eq!(agent.act_text, TEXT_APPROVE_IN_TERMINAL);

    let recs = t.records.lock().unwrap().clone();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].request_id, h.handoff_id);
    assert_eq!(recs[0].status, ApprovalStatus::HandedOff);
    assert_eq!(recs[0].decided_by, None);
    t.stop().await;
}

#[tokio::test]
async fn threshold_boundaries_bytes_and_lines() {
    let mut t = T::start().await;
    // Đúng ngưỡng: vẫn là thẻ duyệt bình thường (policy AutoAllow được hỏi và allow).
    let at = format!("echo {}", "a".repeat(4096 - 5));
    assert_eq!(at.len(), 4096);
    let (res, _) = t
        .permission_now(&tool_hook("PermissionRequest", "s1", "Bash", &bash(&at)))
        .await;
    assert_eq!(
        res.pointer("/hookSpecificOutput/decision/behavior"),
        Some(&json!("allow"))
    );
    assert_eq!(*t.policy_calls.lock().unwrap(), 1);
    assert!(t.core.approvals.handoffs().is_empty());

    // Hơn một byte: trả về terminal.
    let over = format!("{at}a");
    let (res, _) = t
        .permission_now(&tool_hook("PermissionRequest", "s2", "Bash", &bash(&over)))
        .await;
    assert_eq!(res, json!({}));
    assert_eq!(*t.policy_calls.lock().unwrap(), 1);
    t.handed_off().await;

    // 50 dòng ngắn: bình thường; 51 dòng: trả về terminal.
    let fifty = vec!["ls"; 50].join("\n");
    let (res, _) = t
        .permission_now(&tool_hook("PermissionRequest", "s3", "Bash", &bash(&fifty)))
        .await;
    assert_ne!(res, json!({}));
    let fifty_one = format!("{fifty}\nls");
    let (res, _) = t
        .permission_now(&tool_hook(
            "PermissionRequest",
            "s4",
            "Bash",
            &bash(&fifty_one),
        ))
        .await;
    assert_eq!(res, json!({}));
    assert_eq!(t.handed_off().await.lines, 51);
    t.stop().await;
}

#[tokio::test]
async fn threshold_comes_from_config() {
    let mut cfg = Config::default();
    cfg.hub.handoff_max_bytes = 10;
    let (core, tasks) = Hub::start(cfg, PersonaBook::default(), "laptop".to_owned());
    let mut t = T::serve(core, tasks).await;
    let (res, took) = t
        .permission_now(&tool_hook(
            "PermissionRequest",
            "s1",
            "Bash",
            &bash("echo 1234567890"),
        ))
        .await;
    assert_eq!(res, json!({}));
    assert!(took < Duration::from_secs(1));
    assert_eq!(t.handed_off().await.bytes, 15);
    t.stop().await;
}

#[tokio::test]
async fn edit_tools_are_never_handed_off() {
    let t = T::start().await;
    let (res, _) = t
        .permission_now(&tool_hook(
            "PermissionRequest",
            "s1",
            "Write",
            &json!({ "file_path": "/a", "content": "c".repeat(20_000) }),
        ))
        .await;
    // AutoAllow của policy giả: thẻ thường, có cắt nội dung; không phải handoff.
    assert_eq!(
        res.pointer("/hookSpecificOutput/decision/behavior"),
        Some(&json!("allow"))
    );
    assert!(t.core.approvals.handoffs().is_empty());
    t.stop().await;
}

#[tokio::test]
async fn decide_and_input_rejected_for_handoff_id() {
    let mut t = T::start().await;
    t.permission_now(&tool_hook(
        "PermissionRequest",
        "s1",
        "Bash",
        &bash(&long_cmd()),
    ))
    .await;
    let h = t.handed_off().await;
    assert!(h.handoff_id.starts_with("hof_"));
    for (decision, origin) in [
        (Decision::Allow, DecideOrigin::User),
        (Decision::Deny, DecideOrigin::User),
        (Decision::Allow, DecideOrigin::Countdown),
    ] {
        assert_eq!(
            t.core
                .approvals
                .decide(&h.handoff_id, decision, origin, None),
            DecideResult::HandedOff
        );
    }
    assert_eq!(t.core.approvals.tool_input(&h.handoff_id), None);
    assert_eq!(t.core.approvals.handoffs().len(), 1, "Decide không xóa thẻ");
    t.stop().await;
}

#[tokio::test]
async fn flags_computed_on_full_text() {
    let mut t = T::start().await;
    let cmd = format!(
        "echo {}\u{E0041}\u{E0042}{}",
        "a".repeat(3000),
        "b".repeat(3000)
    );
    t.permission_now(&tool_hook("PermissionRequest", "s1", "Bash", &bash(&cmd)))
        .await;
    let h = t.handed_off().await;
    assert!(!h.preview_head.contains("U+E00"));
    assert!(!h.preview_tail.contains("U+E00"));
    assert_eq!(h.suspicions, vec![Suspicion::HiddenChars]);
    t.stop().await;
}

#[tokio::test]
async fn card_cleared_by_supersede_events() {
    // PostToolUse cùng tool + input, Stop, StopFailure, UserPromptSubmit, SessionEnd đều xóa thẻ.
    for (name, same_input) in [
        ("PostToolUse", true),
        ("PostToolUseFailure", true),
        ("Stop", false),
        ("StopFailure", false),
        ("UserPromptSubmit", false),
        ("SessionEnd", false),
    ] {
        let mut t = T::start().await;
        let input = bash(&long_cmd());
        t.permission_now(&tool_hook("PermissionRequest", "s1", "Bash", &input))
            .await;
        let h = t.handed_off().await;
        let ev = if same_input {
            tool_hook(name, "s1", "Bash", &input)
        } else {
            hook(name, "s1")
        };
        t.event(&ev).await;
        assert_eq!(t.cleared().await, h.handoff_id, "{name}");
        assert!(t.core.approvals.handoffs().is_empty(), "{name}");
        t.stop().await;
    }
}

#[tokio::test]
async fn subagent_stop_clears_only_its_card() {
    let mut t = T::start().await;
    let mut req = tool_hook("PermissionRequest", "s1", "Bash", &bash(&long_cmd()));
    req["agent_id"] = json!("a1");
    req["agent_type"] = json!("general-purpose");
    t.permission_now(&req).await;
    let h = t.handed_off().await;
    assert_eq!(h.agent_id, "agt_s1_a1");
    // Cha dừng lượt: không phải agent của thẻ.
    t.event(&hook("Stop", "s1")).await;
    t.no_clear_after_barrier("rao1").await;
    let mut stop = hook("SubagentStop", "s1");
    stop["agent_id"] = json!("a1");
    t.event(&stop).await;
    assert_eq!(t.cleared().await, h.handoff_id);
    t.stop().await;
}

#[tokio::test]
async fn notification_and_other_tools_do_not_clear_card() {
    let mut t = T::start().await;
    let input = bash(&long_cmd());
    t.permission_now(&tool_hook("PermissionRequest", "s1", "Bash", &input))
        .await;
    t.handed_off().await;
    t.event(&hook("Notification", "s1")).await;
    t.event(&tool_hook("PostToolUse", "s1", "Bash", &bash("ls")))
        .await;
    t.event(&tool_hook("PreToolUse", "s1", "Bash", &input))
        .await;
    t.event(&hook("Stop", "s2")).await;
    t.no_clear_after_barrier("rao").await;
    assert_eq!(t.core.approvals.handoffs().len(), 1);
    t.stop().await;
}

#[tokio::test]
async fn handoff_cards_are_capped() {
    let mut t = T::start().await;
    let mut first = None;
    for i in 0..MAX_HANDOFFS {
        t.permission_now(&tool_hook(
            "PermissionRequest",
            &format!("s{i}"),
            "Bash",
            &bash(&long_cmd()),
        ))
        .await;
        let h = t.handed_off().await;
        first.get_or_insert(h.handoff_id);
    }
    t.permission_now(&tool_hook(
        "PermissionRequest",
        "moi",
        "Bash",
        &bash(&long_cmd()),
    ))
    .await;
    assert_eq!(t.cleared().await, first.unwrap(), "thẻ cũ nhất bị bỏ");
    assert_eq!(t.core.approvals.handoffs().len(), MAX_HANDOFFS);
    t.stop().await;
}

#[tokio::test]
async fn paused_team_still_hands_off() {
    let mut t = T::start().await;
    t.core.set_paused(true);
    let (res, took) = t
        .permission_now(&tool_hook(
            "PermissionRequest",
            "s1",
            "Bash",
            &bash(&long_cmd()),
        ))
        .await;
    assert_eq!(res, json!({}));
    assert!(took < Duration::from_secs(1));
    t.handed_off().await;
    t.stop().await;
}
