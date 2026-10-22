//! Endpoint nhận hook (`docs/PLAN.md` §3.3): `/hooks/permission` giữ yêu cầu duyệt, `/hooks/event` trả `204` ngay
//! rồi xử lý bất đồng bộ qua `mpsc` (thứ tự sự kiện được giữ nguyên).
//!
//! T3: vòng sự kiện chỉ làm phần của luồng duyệt (hủy yêu cầu khi `SessionEnd` hoặc khi đã được trả lời ở terminal).
//! T4 thêm `state.apply`, T6 thêm ghi store.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{middleware, Json, Router};
use hub_core::hook::{self, HookInput};
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::approvals::{response_for, Approvals};
use crate::auth;
use crate::record::MAX_BODY_BYTES;

/// Sức chứa hàng đợi `/hooks/event` (§3.5).
pub const EVENT_QUEUE: usize = 1024;

pub struct HubState {
    pub approvals: Arc<Approvals>,
    pub host: String,
    events: mpsc::Sender<HookInput>,
}

/// Dựng router và khởi động vòng sự kiện (cần đang ở trong runtime tokio).
pub fn router(approvals: Arc<Approvals>, host: String, token: String) -> Router {
    let (events, rx) = mpsc::channel(EVENT_QUEUE);
    spawn_event_loop(approvals.clone(), host.clone(), rx);
    let state = Arc::new(HubState {
        approvals,
        host,
        events,
    });
    let token: Arc<str> = Arc::from(token);
    let hooks = Router::new()
        .route("/hooks/event", post(hook_event))
        .route("/hooks/permission", post(hook_permission))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .route_layer(middleware::from_fn_with_state(token, auth::require_bearer))
        .with_state(state);
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(hooks)
}

fn spawn_event_loop(
    approvals: Arc<Approvals>,
    host: String,
    mut rx: mpsc::Receiver<HookInput>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(input) = rx.recv().await {
            let ev = hook::to_event(&input, &host, OffsetDateTime::now_utc());
            approvals.on_event(&ev, &input);
        }
    })
}

async fn hook_permission(State(st): State<Arc<HubState>>, body: Bytes) -> Response {
    let input = match HookInput::from_slice(&body) {
        Ok(input) => input,
        Err(err) => return bad_json(err),
    };
    let ev = hook::to_event(&input, &st.host, OffsetDateTime::now_utc());
    let outcome = st.approvals.request(&input, &ev).await;
    (StatusCode::OK, Json(response_for(&outcome))).into_response()
}

async fn hook_event(State(st): State<Arc<HubState>>, body: Bytes) -> Response {
    let input = match HookInput::from_slice(&body) {
        Ok(input) => input,
        Err(err) => return bad_json(err),
    };
    // Hàng đợi đầy thì bỏ sự kiện: mất `SessionEnd` chỉ làm yêu cầu đang chờ hết giờ và trả `{}` (an toàn).
    if let Err(err) = st.events.try_send(input) {
        tracing::warn!(%err, "bỏ sự kiện hook: hàng đợi đầy hoặc đã đóng");
    }
    StatusCode::NO_CONTENT.into_response()
}

fn bad_json(err: serde_json::Error) -> Response {
    tracing::warn!(%err, "body không phải JSON hợp lệ");
    (StatusCode::BAD_REQUEST, "body không phải JSON hợp lệ").into_response()
}
