//! Đối soát phiên (`docs/PLAN.md` T7): định kỳ hỏi `claude agents --json` rồi dọn phiên mà hub còn giữ nhưng Claude Code
//! không còn chạy (SessionEnd bị mất, hook http không bắn SessionStart, hub khởi động lại giữa chừng…).
//!
//! - Không phải polling thường trực: chỉ chạy `claude` khi hub đang có ít nhất một phiên (ngân sách, quy tắc bất biến 6).
//! - Fail-safe: lệnh lỗi, không có `claude`, quá 5 giây, hoặc JSON lạ thì **không xóa gì** và đếm lại từ đầu.
//! - Một phiên chỉ bị coi là đã kết thúc khi **vắng hai lần quan sát thành công liên tiếp** và đã im lặng (không có sự kiện
//!   hook) ít nhất hai chu kỳ: phiên chạy lệnh dài không bắn sự kiện nên không bị dọn nhầm, và phiên `-p` có thể không nằm
//!   trong danh sách vẫn sống nếu còn bắn sự kiện.
//! - Dọn bằng cách đưa vào chính đường xử lý của một `SessionEnd` thật (hủy yêu cầu đang chờ → trả `{}`, xóa agent và agent
//!   con, ghi sự kiện `session.ended`), không có đường tắt riêng.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::process::Stdio;
use std::sync::Weak;
use std::time::Duration;

use hub_core::hook::HookInput;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tokio::task::JoinHandle;

use crate::hub::Hub;

/// Chờ `claude agents --json` tối đa chừng này.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(5);
/// Số lần quan sát vắng liên tiếp để kết luận phiên đã kết thúc.
pub const MISSES_TO_END: u32 = 2;
/// Đọc tối đa chừng này byte từ `claude` (danh sách phiên rất nhỏ).
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// Nguồn danh sách phiên đang chạy. `None` = không lấy được (lỗi, hết giờ, JSON lạ): không kết luận gì.
pub trait AgentLister: Send + Sync + 'static {
    fn list(&self) -> impl Future<Output = Option<HashSet<String>>> + Send;
}

/// Chạy `claude agents --json`.
pub struct ClaudeAgents {
    program: String,
}

impl Default for ClaudeAgents {
    fn default() -> Self {
        Self::with_program("claude")
    }
}

impl ClaudeAgents {
    pub fn with_program(program: &str) -> Self {
        ClaudeAgents {
            program: program.to_owned(),
        }
    }
}

impl AgentLister for ClaudeAgents {
    fn list(&self) -> impl Future<Output = Option<HashSet<String>>> + Send {
        let program = self.program.clone();
        async move {
            let mut cmd = tokio::process::Command::new(&program);
            cmd.args(["agents", "--json"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            #[cfg(windows)]
            {
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                cmd.creation_flags(CREATE_NO_WINDOW);
            }
            let child = match cmd.spawn() {
                Ok(c) => c,
                Err(err) => {
                    tracing::debug!(%err, program = %program, "đối soát: không chạy được claude");
                    return None;
                }
            };
            // `wait_with_output` bị hủy khi hết giờ thì `kill_on_drop` giết tiến trình.
            let out = match tokio::time::timeout(LIST_TIMEOUT, child.wait_with_output()).await {
                Ok(Ok(o)) if o.status.success() => o,
                Ok(Ok(o)) => {
                    tracing::debug!(status = ?o.status, "đối soát: claude agents thoát lỗi");
                    return None;
                }
                Ok(Err(err)) => {
                    tracing::debug!(%err, "đối soát: lỗi đọc đầu ra của claude");
                    return None;
                }
                Err(_) => {
                    tracing::debug!("đối soát: claude agents quá giờ");
                    return None;
                }
            };
            if out.stdout.len() > MAX_OUTPUT_BYTES {
                tracing::debug!(bytes = out.stdout.len(), "đối soát: đầu ra quá lớn, bỏ qua");
                return None;
            }
            parse_agents(&String::from_utf8_lossy(&out.stdout))
        }
    }
}

/// Mảng JSON các phiên → tập `sessionId`. Phần tử thiếu `sessionId` hoặc sai kiểu bị bỏ qua (Q8: parse chịu lỗi);
/// không phải mảng thì `None`.
pub fn parse_agents(json: &str) -> Option<HashSet<String>> {
    let v: Value = serde_json::from_str(json).ok()?;
    let arr = v.as_array()?;
    Some(
        arr.iter()
            .filter_map(|a| a.get("sessionId").and_then(Value::as_str))
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

/// Đếm số lần vắng liên tiếp của từng phiên. Thuần: không có I/O.
#[derive(Default)]
pub struct Reconciler {
    misses: HashMap<String, u32>,
}

impl Reconciler {
    pub fn reset(&mut self) {
        self.misses.clear();
    }

    /// Một lần quan sát. `known`: phiên hub đang giữ kèm thời điểm sự kiện gần nhất; `live`: kết quả của `claude`;
    /// trả các phiên cần đóng. Phiên còn trong `live`, hoặc có sự kiện trong `min_idle` gần đây, được coi là còn sống và
    /// xóa bộ đếm.
    pub fn observe(
        &mut self,
        known: &[(String, OffsetDateTime)],
        live: Option<&HashSet<String>>,
        now: OffsetDateTime,
        min_idle: Duration,
    ) -> Vec<String> {
        self.misses.retain(|s, _| known.iter().any(|(k, _)| k == s));
        let Some(live) = live else {
            // Không lấy được danh sách: không kết luận, và "liên tiếp" bị đứt.
            self.misses.clear();
            return Vec::new();
        };
        let idle = time::Duration::try_from(min_idle).unwrap_or(time::Duration::MAX);
        let mut ended = Vec::new();
        for (session, last_event) in known {
            if live.contains(session) || now - *last_event < idle {
                self.misses.remove(session);
                continue;
            }
            let n = self.misses.entry(session.clone()).or_insert(0);
            *n += 1;
            if *n >= MISSES_TO_END {
                ended.push(session.clone());
            }
        }
        for s in &ended {
            self.misses.remove(s);
        }
        ended
    }
}

/// Một vòng đối soát. Trả danh sách phiên đã đóng.
pub async fn tick<L: AgentLister>(
    hub: &Hub,
    rec: &mut Reconciler,
    lister: &L,
    min_idle: Duration,
) -> Vec<String> {
    let known = hub.sessions();
    if known.is_empty() {
        // Không có phiên nào: không chạy `claude` (ngân sách), và không giữ bộ đếm cũ.
        rec.reset();
        return Vec::new();
    }
    let live = lister.list().await;
    let ended = rec.observe(&known, live.as_ref(), OffsetDateTime::now_utc(), min_idle);
    for session in &ended {
        tracing::info!(session = ?session, "đối soát: phiên không còn chạy, đóng như SessionEnd");
        hub.ingest(&HookInput::new(json!({
            "hook_event_name": "SessionEnd",
            "session_id": session,
            "reason": "reconcile",
        })));
    }
    ended
}

/// Task nền: mỗi `interval` một vòng; dừng khi `Hub` bị drop.
pub fn spawn<L: AgentLister>(hub: Weak<Hub>, lister: L, interval: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut rec = Reconciler::default();
        let min_idle = interval.saturating_mul(2);
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            timer.tick().await;
            let Some(hub) = hub.upgrade() else { return };
            tick(&hub, &mut rec, &lister, min_idle).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-10-27 10:00 UTC);

    fn old(secs: i64) -> OffsetDateTime {
        NOW - time::Duration::seconds(secs)
    }

    fn known(sessions: &[&str]) -> Vec<(String, OffsetDateTime)> {
        sessions
            .iter()
            .map(|s| ((*s).to_owned(), old(3600)))
            .collect()
    }

    fn live(sessions: &[&str]) -> HashSet<String> {
        sessions.iter().map(|s| (*s).to_owned()).collect()
    }

    const IDLE: Duration = Duration::from_secs(60);

    #[test]
    fn removes_session_missing_twice() {
        let mut r = Reconciler::default();
        let k = known(&["a", "b"]);
        assert!(r.observe(&k, Some(&live(&["a"])), NOW, IDLE).is_empty());
        assert_eq!(r.observe(&k, Some(&live(&["a"])), NOW, IDLE), ["b"]);
    }

    #[test]
    fn keeps_session_missing_once() {
        let mut r = Reconciler::default();
        let k = known(&["a"]);
        assert!(r.observe(&k, Some(&live(&[])), NOW, IDLE).is_empty());
        // Quay lại trong danh sách: bộ đếm về 0, lần vắng sau là lần đầu.
        assert!(r.observe(&k, Some(&live(&["a"])), NOW, IDLE).is_empty());
        assert!(r.observe(&k, Some(&live(&[])), NOW, IDLE).is_empty());
        assert_eq!(r.observe(&k, Some(&live(&[])), NOW, IDLE), ["a"]);
    }

    #[test]
    fn command_failure_removes_nothing_and_breaks_the_streak() {
        let mut r = Reconciler::default();
        let k = known(&["a"]);
        assert!(r.observe(&k, Some(&live(&[])), NOW, IDLE).is_empty());
        // Lần lấy danh sách lỗi: không xóa gì, và hai lần vắng phải là hai lần quan sát thành công liên tiếp.
        assert!(r.observe(&k, None, NOW, IDLE).is_empty());
        assert!(r.observe(&k, Some(&live(&[])), NOW, IDLE).is_empty());
        assert_eq!(r.observe(&k, Some(&live(&[])), NOW, IDLE), ["a"]);
    }

    #[test]
    fn recent_hook_activity_means_alive_even_if_not_listed() {
        let mut r = Reconciler::default();
        // Sự kiện cách đây 10 giây (< 60 giây): phiên chạy `-p` có thể không nằm trong danh sách vẫn sống.
        let k = vec![("a".to_owned(), old(10))];
        for _ in 0..5 {
            assert!(r.observe(&k, Some(&live(&[])), NOW, IDLE).is_empty());
        }
        // Im lặng đủ lâu thì tính như phiên vắng.
        let k = vec![("a".to_owned(), old(120))];
        assert!(r.observe(&k, Some(&live(&[])), NOW, IDLE).is_empty());
        assert_eq!(r.observe(&k, Some(&live(&[])), NOW, IDLE), ["a"]);
    }

    #[test]
    fn counters_of_vanished_sessions_are_dropped() {
        let mut r = Reconciler::default();
        assert!(r
            .observe(&known(&["a"]), Some(&live(&[])), NOW, IDLE)
            .is_empty());
        // `a` biến mất khỏi hub (đã có SessionEnd thật); quay lại sau đó là phiên mới, đếm từ đầu.
        assert!(r.observe(&[], Some(&live(&[])), NOW, IDLE).is_empty());
        assert!(r
            .observe(&known(&["a"]), Some(&live(&[])), NOW, IDLE)
            .is_empty());
    }

    #[test]
    fn parse_agents_is_tolerant() {
        let ok = r#"[{"pid":1,"sessionId":"s1","status":"busy"},{"pid":2,"sessionId":"s2"}]"#;
        assert_eq!(parse_agents(ok), Some(live(&["s1", "s2"])));
        // Phần tử hỏng bị bỏ qua, phần còn lại vẫn dùng được.
        let mixed = r#"[{"sessionId":"s1"},{"pid":3},{"sessionId":7},{"sessionId":""},"x",null]"#;
        assert_eq!(parse_agents(mixed), Some(live(&["s1"])));
        assert_eq!(parse_agents("[]"), Some(HashSet::new()));
        // Không phải mảng, hoặc không phải JSON: không kết luận gì.
        assert_eq!(parse_agents("{}"), None);
        assert_eq!(parse_agents("not json"), None);
        assert_eq!(parse_agents(""), None);
    }

    /// Chạy tay: `cargo test -p hub -- --ignored real_claude`. Cần `claude` trong PATH.
    #[tokio::test]
    #[ignore = "cần Claude Code cài sẵn"]
    async fn real_claude_agents_lists_sessions() {
        let started = std::time::Instant::now();
        let sessions = ClaudeAgents::default().list().await;
        eprintln!(
            "claude agents --json: {sessions:?} sau {:?}",
            started.elapsed()
        );
        assert!(
            sessions.is_some(),
            "claude agents --json phải trả mảng hợp lệ"
        );
    }

    #[tokio::test]
    async fn missing_program_gives_none() {
        let l = ClaudeAgents::with_program("arcturus-khong-co-claude-nay");
        assert_eq!(l.list().await, None);
    }
}
