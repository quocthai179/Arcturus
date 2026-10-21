//! Test tích hợp cho chế độ ghi băng (`hub --record`, T1): hub thật trên cổng ngẫu nhiên, thư mục ghi tạm.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hub::record::{self, RecordConfig};
use hub::{auth, server};

const TOKEN: &str = "test-token-0123456789abcdef";

struct TestHub {
    addr: SocketAddr,
    dir: PathBuf,
    client: reqwest::Client,
}

impl TestHub {
    async fn start(delay: Duration) -> Self {
        let dir = std::env::temp_dir().join(format!("arcturus-record-{}", ulid::Ulid::generate()));
        Self::start_with_dir(dir, delay).await
    }

    async fn start_with_dir(dir: PathBuf, delay: Duration) -> Self {
        let app = record::router(
            RecordConfig {
                dir: dir.clone(),
                delay,
            },
            TOKEN.to_owned(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        TestHub {
            addr,
            dir,
            client: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    async fn post(
        &self,
        path: &str,
        token: Option<&str>,
        body: impl Into<reqwest::Body>,
    ) -> reqwest::Response {
        let mut req = self
            .client
            .post(self.url(path))
            .header("content-type", "application/json")
            .body(body);
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        req.send().await.unwrap()
    }
}

/// Mọi file `.json` dưới `dir` (đệ quy), kèm đường dẫn tương đối.
fn recorded_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.strip_prefix(dir).unwrap().to_path_buf());
            }
        }
    }
    out.sort();
    out
}

fn first_component(p: &Path) -> String {
    p.components()
        .next()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
async fn record_writes_raw_body_by_event_name() {
    let hub = TestHub::start(Duration::ZERO).await;
    let body = r#"{"hook_event_name":"PreToolUse","session_id":"s1","tool_name":"Bash","tool_input":{"command":"echo hi"}}"#;

    let res = hub.post("/hooks/event", Some(TOKEN), body).await;

    assert_eq!(res.status(), 204);
    let files = recorded_files(&hub.dir);
    assert_eq!(files.len(), 1);
    assert_eq!(first_component(&files[0]), "PreToolUse");
    assert_eq!(files[0].extension().unwrap(), "json");
    let saved = std::fs::read(hub.dir.join(&files[0])).unwrap();
    assert_eq!(saved, body.as_bytes(), "phải lưu đúng byte thô nhận được");
}

#[tokio::test]
async fn permission_record_returns_empty_object_never_allow() {
    let hub = TestHub::start(Duration::ZERO).await;
    let body = r#"{"hook_event_name":"PermissionRequest","session_id":"s1","tool_name":"Bash"}"#;

    let res = hub.post("/hooks/permission", Some(TOKEN), body).await;

    assert_eq!(res.status(), 200);
    let ct = res.headers()["content-type"].to_str().unwrap().to_owned();
    assert!(ct.starts_with("application/json"), "content-type: {ct}");
    assert_eq!(res.text().await.unwrap(), "{}");
    let files = recorded_files(&hub.dir);
    assert_eq!(files.len(), 1);
    assert_eq!(first_component(&files[0]), "PermissionRequest");
}

#[tokio::test]
async fn wrong_token_returns_401() {
    let hub = TestHub::start(Duration::ZERO).await;
    let body = r#"{"hook_event_name":"PreToolUse"}"#;

    for path in ["/hooks/event", "/hooks/permission"] {
        for token in [Some("wrong-token-0123456789abcdef"), Some(""), None] {
            let res = hub.post(path, token, body).await;
            assert_eq!(res.status(), 401, "{path} với token {token:?}");
            assert!(!res.text().await.unwrap().contains("allow"));
        }
    }
    // Sai scheme: không phải Bearer.
    let res = hub
        .client
        .post(hub.url("/hooks/permission"))
        .header("authorization", format!("Basic {TOKEN}"))
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    assert!(
        recorded_files(&hub.dir).is_empty(),
        "yêu cầu bị từ chối không được ghi"
    );
}

#[tokio::test]
async fn missing_token_refuses_start() {
    assert!(auth::token_from_env(None).is_err());
    assert!(auth::token_from_env(Some(String::new())).is_err());
    assert!(auth::token_from_env(Some("   ".to_owned())).is_err());
    assert!(
        auth::token_from_env(Some("ngan".to_owned())).is_err(),
        "token quá ngắn"
    );
    assert_eq!(auth::token_from_env(Some(TOKEN.to_owned())).unwrap(), TOKEN);
}

#[tokio::test]
async fn non_loopback_bind_refused() {
    for bad in [
        "0.0.0.0:7878",
        "192.168.1.5:7878",
        "[::]:7878",
        "10.0.0.1:1",
    ] {
        let addr: SocketAddr = bad.parse().unwrap();
        assert!(
            server::validate_bind(addr).is_err(),
            "{bad} phải bị từ chối"
        );
    }
    for good in ["127.0.0.1:7878", "[::1]:7878", "127.0.0.1:0"] {
        let addr: SocketAddr = good.parse().unwrap();
        assert_eq!(server::validate_bind(addr).unwrap(), addr);
    }
}

#[tokio::test]
async fn malformed_json_returns_400() {
    let hub = TestHub::start(Duration::ZERO).await;

    for path in ["/hooks/event", "/hooks/permission"] {
        let res = hub.post(path, Some(TOKEN), "{không phải json").await;
        assert_eq!(res.status(), 400, "{path}");
        assert!(!res.text().await.unwrap().contains("allow"));
    }
    assert!(recorded_files(&hub.dir).is_empty());
}

#[tokio::test]
async fn body_over_1mb_rejected() {
    let hub = TestHub::start(Duration::ZERO).await;
    let filler = "x".repeat(record::MAX_BODY_BYTES);
    let body = format!(r#"{{"hook_event_name":"PreToolUse","pad":"{filler}"}}"#);

    let res = hub.post("/hooks/permission", Some(TOKEN), body).await;

    assert_eq!(res.status(), 413);
    assert!(recorded_files(&hub.dir).is_empty());
}

#[tokio::test]
async fn event_name_path_traversal_goes_to_unknown() {
    let hub = TestHub::start(Duration::ZERO).await;
    let hostile = [
        r#"{"hook_event_name":"../../evil"}"#,
        r#"{"hook_event_name":"..\\..\\evil"}"#,
        r#"{"hook_event_name":"C:\\Windows"}"#,
        r#"{"hook_event_name":"Pre/Tool"}"#,
        r#"{"hook_event_name":"CON"}"#,
        r#"{"hook_event_name":"nul"}"#,
        r#"{"hook_event_name":""}"#,
        r#"{"hook_event_name":42}"#,
        r#"{"session_id":"không có tên sự kiện"}"#,
        r#"["mảng", "không phải object"]"#,
    ];

    for body in hostile {
        let res = hub.post("/hooks/event", Some(TOKEN), body).await;
        assert_eq!(res.status(), 204, "{body}");
    }

    let files = recorded_files(&hub.dir);
    assert_eq!(files.len(), hostile.len());
    for f in &files {
        assert_eq!(first_component(f), "Unknown", "{f:?}");
        assert_eq!(f.components().count(), 2, "chỉ một cấp thư mục: {f:?}");
    }
    let parent = hub.dir.parent().unwrap();
    assert!(!parent.join("evil").exists());
}

#[tokio::test]
async fn record_delay_holds_then_returns_empty_object() {
    let delay = Duration::from_millis(300);
    let hub = TestHub::start(delay).await;
    let body = r#"{"hook_event_name":"PermissionRequest"}"#;

    let started = Instant::now();
    let res = hub.post("/hooks/permission", Some(TOKEN), body).await;
    let elapsed = started.elapsed();

    assert!(
        elapsed >= delay,
        "trả lời sau {elapsed:?}, phải giữ ít nhất {delay:?}"
    );
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "{}");

    // Độ trễ chỉ áp cho /hooks/permission; /hooks/event vẫn trả ngay.
    let started = Instant::now();
    let res = hub
        .post("/hooks/event", Some(TOKEN), r#"{"hook_event_name":"Stop"}"#)
        .await;
    assert_eq!(res.status(), 204);
    assert!(started.elapsed() < delay);
}

#[tokio::test]
async fn write_failure_never_allows() {
    // Thư mục ghi là một FILE: mọi lần ghi đều lỗi.
    let path =
        std::env::temp_dir().join(format!("arcturus-record-file-{}", ulid::Ulid::generate()));
    std::fs::write(&path, "chặn").unwrap();
    let hub = TestHub::start_with_dir(path.clone(), Duration::ZERO).await;

    let res = hub
        .post(
            "/hooks/permission",
            Some(TOKEN),
            r#"{"hook_event_name":"PermissionRequest"}"#,
        )
        .await;

    assert!(
        res.status().is_server_error(),
        "ghi lỗi phải là lỗi không chặn (non-2xx), nhận {}",
        res.status()
    );
    assert!(!res.text().await.unwrap().contains("allow"));
    std::fs::remove_file(&path).unwrap();
}

#[tokio::test]
async fn healthz_ok_without_token() {
    let hub = TestHub::start(Duration::ZERO).await;

    let res = hub.client.get(hub.url("/healthz")).send().await.unwrap();

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "ok");
}
