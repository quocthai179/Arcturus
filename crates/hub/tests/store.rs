//! Test T6 (ARC-27): hub ghi sự kiện và nhật ký duyệt vào SQLite thật (`hub_core::store`), AutoAllow chỉ trả allow sau
//! khi bản ghi đã commit, tắt hub thì bản ghi `cancelled` vẫn được ghi.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hub::approvals::{
    ApprovalCtx, ApprovalsConfig, Assessment, DecideOrigin, PermissionOutcome, Policy,
    PolicyOutcome,
};
use hub::hub::{approvals_config, Hub, HubTasks};
use hub_core::config::Config;
use hub_core::hook::HookInput;
use hub_core::persona::PersonaBook;
use hub_core::policy::danger::DangerConfig;
use hub_core::protocol::ServerMsg;
use hub_core::schema::Decision;
use hub_core::store::{Store, StoreHandle, StoreReader};
use serde_json::json;
use tokio::sync::broadcast;

const WAIT: Duration = Duration::from_secs(5);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("arcturus-hub-store-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct AutoAllow;

impl Policy for AutoAllow {
    fn decide(&self, _ctx: &ApprovalCtx<'_>, a: &Assessment) -> PolicyOutcome {
        PolicyOutcome::AutoAllow {
            reason: "test".to_owned(),
            risk: a.risk,
        }
    }
}

fn cfg() -> Config {
    let mut cfg = Config::default();
    cfg.hub.min_review_ms = 0;
    cfg
}

fn start(store: StoreHandle, policy: Option<Box<dyn Policy>>) -> (Arc<Hub>, HubTasks) {
    match policy {
        None => Hub::start_full(
            cfg(),
            PersonaBook::default(),
            "laptop".to_owned(),
            DangerConfig::default(),
            Box::new(store),
        ),
        Some(p) => Hub::start_with(
            cfg(),
            PersonaBook::default(),
            "laptop".to_owned(),
            ApprovalsConfig {
                min_review: Duration::ZERO,
                ..approvals_config(&cfg(), DangerConfig::default())
            },
            p,
            Box::new(store),
        ),
    }
}

fn permission(session: &str, cmd: &str) -> HookInput {
    HookInput::new(json!({
        "hook_event_name": "PermissionRequest", "session_id": session, "cwd": "/w/proj",
        "tool_name": "Bash", "tool_input": { "command": cmd, "description": "d" },
    }))
}

async fn next_request_id(rx: &mut broadcast::Receiver<ServerMsg>) -> String {
    loop {
        match tokio::time::timeout(WAIT, rx.recv()).await {
            Ok(Ok(ServerMsg::ApprovalRequested { approval })) => return approval.request_id,
            Ok(Ok(_)) => continue,
            other => panic!("không thấy ApprovalRequested: {other:?}"),
        }
    }
}

async fn flush(h: &StoreHandle) {
    let h = h.clone();
    tokio::task::spawn_blocking(move || h.flush())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn events_and_approvals_are_persisted() {
    let dir = TempDir::new();
    let db = dir.0.join("arcturus.db");
    let store = Store::open(&db, 30).unwrap();
    let (core, _tasks) = start(store.handle(), None);
    let mut rx = core.subscribe();

    core.ingest(&HookInput::new(json!({
        "hook_event_name": "PreToolUse", "session_id": "s1", "cwd": "/w/proj",
        "tool_name": "Read", "tool_input": { "file_path": "/w/proj/a.rs" },
    })));
    let c = core.clone();
    let h = tokio::spawn(async move { c.permission(permission("s1", "rm -rf build")).await });
    let id = next_request_id(&mut rx).await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    core.approvals
        .decide(&id, Decision::Allow, DecideOrigin::User, None);
    assert!(matches!(
        tokio::time::timeout(WAIT, h).await.unwrap().unwrap(),
        PermissionOutcome::Decided(_)
    ));

    flush(&store.handle()).await;
    let r = StoreReader::open(&db).unwrap();
    let events = r.session_events("s1", 10).unwrap();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, vec!["tool.pre", "approval.requested"]);
    assert!(events[1]
        .raw_json
        .as_deref()
        .unwrap()
        .contains("rm -rf build"));
    let row = r.approval(&id).unwrap().unwrap();
    assert_eq!(row.status, "allowed");
    assert_eq!(row.decided_by.as_deref(), Some("user"));
    assert_eq!(row.risk, "critical");
    assert_eq!(row.command_original, "rm -rf build");
    assert_eq!(row.channel.as_deref(), Some("ws"));
    assert!(row.latency_ms.unwrap() >= 30, "{:?}", row.latency_ms);
    // Ring buffer trong RAM giữ cùng các sự kiện.
    assert_eq!(core.recent_events(10).len(), 2);
    store.close();
}

#[tokio::test]
async fn auto_allow_waits_for_real_commit() {
    let dir = TempDir::new();
    let db = dir.0.join("arcturus.db");
    let store = Store::open(&db, 30).unwrap();
    let (core, _tasks) = start(store.handle(), Some(Box::new(AutoAllow)));
    let out = core.permission(permission("s1", "cargo test")).await;
    assert_eq!(out, PermissionOutcome::AutoAllowed);
    // Ack chỉ tới sau commit: kết nối đọc thấy bản ghi ngay, không cần flush.
    let r = StoreReader::open(&db).unwrap();
    let rows = r.recent_approvals(10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "allowed");
    assert_eq!(rows[0].decided_by.as_deref(), Some("policy"));
    assert_eq!(rows[0].channel.as_deref(), Some("policy"));
    assert_eq!(rows[0].risk, "low");
    assert_eq!(rows[0].command_original, "cargo test");

    // Lệnh vùng khóa: không bao giờ có dòng `policy`, kể cả với policy tự duyệt.
    let mut rx = core.subscribe();
    let c = core.clone();
    let h = tokio::spawn(async move { c.permission(permission("s1", "git push -f")).await });
    let id = next_request_id(&mut rx).await;
    core.approvals
        .decide(&id, Decision::Deny, DecideOrigin::User, None);
    tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    flush(&store.handle()).await;
    let row = r.approval(&id).unwrap().unwrap();
    assert_eq!(
        (row.status.as_str(), row.risk.as_str()),
        ("denied", "critical")
    );
    assert_eq!(r.approval_count().unwrap(), 2);
    store.close();
}

#[tokio::test]
async fn auto_allow_falls_back_when_store_closed() {
    let dir = TempDir::new();
    let db = dir.0.join("arcturus.db");
    let store = Store::open(&db, 30).unwrap();
    let handle = store.handle();
    store.close();
    let (core, _tasks) = start(handle, Some(Box::new(AutoAllow)));
    let mut rx = core.subscribe();
    let c = core.clone();
    let h = tokio::spawn(async move { c.permission(permission("s1", "cargo test")).await });
    // Không ghi được kiểm toán: hỏi người, không tự allow (quy tắc bất biến 1).
    let id = next_request_id(&mut rx).await;
    assert!(!h.is_finished());
    core.approvals
        .decide(&id, Decision::Deny, DecideOrigin::User, None);
    assert!(matches!(
        tokio::time::timeout(WAIT, h).await.unwrap().unwrap(),
        PermissionOutcome::Decided(_)
    ));
}

#[tokio::test]
async fn shutdown_records_cancelled_before_store_closes() {
    let dir = TempDir::new();
    let db = dir.0.join("arcturus.db");
    let store = Store::open(&db, 30).unwrap();
    let (core, _tasks) = start(store.handle(), None);
    let mut rx = core.subscribe();
    let c = core.clone();
    let h = tokio::spawn(async move { c.permission(permission("s1", "ls")).await });
    let id = next_request_id(&mut rx).await;
    assert_eq!(core.shutdown(), 1);
    assert_eq!(
        tokio::time::timeout(WAIT, h).await.unwrap().unwrap(),
        PermissionOutcome::Cancelled
    );
    // Như `main.rs`: đóng store sau khi hub đã hủy yêu cầu.
    tokio::task::spawn_blocking(move || store.close())
        .await
        .unwrap();
    let r = StoreReader::open(&db).unwrap();
    let row = r.approval(&id).unwrap().unwrap();
    assert_eq!(
        (row.status.as_str(), row.decided_by.as_deref()),
        ("cancelled", Some("cancelled"))
    );
    assert_eq!(row.risk, "low");
}

#[tokio::test]
async fn handoff_is_recorded_with_preview() {
    let dir = TempDir::new();
    let db = dir.0.join("arcturus.db");
    let store = Store::open(&db, 30).unwrap();
    let (core, _tasks) = start(store.handle(), None);
    let long = format!("echo {} && rm -rf /", "x".repeat(5000));
    let out = core.permission(permission("s1", &long)).await;
    let PermissionOutcome::HandedOff { handoff_id } = out else {
        panic!("phải trả về terminal: {out:?}")
    };
    flush(&store.handle()).await;
    let r = StoreReader::open(&db).unwrap();
    let row = r.approval(&handoff_id).unwrap().unwrap();
    assert_eq!(row.status, "handed_off");
    assert_eq!(row.decided_by, None);
    assert_eq!(row.risk, "critical");
    assert!(row.command_original.starts_with("echo xxx"));
    assert!(row.command_original.ends_with("&& rm -rf /"));
    assert!(row.command_original.len() < long.len());
    store.close();
}
