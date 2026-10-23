//! `hub --ui-token-stdout` (T5d · ARC-44, F6 biện pháp B1): token UI được in đúng một dòng ra stdout, không ghi file,
//! log chuyển sang stderr. App (T12) khởi động hub và đọc token qua pipe, nên agent không đọc được token từ đĩa.
//!
//! Chạy binary `hub` thật.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Read, Write};
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
    let db_dir = temp_dir();
    let db = db_dir.join("arcturus.db");
    let mut child = hub(
        &[
            "--ui-token-stdout",
            "--bind",
            "127.0.0.1:0",
            "--db",
            db.to_str().unwrap(),
        ],
        &dir,
    )
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
    let _ = std::fs::remove_dir_all(&db_dir);
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
    // Hub chạy riêng (không do app khởi động) vẫn ghi file (`--ui-token-file` để test không đụng %APPDATA%).
    let dir = temp_dir();
    let file = dir.join("ui-token");
    let db = dir.join("arcturus.db");
    let mut child = hub(
        &[
            "--ui-token-file",
            file.to_str().unwrap(),
            "--bind",
            "127.0.0.1:0",
            "--db",
            db.to_str().unwrap(),
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

/// Cổng trống (có thể bị lấy mất giữa lúc đóng và lúc hub bind, hiếm).
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// POST `/hooks/permission` bằng socket thô; trả (mã trạng thái, body).
fn post_permission(port: u16) -> (u16, String) {
    let body = r#"{"hook_event_name":"PermissionRequest","session_id":"s1","cwd":"/w","tool_name":"Bash","tool_input":{"command":"ls","description":"d"}}"#;
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    write!(
        s,
        "POST /hooks/permission HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {HOOK_TOKEN}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut resp = String::new();
    s.read_to_string(&mut resp).unwrap();
    let status = resp
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = resp.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    (status, body)
}

/// T12: app khởi động hub với `--exit-on-stdin-close`; app thoát (pipe stdin đóng) thì hub tắt êm như Ctrl-C: yêu cầu
/// đang giữ nhận `{}` (không bao giờ allow), tiến trình thoát với mã 0.
#[test]
fn exit_on_stdin_close_shuts_down_gracefully() {
    let dir = temp_dir();
    let db = dir.join("arcturus.db");
    let port = free_port();
    let bind = format!("127.0.0.1:{port}");
    let mut child = hub(
        &[
            "--ui-token-stdout",
            "--exit-on-stdin-close",
            "--bind",
            &bind,
            "--db",
            db.to_str().unwrap(),
        ],
        &dir,
    )
    .stdin(Stdio::piped())
    .spawn()
    .unwrap();
    let stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut token = String::new();
    out.read_line(&mut token).unwrap();
    assert_eq!(token.trim_end().len(), 64);
    let stderr = child.stderr.take().unwrap();
    let logs = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut s);
        s
    });

    let held = std::thread::spawn(move || post_permission(port));
    // Đợi yêu cầu vào hàng đợi (dữ liệu vào stdin không làm hub tắt), rồi đóng stdin như khi app thoát.
    std::thread::sleep(Duration::from_millis(800));
    let mut stdin = stdin;
    stdin.write_all(b"ping\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        child.try_wait().unwrap().is_none(),
        "dữ liệu trên stdin không được làm hub tắt"
    );
    drop(stdin);

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "hub không tắt sau khi stdin đóng"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "{status:?}");
    let (code, body) = held.join().unwrap();
    assert_eq!((code, body.trim()), (200, "{}"));
    let logs = logs.join().unwrap();
    assert!(logs.contains("stdin đã đóng"), "{logs}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Hồi quy T12: stderr của hub bị đóng trước (app crash: không còn ai đọc pipe), rồi stdin đóng. tracing-subscriber
/// báo lỗi ghi log bằng `eprintln!`, và `eprintln!` panic khi stderr gãy: panic từng giết future tắt hub, yêu cầu đang
/// giữ treo tới hết giờ. Hub phải vẫn trả `{}` và thoát.
#[test]
fn broken_stderr_does_not_block_shutdown() {
    let dir = temp_dir();
    let db = dir.join("arcturus.db");
    let port = free_port();
    let bind = format!("127.0.0.1:{port}");
    let mut child = hub(
        &[
            "--ui-token-stdout",
            "--exit-on-stdin-close",
            "--bind",
            &bind,
            "--db",
            db.to_str().unwrap(),
        ],
        &dir,
    )
    .stdin(Stdio::piped())
    .spawn()
    .unwrap();
    let stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut token = String::new();
    out.read_line(&mut token).unwrap();
    let held = std::thread::spawn(move || post_permission(port));
    std::thread::sleep(Duration::from_millis(800));
    // Như app vừa chết: không còn ai đọc stderr, rồi pipe stdin đóng.
    drop(child.stderr.take());
    drop(stdin);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("hub không tắt khi stderr gãy và stdin đóng");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "{status:?}");
    let (code, body) = held.join().unwrap();
    assert_eq!((code, body.trim()), (200, "{}"));
    std::fs::remove_dir_all(&dir).unwrap();
}
