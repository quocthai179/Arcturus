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

/// `cwd` do agent gửi, không tin cậy: đường UNC (`\\host\share`, `//host/share`) làm Windows mở kết nối SMB tới máy lạ,
/// nên chỉ chạy `git` ở thư mục cục bộ.
fn is_local_cwd(cwd: &str) -> bool {
    !(cwd.is_empty() || cwd.starts_with("\\\\") || cwd.starts_with("//"))
}

impl GitLog for SystemGit {
    fn commits(&self, cwd: &str, since: OffsetDateTime) -> Vec<String> {
        if !is_local_cwd(cwd) {
            return Vec::new();
        }
        let mut cmd = Command::new("git");
        // `.git/config` của repo do agent ghi được: `log.showSignature=true` + `gpg.program` sẽ chạy chương trình tùy ý.
        // `-c` trên dòng lệnh thắng cấu hình repo.
        cmd.args([
            "-C",
            cwd,
            "-c",
            "core.fsmonitor=false",
            "-c",
            "log.showSignature=false",
        ])
        .args(["--no-pager", "log"])
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn git(dir: &Path, args: &[&str], stdin: Option<&str>) -> String {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        if let Some(s) = stdin {
            child.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
        } else {
            drop(child.stdin.take());
        }
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "git {args:?} lỗi");
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn temp_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arcturus-standup-{tag}-{}-{}",
            std::process::id(),
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"], None);
        dir
    }

    #[test]
    fn unc_and_empty_cwd_are_not_local() {
        assert!(!is_local_cwd(""));
        assert!(!is_local_cwd(r"\\evil\share\repo"));
        assert!(!is_local_cwd("//evil/share/repo"));
        assert!(is_local_cwd(r"C:\code\repo"));
        assert!(is_local_cwd("/home/u/repo"));
    }

    #[test]
    fn unc_cwd_returns_nothing_without_spawning_git() {
        let since = OffsetDateTime::now_utc() - time::Duration::hours(1);
        assert!(SystemGit
            .commits(r"\\127.0.0.1\nope\repo", since)
            .is_empty());
    }

    /// Hồi quy (cổng v0, phát hiện 5): `.git/config` do agent ghi có `log.showSignature=true` + `gpg.program`; commit mang
    /// header `gpgsig` làm `git log` chạy chương trình đó. `standup` phải tắt kiểm chữ ký.
    #[test]
    fn repo_config_cannot_make_standup_run_gpg_program() {
        let dir = temp_repo("gpg");
        let marker = dir.join("gpg-ran");
        let script = dir.join("fake-gpg.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ntouch \"{}\"\nexit 1\n",
                marker.to_string_lossy().replace('\\', "/")
            ),
        )
        .unwrap();
        git(&dir, &["config", "log.showSignature", "true"], None);
        git(
            &dir,
            &[
                "config",
                "gpg.program",
                &script.to_string_lossy().replace('\\', "/"),
            ],
            None,
        );
        let tree = git(
            &dir,
            &["hash-object", "-t", "tree", "-w", "--stdin"],
            Some(""),
        );
        let ts = OffsetDateTime::now_utc().unix_timestamp();
        let body = format!(
            "tree {tree}\nauthor A <a@b.c> {ts} +0000\ncommitter A <a@b.c> {ts} +0000\n\
             gpgsig -----BEGIN PGP SIGNATURE-----\n \n abcd\n -----END PGP SIGNATURE-----\n\nlam viec\n"
        );
        let commit = git(
            &dir,
            &["hash-object", "-t", "commit", "-w", "--stdin"],
            Some(&body),
        );
        git(&dir, &["update-ref", "HEAD", &commit], None);

        let since = OffsetDateTime::now_utc() - time::Duration::hours(1);
        let lines = SystemGit.commits(&dir.to_string_lossy(), since);

        assert!(
            !marker.exists(),
            "git log đã chạy gpg.program do repo chỉ định"
        );
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].ends_with("lam viec"), "{lines:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
