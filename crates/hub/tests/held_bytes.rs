//! Trần byte cho `tool_input` đang giữ (T5c · ARC-43): hàng đợi không được giữ quá `held_input_max_bytes`.
//!
//! Bất biến: vượt trần thì trả `{}` ngay (Claude Code hỏi trong terminal), không bao giờ allow; mọi đường lấy yêu cầu ra
//! khỏi hàng đợi đều trả lại byte (bộ đếm về 0 khi hàng đợi rỗng).

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use hub::approvals::{
    AlwaysAsk, Approvals, ApprovalsConfig, DecideOrigin, DecideResult, NoStore, PermissionOutcome,
    DEFAULT_HELD_INPUT_MAX_BYTES,
};
use hub_core::hook::{self, HookInput};
use hub_core::protocol::ServerMsg;
use hub_core::schema::Decision;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

const WAIT: Duration = Duration::from_secs(5);

fn approvals_with(cfg: ApprovalsConfig) -> (Arc<Approvals>, broadcast::Receiver<ServerMsg>) {
    let (tx, rx) = broadcast::channel(1024);
    let a = Approvals::new(cfg, Box::new(AlwaysAsk), Box::new(NoStore), tx);
    (Arc::new(a), rx)
}

fn cfg(held_input_max_bytes: usize) -> ApprovalsConfig {
    ApprovalsConfig {
        held_input_max_bytes,
        ..ApprovalsConfig::default()
    }
}

/// `PermissionRequest` cho Write với nội dung `content_len` byte (thẻ bị cắt, không trả về terminal).
fn write_body(session: &str, n: usize, content_len: usize) -> Value {
    json!({
        "hook_event_name": "PermissionRequest",
        "session_id": session,
        "cwd": "D:\\code\\Arcturus",
        "tool_name": "Write",
        "tool_input": { "file_path": format!("D:\\code\\Arcturus\\f{n}.txt"), "content": "x".repeat(content_len) },
    })
}

fn bash_body(session: &str, command: &str) -> Value {
    json!({
        "hook_event_name": "PermissionRequest",
        "session_id": session,
        "cwd": "D:\\code\\Arcturus",
        "tool_name": "Bash",
        "tool_input": { "command": command },
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

fn apply_event(a: &Approvals, body: Value) -> usize {
    let input = HookInput::new(body);
    let ev = hook::to_event(&input, "test-host", OffsetDateTime::now_utc());
    a.on_event(&ev, &input)
}

/// Chờ `ApprovalRequested` kế tiếp, trả `request_id`.
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

/// Kết quả phải là "không quyết định" (`{}`), không bao giờ allow.
fn assert_overloaded(o: &PermissionOutcome) {
    assert!(
        matches!(o, PermissionOutcome::Overloaded),
        "vượt trần phải trả {{}} ngay, nhận {o:?}"
    );
}

// ============================================================================================================

#[test]
fn default_cap_is_8_mib() {
    assert_eq!(DEFAULT_HELD_INPUT_MAX_BYTES, 8 * 1024 * 1024);
    assert_eq!(
        ApprovalsConfig::default().held_input_max_bytes,
        DEFAULT_HELD_INPUT_MAX_BYTES
    );
}

#[tokio::test]
async fn held_bytes_cap_returns_empty_never_allow() {
    let (a, mut rx) = approvals_with(cfg(3_000));

    let h1 = spawn_request(&a, write_body("s1", 1, 1_200));
    next_requested(&mut rx).await;
    let h2 = spawn_request(&a, write_body("s2", 2, 1_200));
    next_requested(&mut rx).await;
    let held = a.held_bytes();
    assert!((2_400..=3_000).contains(&held), "đang giữ {held} byte");

    // Yêu cầu thứ ba làm tổng vượt 3.000 byte: trả `{}` ngay, không vào hàng đợi.
    let o3 = join(spawn_request(&a, write_body("s3", 3, 1_200))).await;
    assert_overloaded(&o3);
    assert_eq!(a.snapshot().len(), 2);
    assert_eq!(a.held_bytes(), held, "yêu cầu bị từ chối không được tính");

    // Lệnh nhỏ vẫn vào được nếu còn chỗ.
    let h4 = spawn_request(&a, bash_body("s4", "echo nho"));
    next_requested(&mut rx).await;
    assert_eq!(a.snapshot().len(), 3);

    assert_eq!(a.shutdown(), 3);
    for h in [h1, h2, h4] {
        assert!(matches!(join(h).await, PermissionOutcome::Cancelled));
    }
    assert_eq!(a.held_bytes(), 0);
}

#[tokio::test]
async fn single_request_larger_than_cap_is_overloaded() {
    let (a, _rx) = approvals_with(cfg(1_000));
    let o = join(spawn_request(&a, write_body("s1", 1, 5_000))).await;
    assert_overloaded(&o);
    assert!(a.snapshot().is_empty());
    assert_eq!(a.held_bytes(), 0);
}

#[tokio::test]
async fn held_bytes_released_on_decide_allow_and_deny() {
    let (a, mut rx) = approvals_with(cfg(DEFAULT_HELD_INPUT_MAX_BYTES));

    for decision in [Decision::Allow, Decision::Deny] {
        let h = spawn_request(&a, write_body("s1", 1, 10_000));
        let id = next_requested(&mut rx).await;
        assert!(a.held_bytes() >= 10_000);
        // Thẻ Write đã cắt: phải mở bản đầy đủ trước khi allow (T5b).
        assert!(a.tool_input_for_review(&id).is_some());
        assert!(matches!(
            a.decide(&id, decision, DecideOrigin::User, None),
            DecideResult::Resolved
        ));
        assert!(matches!(join(h).await, PermissionOutcome::Decided(_)));
        assert_eq!(a.held_bytes(), 0, "sau {decision:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn held_bytes_released_on_timeout() {
    let (a, mut rx) = approvals_with(cfg(DEFAULT_HELD_INPUT_MAX_BYTES));
    let h = spawn_request(&a, write_body("s1", 1, 10_000));
    next_requested(&mut rx).await;
    assert!(a.held_bytes() >= 10_000);

    tokio::time::advance(ApprovalsConfig::default().timeout + Duration::from_secs(1)).await;

    assert!(matches!(join(h).await, PermissionOutcome::TimedOut));
    assert_eq!(a.held_bytes(), 0);
}

#[tokio::test]
async fn held_bytes_released_on_session_end_and_supersede() {
    let (a, mut rx) = approvals_with(cfg(DEFAULT_HELD_INPUT_MAX_BYTES));

    // Kết thúc phiên.
    let h = spawn_request(&a, write_body("s1", 1, 10_000));
    next_requested(&mut rx).await;
    assert_eq!(
        apply_event(
            &a,
            json!({ "hook_event_name": "SessionEnd", "session_id": "s1" })
        ),
        1
    );
    assert!(matches!(join(h).await, PermissionOutcome::Cancelled));
    assert_eq!(a.held_bytes(), 0, "sau SessionEnd");

    // Người trả lời ở terminal (Q1b): PostToolUse cùng tool và cùng input.
    let body = write_body("s2", 2, 10_000);
    let h = spawn_request(&a, body.clone());
    next_requested(&mut rx).await;
    let mut post = body;
    post["hook_event_name"] = json!("PostToolUse");
    assert_eq!(apply_event(&a, post), 1);
    assert!(matches!(join(h).await, PermissionOutcome::Cancelled));
    assert_eq!(a.held_bytes(), 0, "sau supersede");
}

#[tokio::test]
async fn held_bytes_released_on_client_disconnect() {
    let (a, mut rx) = approvals_with(cfg(DEFAULT_HELD_INPUT_MAX_BYTES));
    let h = spawn_request(&a, write_body("s1", 1, 10_000));
    next_requested(&mut rx).await;
    // Hủy future của handler: guard trong `request` hủy yêu cầu (client đóng kết nối).
    h.abort();
    let _ = h.await;
    assert!(a.snapshot().is_empty());
    assert_eq!(a.held_bytes(), 0);
}

#[tokio::test]
async fn count_and_byte_limits_both_apply() {
    // Giới hạn số lượng đến trước với lệnh nhỏ.
    let (a, mut rx) = approvals_with(ApprovalsConfig {
        max_pending: 2,
        ..cfg(DEFAULT_HELD_INPUT_MAX_BYTES)
    });
    let h1 = spawn_request(&a, bash_body("s1", "echo 1"));
    next_requested(&mut rx).await;
    let h2 = spawn_request(&a, bash_body("s2", "echo 2"));
    next_requested(&mut rx).await;
    assert_overloaded(&join(spawn_request(&a, bash_body("s3", "echo 3"))).await);
    a.shutdown();
    for h in [h1, h2] {
        join(h).await;
    }
    assert_eq!(a.held_bytes(), 0);

    // Giới hạn byte đến trước với nội dung lớn, dù còn chỗ theo số lượng.
    let (a, mut rx) = approvals_with(ApprovalsConfig {
        max_pending: 64,
        ..cfg(50_000)
    });
    let h1 = spawn_request(&a, write_body("s1", 1, 40_000));
    next_requested(&mut rx).await;
    assert_overloaded(&join(spawn_request(&a, write_body("s2", 2, 40_000))).await);
    a.shutdown();
    join(h1).await;
    assert_eq!(a.held_bytes(), 0);
}

/// Kịch bản đo ngân sách T5b: 64 Write ~1 MB cùng lúc từng đẩy hub lên 93 MB. Với trần mặc định 8 MiB, hàng đợi chỉ
/// giữ tối đa 8 MiB nội dung; phần còn lại nhận `{}` ngay.
#[tokio::test]
async fn many_large_writes_stay_under_cap() {
    let (a, _rx) = approvals_with(ApprovalsConfig::default());
    let one_mb = 1024 * 1024 - 512;
    // 4 phiên × 16 (giới hạn mỗi phiên) = 64 yêu cầu.
    let handles: Vec<_> = (0..64)
        .map(|i| spawn_request(&a, write_body(&format!("s{}", i % 4), i, one_mb)))
        .collect();

    // Chờ mọi yêu cầu hoặc bị từ chối hoặc đã vào hàng đợi.
    let deadline = tokio::time::Instant::now() + WAIT;
    while handles.iter().filter(|h| h.is_finished()).count() + a.snapshot().len() < 64 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "hàng đợi không ổn định"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let held = a.held_bytes();
    let queued = a.snapshot().len();
    assert!(held <= DEFAULT_HELD_INPUT_MAX_BYTES, "giữ {held} byte");
    assert!(
        (1..=8).contains(&queued),
        "chỉ giữ được ≤8 yêu cầu ~1 MB, giữ {queued}"
    );

    a.shutdown();
    let mut overloaded = 0;
    for h in handles {
        match join(h).await {
            PermissionOutcome::Overloaded => overloaded += 1,
            PermissionOutcome::Cancelled => {}
            other => panic!("chỉ được {{}}: {other:?}"),
        }
    }
    assert_eq!(overloaded + queued, 64);
    assert_eq!(a.held_bytes(), 0);
}
