//! Test T15: replay chạy với hub thật (cổng ngẫu nhiên, không store) và đóng vai người duyệt qua WebSocket.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use hub::approvals::{AlwaysAsk, ApprovalsConfig, NoStore};
use hub::hub::{Hub, HubTasks};
use hub::ws::{self, WsConfig};
use hub::{hooks, server};
use hub_core::config::Config;
use hub_core::persona::PersonaBook;
use replay::send::{run, Kind, Options, Outcome, PermissionMode};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const HOOK: &str = "hook-token-0123456789abcdef-arcturus-t15";
const UI: &str = "ui-token-fedcba9876543210-arcturus-t15-xyz";

struct Server {
    addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
    _tasks: HubTasks,
}

impl Server {
    async fn start() -> Server {
        let cfg = ApprovalsConfig {
            min_review: Duration::ZERO,
            ..ApprovalsConfig::default()
        };
        let (core, tasks) = Hub::start_with(
            Config::default(),
            PersonaBook::default(),
            "laptop".to_owned(),
            cfg,
            Box::new(AlwaysAsk),
            Box::new(NoStore),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (ui, ws_handle) = ws::router(
            core.clone(),
            UI.to_owned(),
            addr.port(),
            WsConfig::default(),
        );
        let app = hooks::router(core.clone(), HOOK.to_owned()).merge(ui);
        let (stop, stop_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            server::serve_until(listener, app, async move {
                let _ = stop_rx.await;
                core.shutdown();
                ws_handle.shutdown();
            })
            .await
            .unwrap();
        });
        Server {
            addr,
            stop: Some(stop),
            task,
            _tasks: tasks,
        }
    }

    async fn stop(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        let _ = tokio::time::timeout(Duration::from_secs(5), self.task).await;
    }

    fn opts(&self, mode: PermissionMode) -> Options {
        Options {
            dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/hooks"),
            url: format!("http://{}", self.addr),
            sessions: 3,
            rate: 100.0,
            duration: Duration::from_millis(600),
            permission_mode: mode,
            allow_delay: Duration::from_millis(30),
            hook_token: HOOK.to_owned(),
            ui_token: Some(UI.to_owned()),
        }
    }
}

#[tokio::test]
async fn skip_mode_sends_only_events_and_all_are_accepted() {
    let s = Server::start().await;
    let report = run(s.opts(PermissionMode::Skip)).await.unwrap();
    assert_eq!(report.samples.len(), 60);
    assert!(report.samples.iter().all(|x| x.kind == Kind::Event));
    assert_eq!(report.count(Kind::Event, Outcome::Accepted), 60);
    assert!(report.check(None).is_ok());
    assert!(report.summary().contains("/hooks/event: 60 yêu cầu, 0 lỗi"));
    s.stop().await;
}

#[tokio::test]
async fn allow_mode_acts_as_the_user_and_permission_requests_get_allowed() {
    let s = Server::start().await;
    let report = run(s.opts(PermissionMode::Allow)).await.unwrap();
    let perm = report
        .samples
        .iter()
        .filter(|x| x.kind == Kind::Permission)
        .count();
    assert!(perm >= 1, "kế hoạch phải có PermissionRequest");
    assert_eq!(
        report.count(Kind::Permission, Outcome::Allow),
        perm,
        "mọi yêu cầu giữ phải được người duyệt giả trả lời allow: {report:?}"
    );
    assert_eq!(report.errors(Kind::Event), 0);
    s.stop().await;
}

#[tokio::test]
async fn wrong_hook_token_is_reported_as_errors_and_check_fails() {
    let s = Server::start().await;
    let mut opts = s.opts(PermissionMode::Skip);
    opts.hook_token = "sai-token".to_owned();
    opts.duration = Duration::from_millis(200);
    let report = run(opts).await.unwrap();
    assert!(report.errors(Kind::Event) > 0);
    assert_eq!(
        report.count(Kind::Event, Outcome::HttpError(401)),
        report.samples.len()
    );
    assert!(report.check(None).is_err());
    s.stop().await;
}

#[tokio::test]
async fn allow_mode_without_ui_token_is_rejected_before_sending() {
    let s = Server::start().await;
    let mut opts = s.opts(PermissionMode::Allow);
    opts.ui_token = None;
    let err = run(opts).await.unwrap_err().to_string();
    assert!(err.contains("token UI"), "{err}");
    s.stop().await;
}

#[tokio::test]
async fn allow_mode_with_wrong_ui_token_fails_to_connect() {
    let s = Server::start().await;
    let mut opts = s.opts(PermissionMode::Allow);
    opts.ui_token = Some("sai-token-ui".to_owned());
    assert!(run(opts).await.is_err());
    s.stop().await;
}

#[tokio::test]
async fn invalid_options_are_rejected() {
    let s = Server::start().await;
    let mut a = s.opts(PermissionMode::Skip);
    a.sessions = 0;
    assert!(run(a).await.is_err());
    let mut b = s.opts(PermissionMode::Skip);
    b.rate = 0.0;
    assert!(run(b).await.is_err());
    let mut c = s.opts(PermissionMode::Skip);
    c.dir = PathBuf::from("khong-co-thu-muc-nay");
    assert!(run(c).await.is_err());
    s.stop().await;
}
