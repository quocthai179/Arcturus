//! Sàn thời gian duyệt (T5d · ARC-44, biện pháp B4 cho F6): mọi `allow` (của người hay đếm ngược) tới sớm hơn
//! `min_review` kể từ lúc thẻ được phát bị giữ lại (không tính là từ chối). Mục tiêu: script đọc được token UI không tự duyệt ngay lập tức được, và
//! người dùng không lỡ tay duyệt thẻ vừa bật lên.
//!
//! Dùng thời gian thật (thẻ đo bằng `std::time::Instant`), với sàn ngắn để test nhanh.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use hub::approvals::{
    AlwaysAsk, ApprovalCtx, Approvals, ApprovalsConfig, DecideOrigin, DecideResult, HeldReason,
    NoStore, PermissionOutcome, Policy, PolicyOutcome, DEFAULT_MIN_REVIEW,
};
use hub_core::hook::{self, HookInput};
use hub_core::protocol::ServerMsg;
use hub_core::schema::{Decision, Risk};
use serde_json::{json, Value};
use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

const WAIT: Duration = Duration::from_secs(5);
const FLOOR: Duration = Duration::from_millis(300);

struct Countdown(Duration);

impl Policy for Countdown {
    fn decide(&self, _ctx: &ApprovalCtx<'_>, _a: &hub::approvals::Assessment) -> PolicyOutcome {
        PolicyOutcome::AskUser {
            risk: Risk::Low,
            countdown: Some(self.0),
        }
    }
}

fn approvals(
    min_review: Duration,
    policy: Box<dyn Policy>,
) -> (Arc<Approvals>, broadcast::Receiver<ServerMsg>) {
    let (tx, rx) = broadcast::channel(256);
    let cfg = ApprovalsConfig {
        min_review,
        ..ApprovalsConfig::default()
    };
    (
        Arc::new(Approvals::new(cfg, policy, Box::new(NoStore), tx)),
        rx,
    )
}

fn bash_body(session: &str) -> Value {
    json!({
        "hook_event_name": "PermissionRequest",
        "session_id": session,
        "cwd": "D:\\code\\Arcturus",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
    })
}

fn spawn_request(a: &Arc<Approvals>, body: Value) -> JoinHandle<PermissionOutcome> {
    let a = a.clone();
    tokio::spawn(async move {
        let input = HookInput::new(body);
        let ev = hook::to_event(&input, "test-host", OffsetDateTime::now_utc());
        a.request(input, &ev, "Khoa").await
    })
}

async fn next_requested(rx: &mut broadcast::Receiver<ServerMsg>) -> String {
    loop {
        match tokio::time::timeout(WAIT, rx.recv()).await {
            Ok(Ok(ServerMsg::ApprovalRequested { approval })) => return approval.request_id,
            Ok(Ok(_)) => continue,
            other => panic!("không thấy ApprovalRequested: {other:?}"),
        }
    }
}

async fn join<T>(h: JoinHandle<T>) -> T {
    tokio::time::timeout(WAIT, h)
        .await
        .expect("handler không trả lời")
        .unwrap()
}

fn allowed(o: &PermissionOutcome) -> bool {
    matches!(
        hub::approvals::response_for(o)
            .pointer("/hookSpecificOutput/decision/behavior")
            .and_then(Value::as_str),
        Some("allow")
    )
}

// ============================================================================================================

#[test]
fn default_min_review_is_one_second() {
    assert_eq!(DEFAULT_MIN_REVIEW, Duration::from_millis(1000));
    assert_eq!(ApprovalsConfig::default().min_review, DEFAULT_MIN_REVIEW);
}

#[tokio::test]
async fn allow_before_min_review_is_held_then_accepted() {
    let (a, mut rx) = approvals(FLOOR, Box::new(AlwaysAsk));
    let h = spawn_request(&a, bash_body("s1"));
    let id = next_requested(&mut rx).await;

    // Bấm ngay khi thẻ vừa hiện: bị giữ, yêu cầu vẫn chờ, không bị từ chối.
    assert!(matches!(
        a.decide(&id, Decision::Allow, DecideOrigin::User, None),
        DecideResult::Held(HeldReason::TooFast)
    ));
    assert!(a.pending(&id).is_some(), "yêu cầu phải còn chờ");
    assert!(!h.is_finished());

    // Sau sàn: được nhận.
    tokio::time::sleep(FLOOR + Duration::from_millis(50)).await;
    assert!(matches!(
        a.decide(&id, Decision::Allow, DecideOrigin::User, None),
        DecideResult::Resolved
    ));
    assert!(allowed(&join(h).await));
}

#[tokio::test]
async fn deny_ignores_min_review() {
    let (a, mut rx) = approvals(Duration::from_secs(60), Box::new(AlwaysAsk));
    let h = spawn_request(&a, bash_body("s1"));
    let id = next_requested(&mut rx).await;

    assert!(matches!(
        a.decide(&id, Decision::Deny, DecideOrigin::User, None),
        DecideResult::Resolved
    ));
    let o = join(h).await;
    assert!(!allowed(&o));
    assert!(matches!(o, PermissionOutcome::Decided(_)));
}

#[tokio::test]
async fn min_review_zero_disables_floor() {
    let (a, mut rx) = approvals(Duration::ZERO, Box::new(AlwaysAsk));
    let h = spawn_request(&a, bash_body("s1"));
    let id = next_requested(&mut rx).await;

    assert!(matches!(
        a.decide(&id, Decision::Allow, DecideOrigin::User, None),
        DecideResult::Resolved
    ));
    assert!(allowed(&join(h).await));
}

#[tokio::test]
async fn countdown_allow_also_respects_floor() {
    // `origin: countdown` do client khai (audit ARC-44 finding 4): đếm ngược 100 ms nhưng sàn 300 ms, allow lúc 150 ms
    // vẫn bị giữ; sau sàn mới được nhận.
    let (a, mut rx) = approvals(FLOOR, Box::new(Countdown(Duration::from_millis(100))));
    let h = spawn_request(&a, bash_body("s1"));
    let id = next_requested(&mut rx).await;

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(matches!(
        a.decide(&id, Decision::Allow, DecideOrigin::Countdown, None),
        DecideResult::Held(HeldReason::TooFast)
    ));
    assert!(a.pending(&id).is_some());

    tokio::time::sleep(FLOOR).await;
    assert!(matches!(
        a.decide(&id, Decision::Allow, DecideOrigin::Countdown, None),
        DecideResult::Resolved
    ));
    assert!(allowed(&join(h).await));
}

#[tokio::test]
async fn held_too_fast_never_allows_on_timeout_or_shutdown() {
    let (a, mut rx) = approvals(Duration::from_secs(60), Box::new(AlwaysAsk));
    let h = spawn_request(&a, bash_body("s1"));
    let id = next_requested(&mut rx).await;
    for _ in 0..5 {
        assert!(matches!(
            a.decide(&id, Decision::Allow, DecideOrigin::User, None),
            DecideResult::Held(HeldReason::TooFast)
        ));
    }
    assert_eq!(a.shutdown(), 1);
    let o = join(h).await;
    assert!(!allowed(&o), "bị giữ rồi tắt hub phải là {{}}");
    assert!(matches!(o, PermissionOutcome::Cancelled));
}
