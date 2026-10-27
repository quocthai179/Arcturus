//! WebSocket `/ws` cho UI và `GET /approvals/{id}/input` (`docs/PLAN.md` §3.3, §3.4; mô tả ở `docs/PROTOCOL.md`).
//!
//! - Xác thực bằng token UI riêng (D1), không bao giờ bằng token của hook. `/ws` nhận `Authorization: Bearer` hoặc
//!   `?token=` (webview không đặt được header cho WebSocket); không log URI/token.
//! - `Host` phải là địa chỉ loopback của chính hub (chặn DNS rebinding); `Origin`, nếu có, phải thuộc
//!   [`ALLOWED_ORIGINS`] (chặn trang web lạ mở WebSocket tới 127.0.0.1).
//! - Mỗi `ServerMsg` được serialize **một lần** thành frame dùng chung cho mọi client. Client tụt sau kênh nhận
//!   `Snapshot` mới; client không nhận được frame trong [`WsConfig::write_timeout`] bị ngắt.
//! - Client mới: đăng ký kênh trước rồi mới dựng `Snapshot`, nên có thể nhận trùng một thay đổi (UI upsert) nhưng không
//!   bao giờ lỡ.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use hub_core::protocol::{ClientMsg, ServerMsg};
use hub_core::schema::Decision;
use serde::Deserialize;
use time::OffsetDateTime;
use tokio::sync::{broadcast, watch};
use tokio::time::Instant;

use crate::approvals::{DecideResult, HeldReason};
use crate::auth;
use crate::hub::Hub;
use crate::standup::{self, StandupError, SystemGit};

/// Origin được phép mở WebSocket: webview Tauri (Windows / macOS, Linux).
pub const ALLOWED_ORIGINS: [&str; 3] = [
    "http://tauri.localhost",
    "https://tauri.localhost",
    "tauri://localhost",
];

/// Vite dev server của `ui/` (`pnpm -C ui dev`): chỉ nhận ở bản debug của hub. Cổng 5173 là mặc định của mọi dự án
/// Vite, nên bản release không tin origin này (audit T5b finding 8).
pub const DEV_ORIGINS: [&str; 2] = ["http://127.0.0.1:5173", "http://localhost:5173"];

/// Tối đa kết nối WebSocket cùng lúc (audit T5b finding 5).
pub const MAX_CLIENTS: usize = 8;

fn origin_allowed(origin: &str) -> bool {
    let dev: &[&str] = if cfg!(debug_assertions) {
        &DEV_ORIGINS
    } else {
        &[]
    };
    ALLOWED_ORIGINS
        .iter()
        .chain(dev)
        .any(|ok| origin.eq_ignore_ascii_case(ok))
}

/// Kích thước tối đa một thông điệp từ client.
pub const MAX_CLIENT_MESSAGE_BYTES: usize = 64 * 1024;

/// Độ dài tối đa lý do từ chối (ký tự Unicode).
pub const MAX_DENY_MESSAGE_CHARS: usize = 500;

#[derive(Clone, Debug)]
pub struct WsConfig {
    pub ping_interval: Duration,
    /// Client không nhận một frame trong chừng này thì bị ngắt.
    pub write_timeout: Duration,
    /// Sức chứa kênh frame dùng chung; client tụt hơn chừng này frame thì nhận `Snapshot` mới.
    pub frames_capacity: usize,
    /// Số lệnh `Decide` tối đa mỗi giây trên một kết nối.
    pub decide_per_sec: u32,
}

impl Default for WsConfig {
    fn default() -> Self {
        WsConfig {
            ping_interval: Duration::from_secs(30),
            write_timeout: Duration::from_secs(10),
            frames_capacity: 256,
            decide_per_sec: 20,
        }
    }
}

#[derive(Clone)]
enum Frame {
    Msg(Utf8Bytes),
    /// Bộ phát frame bị tụt sau kênh `ServerMsg`: mọi client cần `Snapshot` mới.
    Resync,
}

struct WsState {
    hub: Arc<Hub>,
    ui_token: Arc<str>,
    port: u16,
    cfg: WsConfig,
    frames: broadcast::Sender<Frame>,
    shutdown: watch::Receiver<bool>,
    clients: Arc<AtomicUsize>,
    next_conn: AtomicU64,
}

/// Điều khiển phần WebSocket từ ngoài router.
#[derive(Clone)]
pub struct WsHandle {
    shutdown: Arc<watch::Sender<bool>>,
    clients: Arc<AtomicUsize>,
}

impl WsHandle {
    /// Gửi Close cho mọi client và không nhận client mới (gọi khi tắt hub, cùng `Hub::shutdown`).
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }

    /// Số kết nối WebSocket đang mở.
    pub fn clients(&self) -> usize {
        self.clients.load(Ordering::SeqCst)
    }
}

/// Router cho UI. `port` là cổng hub đang nghe (để kiểm `Host`). Cần đang ở trong runtime tokio.
pub fn router(hub: Arc<Hub>, ui_token: String, port: u16, cfg: WsConfig) -> (Router, WsHandle) {
    let (frames, _) = broadcast::channel(cfg.frames_capacity.max(1));
    let (shutdown_tx, shutdown) = watch::channel(false);
    let clients = Arc::new(AtomicUsize::new(0));
    tokio::spawn(fanout(hub.subscribe(), frames.clone(), shutdown.clone()));
    let state = Arc::new(WsState {
        hub,
        ui_token: Arc::from(ui_token),
        port,
        cfg,
        frames,
        shutdown,
        clients: clients.clone(),
        next_conn: AtomicU64::new(1),
    });
    let router = Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/approvals/{request_id}/input", get(approval_input))
        .route("/standup", get(standup_handler))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_ui_token,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_local_origin,
        ))
        .with_state(state);
    (
        router,
        WsHandle {
            shutdown: Arc::new(shutdown_tx),
            clients,
        },
    )
}

/// `Host` loopback của chính hub; `Origin` (nếu có) trong danh sách. Sai → 403.
async fn require_local_origin(
    State(st): State<Arc<WsState>>,
    req: Request,
    next: Next,
) -> Response {
    let host_ok = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| {
            [
                format!("127.0.0.1:{}", st.port),
                format!("localhost:{}", st.port),
                format!("[::1]:{}", st.port),
            ]
            .iter()
            .any(|ok| h.eq_ignore_ascii_case(ok))
        });
    let origin_ok = match req.headers().get(header::ORIGIN) {
        None => true,
        Some(o) => o.to_str().is_ok_and(origin_allowed),
    };
    if host_ok && origin_ok {
        next.run(req).await
    } else {
        tracing::warn!(
            host_ok,
            origin_ok,
            "từ chối kết nối UI: Host/Origin không hợp lệ"
        );
        StatusCode::FORBIDDEN.into_response()
    }
}

/// Token UI qua `Authorization: Bearer`, hoặc `?token=` chỉ cho `/ws`. Token của hook không được chấp nhận.
async fn require_ui_token(State(st): State<Arc<WsState>>, req: Request, next: Next) -> Response {
    let header_value = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let mut ok = auth::bearer_matches(header_value, &st.ui_token);
    if !ok && req.uri().path() == "/ws" {
        ok = req
            .uri()
            .query()
            .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("token=")))
            .is_some_and(|t| auth::constant_time_eq(t.as_bytes(), st.ui_token.as_bytes()));
    }
    if ok {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

async fn ws_upgrade(State(st): State<Arc<WsState>>, ws: WebSocketUpgrade) -> Response {
    if *st.shutdown.borrow() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if st.clients.load(Ordering::SeqCst) >= MAX_CLIENTS {
        tracing::warn!(max = MAX_CLIENTS, "từ chối client UI: đã đủ số kết nối");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    ws.max_message_size(MAX_CLIENT_MESSAGE_BYTES)
        .max_frame_size(MAX_CLIENT_MESSAGE_BYTES)
        .on_upgrade(move |socket| client(st, socket))
}

/// `tool_input` đầy đủ của yêu cầu đang chờ (nút "Xem đầy đủ"). Không còn chờ, hoặc là thẻ duyệt-trong-terminal → 404.
async fn approval_input(
    State(st): State<Arc<WsState>>,
    Path(request_id): Path<String>,
) -> Response {
    match st.hub.approvals.tool_input_for_review(&request_id) {
        Some(v) => (
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                ),
                (
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                ),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
            v.to_string(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
struct StandupQuery {
    /// Số giờ gần nhất (1–168, mặc định 24).
    hours: Option<u32>,
}

/// `GET /standup?hours=24` (T19): standup sinh từ sự kiện đã lưu và `git log`, không LLM. Chỉ token UI; `Cache-Control:
/// no-store` vì có tiêu đề commit và lệnh test của người dùng.
async fn standup_handler(
    State(st): State<Arc<WsState>>,
    Query(q): Query<StandupQuery>,
) -> Response {
    let hours = q.hours.unwrap_or(standup::DEFAULT_HOURS);
    if !(1..=standup::MAX_HOURS).contains(&hours) {
        return (
            StatusCode::BAD_REQUEST,
            format!("hours phải trong khoảng 1..={}", standup::MAX_HOURS),
        )
            .into_response();
    }
    let hub = st.hub.clone();
    // Đọc SQLite và chạy `git` đều chặn: đưa ra khỏi luồng async.
    let result = tokio::task::spawn_blocking(move || {
        standup::standup_for(&hub, hours, OffsetDateTime::now_utc(), &SystemGit)
    })
    .await;
    match result {
        Ok(Ok(s)) => (
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                ),
                (
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                ),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
            serde_json::to_string(&s).unwrap_or_else(|_| "{}".to_owned()),
        )
            .into_response(),
        Ok(Err(StandupError::NoDatabase)) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        Ok(Err(StandupError::Store(err))) => {
            tracing::warn!(%err, "standup: lỗi đọc cơ sở dữ liệu");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(err) => {
            tracing::warn!(%err, "standup: task bị hủy");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Serialize mỗi `ServerMsg` một lần cho mọi client.
async fn fanout(
    mut rx: broadcast::Receiver<ServerMsg>,
    frames: broadcast::Sender<Frame>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            r = rx.recv() => match r {
                Ok(m) => match serde_json::to_string(&m) {
                    Ok(s) => {
                        let _ = frames.send(Frame::Msg(Utf8Bytes::from(s)));
                    }
                    Err(err) => tracing::error!(%err, "không serialize được ServerMsg"),
                },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "bộ phát frame bị tụt, yêu cầu mọi client lấy Snapshot mới");
                    let _ = frames.send(Frame::Resync);
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            changed = shutdown.changed() => if changed.is_err() || *shutdown.borrow() { break },
        }
    }
}

struct ClientGuard(Arc<AtomicUsize>);

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Giới hạn `Decide` theo cửa sổ 1 giây.
struct Limiter {
    window_start: Instant,
    count: u32,
    max: u32,
}

impl Limiter {
    fn allow(&mut self) -> bool {
        if self.window_start.elapsed() >= Duration::from_secs(1) {
            self.window_start = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        self.count <= self.max
    }
}

async fn client(st: Arc<WsState>, socket: WebSocket) {
    let conn = st.next_conn.fetch_add(1, Ordering::Relaxed);
    st.clients.fetch_add(1, Ordering::SeqCst);
    let _guard = ClientGuard(st.clients.clone());
    // Đăng ký trước khi dựng Snapshot: không lỡ thay đổi nào xảy ra giữa hai bước.
    let mut frames = st.frames.subscribe();
    let mut shutdown = st.shutdown.clone();
    let (mut tx, mut rx) = socket.split();
    let wt = st.cfg.write_timeout;
    tracing::info!(conn, "client UI kết nối");

    // Gửi có hạn giờ: client không nhận trong `write_timeout` thì coi như treo và bị ngắt.
    macro_rules! send {
        ($m:expr) => {
            matches!(tokio::time::timeout(wt, tx.send($m)).await, Ok(Ok(())))
        };
    }

    if !send!(snapshot(&st.hub)) {
        return;
    }
    let mut ping =
        tokio::time::interval_at(Instant::now() + st.cfg.ping_interval, st.cfg.ping_interval);
    let mut missed_pongs = 0u32;
    let mut limiter = Limiter {
        window_start: Instant::now(),
        count: 0,
        max: st.cfg.decide_per_sec,
    };
    let reason = loop {
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    let _ = send!(Message::Close(Some(CloseFrame {
                        code: 1001,
                        reason: Utf8Bytes::from_static("hub tắt"),
                    })));
                    break "hub tắt";
                }
            }
            m = rx.next() => match m {
                Some(Ok(Message::Text(text))) => {
                    let mut ok = true;
                    for reply in handle(&st, conn, text.as_str(), &mut limiter) {
                        if !send!(reply) {
                            ok = false;
                            break;
                        }
                    }
                    if !ok {
                        break "ghi quá giờ";
                    }
                }
                Some(Ok(Message::Binary(_))) => {
                    if !send!(feed("Chỉ nhận thông điệp JSON dạng text")) {
                        break "ghi quá giờ";
                    }
                }
                Some(Ok(Message::Pong(_))) => missed_pongs = 0,
                // axum tự trả Pong cho Ping.
                Some(Ok(Message::Ping(_))) => {}
                Some(Ok(Message::Close(_))) | None => break "client đóng",
                Some(Err(_)) => break "lỗi kết nối",
            },
            f = frames.recv() => match f {
                Ok(Frame::Msg(text)) => {
                    if !send!(Message::Text(text)) {
                        break "ghi quá giờ";
                    }
                }
                Ok(Frame::Resync) | Err(broadcast::error::RecvError::Lagged(_)) => {
                    tracing::info!(conn, "client tụt sau kênh, gửi Snapshot mới");
                    if !send!(snapshot(&st.hub)) {
                        break "ghi quá giờ";
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break "kênh đóng",
            },
            _ = ping.tick() => {
                if missed_pongs >= 2 {
                    break "không trả lời ping";
                }
                missed_pongs += 1;
                if !send!(Message::Ping(axum::body::Bytes::new())) {
                    break "ghi quá giờ";
                }
            }
        }
    };
    tracing::info!(conn, reason, "client UI ngắt");
}

fn snapshot(hub: &Hub) -> Message {
    let msg = ServerMsg::Snapshot {
        agents: hub.agent_views(),
        queue: hub.approvals.views(),
        handoffs: hub.approvals.handoffs(),
        team: hub.team_view(),
    };
    text(&msg)
}

fn feed(text_: &str) -> Message {
    text(&ServerMsg::Feed {
        text: text_.to_owned(),
        ts: OffsetDateTime::now_utc(),
    })
}

fn text(msg: &ServerMsg) -> Message {
    // `ServerMsg` chỉ gồm chuỗi, số, enum: serialize không lỗi; nếu có thì gửi Feed rỗng thay vì panic.
    Message::Text(Utf8Bytes::from(
        serde_json::to_string(msg).unwrap_or_else(|_| "{\"t\":\"feed\",\"text\":\"\"}".to_owned()),
    ))
}

/// Xử lý một lệnh từ client; trả các `Feed` gửi riêng cho client đó. Mọi thay đổi trạng thái đi qua kênh broadcast.
fn handle(st: &WsState, conn: u64, raw: &str, limiter: &mut Limiter) -> Vec<Message> {
    let msg: ClientMsg = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(_) => return vec![feed("Thông điệp không hợp lệ")],
    };
    match msg {
        ClientMsg::Decide {
            request_id,
            decision,
            message,
            origin,
        } => {
            if !limiter.allow() {
                return vec![feed("Quá nhiều lệnh duyệt, hãy thử lại sau")];
            }
            if message
                .as_deref()
                .is_some_and(|m| m.chars().count() > MAX_DENY_MESSAGE_CHARS)
            {
                return vec![feed("Lý do từ chối quá dài (tối đa 500 ký tự)")];
            }
            match st
                .hub
                .approvals
                .decide(&request_id, decision, origin, message)
            {
                DecideResult::Resolved => {
                    // Ghi mọi quyết định kèm kết nối (audit T5a: DecideOrigin do client tự khai).
                    tracing::info!(conn, request_id = ?request_id, ?decision, ?origin, "UI quyết định");
                    if decision == Decision::Allow {
                        tracing::warn!(conn, request_id = ?request_id, "UI allow");
                    }
                    Vec::new()
                }
                DecideResult::Expired => vec![feed("Yêu cầu đã hết hạn")],
                DecideResult::Held(HeldReason::NotReviewed) => {
                    vec![feed("Cần mở bản đầy đủ trước khi duyệt")]
                }
                DecideResult::Held(HeldReason::TooFast) => {
                    vec![feed("Duyệt quá nhanh: xem lại thẻ rồi bấm lại")]
                }
                DecideResult::Held(_) => vec![feed("Cần trưởng nhóm bấm duyệt")],
                DecideResult::HandedOff => {
                    vec![feed("Yêu cầu này đang được duyệt trong terminal")]
                }
            }
        }
        // Lệnh đội chung giới hạn với Decide: mỗi lệnh broadcast tới mọi client (audit T5b finding 5).
        ClientMsg::TeamPause | ClientMsg::TeamResume | ClientMsg::FocusSet { .. }
            if !limiter.allow() =>
        {
            vec![feed("Quá nhiều lệnh, hãy thử lại sau")]
        }
        ClientMsg::TeamPause => {
            st.hub.set_paused(true);
            Vec::new()
        }
        ClientMsg::TeamResume => {
            st.hub.set_paused(false);
            Vec::new()
        }
        ClientMsg::FocusSet { on } => {
            st.hub.set_focus(on);
            Vec::new()
        }
        ClientMsg::DecideGroup { .. }
        | ClientMsg::BatchOpen { .. }
        | ClientMsg::BatchClose { .. }
        | ClientMsg::SuggestionAnswer { .. } => vec![feed("Lệnh này chưa được hỗ trợ")],
    }
}
