//! Kết nối tới hub (docs/PLAN.md T12).
//!
//! - Hub đã chạy sẵn (người dùng tự khởi động): đọc token UI từ file như T5b.
//! - Chưa có: app khởi động hub làm tiến trình con với `--ui-token-stdout --exit-on-stdin-close`, đọc token từ dòng đầu
//!   stdout (không có file token trên đĩa, F6 biện pháp B1). App giữ pipe stdin của hub: app thoát hay crash thì pipe
//!   đóng và hub tự tắt êm (trả `{}` cho mọi yêu cầu đang chờ). stderr của hub luôn được đọc (pipe đầy thì log chặn
//!   luồng của hub, audit ARC-44 finding 5) và chuyển ra stderr của app.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::http;

/// Chờ hub in token tối đa chừng này.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(15);

/// Giữ vài dòng stderr cuối của hub để báo lỗi khởi động cho người dùng.
const STDERR_TAIL: usize = 8;

/// Thông tin cho webview (`invoke('hub_info')`) và kết nối nhẹ phía Rust.
#[derive(Clone)]
pub struct HubInfo {
    pub addr: SocketAddr,
    pub token: String,
}

impl HubInfo {
    pub fn ws_url(&self) -> String {
        format!("ws://{}/ws", self.addr)
    }
}

/// Hub do app khởi động. Drop: đóng stdin (hub tự tắt êm) rồi chờ tối đa vài giây.
pub struct HubChild {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl HubChild {
    /// Tiến trình hub còn chạy.
    pub fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn stop(&mut self, wait: Duration) {
        drop(self.stdin.take());
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        // Không tắt kịp: buộc dừng (lỗi kết nối với Claude Code là lỗi không chặn, terminal vẫn hỏi).
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for HubChild {
    fn drop(&mut self) {
        self.stop(Duration::from_secs(3));
    }
}

/// Địa chỉ hub: `ARCTURUS_HUB_ADDR` (chỉ loopback) hoặc mặc định.
pub fn hub_addr() -> SocketAddr {
    let default = SocketAddr::from(([127, 0, 0, 1], 7878));
    std::env::var("ARCTURUS_HUB_ADDR")
        .ok()
        .and_then(|v| v.parse::<SocketAddr>().ok())
        .filter(|a| a.ip().is_loopback())
        .unwrap_or(default)
}

pub fn healthy(addr: SocketAddr) -> bool {
    http::get(addr, "/healthz", None, Duration::from_millis(500)).is_ok_and(|r| r.status == 200)
}

/// Token UI hợp lệ: 64 ký tự hex (hub sinh 32 byte ngẫu nhiên).
pub fn valid_token(t: &str) -> bool {
    t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Kết nối tới hub đang chạy, hoặc khởi động hub mới. `Some(child)` khi app sở hữu tiến trình hub. `hook_token`: token
/// hook do app quản lý (`setup::hook_token`), đưa cho hub con qua `ARCTURUS_TOKEN`; `None` thì hub con thừa hưởng biến
/// môi trường của app như trước.
pub fn connect(hook_token: Option<&str>) -> Result<(HubInfo, Option<HubChild>), String> {
    let addr = hub_addr();
    if healthy(addr) {
        let token = read_token_file()?;
        return Ok((HubInfo { addr, token }, None));
    }
    let (token, child) = spawn(addr, hook_token)?;
    // Hub in token trước khi phục vụ; chờ nó nhận kết nối (chỉ lúc khởi động, không polling khi chạy).
    let deadline = Instant::now() + Duration::from_secs(5);
    while !healthy(addr) {
        if Instant::now() >= deadline {
            return Err(format!("hub không trả lời ở {addr}"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok((HubInfo { addr, token }, Some(child)))
}

fn token_file() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ARCTURUS_UI_TOKEN_FILE") {
        return Some(PathBuf::from(p));
    }
    dirs::data_dir().map(|d| d.join("arcturus").join("ui-token"))
}

fn read_token_file() -> Result<String, String> {
    let path = token_file().ok_or("không tìm thấy thư mục dữ liệu người dùng")?;
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "hub đang chạy nhưng không đọc được token UI ở {}: {e}",
            path.display()
        )
    })?;
    let token = text.trim().to_owned();
    if !valid_token(&token) {
        return Err(format!("token UI ở {} không hợp lệ", path.display()));
    }
    Ok(token)
}

/// Binary hub: `ARCTURUS_HUB_BIN`, cạnh binary app, bản build trong repo (khi phát triển), rồi `PATH`.
fn hub_binary() -> PathBuf {
    if let Ok(p) = std::env::var("ARCTURUS_HUB_BIN") {
        return PathBuf::from(p);
    }
    let name = if cfg!(windows) { "hub.exe" } else { "hub" };
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    {
        let p = dir.join(name);
        if p.is_file() {
            return p;
        }
    }
    if cfg!(debug_assertions) {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target");
        for profile in ["debug", "release"] {
            let p = repo.join(profile).join(name);
            if p.is_file() {
                return p;
            }
        }
    }
    PathBuf::from(name)
}

fn spawn(addr: SocketAddr, hook_token: Option<&str>) -> Result<(String, HubChild), String> {
    let bin = hub_binary();
    let mut cmd = Command::new(&bin);
    if let Some(t) = hook_token {
        cmd.env(crate::setup::TOKEN_ENV, t);
    }
    cmd.args(["--ui-token-stdout", "--exit-on-stdin-close", "--bind"])
        .arg(addr.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("không khởi động được hub ({}): {e}", bin.display()))?;
    let stdin = child.stdin.take();
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        return Err("không mở được pipe tới hub".to_owned());
    };

    // stderr: đọc liên tục suốt đời hub, chuyển ra stderr của app, giữ vài dòng cuối để báo lỗi.
    let tail = Arc::new(Mutex::new(VecDeque::<String>::new()));
    let tail_w = tail.clone();
    std::thread::Builder::new()
        .name("hub-stderr".to_owned())
        .spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                eprintln!("[hub] {line}");
                let mut t = tail_w.lock().unwrap_or_else(|e| e.into_inner());
                if t.len() >= STDERR_TAIL {
                    t.pop_front();
                }
                t.push_back(line);
            }
        })
        .map_err(|e| e.to_string())?;

    // stdout: dòng đầu là token; phần còn lại (không có gì) vẫn đọc bỏ để pipe không bao giờ đầy.
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("hub-stdout".to_owned())
        .spawn(move || {
            let mut r = BufReader::new(stdout);
            let mut line = String::new();
            let first = r.read_line(&mut line).map(|_| line);
            let _ = tx.send(first);
            let mut sink = [0u8; 1024];
            while matches!(r.read(&mut sink), Ok(n) if n > 0) {}
        })
        .map_err(|e| e.to_string())?;

    let mut hub = HubChild { child, stdin };
    let line = match rx.recv_timeout(TOKEN_TIMEOUT) {
        Ok(Ok(line)) => line,
        Ok(Err(e)) => return Err(format!("không đọc được token từ hub: {e}")),
        Err(_) => return Err("hub không in token UI kịp".to_owned()),
    };
    let token = line.trim_end_matches(['\r', '\n']).to_owned();
    if !valid_token(&token) {
        // Hub thoát trước khi in token (thiếu ARCTURUS_TOKEN, cổng bận…): báo dòng lỗi cuối của nó.
        std::thread::sleep(Duration::from_millis(200));
        hub.stop(Duration::from_millis(500));
        let last = tail
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .rev()
            .find(|l| !l.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "không rõ lý do".to_owned());
        return Err(format!("hub không khởi động được: {last}"));
    }
    Ok((token, hub))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_must_be_64_hex() {
        assert!(valid_token(&"a1".repeat(32)));
        assert!(!valid_token(&"a1".repeat(31)));
        assert!(!valid_token(&format!("{}zz", "a".repeat(62))));
        assert!(!valid_token(""));
    }

    #[test]
    fn ws_url_from_addr() {
        let info = HubInfo {
            addr: SocketAddr::from(([127, 0, 0, 1], 7878)),
            token: String::new(),
        };
        assert_eq!(info.ws_url(), "ws://127.0.0.1:7878/ws");
    }
}
