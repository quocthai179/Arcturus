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
use tracing_subscriber::fmt::writer::BoxMakeWriter;
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

    /// In token UI đúng một dòng ra stdout thay vì ghi file (cho app khởi động hub và đọc qua pipe, F6). Log chuyển
    /// sang stderr.
    #[arg(long, conflicts_with_all = ["record", "ui_token_file"])]
    ui_token_stdout: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // `--ui-token-stdout`: stdout chỉ dành cho token, log sang stderr.
    let writer = if cli.ui_token_stdout {
        BoxMakeWriter::new(std::io::stderr)
    } else {
        BoxMakeWriter::new(std::io::stdout)
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(writer)
        .init();

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
        // Cấu hình nằm trong thư mục làm việc, agent ghi được workspace thì sửa được (audit ARC-44 finding 7).
        if cfg.hub.min_review_ms == 0 {
            tracing::warn!("min_review_ms = 0: tắt sàn thời gian duyệt (F6)");
        }
        if cfg.hub.held_input_max_bytes > 32 * 1024 * 1024 {
            tracing::warn!(
                held_input_max_bytes = cfg.hub.held_input_max_bytes,
                "trần byte đang giữ > 32 MiB: hub có thể vượt ngân sách 40 MB (quy tắc bất biến 6)"
            );
        }
        let timeout_s = cfg.hub.approval_timeout_secs;
        let n_personas = cfg.personas.len();
        // Token UI riêng (D1): sinh mới, ghi vào file chỉ người dùng đọc được; không bao giờ log giá trị.
        let ui_token =
            ui_token::generate().map_err(|e| anyhow::anyhow!("không sinh được token UI: {e}"))?;
        let delivery = if cli.ui_token_stdout {
            ui_token::Delivery::Stdout
        } else {
            ui_token::Delivery::File(match cli.ui_token_file {
                Some(p) => p,
                None => ui_token::default_path().context(
                    "không tìm thấy thư mục dữ liệu người dùng; chỉ định --ui-token-file",
                )?,
            })
        };
        let ui_token_dest = match &delivery {
            ui_token::Delivery::File(p) => p.display().to_string(),
            ui_token::Delivery::Stdout => "stdout".to_owned(),
        };
        ui_token::deliver(&delivery, &ui_token, &mut std::io::stdout().lock())
            .with_context(|| format!("không trao được token UI ({ui_token_dest})"))?;

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
            ui_token = %ui_token_dest,
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
