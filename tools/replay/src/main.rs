//! Công cụ fixture hook cho Arcturus.
//!
//! - `replay [DIR] --sessions N --rate R --duration D`: phát lại fixture vào hub đang chạy (T15), in p50/p95/p99.
//! - `replay [DIR] --dry-run`: chỉ đếm fixture.
//! - `replay scrub <IN> <OUT>`: làm sạch JSON đã ghi băng (T1) trước khi đưa vào `fixtures/hooks/`.

mod scrub;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use replay::files::{count_json_files, json_files};
use replay::send::{self, PermissionMode};
use scrub::Scrubber;

#[derive(Parser, Debug)]
#[command(name = "replay", version, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Thư mục fixture (mặc định fixtures/hooks).
    dir: Option<PathBuf>,

    /// Gốc HTTP của hub.
    #[arg(long, default_value = "http://127.0.0.1:7878")]
    url: String,

    /// Số phiên giả (`replay-0`, `replay-1`, …).
    #[arg(long, default_value_t = 1)]
    sessions: usize,

    /// Số yêu cầu mỗi giây (chia đều cho các phiên).
    #[arg(long, default_value_t = 20.0)]
    rate: f64,

    /// Thời lượng gửi (giây).
    #[arg(long, default_value_t = 5.0)]
    duration: f64,

    /// `allow`: gửi cả PermissionRequest và duyệt allow qua WebSocket (cần token UI); `skip`: chỉ gửi sự kiện.
    #[arg(long, value_enum, default_value_t = PermissionMode::Skip)]
    permission_mode: PermissionMode,

    /// Chờ bao lâu (ms) rồi mới duyệt allow; phải lớn hơn `min_review_ms` của hub.
    #[arg(long, default_value_t = 1100)]
    allow_delay_ms: u64,

    /// Thoát với mã ≠ 0 nếu p99 của /hooks/event vượt ngưỡng (ms), hoặc có lỗi.
    #[arg(long)]
    assert_p99_ms: Option<f64>,

    /// Token hook của hub.
    #[arg(long, env = "ARCTURUS_TOKEN", hide_env_values = true)]
    hook_token: Option<String>,

    /// Token UI (chỉ cần với `--permission-mode allow`).
    #[arg(long, env = "ARCTURUS_UI_TOKEN", hide_env_values = true)]
    ui_token: Option<String>,

    /// Đọc token UI từ file (ví dụ file do `hub --ui-token-file` ghi).
    #[arg(long)]
    ui_token_file: Option<PathBuf>,

    /// Chỉ đếm fixture, không gửi gì.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Làm sạch JSON hook: thay thư mục home, xóa token/khóa API. Giữ nguyên cấu trúc thư mục.
    Scrub {
        /// Thư mục ghi băng (đầu ra của `hub --record`).
        input: PathBuf,
        /// Thư mục đích (ví dụ fixtures/hooks). Không được trùng, nằm trong, hoặc chứa IN.
        output: PathBuf,
        /// Thư mục home cần thay (mặc định USERPROFILE hoặc HOME).
        #[arg(long)]
        home: Option<String>,
        /// Tên người dùng cần thay thêm (lặp được; mặc định USERNAME hoặc USER).
        #[arg(long)]
        username: Vec<String>,
        /// Biến môi trường chứa giá trị cần xóa (lặp được). Mặc định: token hub và tên máy.
        #[arg(long = "secret-env", default_values_t = ["ARCTURUS_TOKEN".to_owned(), "COMPUTERNAME".to_owned(), "HOSTNAME".to_owned()])]
        secret_env: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Scrub {
            input,
            output,
            home,
            username,
            secret_env,
        }) => {
            let home = home
                .or_else(|| std::env::var("USERPROFILE").ok())
                .or_else(|| std::env::var("HOME").ok());
            let usernames = if username.is_empty() {
                ["USERNAME", "USER"]
                    .iter()
                    .filter_map(|k| std::env::var(k).ok())
                    .collect()
            } else {
                username
            };
            let secrets = secret_env.iter().filter_map(|k| std::env::var(k).ok());
            let scrubber = Scrubber::new(home.as_deref(), usernames, secrets);
            let n = scrub_dir(&scrubber, &input, &output)?;
            println!(
                "scrub: {n} file từ {} → {}",
                input.display(),
                output.display()
            );
        }
        None => {
            let dir = cli
                .dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("fixtures/hooks"));
            if cli.dry_run {
                let n = count_json_files(&dir)
                    .with_context(|| format!("không đọc được {}", dir.display()))?;
                println!("replay: {n} fixture trong {}", dir.display());
                return Ok(());
            }
            let opts = send_options(&cli, dir)?;
            println!(
                "replay: {} phiên, {} yêu cầu/giây, {} giây, tới {} (permission-mode {:?})",
                opts.sessions,
                opts.rate,
                opts.duration.as_secs_f64(),
                opts.url,
                opts.permission_mode
            );
            let report = send::run(opts).await?;
            print!("{}", report.summary());
            if let Err(why) = report.check(cli.assert_p99_ms) {
                bail!("{why}");
            }
        }
    }
    Ok(())
}

/// Gom tùy chọn dòng lệnh thành [`send::Options`]: token hook bắt buộc; token UI từ cờ, biến môi trường hoặc file.
fn send_options(cli: &Cli, dir: PathBuf) -> anyhow::Result<send::Options> {
    let hook_token = cli
        .hook_token
        .clone()
        .context("thiếu token hook: đặt ARCTURUS_TOKEN hoặc --hook-token")?;
    let ui_token = match (&cli.ui_token, &cli.ui_token_file) {
        (Some(t), _) => Some(t.clone()),
        (None, Some(f)) => Some(
            std::fs::read_to_string(f)
                .with_context(|| format!("không đọc được {}", f.display()))?
                .trim()
                .to_owned(),
        ),
        (None, None) => None,
    };
    if !(cli.duration.is_finite() && cli.duration > 0.0) {
        bail!("--duration phải > 0");
    }
    Ok(send::Options {
        dir,
        url: cli.url.trim_end_matches('/').to_owned(),
        sessions: cli.sessions,
        rate: cli.rate,
        duration: Duration::from_secs_f64(cli.duration),
        permission_mode: cli.permission_mode,
        allow_delay: Duration::from_millis(cli.allow_delay_ms),
        hook_token,
        ui_token,
    })
}

/// Làm sạch mọi `.json` trong `input`, ghi JSON định dạng đẹp vào `output` với cùng đường dẫn tương đối.
/// File không phải JSON hợp lệ làm cả lệnh dừng (không lặng lẽ chép file chưa làm sạch).
fn scrub_dir(scrubber: &Scrubber, input: &Path, output: &Path) -> anyhow::Result<usize> {
    let input_abs = std::fs::canonicalize(input)
        .with_context(|| format!("không mở được {}", input.display()))?;
    std::fs::create_dir_all(output)
        .with_context(|| format!("không tạo được {}", output.display()))?;
    let output_abs = std::fs::canonicalize(output)?;
    // Hai chiều: OUT trong IN thì đọc lại chính đầu ra; IN trong OUT thì bản ghi thô nằm lẫn trong fixture.
    if output_abs.starts_with(&input_abs) || input_abs.starts_with(&output_abs) {
        bail!(
            "IN ({}) và OUT ({}) không được trùng hoặc lồng nhau",
            input.display(),
            output.display()
        );
    }

    let files = json_files(&input_abs)?;
    for rel in &files {
        let src = input_abs.join(rel);
        let text = std::fs::read_to_string(&src)
            .with_context(|| format!("không đọc được {}", src.display()))?;
        let mut value: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("{} không phải JSON hợp lệ", src.display()))?;
        scrubber.scrub_value(&mut value);
        let dst = output_abs.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut pretty = serde_json::to_string_pretty(&value)?;
        pretty.push('\n');
        std::fs::write(&dst, pretty)
            .with_context(|| format!("không ghi được {}", dst.display()))?;
    }
    Ok(files.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "replay-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Kiểm bộ fixture đã commit: tên thư mục khớp `hook_event_name`, đủ số lượng, không còn dữ liệu nhạy cảm.
    #[test]
    fn committed_fixtures_are_consistent_and_scrubbed() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/hooks");
        let files = json_files(&root).unwrap();
        assert!(files.len() >= 30, "T1 cần ≥30 fixture, có {}", files.len());

        // Scrub lại bằng thông tin của máy đang chạy test (home, tên người dùng, tên máy): phải không đổi gì.
        let env = |k: &str| std::env::var(k).ok();
        let home = env("USERPROFILE").or_else(|| env("HOME"));
        let users: Vec<String> = ["USERNAME", "USER"].iter().filter_map(|k| env(k)).collect();
        let secrets: Vec<String> = ["ARCTURUS_TOKEN", "COMPUTERNAME", "HOSTNAME"]
            .iter()
            .filter_map(|k| env(k))
            .collect();
        let scrubber = Scrubber::new(home.as_deref(), users, secrets);

        for rel in &files {
            let text = std::fs::read_to_string(root.join(rel)).unwrap();
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            let dir = rel
                .components()
                .next()
                .unwrap()
                .as_os_str()
                .to_string_lossy();
            assert_eq!(v["hook_event_name"], *dir, "{rel:?}");

            let mut again = v.clone();
            scrubber.scrub_value(&mut again);
            assert_eq!(
                again, v,
                "{rel:?}: scrub lại vẫn thay đổi, fixture còn dữ liệu nhạy cảm"
            );

            let mut strings = Vec::new();
            collect_strings(&v, &mut strings);
            for s in strings {
                let lower = s.to_ascii_lowercase();
                for pat in ["\\users\\", "/users/", "-users-", "/home/"] {
                    for (i, _) in lower.match_indices(pat) {
                        assert!(
                            lower[i + pat.len()..]
                                .trim_start_matches(['\\', '/'])
                                .starts_with("user"),
                            "{rel:?}: đường dẫn home chưa scrub: {s}"
                        );
                    }
                }
                assert!(!looks_like_email(s), "{rel:?}: có địa chỉ email: {s}");
            }
        }
    }

    /// Duyệt cả khóa lẫn giá trị chuỗi.
    fn collect_strings<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a str>) {
        match v {
            serde_json::Value::String(s) => out.push(s),
            serde_json::Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    out.push(k);
                    collect_strings(x, out);
                }
            }
            _ => {}
        }
    }

    fn looks_like_email(s: &str) -> bool {
        s.split(|c: char| c.is_whitespace() || "<>()\"',;`".contains(c))
            .any(|w| match w.split_once('@') {
                Some((user, domain)) => {
                    !user.is_empty()
                        && user
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
                        && domain.contains('.')
                        && domain
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
                }
                None => false,
            })
    }

    #[test]
    fn email_detector() {
        assert!(looks_like_email("liên hệ a.b+c@example.com nhé"));
        assert!(!looks_like_email("git@ không phải"));
        assert!(!looks_like_email("@scope/pkg"));
    }

    #[test]
    fn scrub_redacts_token_and_hostname_by_default() {
        let cli = Cli::try_parse_from(["replay", "scrub", "in", "out"]).unwrap();
        let Some(Command::Scrub { secret_env, .. }) = cli.command else {
            panic!("phải là lệnh scrub");
        };
        for var in ["ARCTURUS_TOKEN", "COMPUTERNAME", "HOSTNAME"] {
            assert!(secret_env.iter().any(|v| v == var), "thiếu {var}");
        }
    }

    #[test]
    fn missing_dir_counts_zero() {
        assert_eq!(count_json_files(Path::new("không-tồn-tại-xyz")).unwrap(), 0);
    }

    #[test]
    fn counts_json_recursively() {
        let root = temp_dir("count");
        let sub = root.join("PreToolUse");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(root.join("a.json"), "{}").unwrap();
        std::fs::write(sub.join("b.json"), "{}").unwrap();
        std::fs::write(sub.join("note.txt"), "x").unwrap();
        let n = count_json_files(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn scrub_dir_keeps_layout_and_cleans_values() {
        let input = temp_dir("in");
        let output = temp_dir("out");
        std::fs::create_dir_all(input.join("SessionStart")).unwrap();
        std::fs::write(
            input.join("SessionStart").join("01.json"),
            r#"{"hook_event_name":"SessionStart","cwd":"C:\\Users\\PC\\x","token":"t"}"#,
        )
        .unwrap();

        let s = Scrubber::new(Some("C:\\Users\\PC"), Vec::new(), Vec::new());
        let n = scrub_dir(&s, &input, &output).unwrap();

        let out = std::fs::read_to_string(output.join("SessionStart").join("01.json")).unwrap();
        std::fs::remove_dir_all(&input).unwrap();
        std::fs::remove_dir_all(&output).unwrap();
        assert_eq!(n, 1);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["cwd"], "C:\\Users\\user\\x");
        assert_eq!(v["token"], scrub::REDACTED);
        assert_eq!(v["hook_event_name"], "SessionStart");
    }

    #[test]
    fn scrub_dir_rejects_invalid_json() {
        let input = temp_dir("bad");
        let output = temp_dir("badout");
        std::fs::write(input.join("x.json"), "{hỏng").unwrap();
        let err = scrub_dir(
            &Scrubber::new(None, Vec::new(), Vec::new()),
            &input,
            &output,
        );
        std::fs::remove_dir_all(&input).unwrap();
        std::fs::remove_dir_all(&output).unwrap();
        assert!(err.is_err());
    }

    #[test]
    fn scrub_dir_rejects_output_inside_input() {
        let input = temp_dir("nest");
        let err = scrub_dir(
            &Scrubber::new(None, Vec::new(), Vec::new()),
            &input,
            &input.join("out"),
        );
        std::fs::remove_dir_all(&input).unwrap();
        assert!(err.is_err());
    }

    #[test]
    fn scrub_dir_rejects_input_inside_output() {
        let output = temp_dir("outer");
        let input = output.join("raw");
        std::fs::create_dir_all(&input).unwrap();
        let err = scrub_dir(
            &Scrubber::new(None, Vec::new(), Vec::new()),
            &input,
            &output,
        );
        std::fs::remove_dir_all(&output).unwrap();
        assert!(
            err.is_err(),
            "bản ghi thô không được nằm trong thư mục fixture"
        );
    }
}
