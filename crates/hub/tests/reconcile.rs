//! Test T7 (ARC-28): đối soát phiên với hub thật và một `AgentLister` giả.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hub::approvals::{AlwaysAsk, ApprovalsConfig, NoStore, PermissionOutcome};
use hub::hub::Hub;
use hub::reconcile::{spawn, tick, AgentLister, Reconciler};
use hub_core::config::Config;
use hub_core::hook::{self, HookInput};
use hub_core::persona::PersonaBook;
use hub_core::protocol::ServerMsg;
use hub_core::schema::DecidedBy;
use serde_json::json;
use time::OffsetDateTime;

/// Trả lần lượt các kết quả đã soạn; hết thì lặp lại kết quả cuối. Đếm số lần được hỏi.
struct Script {
    answers: Mutex<VecDeque<Option<HashSet<String>>>>,
    last: Mutex<Option<HashSet<String>>>,
    calls: Arc<AtomicUsize>,
}

impl Script {
    fn new(answers: Vec<Option<&[&str]>>) -> (Script, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let q = answers
            .into_iter()
            .map(|a| a.map(|s| s.iter().map(|x| (*x).to_owned()).collect()))
            .collect();
        (
            Script {
                answers: Mutex::new(q),
                last: Mutex::new(None),
                calls: calls.clone(),
            },
            calls,
        )
    }
}

impl AgentLister for Script {
    fn list(&self) -> impl Future<Output = Option<HashSet<String>>> + Send {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = self.answers.lock().unwrap().pop_front();
        let out = match next {
            Some(a) => {
                *self.last.lock().unwrap() = a.clone();
                a
            }
            None => self.last.lock().unwrap().clone(),
        };
        async move { out }
    }
}

fn hub() -> Arc<Hub> {
    let cfg = ApprovalsConfig {
        min_review: Duration::ZERO,
        ..ApprovalsConfig::default()
    };
    let (h, tasks) = Hub::start_with(
        Config::default(),
        PersonaBook::default(),
        "laptop".to_owned(),
        cfg,
        Box::new(AlwaysAsk),
        Box::new(NoStore),
    );
    // Các task nền sống tới hết test (giữ bằng `forget`: `HubTasks` không dừng khi bị drop, xem `Hub::start_with`).
    std::mem::forget(tasks);
    h
}

fn prompt(h: &Hub, session: &str) {
    h.ingest(&HookInput::new(json!({
        "hook_event_name": "UserPromptSubmit", "session_id": session, "cwd": "/w/proj",
    })));
}

fn agent_ids(h: &Hub) -> Vec<String> {
    let mut v: Vec<String> = h.agent_views().into_iter().map(|a| a.id).collect();
    v.sort();
    v
}

#[tokio::test]
async fn session_missing_twice_is_closed_like_a_real_session_end() {
    let h = hub();
    prompt(&h, "s1");
    prompt(&h, "s2");
    // Yêu cầu duyệt đang giữ của s2 phải được hủy (trả `{}`), không treo tới hết giờ.
    let input = HookInput::new(json!({
        "hook_event_name": "PermissionRequest", "session_id": "s2", "cwd": "/w/proj",
        "tool_name": "Bash", "tool_input": {"command": "npm test"},
    }));
    let ev = hook::to_event(&input, "laptop", OffsetDateTime::now_utc());
    let h2 = h.clone();
    let held = tokio::spawn(async move { h2.approvals.request(input, &ev, "Khoa").await });
    for _ in 0..100 {
        if !h.approvals.views().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(h.approvals.views().len(), 1);
    let mut rx = h.subscribe();

    let (lister, _) = Script::new(vec![Some(&["s1"][..])]);
    let mut rec = Reconciler::default();
    let first = tick(&h, &mut rec, &lister, Duration::ZERO).await;
    assert!(first.is_empty(), "một lần vắng chưa đủ");
    assert_eq!(agent_ids(&h), ["agt_s1", "agt_s2"]);
    let second = tick(&h, &mut rec, &lister, Duration::ZERO).await;
    assert_eq!(second, ["s2"]);
    assert_eq!(
        agent_ids(&h),
        ["agt_s1"],
        "s1 còn trong danh sách nên ở lại"
    );

    assert_eq!(
        held.await.unwrap(),
        PermissionOutcome::Cancelled,
        "yêu cầu đang giữ của phiên bị dọn phải trả {{}}, không bao giờ allow"
    );
    // Cùng đường xử lý của SessionEnd thật: hủy yêu cầu trước, xóa agent sau.
    let mut seen = Vec::new();
    while let Ok(m) = rx.try_recv() {
        seen.push(m);
    }
    let resolved = seen
        .iter()
        .position(|m| {
            matches!(
                m,
                ServerMsg::ApprovalResolved {
                    decided_by: DecidedBy::Cancelled,
                    ..
                }
            )
        })
        .expect("ApprovalResolved(cancelled)");
    let removed = seen
        .iter()
        .position(|m| matches!(m, ServerMsg::AgentRemoved { id } if id == "agt_s2"))
        .expect("AgentRemoved(agt_s2)");
    assert!(resolved < removed);
}

#[tokio::test]
async fn lister_failure_closes_nothing() {
    let h = hub();
    prompt(&h, "s1");
    let (lister, calls) = Script::new(vec![None]);
    let mut rec = Reconciler::default();
    for _ in 0..5 {
        assert!(tick(&h, &mut rec, &lister, Duration::ZERO).await.is_empty());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    assert_eq!(agent_ids(&h), ["agt_s1"]);
}

#[tokio::test]
async fn subagent_activity_keeps_its_session_alive_and_is_removed_with_it() {
    let h = hub();
    h.ingest(&HookInput::new(json!({
        "hook_event_name": "SubagentStart", "session_id": "s1", "cwd": "/w/proj",
        "agent_id": "a1", "agent_type": "general-purpose",
    })));
    assert_eq!(agent_ids(&h), ["agt_s1", "agt_s1_a1"]);
    assert_eq!(h.sessions().len(), 1, "agent con không phải phiên riêng");
    let (lister, _) = Script::new(vec![Some(&[][..])]);
    let mut rec = Reconciler::default();
    tick(&h, &mut rec, &lister, Duration::ZERO).await;
    assert_eq!(tick(&h, &mut rec, &lister, Duration::ZERO).await, ["s1"]);
    assert!(agent_ids(&h).is_empty(), "agent con đi theo phiên");
}

#[tokio::test]
async fn recent_activity_protects_a_session_that_claude_does_not_list() {
    let h = hub();
    prompt(&h, "s1");
    let (lister, _) = Script::new(vec![Some(&[][..])]);
    let mut rec = Reconciler::default();
    for _ in 0..4 {
        // Sự kiện vừa tới (< 1 giờ): không bị dọn dù `claude` không liệt kê.
        assert!(tick(&h, &mut rec, &lister, Duration::from_secs(3600))
            .await
            .is_empty());
    }
    assert_eq!(agent_ids(&h), ["agt_s1"]);
}

#[tokio::test]
async fn empty_hub_never_runs_the_lister() {
    let h = hub();
    let (lister, calls) = Script::new(vec![Some(&["x"][..])]);
    let mut rec = Reconciler::default();
    for _ in 0..3 {
        assert!(tick(&h, &mut rec, &lister, Duration::ZERO).await.is_empty());
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "không có phiên thì không chạy claude"
    );
}

#[tokio::test(start_paused = true)]
async fn spawned_task_ticks_on_the_interval_and_stops_when_hub_is_dropped() {
    let h = hub();
    prompt(&h, "s1");
    let (lister, calls) = Script::new(vec![Some(&["s1"][..])]);
    let task = spawn(Arc::downgrade(&h), lister, Duration::from_secs(30));
    tokio::time::sleep(Duration::from_secs(29)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0, "chưa tới chu kỳ đầu");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    drop(h);
    // Hub đã bị drop (các task nền chỉ giữ Weak): vòng kế tiếp thấy vậy và dừng.
    tokio::time::sleep(Duration::from_secs(31)).await;
    assert!(task.is_finished());
}
