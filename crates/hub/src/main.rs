//! Daemon Arcturus. Mặc định: luồng duyệt (T3) với trạng thái agent và cấu hình `arcturus.toml` (T4, T5a).
//! `--record`: chế độ ghi băng của T1.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use hub::hub::Hub;
use hub::record::{self, RecordConfig};
use hub::{auth, hooks, server, ui_token, ws};
use hub_core::config::Config;
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

    /// File cấu hình (persona, timeout duyệt, bind…). Không có file thì dùng mặc định.
    #[arg(long, value_name = "FILE", default_value = "arcturus.toml")]
    config: PathBuf,

    /// Địa chỉ lắng nghe, ghi đè `hub.bind` trong cấu hình; chỉ chấp nhận loopback.
    #[arg(long)]
    bind: Option<SocketAddr>,

    /// Nơi ghi token UI (sinh mới mỗi lần chạy) cho app đọc. Mặc định `<data_dir>/arcturus/ui-token`.
    #[arg(long, value_name = "FILE", conflicts_with = "record")]
    ui_token_file: Option<PathBuf>,
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
    let cfg = Config::load(&cli.config)?;
    // Ghi rõ file thật sự dùng: cấu hình mặc định đọc từ thư mục hiện tại (audit T5a finding 8).
    let cfg_path = std::path::absolute(&cli.config).unwrap_or_else(|_| cli.config.clone());
    let cfg_found = cfg_path.is_file();
    let bind = match cli.bind {
        Some(b) => b,
        None => cfg
            .bind_addr()
            .context("hub.bind không hợp lệ (đã kiểm khi đọc cấu hình)")?,
    };
    let bind = server::validate_bind(bind)?;
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
        let personas = cfg
            .persona_book()
            .context("persona trong cấu hình không hợp lệ")?;
        let timeout_s = cfg.hub.approval_timeout_secs;
        let n_personas = cfg.personas.len();
        // Token UI riêng (D1): sinh mới, ghi vào file chỉ người dùng đọc được; không bao giờ log giá trị.
        let ui_token =
            ui_token::generate().map_err(|e| anyhow::anyhow!("không sinh được token UI: {e}"))?;
        let ui_token_path = match cli.ui_token_file {
            Some(p) => p,
            None => ui_token::default_path()
                .context("không tìm thấy thư mục dữ liệu người dùng; chỉ định --ui-token-file")?,
        };
        ui_token::write(&ui_token_path, &ui_token)
            .with_context(|| format!("không ghi được token UI vào {}", ui_token_path.display()))?;

        let (hub, _tasks) = Hub::start(cfg, personas, hostname());
        let port = listener.local_addr().context("không đọc được cổng")?.port();
        let (ui, ws_handle) = ws::router(hub.clone(), ui_token, port, ws::WsConfig::default());
        let app = hooks::router(hub.clone(), token).merge(ui);
        tracing::info!(
            %bind,
            timeout_s,
            personas = n_personas,
            config = %cfg_path.display(),
            config_found = cfg_found,
            ui_token_file = %ui_token_path.display(),
            "hub đang chạy"
        );
        server::serve_until(listener, app, async move {
            server::ctrl_c().await;
            hub.shutdown();
            ws_handle.shutdown();
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
