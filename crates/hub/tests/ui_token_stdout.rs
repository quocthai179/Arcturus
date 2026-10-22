//! `hub --ui-token-stdout` (T5d · ARC-44, F6 biện pháp B1): token UI được in đúng một dòng ra stdout, không ghi file,
//! log chuyển sang stderr. App (T12) sẽ khởi động hub và đọc token qua pipe, nên agent không đọc được token từ đĩa.
//!
//! Chạy binary `hub` thật.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const HOOK_TOKEN: &str = "test-token-0123456789abcdef-arcturus-t5d";

fn temp_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("arcturus-uitok-{}", ulid::Ulid::generate()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn hub(args: &[&str], dir: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_hub"));
    c.args(args)
        .arg("--config")
        .arg(dir.join("khong-co.toml"))
        .env("ARCTURUS_TOKEN", HOOK_TOKEN)
        .env("RUST_LOG", "info")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

#[test]
fn ui_token_stdout_prints_one_line_and_logs_to_stderr() {
    let dir = temp_dir();
    let mut child = hub(&["--ui-token-stdout", "--bind", "127.0.0.1:0"], &dir)
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut r = BufReader::new(stdout);
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        tx.send(line).unwrap();
        // Mọi thứ còn lại trên stdout (phải rỗng: log đi stderr).
        let mut rest = String::new();
        let _ = r.read_to_string(&mut rest);
        rest
    });
    let line = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("hub không in token trong 10 s");

    let token = line.trim_end_matches(['\r', '\n']);
    assert_eq!(token.len(), 64, "token: {token:?}");
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(
        line.ends_with('\n'),
        "đúng một dòng, kết thúc bằng xuống dòng"
    );

    // Chờ log khởi động rồi dừng hub.
    std::thread::sleep(Duration::from_millis(500));
    child.kill().unwrap();
    let _ = child.wait();

    let rest = reader.join().unwrap();
    assert!(
        rest.trim().is_empty(),
        "stdout chỉ có token, còn lại: {rest:?}"
    );
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        stderr.contains("hub đang chạy"),
        "log phải nằm ở stderr: {stderr}"
    );
    assert!(!stderr.contains(token), "không bao giờ log token UI");
    // Không có file token nào cạnh cấu hình hay trong thư mục tạm.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ui_token_stdout_conflicts_with_file_and_record() {
    let dir = temp_dir();
    let file = dir.join("ui-token");
    for args in [
        vec![
            "--ui-token-stdout",
            "--ui-token-file",
            file.to_str().unwrap(),
            "--bind",
            "127.0.0.1:0",
        ],
        vec![
            "--ui-token-stdout",
            "--record",
            dir.to_str().unwrap(),
            "--bind",
            "127.0.0.1:0",
        ],
    ] {
        let out = hub(&args, &dir).output().unwrap();
        assert!(!out.status.success(), "{args:?} phải bị từ chối");
        assert!(out.stdout.is_empty(), "không in token khi từ chối");
    }
    assert!(!file.exists(), "không được ghi file token");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn default_still_writes_token_file() {
    // Tới khi T12 khởi động hub qua pipe, mặc định vẫn ghi file (`--ui-token-file` để test không đụng %APPDATA%).
    let dir = temp_dir();
    let file = dir.join("ui-token");
    let mut child = hub(
        &[
            "--ui-token-file",
            file.to_str().unwrap(),
            "--bind",
            "127.0.0.1:0",
        ],
        &dir,
    )
    .spawn()
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !file.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "không thấy file token"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    child.kill().unwrap();
    let _ = child.wait();
    let token = std::fs::read_to_string(&file).unwrap();
    assert_eq!(token.len(), 64);
    std::fs::remove_dir_all(&dir).unwrap();
}
