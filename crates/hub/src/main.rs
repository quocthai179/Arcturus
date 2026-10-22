//! Daemon Arcturus. Mặc định: luồng duyệt (T3). `--record`: chế độ ghi băng của T1.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use hub::approvals::{AlwaysAsk, Approvals, ApprovalsConfig, NoStore};
use hub::record::{self, RecordConfig};
use hub::{auth, hooks, server};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "hub",
    version,
    about = "Arcturus hub: nhận hook Claude Code ở 127.0.0.1"
)]
struct Cli {
    /// Ghi băng: lưu body thô của mọi hook vào <DIR>/<hook_event_name>/<ulid>.json, không quyết định gì.
    #[arg(long, value_name = "DIR")]
    record: Option<PathBuf>,

    /// Giữ /hooks/permission bao nhiêu giây trước khi trả {} (thí nghiệm Q1).
    #[arg(long, value_name = "GIÂY", default_value_t = 0, requires = "record",
          value_parser = clap::value_parser!(u64).range(0..=600))]
    record_delay: u64,

    /// Giữ yêu cầu duyệt tối đa bao nhiêu giây rồi trả {}; phải nhỏ hơn timeout 120 s của hook.
    #[arg(long, value_name = "GIÂY", default_value_t = 110, conflicts_with = "record",
          value_parser = clap::value_parser!(u64).range(1..=115))]
    approval_timeout: u64,

    /// Địa chỉ lắng nghe; chỉ chấp nhận loopback.
    #[arg(long, default_value = server::DEFAULT_BIND)]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let token = auth::token_from_env(std::env::var(auth::TOKEN_ENV).ok())?;
    let bind = server::validate_bind(cli.bind)?;
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("không bind được {bind}"))?;

    if let Some(dir) = cli.record {
        if cli.record_delay >= 120 {
            tracing::warn!(
                delay = cli.record_delay,
                "độ trễ ≥ timeout của hook (120 s): Claude Code sẽ hết giờ trước khi hub trả lời"
            );
        }
        let app = record::router(
            RecordConfig {
                dir: dir.clone(),
                delay: Duration::from_secs(cli.record_delay),
            },
            token,
        );
        tracing::info!(%bind, dir = %dir.display(), delay_s = cli.record_delay, "hub ghi băng đang chạy");
        server::serve(listener, app)
            .await
            .context("server dừng do lỗi")?;
    } else {
        // T8 thay AlwaysAsk bằng policy thật, T6 thay NoStore bằng SQLite.
        let approvals = Arc::new(Approvals::new(
            ApprovalsConfig {
                timeout: Duration::from_secs(cli.approval_timeout),
                ..ApprovalsConfig::default()
            },
            Box::new(AlwaysAsk),
            Box::new(NoStore),
        ));
        let app = hooks::router(approvals.clone(), hostname(), token);
        tracing::info!(%bind, timeout_s = cli.approval_timeout, "hub đang chạy");
        server::serve_until(listener, app, async move {
            server::ctrl_c().await;
            approvals.shutdown();
        })
        .await
        .context("server dừng do lỗi")?;
    }
    tracing::info!("đã tắt");
    Ok(())
}

/// Tên máy cho trường `host` của Event; không có thì `"localhost"`.
fn hostname() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "localhost".to_owned())
}
