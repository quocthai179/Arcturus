//! Công cụ fixture hook cho Arcturus.
//!
//! - `replay [DIR]`: đếm fixture; gửi tới hub được thêm ở T15.
//! - `replay scrub <IN> <OUT>`: làm sạch JSON đã ghi băng (T1) trước khi đưa vào `fixtures/hooks/`.

mod scrub;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use scrub::Scrubber;

#[derive(Parser, Debug)]
#[command(name = "replay", version, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Thư mục fixture (mặc định fixtures/hooks).
    dir: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Làm sạch JSON hook: thay thư mục home, xóa token/khóa API. Giữ nguyên cấu trúc thư mục.
    Scrub {
        /// Thư mục ghi băng (đầu ra của `hub --record`).
        input: PathBuf,
        /// Thư mục đích (ví dụ fixtures/hooks). Không được trùng hoặc nằm trong IN.
        output: PathBuf,
        /// Thư mục home cần thay (mặc định USERPROFILE hoặc HOME).
        #[arg(long)]
        home: Option<String>,
        /// Biến môi trường chứa giá trị cần xóa (lặp được). Mặc định: token hub và tên máy.
        #[arg(long = "secret-env", default_values_t = ["ARCTURUS_TOKEN".to_owned(), "COMPUTERNAME".to_owned(), "HOSTNAME".to_owned()])]
        secret_env: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Scrub {
            input,
            output,
            home,
            secret_env,
        }) => {
            let home = home
                .or_else(|| std::env::var("USERPROFILE").ok())
                .or_else(|| std::env::var("HOME").ok());
            let secrets = secret_env.iter().filter_map(|k| std::env::var(k).ok());
            let scrubber = Scrubber::new(home.as_deref(), secrets);
            let n = scrub_dir(&scrubber, &input, &output)?;
            println!(
                "scrub: {n} file từ {} → {}",
                input.display(),
                output.display()
            );
        }
        None => {
            let dir = cli.dir.unwrap_or_else(|| PathBuf::from("fixtures/hooks"));
            let n = count_json_files(&dir)
                .with_context(|| format!("không đọc được {}", dir.display()))?;
            println!(
                "replay: {n} fixture trong {} (chưa gửi tới hub, xem T15)",
                dir.display()
            );
        }
    }
    Ok(())
}

/// Đếm file `.json` trong `dir` và các thư mục con. Thư mục chưa tồn tại thì trả 0.
fn count_json_files(dir: &Path) -> std::io::Result<usize> {
    Ok(json_files(dir)?.len())
}

/// Danh sách file `.json` (đường dẫn tương đối so với `dir`), sắp xếp. Thư mục chưa tồn tại thì rỗng.
fn json_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                if let Ok(rel) = path.strip_prefix(dir) {
                    out.push(rel.to_path_buf());
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Làm sạch mọi `.json` trong `input`, ghi JSON định dạng đẹp vào `output` với cùng đường dẫn tương đối.
/// File không phải JSON hợp lệ làm cả lệnh dừng (không lặng lẽ chép file chưa làm sạch).
fn scrub_dir(scrubber: &Scrubber, input: &Path, output: &Path) -> anyhow::Result<usize> {
    let input_abs = std::fs::canonicalize(input)
        .with_context(|| format!("không mở được {}", input.display()))?;
    std::fs::create_dir_all(output)
        .with_context(|| format!("không tạo được {}", output.display()))?;
    let output_abs = std::fs::canonicalize(output)?;
    if output_abs.starts_with(&input_abs) {
        bail!(
            "OUT ({}) không được nằm trong IN ({})",
            output.display(),
            input.display()
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

    /// Kiểm bộ fixture đã commit: tên thư mục khớp `hook_event_name`, đủ số lượng, home đã được scrub.
    #[test]
    fn committed_fixtures_are_consistent_and_scrubbed() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/hooks");
        let files = json_files(&root).unwrap();
        assert!(files.len() >= 30, "T1 cần ≥30 fixture, có {}", files.len());
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
            let mut strings = Vec::new();
            collect_strings(&v, &mut strings);
            for s in strings {
                for pat in ["\\Users\\", "/Users/", "C--Users-", "/home/"] {
                    for (i, _) in s.match_indices(pat) {
                        assert!(
                            s[i + pat.len()..].starts_with("user"),
                            "{rel:?}: đường dẫn home chưa scrub: {s}"
                        );
                    }
                }
            }
        }
    }

    fn collect_strings<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a str>) {
        match v {
            serde_json::Value::String(s) => out.push(s),
            serde_json::Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
            serde_json::Value::Object(m) => m.values().for_each(|x| collect_strings(x, out)),
            _ => {}
        }
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

        let s = Scrubber::new(Some("C:\\Users\\PC"), Vec::new());
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
        let err = scrub_dir(&Scrubber::new(None, Vec::new()), &input, &output);
        std::fs::remove_dir_all(&input).unwrap();
        std::fs::remove_dir_all(&output).unwrap();
        assert!(err.is_err());
    }

    #[test]
    fn scrub_dir_rejects_output_inside_input() {
        let input = temp_dir("nest");
        let err = scrub_dir(&Scrubber::new(None, Vec::new()), &input, &input.join("out"));
        std::fs::remove_dir_all(&input).unwrap();
        assert!(err.is_err());
    }
}
