//! Phát lại fixture hook vào hub. T0 chỉ là khung: đếm fixture; gửi HTTP được thêm ở T15.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fixtures/hooks"));
    match count_json_files(&dir) {
        Ok(n) => {
            println!(
                "replay: {n} fixture trong {} (chưa gửi tới hub, xem T15)",
                dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("replay: không đọc được {}: {e}", dir.display());
            ExitCode::FAILURE
        }
    }
}

/// Đếm file `.json` trong `dir` và các thư mục con. Thư mục chưa tồn tại thì trả 0.
fn count_json_files(dir: &Path) -> std::io::Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    let mut n = 0;
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            n += count_json_files(&path)?;
        } else if path.extension().is_some_and(|e| e == "json") {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_dir_counts_zero() {
        assert_eq!(count_json_files(Path::new("không-tồn-tại-xyz")).unwrap(), 0);
    }

    #[test]
    fn counts_json_recursively() {
        let root = std::env::temp_dir().join(format!("replay-test-{}", std::process::id()));
        let sub = root.join("PreToolUse");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(root.join("a.json"), "{}").unwrap();
        std::fs::write(sub.join("b.json"), "{}").unwrap();
        std::fs::write(sub.join("note.txt"), "x").unwrap();
        let n = count_json_files(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(n, 2);
    }
}
