//! Daemon Arcturus. Mặc định: luồng duyệt (T3) với trạng thái agent và cấu hình `arcturus.toml` (T4, T5a).
//! `--record`: chế độ ghi băng của T1.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use hub::hub::Hub;
use hub::record::{self, RecordConfig};
use hub::{auth, hooks, reconcile, server, ui_token, ws};
use hub_core::config::Config;
use hub_core::policy::danger::DangerConfig;
use hub_core::store::{self, Store};
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

    /// File cấu hình (persona, timeout duyệt, bind…). Mặc định `<config_dir>/arcturus/arcturus.toml`, ngoài thư mục làm
    /// việc của agent (T8). Không có file thì dùng mặc định.
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Địa chỉ lắng nghe, ghi đè `hub.bind` trong cấu hình; chỉ chấp nhận loopback.
    #[arg(long)]
    bind: Option<SocketAddr>,

    /// Nơi ghi token UI (sinh mới mỗi lần chạy) cho app đọc. Mặc định `<data_dir>/arcturus/ui-token`.
    #[arg(long, value_name = "FILE", conflicts_with = "record")]
    ui_token_file: Option<PathBuf>,

    /// Cơ sở dữ liệu SQLite (sự kiện, nhật ký duyệt). Mặc định `<data_dir>/arcturus/arcturus.db`.
    #[arg(long, value_name = "FILE", conflicts_with = "record")]
    db: Option<PathBuf>,

    /// In token UI đúng một dòng ra stdout thay vì ghi file (cho app khởi động hub và đọc qua pipe, F6). Log chuyển
    /// sang stderr.
    #[arg(long, conflicts_with_all = ["record", "ui_token_file"])]
    ui_token_stdout: bool,

    /// Tắt êm (như Ctrl-C) khi stdin đóng. Dành cho app khởi động hub làm tiến trình con (T12): app thoát hay crash thì
    /// pipe đóng, hub trả `{}` cho mọi yêu cầu đang chờ rồi thoát, không mồ côi giữ cổng với token UI không ai biết.
    #[arg(long, conflicts_with = "record")]
    exit_on_stdin_close: bool,
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
        // Không ghi được log (stderr/stdout gãy: app khởi động hub đã chết, terminal đã đóng) thì bỏ dòng log đó.
        // Mặc định tracing-subscriber báo lỗi bằng `eprintln!`, mà `eprintln!` panic khi stderr gãy: panic giết task đang
        // ghi log (từng giết future tắt hub, để yêu cầu đang giữ treo tới hết giờ thay vì nhận `{}`).
        .log_internal_errors(false)
        .init();

    let token = auth::token_from_env(std::env::var(auth::TOKEN_ENV).ok())?;
    let cfg_arg = match cli.config {
        Some(p) => p,
        None => default_config_path()
            .context("không tìm thấy thư mục cấu hình người dùng; chỉ định --config")?,
    };
    let cfg = Config::load(&cfg_arg)?;
    // Ghi rõ file thật sự dùng (audit T5a finding 8).
    let cfg_path = std::path::absolute(&cfg_arg).unwrap_or_else(|_| cfg_arg.clone());
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
        // Cấu hình mà agent ghi được thì agent tự nới được giới hạn (audit ARC-44 finding 7): mặc định nằm ở thư mục cấu
        // hình người dùng, và luật danger `arcturus_state` đưa mọi lệnh chạm file đó về Critical (T8).
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
        let reconcile_every = Duration::from_secs(cfg.hub.reconcile_interval_secs);
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

        let db_path = match cli.db {
            Some(p) => p,
            None => dirs::data_dir()
                .map(|d| store::default_path(&d))
                .context("không tìm thấy thư mục dữ liệu người dùng; chỉ định --db")?,
        };
        let db = Store::open(&db_path, cfg.hub.event_ttl_days)
            .with_context(|| format!("không mở được cơ sở dữ liệu {}", db_path.display()))?;
        let port = listener.local_addr().context("không đọc được cổng")?.port();
        // Luật danger biết cổng hub, file token UI và file cấu hình đang dùng (T8, F6 biện pháp B2).
        let danger = DangerConfig {
            hub_port: Some(port),
            token_paths: match &delivery {
                ui_token::Delivery::File(p) => vec![absolute_display(p)],
                ui_token::Delivery::Stdout => Vec::new(),
            },
            state_paths: vec![cfg_path.display().to_string(), absolute_display(&db_path)],
            home: dirs::home_dir().map(|h| h.display().to_string()),
        };
        let (hub, _tasks) =
            Hub::start_full(cfg, personas, hostname(), danger, Box::new(db.handle()));
        // Nạp lại grant và chuỗi duyệt từ lần chạy trước (T10). Lỗi đọc chỉ làm mất quyền tự chạy (an toàn: hỏi người).
        match store::StoreReader::open(&db_path).and_then(|r| {
            let now = time::OffsetDateTime::now_utc();
            Ok((r.trust_grants(now)?, r.streaks()?))
        }) {
            Ok((grants, streaks)) => {
                let n = hub.restore_trust(grants, streaks);
                if n > 0 {
                    tracing::info!(n, "đã nạp lại trust grant");
                }
            }
            Err(err) => tracing::warn!(%err, "không nạp lại được trust grant"),
        }
        // Nạp lại batch rule còn hạn từ lần chạy trước (T9). Lỗi đọc chỉ làm mất quy tắc (an toàn: mọi thứ hỏi người).
        match store::StoreReader::open(&db_path)
            .and_then(|r| r.batch_rules(time::OffsetDateTime::now_utc()))
        {
            Ok(rows) => {
                let n = hub.restore_batch(rows);
                if n > 0 {
                    tracing::info!(n, "đã nạp lại batch rule");
                }
            }
            Err(err) => tracing::warn!(%err, "không nạp lại được batch rule"),
        }
        // Đối soát phiên (T7): chỉ chạy `claude agents --json` khi hub đang có phiên.
        let _reconcile = reconcile::spawn(
            Arc::downgrade(&hub),
            reconcile::ClaudeAgents::default(),
            reconcile_every,
        );
        hub.set_db_path(db_path.clone());
        let (ui, ws_handle) = ws::router(hub.clone(), ui_token, port, ws::WsConfig::default());
        let app = hooks::router_on_port(hub.clone(), token, port).merge(ui);
        tracing::info!(
            %bind,
            timeout_s,
            personas = n_personas,
            config = %cfg_path.display(),
            config_found = cfg_found,
            ui_token = %ui_token_dest,
            db = %db_path.display(),
            "hub đang chạy"
        );
        let watch_stdin = cli.exit_on_stdin_close;
        server::serve_until(listener, app, async move {
            tokio::select! {
                () = server::ctrl_c() => {}
                () = server::stdin_closed(), if watch_stdin => {
                    tracing::info!("stdin đã đóng (app thoát): tắt hub");
                }
            }
            hub.shutdown();
            ws_handle.shutdown();
        })
        .await
        .context("server dừng do lỗi")?;
        // Ghi nốt các bản ghi đang chờ (gồm `cancelled` của yêu cầu bị hủy khi tắt) rồi dừng thread ghi.
        db.close();
    }
    tracing::info!("đã tắt");
    Ok(())
}

/// `<config_dir>/arcturus/arcturus.toml` (Linux `~/.config`, macOS `~/Library/Application Support`, Windows
/// `%APPDATA%`).
fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("arcturus").join("arcturus.toml"))
}

fn absolute_display(p: &std::path::Path) -> String {
    std::path::absolute(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .display()
        .to_string()
}

/// Tên máy cho trường `host` của Event; không có thì `"localhost"`.
fn hostname() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "localhost".to_owned())
}
