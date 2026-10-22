//! Endpoint nhận hook (`docs/PLAN.md` §3.3): `/hooks/permission` giữ yêu cầu duyệt, `/hooks/event` trả `204` ngay
//! rồi xử lý bất đồng bộ qua `mpsc` (thứ tự sự kiện được giữ nguyên, xem `crate::hub`).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{middleware, Json, Router};
use hub_core::hook::HookInput;

use crate::approvals::{response_for, PermissionOutcome};
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

async fn hook_permission(State(hub): State<Arc<Hub>>, req: Request) -> Response {
    // Đặt chỗ trong ngân sách byte của hàng đợi TRƯỚC khi đọc body (T5c · ARC-43): hết chỗ thì `{}` ngay, Claude Code hỏi
    // trong terminal. Không có `Content-Length` (chunked) thì tính mức tối đa.
    let declared = match req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
    {
        Some(n) if n > MAX_BODY_BYTES => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        Some(n) => n,
        None => MAX_BODY_BYTES,
    };
    let Some(mut reservation) = hub.approvals.reserve_body(declared) else {
        tracing::warn!(
            declared,
            "hàng đợi duyệt đầy bộ nhớ, đọc bỏ body rồi trả {{}} để Claude Code hỏi trong terminal"
        );
        // Đọc bỏ (không đệm) để client nhận được `{}`: trả lời khi body chưa đọc làm Windows reset kết nối và client
        // không nhận được gì (đo ở ac866b2).
        discard_body(req.into_body()).await;
        return overloaded();
    };
    let body = match axum::body::to_bytes(req.into_body(), MAX_BODY_BYTES).await {
        Ok(body) => body,
        Err(err) => {
            tracing::warn!(%err, "không đọc được body (quá lớn hoặc client ngắt)");
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
    };
    // JSON nhiều nút nhỏ (`[[0],[0],…]`) phình tới ~40 lần khi parse (audit ARC-43 finding 2): ước lượng trước khi parse
    // và nới chỗ đặt; không đủ thì `{}` ngay.
    let estimate = parse_estimate(&body);
    if !reservation.grow_to(estimate) {
        tracing::warn!(
            bytes = body.len(),
            estimate,
            "không đủ ngân sách byte để parse body (hàng đợi gần đầy hoặc JSON phình lớn), trả {{}}"
        );
        return overloaded();
    }
    let input = match HookInput::from_slice(&body) {
        Ok(input) => input,
        Err(err) => return bad_json(err),
    };
    // Không giữ body thô suốt thời gian chờ duyệt; từ đây `tool_input` được tính trong `held_bytes` khi vào hàng đợi.
    drop(body);
    drop(reservation);
    let outcome = hub.permission(input).await;
    (StatusCode::OK, Json(response_for(&outcome))).into_response()
}

/// Ước lượng (dư) bộ nhớ khi parse `body` thành `serde_json::Value`: body + mỗi dấu phân cách cấu trúc (`[`, `{`, `,`,
/// `:`) có thể là một nút ~32 B cộng phần dư của mảng. Đếm cả dấu nằm trong chuỗi, nên chỉ có thể ước lượng thừa.
pub fn parse_estimate(body: &[u8]) -> usize {
    let nodes = body
        .iter()
        .filter(|b| matches!(b, b'[' | b'{' | b',' | b':'))
        .count();
    body.len() + nodes * 2 * std::mem::size_of::<serde_json::Value>()
}

/// Đọc bỏ tối đa chừng này thời gian trước khi trả lời yêu cầu bị từ chối sớm (client treo thì không giữ handler lâu).
pub const DRAIN_TIMEOUT: Duration = Duration::from_secs(3);

/// Đọc bỏ body theo từng khung (không đệm), tối đa [`MAX_BODY_BYTES`] và [`DRAIN_TIMEOUT`].
async fn discard_body(mut body: axum::body::Body) {
    use http_body_util::BodyExt;
    let drain = async {
        let mut seen = 0usize;
        while let Some(frame) = body.frame().await {
            let Ok(frame) = frame else { break };
            if let Some(data) = frame.data_ref() {
                seen += data.len();
                if seen > MAX_BODY_BYTES {
                    break;
                }
            }
        }
    };
    if tokio::time::timeout(DRAIN_TIMEOUT, drain).await.is_err() {
        tracing::warn!("client gửi body quá chậm, ngừng đọc bỏ");
    }
}

fn overloaded() -> Response {
    (
        StatusCode::OK,
        Json(response_for(&PermissionOutcome::Overloaded)),
    )
        .into_response()
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
