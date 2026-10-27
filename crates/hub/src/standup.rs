//! `GET /standup` (`docs/PLAN.md` T19): ghép sự kiện đã lưu, trạng thái agent đang chạy, hàng đợi duyệt và `git log` thành
//! [`Standup`]. Phần thuần nằm ở `hub_core::standup`; ở đây chỉ có I/O (SQLite, tiến trình `git`).
//!
//! Mọi lời gọi ở đây chặn (đọc SQLite, chờ `git`): handler HTTP chạy trong `spawn_blocking`.

use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use hub_core::schema::Event;
use hub_core::standup::{build, GitLog, Input, Standup};
use hub_core::store::{format_ts, StoreReader};
use time::OffsetDateTime;

use crate::hub::Hub;

/// Số giờ mặc định và tối đa của khoảng standup.
pub const DEFAULT_HOURS: u32 = 24;
pub const MAX_HOURS: u32 = 168;
/// Tối đa sự kiện đọc cho một standup (khoảng 7 ngày hoạt động nhẹ; vượt thì bỏ phần cũ nhất của khoảng).
pub const MAX_EVENTS: usize = 50_000;
/// Chờ `git log` tối đa chừng này mỗi thư mục.
pub const GIT_TIMEOUT: Duration = Duration::from_secs(3);
/// Tối đa commit hỏi mỗi thư mục, và byte đọc từ `git`.
const GIT_MAX_COMMITS: &str = "30";
const GIT_MAX_BYTES: u64 = 256 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum StandupError {
    /// Hub chạy không có cơ sở dữ liệu (test, hoặc cấu hình tắt).
    NoDatabase,
    Store(String),
}

/// Chạy `git log` thật. Lỗi ở mọi bước (không có git, không phải repo, hết giờ) cho kết quả rỗng.
pub struct SystemGit;

impl GitLog for SystemGit {
    fn commits(&self, cwd: &str, since: OffsetDateTime) -> Vec<String> {
        let mut cmd = Command::new("git");
        cmd.args(["-C", cwd, "-c", "core.fsmonitor=false", "--no-pager", "log"])
            .args(["--no-merges", "-n", GIT_MAX_COMMITS, "--format=%h %s"])
            .arg(format!("--since={}", format_ts(since)))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let Ok(mut child) = cmd.spawn() else {
            return Vec::new();
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            return Vec::new();
        };
        // Đọc ở thread riêng để pipe không đầy khi chờ; thread kết thúc khi `git` thoát hoặc bị kill (EOF).
        let reader = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.take(GIT_MAX_BYTES).read_to_end(&mut buf);
            buf
        });
        let deadline = Instant::now() + GIT_TIMEOUT;
        let ok = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break false;
                }
            }
        };
        let out = reader.join().unwrap_or_default();
        if !ok {
            return Vec::new();
        }
        String::from_utf8_lossy(&out)
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

/// Standup của `hours` giờ gần nhất tính tới `now`.
pub fn standup_for(
    hub: &Hub,
    hours: u32,
    now: OffsetDateTime,
    git: &dyn GitLog,
) -> Result<Standup, StandupError> {
    let path = hub.db_path().ok_or(StandupError::NoDatabase)?;
    let since = now - time::Duration::hours(i64::from(hours.clamp(1, MAX_HOURS)));
    let reader = StoreReader::open(path).map_err(|e| StandupError::Store(e.to_string()))?;
    let rows = reader
        .events_since(since, MAX_EVENTS)
        .map_err(|e| StandupError::Store(e.to_string()))?;
    // Sự kiện hỏng hoặc từ phiên bản lạ bị bỏ qua, không làm cả standup lỗi.
    let events: Vec<Event> = rows
        .iter()
        .filter_map(|r| serde_json::from_str(&r.event_json).ok())
        .collect();

    // Tên persona: agent đang chạy (đúng tên đang hiện trong Văn phòng) → `match_cwd` → "Agent <4 ký tự đầu session>".
    let live: BTreeMap<String, String> = hub
        .agent_views()
        .into_iter()
        .map(|a| (a.id, a.name))
        .collect();
    let name_of = |ev: &Event| -> String {
        live.get(&ev.agent.id)
            .cloned()
            .or_else(|| {
                ev.cwd
                    .as_deref()
                    .and_then(|c| hub.personas.find(c))
                    .map(|p| p.name.clone())
            })
            .unwrap_or_else(|| {
                let short: String = ev.session_id.chars().take(4).collect();
                format!("Agent {short}")
            })
    };
    let pending: Vec<String> = hub
        .approvals
        .views()
        .into_iter()
        .map(|v| v.agent_name)
        .collect();
    Ok(build(
        &Input {
            events: &events,
            pending: &pending,
            since,
            until: now,
            name_of: &name_of,
        },
        git,
    ))
}
