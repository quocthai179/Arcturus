//! Test T5a: hub nối luồng duyệt (T3) với trạng thái agent, persona, cấu hình (T4) và kênh broadcast `ServerMsg`.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use hub::approvals::{AlwaysAsk, ApprovalsConfig, DecideOrigin, NoStore, Policy, PolicyOutcome};
use hub::hub::{Hub, HubTasks};
use hub::{hooks, server};
use hub_core::config::Config;
use hub_core::persona::PersonaBook;
use hub_core::protocol::{AgentView, ApprovalView, ServerMsg, Suspicion};
use hub_core::schema::{Activity, DecidedBy, Decision, Mood, Risk};
use serde_json::{json, Value};
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;

const TOKEN: &str = "test-token-0123456789abcdef-arcturus-t5";
const WAIT: Duration = Duration::from_secs(5);
const CWD: &str = r"D:\code\Arcturus\.claude\worktrees\hub";

struct T {
    addr: SocketAddr,
    core: Arc<Hub>,
    rx: broadcast::Receiver<ServerMsg>,
    client: reqwest::Client,
    stop: Option<oneshot::Sender<()>>,
    server: JoinHandle<()>,
    _tasks: HubTasks,
}

impl T {
    async fn start() -> T {
        Self::start_with(
            Config::default(),
            ApprovalsConfig::default(),
            Box::new(AlwaysAsk),
        )
        .await
    }

    async fn start_with(cfg: Config, acfg: ApprovalsConfig, policy: Box<dyn Policy>) -> T {
        let personas = cfg.persona_book().unwrap();
        let (core, tasks) = Hub::start_with(
            cfg,
            personas,
            "laptop".to_owned(),
            acfg,
            policy,
            Box::new(NoStore),
        );
        Self::serve(core, tasks).await
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
            stop: Some(stop),
            server,
            _tasks: tasks,
        }
    }

    fn post(&self, path: &str, body: Value) -> reqwest::RequestBuilder {
        self.client
            .post(format!("http://{}{}", self.addr, path))
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json")
            .body(body.to_string())
    }

    async fn event(&self, body: Value) {
        let res = self.post("/hooks/event", body).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 204);
    }

    fn permission(&self, body: Value) -> JoinHandle<Value> {
        let req = self.post("/hooks/permission", body);
        tokio::spawn(async move { req.send().await.unwrap().json::<Value>().await.unwrap() })
    }

    /// Thông điệp kế tiếp thỏa `pick`, bỏ qua các thông điệp khác.
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

    async fn agent(&mut self, id: &str, mood: Mood) -> AgentView {
        self.next(|m| match m {
            ServerMsg::AgentUpdated { agent } if agent.id == id && agent.mood == mood => {
                Some(agent.clone())
            }
            _ => None,
        })
        .await
    }

    async fn requested(&mut self) -> ApprovalView {
        self.next(|m| match m {
            ServerMsg::ApprovalRequested { approval } => Some(approval.clone()),
            _ => None,
        })
        .await
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
    json!({ "hook_event_name": name, "session_id": session, "cwd": CWD })
}

fn tool_hook(name: &str, session: &str, tool: &str, input: Value) -> Value {
    let mut v = hook(name, session);
    v["tool_name"] = json!(tool);
    v["tool_input"] = input;
    v
}

fn bash_permission(session: &str, cmd: &str) -> Value {
    tool_hook(
        "PermissionRequest",
        session,
        "Bash",
        json!({ "command": cmd, "description": "d" }),
    )
}

fn khoa_config() -> Config {
    Config::from_toml(
        r##"
[[persona]]
match_cwd = "**/.claude/worktrees/hub*"
name = "Khoa"
role = "backend"
palette = { hair = "#1F1F24", outfit = "#4C8DF6", acc = "#9AA3B5" }
"##,
        std::path::Path::new("test.toml"),
    )
    .unwrap()
}

#[tokio::test]
async fn event_loop_applies_state_and_broadcasts() {
    let mut t = T::start_with(
        khoa_config(),
        ApprovalsConfig::default(),
        Box::new(AlwaysAsk),
    )
    .await;
    t.event(tool_hook(
        "PreToolUse",
        "s1",
        "Edit",
        json!({ "file_path": r"D:\code\ui\LoginForm.tsx" }),
    ))
    .await;
    let a = t.agent("agt_s1", Mood::Working).await;
    assert_eq!(a.name, "Khoa", "persona từ arcturus.toml");
    assert_eq!(a.activity, Activity::Typing);
    assert_eq!(a.act_text, "Đang sửa LoginForm.tsx");
    assert_eq!(a.host, "laptop");
    assert_eq!(t.core.agent_views().len(), 1);
    t.stop().await;
}

#[tokio::test]
async fn permission_sets_agent_approval_then_thinking() {
    let mut t = T::start_with(
        khoa_config(),
        ApprovalsConfig::default(),
        Box::new(AlwaysAsk),
    )
    .await;
    let h = t.permission(bash_permission("s1", "git push"));
    let a = t.agent("agt_s1", Mood::Approval).await;
    assert_eq!(a.activity, Activity::Waiting);
    assert_eq!(a.act_text, "Chờ duyệt: Bash");
    let card = t.requested().await;
    assert_eq!(card.agent_name, "Khoa");
    assert_eq!(card.agent_id, "agt_s1");
    assert_eq!(card.command_normalized, "git push");
    assert_eq!(card.risk, Risk::Medium);

    t.core
        .approvals
        .decide(&card.request_id, Decision::Allow, DecideOrigin::User, None);
    let resolved = t
        .next(|m| match m {
            ServerMsg::ApprovalResolved {
                request_id,
                decision,
                decided_by,
            } => Some((request_id.clone(), *decision, *decided_by)),
            _ => None,
        })
        .await;
    assert_eq!(
        resolved,
        (card.request_id, Some(Decision::Allow), DecidedBy::User)
    );
    let a = t.agent("agt_s1", Mood::Working).await;
    assert_eq!(a.activity, Activity::Thinking);
    let res = tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    assert_eq!(
        res.pointer("/hookSpecificOutput/decision/behavior"),
        Some(&json!("allow"))
    );
    t.stop().await;
}

#[tokio::test]
async fn timeout_keeps_agent_waiting_and_uses_configured_timeout() {
    let mut cfg = Config::default();
    cfg.hub.approval_timeout_secs = 1;
    let (core, tasks) = Hub::start(cfg, PersonaBook::default(), "laptop".to_owned());
    let mut t = T::serve(core, tasks).await;
    let started = std::time::Instant::now();
    let h = t.permission(bash_permission("s1", "ls"));
    t.requested().await;
    let res = tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    assert_eq!(res, json!({}));
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_secs(4),
        "timeout lấy từ cấu hình (1 s), đo được {took:?}"
    );
    // Terminal vẫn đang hỏi người: nhân vật vẫn giơ tay.
    let views = t.core.agent_views();
    assert_eq!(views[0].mood, Mood::Approval);
    t.stop().await;
}

#[tokio::test]
async fn session_end_resolves_then_removes() {
    let mut t = T::start().await;
    let h = t.permission(bash_permission("s1", "ls"));
    let card = t.requested().await;
    t.event(hook("SessionEnd", "s1")).await;
    let order = t
        .next(|m| match m {
            ServerMsg::ApprovalResolved { request_id, .. } => {
                Some(format!("resolved {request_id}"))
            }
            ServerMsg::AgentRemoved { id } => Some(format!("removed {id}")),
            _ => None,
        })
        .await;
    assert_eq!(order, format!("resolved {}", card.request_id));
    let next = t
        .next(|m| match m {
            ServerMsg::AgentRemoved { id } => Some(id.clone()),
            _ => None,
        })
        .await;
    assert_eq!(next, "agt_s1");
    assert_eq!(
        tokio::time::timeout(WAIT, h).await.unwrap().unwrap(),
        json!({})
    );
    assert!(t.core.agent_views().is_empty());
    t.stop().await;
}

#[tokio::test]
async fn broadcast_card_is_truncated_full_input_kept() {
    let mut t = T::start().await;
    let long = "x".repeat(5000);
    let h = t.permission(tool_hook(
        "PermissionRequest",
        "s1",
        "Write",
        json!({ "file_path": "/repo/a.txt", "content": long }),
    ));
    let card = t.requested().await;
    assert!(card.truncated);
    assert!(card.command_original.chars().count() < 2300);
    assert!(card.command_original.contains("/repo/a.txt"));
    let full = t.core.approvals.tool_input(&card.request_id).unwrap();
    assert_eq!(full["content"].as_str().unwrap().len(), 5000);
    t.core
        .approvals
        .decide(&card.request_id, Decision::Deny, DecideOrigin::User, None);
    tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    assert_eq!(t.core.approvals.tool_input(&card.request_id), None);
    t.stop().await;
}

#[tokio::test]
async fn hidden_chars_flagged_on_card() {
    let mut t = T::start().await;
    let _h = t.permission(bash_permission("s1", "echo ok\u{202E}txt.exe"));
    let card = t.requested().await;
    assert_eq!(card.command_normalized, "echo ok⟨U+202E⟩txt.exe");
    assert_eq!(card.suspicions, vec![Suspicion::HiddenChars]);
    t.stop().await;
}

struct Countdown;

impl Policy for Countdown {
    fn decide(&self, _ctx: &hub::approvals::ApprovalCtx<'_>) -> PolicyOutcome {
        PolicyOutcome::AskUser {
            risk: Risk::Low,
            countdown: Some(Duration::from_secs(30)),
        }
    }
}

#[tokio::test]
async fn pause_broadcasts_team_and_clears_countdown() {
    let mut t = T::start_with(
        Config::default(),
        ApprovalsConfig::default(),
        Box::new(Countdown),
    )
    .await;
    let _h = t.permission(bash_permission("s1", "ls"));
    let card = t.requested().await;
    assert_eq!(card.countdown_ms, Some(30_000));

    t.core.set_paused(true);
    let team = t
        .next(|m| match m {
            ServerMsg::TeamUpdated { team } => Some(*team),
            _ => None,
        })
        .await;
    assert!(team.paused);
    assert_eq!(team.roster_max, 5);
    assert!(t.core.team_view().paused);
    assert_eq!(t.core.approvals.views()[0].countdown_ms, None);

    t.core.set_paused(false);
    t.core.set_focus(true);
    let team = t
        .next(|m| match m {
            ServerMsg::TeamUpdated { team } if team.focus => Some(*team),
            _ => None,
        })
        .await;
    assert!(!team.paused);
    assert_eq!(
        t.core.approvals.views()[0].countdown_ms,
        None,
        "tiếp tục không khôi phục đếm ngược"
    );
    t.stop().await;
}

#[tokio::test]
async fn stop_turns_done_then_idle_via_tick() {
    let mut t = T::start().await;
    t.event(hook("UserPromptSubmit", "s1")).await;
    t.agent("agt_s1", Mood::Working).await;
    t.event(hook("Stop", "s1")).await;
    t.agent("agt_s1", Mood::Done).await;
    let started = std::time::Instant::now();
    let idle = t.agent("agt_s1", Mood::Idle).await;
    assert_eq!(idle.act_text, "Đang rảnh");
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(2500) && took < Duration::from_millis(4500),
        "done giữ khoảng 3 s, đo được {took:?}"
    );
    t.stop().await;
}

#[tokio::test]
async fn hub_stops_background_tasks_when_dropped() {
    let (core, tasks) = Hub::start(
        Config::default(),
        PersonaBook::default(),
        "laptop".to_owned(),
    );
    drop(core);
    tokio::time::timeout(WAIT, tasks.events)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(WAIT, tasks.tick)
        .await
        .unwrap()
        .unwrap();
}
