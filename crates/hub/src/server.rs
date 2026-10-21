//! Khởi động server HTTP: chỉ loopback (quy tắc bất biến 4), tắt êm khi nhận Ctrl-C.

use std::fmt;
use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;

/// Địa chỉ mặc định (`docs/PLAN.md` §3.3).
pub const DEFAULT_BIND: &str = "127.0.0.1:7878";

#[derive(Debug, PartialEq, Eq)]
pub struct NonLoopbackBind(pub SocketAddr);

impl fmt::Display for NonLoopbackBind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "từ chối bind {}: v0 chỉ cho phép địa chỉ loopback (127.0.0.1 hoặc ::1)",
            self.0
        )
    }
}

impl std::error::Error for NonLoopbackBind {}

/// Chỉ chấp nhận địa chỉ loopback. `0.0.0.0`/`::` (mọi giao diện) bị từ chối.
pub fn validate_bind(addr: SocketAddr) -> Result<SocketAddr, NonLoopbackBind> {
    if addr.ip().is_loopback() {
        Ok(addr)
    } else {
        Err(NonLoopbackBind(addr))
    }
}

/// Phục vụ `app` trên `listener` tới khi nhận Ctrl-C.
pub async fn serve(listener: TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            // Không cài được handler thì vẫn phục vụ; tắt bằng cách giết process.
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        })
        .await
}
