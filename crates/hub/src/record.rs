//! Chế độ ghi băng (`hub --record <dir>`, T1): lưu nguyên body hook nhận được để làm fixture.
//!
//! Không quyết định gì: `/hooks/permission` luôn trả `{}` (2xx, không có `decision`) để Claude Code tự hỏi người
//! trong terminal. Ghi lỗi thì trả non-2xx, cũng là lỗi không chặn. Không nhánh nào trả `allow` (quy tắc bất biến 1).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{middleware, Json, Router};
use serde_json::Value;

use crate::auth;

/// Body tối đa (`docs/PLAN.md` §3.3). Lớn hơn → `413`.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Thư mục cho sự kiện thiếu tên hoặc có tên không an toàn để làm tên thư mục.
pub const UNKNOWN_DIR: &str = "Unknown";

#[derive(Clone, Debug)]
pub struct RecordConfig {
    /// Thư mục gốc; mỗi sự kiện ghi vào `<dir>/<hook_event_name>/<ulid>.json`.
    pub dir: PathBuf,
    /// Giữ `/hooks/permission` bao lâu trước khi trả `{}` (thí nghiệm Q1).
    pub delay: Duration,
}

pub fn router(cfg: RecordConfig, token: String) -> Router {
    let token: Arc<str> = Arc::from(token);
    let hooks = Router::new()
        .route("/hooks/event", post(record_event))
        .route("/hooks/permission", post(record_permission))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .route_layer(middleware::from_fn_with_state(token, auth::require_bearer))
        .with_state(Arc::new(cfg));
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(hooks)
}

async fn record_event(State(cfg): State<Arc<RecordConfig>>, body: Bytes) -> Response {
    match save(&cfg.dir, &body).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => e.into_response(),
    }
}

async fn record_permission(State(cfg): State<Arc<RecordConfig>>, body: Bytes) -> Response {
    if let Err(e) = save(&cfg.dir, &body).await {
        return e.into_response();
    }
    if !cfg.delay.is_zero() {
        tokio::time::sleep(cfg.delay).await;
    }
    // Không có `decision`: Claude Code hiển thị hộp thoại duyệt bình thường.
    (StatusCode::OK, Json(serde_json::json!({}))).into_response()
}

#[derive(Debug)]
enum RecordError {
    BadJson(serde_json::Error),
    Write { path: PathBuf, err: std::io::Error },
}

impl IntoResponse for RecordError {
    fn into_response(self) -> Response {
        match self {
            RecordError::BadJson(err) => {
                tracing::warn!(%err, "body không phải JSON hợp lệ");
                (StatusCode::BAD_REQUEST, "body không phải JSON hợp lệ").into_response()
            }
            RecordError::Write { path, err } => {
                tracing::error!(path = %path.display(), %err, "không ghi được fixture");
                (StatusCode::INTERNAL_SERVER_ERROR, "không ghi được fixture").into_response()
            }
        }
    }
}

/// Ghi nguyên byte `body` (không định dạng lại) vào thư mục theo `hook_event_name`.
async fn save(base: &Path, body: &[u8]) -> Result<(), RecordError> {
    let value: Value = serde_json::from_slice(body).map_err(RecordError::BadJson)?;
    let name = event_dir_name(&value);
    let dir = base.join(name);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|err| RecordError::Write {
            path: dir.clone(),
            err,
        })?;
    let path = dir.join(format!("{}.json", ulid::Ulid::generate()));
    tokio::fs::write(&path, body)
        .await
        .map_err(|err| RecordError::Write {
            path: path.clone(),
            err,
        })?;
    // Không log nội dung: hook có thể chứa lệnh và nội dung file.
    tracing::info!(event = name, bytes = body.len(), "đã ghi");
    Ok(())
}

/// Tên thư mục cho một sự kiện. `hook_event_name` đến từ bên ngoài nên chỉ nhận `[A-Za-z0-9_-]{1,64}`
/// và không trùng tên thiết bị của Windows; còn lại về [`UNKNOWN_DIR`] (chặn `../`, đường dẫn tuyệt đối…).
pub fn event_dir_name(value: &Value) -> &str {
    match value.get("hook_event_name").and_then(Value::as_str) {
        Some(name) if is_safe_dir_name(name) => name,
        _ => UNKNOWN_DIR,
    }
}

fn is_safe_dir_name(name: &str) -> bool {
    const WINDOWS_RESERVED: [&str; 24] = [
        "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
        "LPT9",
    ];
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && !WINDOWS_RESERVED
            .iter()
            .any(|r| r.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn known_event_names_kept() {
        for n in [
            "PreToolUse",
            "PermissionRequest",
            "SubagentStart",
            "Stop",
            "Session_End-2",
        ] {
            assert_eq!(event_dir_name(&json!({ "hook_event_name": n })), n);
        }
    }

    #[test]
    fn unsafe_names_map_to_unknown() {
        for n in [
            "",
            ".",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "C:",
            "CON",
            "lpt1",
            "COM0",
            "lpt0",
            "Tên",
            &"x".repeat(65),
        ] {
            assert_eq!(
                event_dir_name(&json!({ "hook_event_name": n })),
                UNKNOWN_DIR,
                "{n:?}"
            );
        }
        assert_eq!(event_dir_name(&json!({})), UNKNOWN_DIR);
        assert_eq!(event_dir_name(&json!([1])), UNKNOWN_DIR);
    }
}
