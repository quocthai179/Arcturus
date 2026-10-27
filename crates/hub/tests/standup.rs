//! Test T19 (ARC-40): `GET /standup` ghép sự kiện đã lưu (SQLite thật), persona, hàng đợi duyệt và `git log` thật.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use hub::hub::{Hub, HubTasks};
use hub::standup::{standup_for, StandupError, SystemGit};
use hub::ws::{self, WsConfig};
use hub::{hooks, server};
use hub_core::config::Config;
use hub_core::hook::HookInput;
use hub_core::persona::PersonaSpec;
use hub_core::policy::danger::DangerConfig;
use hub_core::protocol::Palette;
use hub_core::standup::GitLog;
use hub_core::store::{Store, StoreHandle};
use serde_json::{json, Value};
use time::OffsetDateTime;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const HOOK: &str = "hook-token-0123456789abcdef-arcturus-t19";
const UI: &str = "ui-token-fedcba9876543210-arcturus-t19-xyz";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("arcturus-standup-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn have_git() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

struct T {
    addr: SocketAddr,
    hub: Arc<Hub>,
    db: Option<Store>,
    handle: StoreHandle,
    http: reqwest::Client,
    stop: Option<oneshot::Sender<()>>,
    server: JoinHandle<()>,
    _tasks: HubTasks,
    _dir: TempDir,
}

impl T {
    /// `cwd_match`: glob của persona "Khoa" (cwd của sự kiện test).
    async fn start(cwd_match: &str, with_db_path: bool) -> T {
        let dir = TempDir::new();
        let db_path = dir.0.join("arcturus.db");
        let db = Store::open(&db_path, 30).unwrap();
        let handle = db.handle();
        let mut cfg = Config::default();
        cfg.hub.min_review_ms = 0;
        cfg.personas.push(PersonaSpec {
            match_cwd: cwd_match.to_owned(),
            name: "Khoa".to_owned(),
            role: "backend".to_owned(),
            palette: Palette {
                hair: "#1F1F24".to_owned(),
                outfit: "#4C8DF6".to_owned(),
                acc: "#9AA3B5".to_owned(),
            },
        });
        let personas = cfg.persona_book().unwrap();
        let (hub, tasks) = Hub::start_full(
            cfg,
            personas,
            "laptop".to_owned(),
            DangerConfig::default(),
            Box::new(handle.clone()),
        );
        if with_db_path {
            hub.set_db_path(db_path);
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (ui, ws_handle) =
            ws::router(hub.clone(), UI.to_owned(), addr.port(), WsConfig::default());
        let app = hooks::router(hub.clone(), HOOK.to_owned()).merge(ui);
        let (stop, stop_rx) = oneshot::channel::<()>();
        let (c, w) = (hub.clone(), ws_handle.clone());
        let server = tokio::spawn(async move {
            server::serve_until(listener, app, async move {
                let _ = stop_rx.await;
                c.shutdown();
                w.shutdown();
            })
            .await
            .unwrap();
        });
        T {
            addr,
            hub,
            db: Some(db),
            handle,
            http: reqwest::Client::new(),
            stop: Some(stop),
            server,
            _tasks: tasks,
            _dir: dir,
        }
    }

    fn ingest(&self, name: &str, session: &str, cwd: &str, tool: Option<(&str, &str)>) {
        let mut v = json!({"hook_event_name": name, "session_id": session, "cwd": cwd});
        if let Some((tool_name, cmd)) = tool {
            v["tool_name"] = json!(tool_name);
            v["tool_input"] = json!({ "command": cmd });
        }
        self.hub.ingest(&HookInput::new(v));
    }

    async fn get(&self, path: &str, token: Option<&str>) -> reqwest::Response {
        let mut r = self.http.get(format!("http://{}{path}", self.addr));
        if let Some(t) = token {
            r = r.bearer_auth(t);
        }
        r.send().await.unwrap()
    }

    async fn standup(&self, query: &str) -> Value {
        self.handle.flush().unwrap();
        let r = self.get(&format!("/standup{query}"), Some(UI)).await;
        assert_eq!(r.status(), 200);
        r.json().await.unwrap()
    }

    async fn finish(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        let _ = tokio::time::timeout(Duration::from_secs(5), self.server).await;
        if let Some(db) = self.db.take() {
            db.close();
        }
    }
}

#[tokio::test]
async fn standup_counts_from_stored_events_by_persona() {
    let t = T::start("/w/proj", true).await;
    t.ingest("UserPromptSubmit", "s1", "/w/proj", None);
    t.ingest(
        "PostToolUse",
        "s1",
        "/w/proj",
        Some(("Bash", "cargo test -p hub")),
    );
    t.ingest("PostToolUse", "s1", "/w/proj", Some(("Bash", "ls")));
    for _ in 0..3 {
        t.ingest(
            "PostToolUseFailure",
            "s1",
            "/w/proj",
            Some(("Bash", "cargo test -p hub")),
        );
    }
    t.ingest("Stop", "s1", "/w/proj", None);
    t.ingest("Stop", "s1", "/w/proj", None);
    // Phiên khác không khớp persona nào: tên mặc định theo agent đang chạy, không lẫn vào Khoa.
    t.ingest("Stop", "s2", "/elsewhere", None);

    let s = t.standup("").await;
    let personas = s["personas"].as_array().unwrap();
    let khoa = personas
        .iter()
        .find(|p| p["name"] == "Khoa")
        .expect("có Khoa");
    assert_eq!(khoa["turns_done"], 2);
    assert_eq!(khoa["tests_passed"], 1);
    assert_eq!(khoa["tests_failed"], 3);
    assert_eq!(khoa["repeated_errors"][0]["count"], 3);
    assert_eq!(khoa["repeated_errors"][0]["what"], "cargo test -p hub");
    assert!(
        personas.iter().any(|p| p["name"] != "Khoa"),
        "phiên s2 là persona khác"
    );
    let text = s["text"].as_str().unwrap();
    assert!(text.starts_with("Standup "), "{text}");
    assert!(text.contains("Khoa: xong 2 lượt"), "{text}");
    t.finish().await;
}

#[tokio::test]
async fn standup_lists_pending_approvals_by_persona() {
    let t = T::start("/w/proj", true).await;
    t.ingest("Stop", "s1", "/w/proj", None);
    let hub = t.hub.clone();
    // Yêu cầu duyệt đang giữ: chạy ở task riêng, vì `request` chỉ trả khi có quyết định.
    let pending = tokio::spawn(async move {
        let input = HookInput::new(json!({
            "hook_event_name": "PermissionRequest", "session_id": "s1", "cwd": "/w/proj",
            "tool_name": "Bash", "tool_input": {"command": "npm test"},
        }));
        let ev = hub_core::hook::to_event(&input, "laptop", OffsetDateTime::now_utc());
        hub.approvals.request(input, &ev, "Khoa").await
    });
    for _ in 0..100 {
        if !t.hub.approvals.views().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let s = t.standup("").await;
    let khoa = s["personas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Khoa")
        .unwrap()
        .clone();
    assert_eq!(khoa["pending"], 1);
    t.hub.approvals.shutdown();
    let _ = pending.await;
    t.finish().await;
}

#[tokio::test]
async fn standup_requires_ui_token_and_rejects_bad_hours() {
    let t = T::start("/w/proj", true).await;
    assert_eq!(t.get("/standup", None).await.status(), 401);
    assert_eq!(
        t.get("/standup", Some(HOOK)).await.status(),
        401,
        "token hook không được chấp nhận"
    );
    assert_eq!(t.get("/standup?hours=0", Some(UI)).await.status(), 400);
    assert_eq!(t.get("/standup?hours=169", Some(UI)).await.status(), 400);
    assert_eq!(t.get("/standup?hours=abc", Some(UI)).await.status(), 400);
    let ok = t.get("/standup?hours=168", Some(UI)).await;
    assert_eq!(ok.status(), 200);
    assert_eq!(ok.headers()["cache-control"], "no-store");
    assert_eq!(ok.headers()["x-content-type-options"], "nosniff");
    t.finish().await;
}

#[tokio::test]
async fn standup_without_database_is_503_and_standup_for_reports_it() {
    let t = T::start("/w/proj", false).await;
    assert_eq!(t.get("/standup", Some(UI)).await.status(), 503);
    let err = standup_for(&t.hub, 24, OffsetDateTime::now_utc(), &SystemGit).unwrap_err();
    assert_eq!(err, StandupError::NoDatabase);
    t.finish().await;
}

#[tokio::test]
async fn standup_rejects_foreign_host_header() {
    let t = T::start("/w/proj", true).await;
    let r = t
        .http
        .get(format!("http://{}/standup", t.addr))
        .header("host", "evil.example:80")
        .bearer_auth(UI)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403, "chặn DNS rebinding như các route UI khác");
    t.finish().await;
}

#[test]
fn system_git_reads_commits_of_a_real_repo_and_survives_bad_dirs() {
    if !have_git() {
        eprintln!("bỏ qua: máy không có git");
        return;
    }
    let dir = TempDir::new();
    assert!(git(&dir.0, &["init", "-q"]));
    std::fs::write(dir.0.join("a.txt"), "x").unwrap();
    assert!(git(&dir.0, &["add", "."]));
    assert!(git(&dir.0, &["commit", "-q", "-m", "feat: lệnh đầu tiên"]));
    let since = OffsetDateTime::now_utc() - time::Duration::hours(1);
    let commits = SystemGit.commits(dir.0.to_str().unwrap(), since);
    assert_eq!(commits.len(), 1, "{commits:?}");
    assert!(commits[0].ends_with("feat: lệnh đầu tiên"), "{commits:?}");
    // Commit trước `since` không được tính.
    let future = OffsetDateTime::now_utc() + time::Duration::hours(1);
    assert!(SystemGit
        .commits(dir.0.to_str().unwrap(), future)
        .is_empty());
    // Thư mục không phải repo, không tồn tại, hoặc chuỗi có NUL: rỗng, không panic.
    let plain = TempDir::new();
    assert!(SystemGit
        .commits(plain.0.to_str().unwrap(), since)
        .is_empty());
    assert!(SystemGit.commits("/khong/co/thu/muc/nay", since).is_empty());
    assert!(SystemGit.commits("a\0b", since).is_empty());
}
