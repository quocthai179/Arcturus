//! Test T17 (ARC-38): hub thật + Claude Code thật (`claude -p`, headless) cho bốn kịch bản duyệt.
//!
//! Chạy tay, vì cần Claude Code cài sẵn và đã đăng nhập, mất vài chục giây và tốn một ít lượt dùng mỗi kịch bản:
//!
//! ```text
//! cargo test -p hub --test e2e_claude -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Biến tùy chọn: `ARCTURUS_E2E_MODEL` (ví dụ `haiku`, rẻ hơn), `ARCTURUS_E2E_CLAUDE` (đường dẫn `claude`).
//!
//! Mỗi kịch bản tạo một dự án tạm có `npm test` vô hại (ghi file `marker.txt`), nạp hook về hub trong tiến trình test bằng
//! `--settings` (không đụng `~/.claude`), chạy `claude -p "chạy npm test" --permission-mode default` và kiểm file đánh dấu:
//!
//! | kịch bản | hub | file đánh dấu |
//! |---|---|---|
//! | allow | người duyệt giả trả `allow` | **có** |
//! | deny | người duyệt giả trả `deny` | không |
//! | timeout | không ai trả lời, hub hết giờ (5 giây) và trả `{}` | không |
//! | hub tắt | không có gì lắng nghe | không |
//!
//! Ba kịch bản cuối kiểm quy tắc bất biến 1: hub lỗi, chết hay hết giờ thì **không bao giờ** có allow; Claude Code rơi về hộp
//! thoại terminal, mà ở `-p` không có ai hỏi nên lệnh bị từ chối (`docs/HOOK_FINDINGS.md` Q2).

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hub::approvals::{ApprovalsConfig, NoStore};
use hub::hub::{approvals_config, Hub, HubTasks};
use hub::{hooks, server};
use hub_core::config::Config;
use hub_core::persona::PersonaBook;
use hub_core::policy::danger::DangerConfig;
use hub_core::policy::RulesPolicy;
use hub_core::protocol::{DecideOrigin, ServerMsg};
use hub_core::schema::{DecidedBy, Decision};
use serde_json::json;
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;

const TOKEN: &str = "e2e-hook-token-0123456789abcdef-arcturus-t17";
const PROMPT: &str = "Run the shell command `npm test` exactly once with the Bash tool, then tell me whether it ran. \
                      Do not run anything else and do not try any other way to run it.";
/// Giới hạn thời gian cho một lần chạy `claude -p`.
const CLAUDE_TIMEOUT: Duration = Duration::from_secs(180);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("arcturus-e2e-{tag}-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn claude_bin() -> String {
    std::env::var("ARCTURUS_E2E_CLAUDE").unwrap_or_else(|_| "claude".to_owned())
}

/// `false` (và in lý do) nếu máy không có Claude Code: kịch bản tự bỏ qua thay vì làm hỏng bộ test.
fn claude_available() -> bool {
    let ok = std::process::Command::new(claude_bin())
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("bỏ qua: không chạy được `{} --version`", claude_bin());
    }
    ok
}

/// Dự án tạm: `npm test` ghi `marker.txt`.
fn project() -> TempDir {
    let dir = TempDir::new("proj");
    std::fs::write(
        dir.0.join("package.json"),
        r#"{ "name": "arcturus-e2e", "version": "1.0.0", "private": true, "scripts": { "test": "node mark.js" } }"#,
    )
    .unwrap();
    std::fs::write(
        dir.0.join("mark.js"),
        "require('fs').writeFileSync('marker.txt', 'đã chạy npm test');\nconsole.log('ok');\n",
    )
    .unwrap();
    dir
}

fn marker(dir: &TempDir) -> bool {
    dir.0.join("marker.txt").exists()
}

/// `settings.json` chỉ có hook `PermissionRequest` trỏ về `addr` (token qua biến môi trường, như bản dogfood).
fn settings_file(dir: &Path, addr: SocketAddr) -> PathBuf {
    let path = dir.join("arcturus-e2e-settings.json");
    let settings = json!({
        "hooks": {
            "PermissionRequest": [{
                "hooks": [{
                    "type": "http",
                    "url": format!("http://{addr}/hooks/permission"),
                    "timeout": 120,
                    "headers": { "Authorization": "Bearer $ARCTURUS_TOKEN" },
                    "allowedEnvVars": ["ARCTURUS_TOKEN"]
                }]
            }]
        }
    });
    std::fs::write(&path, settings.to_string()).unwrap();
    path
}

struct Run {
    elapsed: Duration,
    stdout: String,
    stderr: String,
    status: Option<i32>,
}

/// Chạy `claude -p` trong `proj` với hook về `addr`. Không bao giờ truyền token UI; chỉ token hook, qua biến môi trường.
async fn run_claude(proj: &TempDir, settings: &Path) -> Run {
    let mut cmd = tokio::process::Command::new(claude_bin());
    cmd.current_dir(&proj.0)
        .args(["-p", PROMPT, "--permission-mode", "default", "--settings"])
        .arg(settings)
        .env("ARCTURUS_TOKEN", TOKEN)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Ok(model) = std::env::var("ARCTURUS_E2E_MODEL") {
        cmd.args(["--model", &model]);
    }
    // Phiên Claude Code cha (nếu có) không được kéo phiên con vào cùng ngữ cảnh.
    for (k, _) in std::env::vars() {
        if k == "CLAUDECODE" || k.starts_with("CLAUDE_CODE_") {
            cmd.env_remove(k);
        }
    }
    let started = Instant::now();
    let out = tokio::time::timeout(CLAUDE_TIMEOUT, cmd.output())
        .await
        .expect("claude -p phải xong trong thời hạn (không treo khi hub lỗi)")
        .expect("chạy được claude");
    Run {
        elapsed: started.elapsed(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        status: out.status.code(),
    }
}

impl Run {
    fn describe(&self) -> String {
        format!(
            "claude thoát {:?} sau {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.status, self.elapsed, self.stdout, self.stderr
        )
    }
}

struct Hubd {
    addr: SocketAddr,
    hub: Arc<Hub>,
    stop: Option<oneshot::Sender<()>>,
    server: JoinHandle<()>,
    _tasks: HubTasks,
}

impl Hubd {
    async fn start(timeout: Duration) -> Hubd {
        let cfg = ApprovalsConfig {
            timeout,
            min_review: Duration::ZERO,
            ..approvals_config(&Config::default(), DangerConfig::default())
        };
        let (hub, tasks) = Hub::start_with(
            Config::default(),
            PersonaBook::default(),
            "laptop".to_owned(),
            cfg,
            Box::new(RulesPolicy::new()),
            Box::new(NoStore),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = hooks::router(hub.clone(), TOKEN.to_owned());
        let (stop, stop_rx) = oneshot::channel::<()>();
        let h = hub.clone();
        let server = tokio::spawn(async move {
            server::serve_until(listener, app, async move {
                let _ = stop_rx.await;
                h.shutdown();
            })
            .await
            .unwrap();
        });
        Hubd {
            addr,
            hub,
            stop: Some(stop),
            server,
            _tasks: tasks,
        }
    }

    async fn stop(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        let _ = tokio::time::timeout(Duration::from_secs(10), self.server).await;
    }

    /// Người duyệt giả: trả `decision` cho mọi yêu cầu sau 300 ms và ghi lại mọi thông điệp hub phát.
    fn responder(&self, decision: Option<Decision>) -> Responder {
        let hub = self.hub.clone();
        let mut rx: broadcast::Receiver<ServerMsg> = hub.subscribe();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let task = tokio::spawn(async move {
            // Kênh broadcast không đóng khi `Hub` còn sống, nên vòng này chạy tới khi bị `abort` (xem `Responder::finish`).
            while let Ok(msg) = rx.recv().await {
                if let (ServerMsg::ApprovalRequested { approval }, Some(d)) = (&msg, decision) {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    hub.approvals
                        .decide(&approval.request_id, d, DecideOrigin::User, None);
                }
                log.lock().unwrap().push(msg);
            }
        });
        Responder { task, seen }
    }
}

struct Responder {
    task: JoinHandle<()>,
    seen: Arc<Mutex<Vec<ServerMsg>>>,
}

impl Responder {
    /// Dừng người duyệt giả và trả các thông điệp đã thấy.
    async fn finish(self) -> Vec<ServerMsg> {
        self.task.abort();
        let _ = self.task.await;
        std::mem::take(&mut *self.seen.lock().unwrap())
    }
}

fn requested(seen: &[ServerMsg]) -> Vec<&hub_core::protocol::ApprovalView> {
    seen.iter()
        .filter_map(|m| match m {
            ServerMsg::ApprovalRequested { approval } => Some(approval),
            _ => None,
        })
        .collect()
}

/// Bắt đầu một kịch bản có hub: bỏ qua nếu không có Claude Code.
macro_rules! scenario {
    ($timeout:expr) => {{
        if !claude_available() {
            return;
        }
        let proj = project();
        let hubd = Hubd::start($timeout).await;
        let settings = settings_file(&proj.0, hubd.addr);
        (proj, hubd, settings)
    }};
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "cần Claude Code thật; chạy tay, xem đầu tệp"]
async fn e2e_allow_runs_the_command() {
    let (proj, hubd, settings) = scenario!(Duration::from_secs(60));
    let responder = hubd.responder(Some(Decision::Allow));
    let run = run_claude(&proj, &settings).await;
    hubd.stop().await;
    let seen = responder.finish().await;
    let reqs = requested(&seen);
    assert!(
        reqs.iter()
            .any(|a| a.tool == "Bash" && a.command_normalized.contains("npm test")),
        "hub phải thấy PermissionRequest của `npm test`: {reqs:?}\n{}",
        run.describe()
    );
    assert!(
        marker(&proj),
        "đã duyệt nên lệnh phải chạy\n{}",
        run.describe()
    );
    assert!(
        seen.iter().any(|m| matches!(
            m,
            ServerMsg::ApprovalResolved {
                decision: Some(Decision::Allow),
                decided_by: DecidedBy::User,
                ..
            }
        )),
        "hub phải ghi quyết định allow của người: {seen:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "cần Claude Code thật; chạy tay, xem đầu tệp"]
async fn e2e_deny_does_not_run_the_command() {
    let (proj, hubd, settings) = scenario!(Duration::from_secs(60));
    let responder = hubd.responder(Some(Decision::Deny));
    let run = run_claude(&proj, &settings).await;
    hubd.stop().await;
    let seen = responder.finish().await;
    assert!(
        !requested(&seen).is_empty(),
        "hub phải thấy yêu cầu\n{}",
        run.describe()
    );
    assert!(
        !marker(&proj),
        "đã từ chối nên lệnh không được chạy\n{}",
        run.describe()
    );
    assert!(seen.iter().any(|m| matches!(
        m,
        ServerMsg::ApprovalResolved {
            decision: Some(Decision::Deny),
            ..
        }
    )));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "cần Claude Code thật; chạy tay, xem đầu tệp"]
async fn e2e_timeout_returns_empty_object_and_never_allows() {
    let (proj, hubd, settings) = scenario!(Duration::from_secs(5));
    let responder = hubd.responder(None);
    let run = run_claude(&proj, &settings).await;
    hubd.stop().await;
    let seen = responder.finish().await;
    let reqs = requested(&seen);
    assert!(
        !reqs.is_empty(),
        "hub phải thấy yêu cầu\n{}",
        run.describe()
    );
    // Hết giờ: hub tự trả `{}` (không quyết định), Claude Code rơi về hộp thoại terminal, ở `-p` thì lệnh bị từ chối.
    assert!(
        seen.iter().any(|m| matches!(
            m,
            ServerMsg::ApprovalResolved {
                decision: None,
                decided_by: DecidedBy::Timeout,
                ..
            }
        )),
        "yêu cầu phải hết giờ với decision = null: {seen:?}\n{}",
        run.describe()
    );
    assert!(
        !marker(&proj),
        "hết giờ không bao giờ được allow\n{}",
        run.describe()
    );
    assert!(run.elapsed >= Duration::from_secs(5), "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "cần Claude Code thật; chạy tay, xem đầu tệp"]
async fn e2e_hub_down_never_allows_and_claude_does_not_hang() {
    if !claude_available() {
        return;
    }
    let proj = project();
    // Cổng không có gì lắng nghe: bind rồi đóng ngay.
    let addr = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap()
    };
    let settings = settings_file(&proj.0, addr);
    let run = run_claude(&proj, &settings).await;
    assert!(
        !marker(&proj),
        "hub tắt: lỗi kết nối là lỗi không chặn, không có allow\n{}",
        run.describe()
    );
    assert!(run.elapsed < CLAUDE_TIMEOUT, "{}", run.describe());
}

/// Máy chủ giả trả `status` cho mọi yêu cầu (non-2xx từ hub). Trả cổng; máy chủ chạy tới hết test.
async fn stub_status(status: u16) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let code = axum::http::StatusCode::from_u16(status).unwrap();
    let app = axum::Router::new().fallback(move || async move { (code, "stub") });
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

/// Non-2xx (CLAUDE.md "CHƯA kiểm chứng", T17): hub lỗi (500) hay sai token (401) là lỗi không chặn; Claude Code không
/// treo và không bao giờ coi đó là allow.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "cần Claude Code thật; chạy tay, xem đầu tệp"]
async fn e2e_non_2xx_never_allows_and_claude_does_not_hang() {
    if !claude_available() {
        return;
    }
    for status in [500u16, 401] {
        let proj = project();
        let addr = stub_status(status).await;
        let settings = settings_file(&proj.0, addr);
        let run = run_claude(&proj, &settings).await;
        assert!(
            !marker(&proj),
            "hub trả {status}: không có allow\n{}",
            run.describe()
        );
        assert!(run.elapsed < CLAUDE_TIMEOUT, "{}", run.describe());
    }
}
