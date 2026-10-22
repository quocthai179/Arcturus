//! Endpoint nhận hook (`docs/PLAN.md` §3.3): `/hooks/permission` giữ yêu cầu duyệt, `/hooks/event` trả `204` ngay
//! rồi xử lý bất đồng bộ qua `mpsc` (thứ tự sự kiện được giữ nguyên, xem `crate::hub`).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{middleware, Json, Router};
use hub_core::hook::HookInput;

use crate::approvals::response_for;
use crate::auth;
use crate::hub::Hub;
use crate::record::MAX_BODY_BYTES;

/// Router cho hook. `token` là token của hook (`ARCTURUS_TOKEN`).
pub fn router(hub: Arc<Hub>, token: String) -> Router {
    let token: Arc<str> = Arc::from(token);
    let hooks = Router::new()
        .route("/hooks/event", post(hook_event))
        .route("/hooks/permission", post(hook_permission))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .route_layer(middleware::from_fn_with_state(token, auth::require_bearer))
        .with_state(hub);
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(hooks)
}

async fn hook_permission(State(hub): State<Arc<Hub>>, body: Bytes) -> Response {
    let input = match HookInput::from_slice(&body) {
        Ok(input) => input,
        Err(err) => return bad_json(err),
    };
    // Không giữ body thô suốt thời gian chờ duyệt.
    drop(body);
    let outcome = hub.permission(input).await;
    (StatusCode::OK, Json(response_for(&outcome))).into_response()
}

async fn hook_event(State(hub): State<Arc<Hub>>, body: Bytes) -> Response {
    match HookInput::from_slice(&body) {
        Ok(input) => {
            hub.enqueue_event(input);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => bad_json(err),
    }
}

fn bad_json(err: serde_json::Error) -> Response {
    tracing::warn!(%err, "body không phải JSON hợp lệ");
    (StatusCode::BAD_REQUEST, "body không phải JSON hợp lệ").into_response()
}
