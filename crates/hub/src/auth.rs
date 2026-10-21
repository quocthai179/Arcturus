//! Xác thực Bearer token cho hook và WebSocket (quy tắc bất biến 4, `docs/PLAN.md` §3.3).

use std::fmt;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Biến môi trường chứa token. Thiếu thì hub không khởi động.
pub const TOKEN_ENV: &str = "ARCTURUS_TOKEN";

/// Token ngắn hơn mức này bị từ chối (gợi ý tạo: `openssl rand -hex 24`).
pub const MIN_TOKEN_LEN: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub enum TokenError {
    Missing,
    TooShort { len: usize },
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenError::Missing => write!(
                f,
                "chưa đặt {TOKEN_ENV}; tạo bằng `openssl rand -hex 24` rồi đặt vào biến môi trường"
            ),
            TokenError::TooShort { len } => write!(
                f,
                "{TOKEN_ENV} chỉ có {len} ký tự, cần ít nhất {MIN_TOKEN_LEN}; tạo bằng `openssl rand -hex 24`"
            ),
        }
    }
}

impl std::error::Error for TokenError {}

/// Kiểm tra giá trị token đọc từ môi trường (nhận `Option` để test không phải đổi biến môi trường thật).
pub fn token_from_env(value: Option<String>) -> Result<String, TokenError> {
    let token = value.map(|v| v.trim().to_owned()).unwrap_or_default();
    if token.is_empty() {
        return Err(TokenError::Missing);
    }
    let len = token.chars().count();
    if len < MIN_TOKEN_LEN {
        return Err(TokenError::TooShort { len });
    }
    Ok(token)
}

/// So sánh không phụ thuộc vị trí byte khác đầu tiên. Độ dài khác nhau thì trả `false` ngay
/// (chỉ lộ độ dài, không lộ nội dung).
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `true` khi header `Authorization` có dạng `Bearer <token>` và token khớp.
pub fn bearer_matches(header_value: Option<&str>, token: &str) -> bool {
    match header_value.and_then(|v| v.strip_prefix("Bearer ")) {
        Some(given) => constant_time_eq(given.as_bytes(), token.as_bytes()),
        None => false,
    }
}

/// Middleware axum: sai hoặc thiếu token → `401`. Claude Code coi đó là lỗi không chặn và tự hỏi trong terminal.
pub async fn require_bearer(State(token): State<Arc<str>>, req: Request, next: Next) -> Response {
    let given = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if bearer_matches(given, &token) {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_basic() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn bearer_requires_scheme_and_exact_token() {
        let t = "0123456789abcdef";
        assert!(bearer_matches(Some("Bearer 0123456789abcdef"), t));
        assert!(!bearer_matches(Some("bearer 0123456789abcdef"), t));
        assert!(!bearer_matches(Some("Bearer  0123456789abcdef"), t));
        assert!(!bearer_matches(Some("Bearer 0123456789abcdeg"), t));
        assert!(!bearer_matches(Some("0123456789abcdef"), t));
        assert!(!bearer_matches(None, t));
    }

    #[test]
    fn token_is_trimmed() {
        assert_eq!(
            token_from_env(Some("  0123456789abcdef\n".to_owned())),
            Ok("0123456789abcdef".to_owned())
        );
    }
}
