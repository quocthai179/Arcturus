//! Kết nối WebSocket nhẹ phía Rust của app (docs/PLAN.md T12): đếm yêu cầu đang chờ cho badge tray và gửi
//! `team_pause`/`team_resume` từ menu tray. Không cần webview (popup đóng thì webview bị hủy, link này vẫn sống).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::hub_proc::HubInfo;

/// Lệnh từ tray gửi tới hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    TeamPause,
    TeamResume,
    FocusSet(bool),
}

/// Trạng thái badge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Badge {
    /// Yêu cầu trên thẻ duyệt cần báo. Giờ tập trung (`focus`): chỉ đếm rủi ro cao và vùng khóa.
    pub pending: usize,
    /// Yêu cầu rủi ro thấp/vừa đang bị ẩn khỏi chấm báo vì giờ tập trung (vẫn nằm trong hàng đợi).
    pub quiet: usize,
    /// Thẻ "duyệt trong terminal" (cũng đang chờ người, trong terminal).
    pub in_terminal: usize,
    pub paused: bool,
    /// Giờ tập trung đang bật.
    pub focus: bool,
    pub connected: bool,
}

/// Áp `snapshot`/delta như UI (docs/PROTOCOL.md "Vòng đời"), chỉ giữ id.
#[derive(Default)]
pub struct Counter {
    /// `request_id` → có cần báo kể cả trong giờ tập trung (rủi ro cao/vùng khóa, hoặc không rõ).
    queue: HashMap<String, bool>,
    handoffs: HashSet<String>,
    paused: bool,
    focus: bool,
}

/// Chỉ `low`/`medium` được coi là yên lặng được; thiếu hoặc lạ thì vẫn báo (an toàn).
fn needs_attention(approval: &Value) -> bool {
    !matches!(
        approval.get("risk").and_then(Value::as_str),
        Some("low" | "medium")
    )
}

fn queue_of(v: &Value) -> HashMap<String, bool> {
    v.get("queue")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let id = x.get("request_id").and_then(Value::as_str)?;
                    Some((id.to_owned(), needs_attention(x)))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ids(v: &Value, list: &str, key: &str) -> HashSet<String> {
    v.get(list)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get(key).and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

impl Counter {
    /// Áp một thông điệp; `true` khi số đếm hoặc cờ tạm dừng đổi.
    pub fn apply(&mut self, msg: &Value) -> bool {
        let before = self.badge(true);
        let str_of = |k: &str| msg.get(k).and_then(Value::as_str).map(str::to_owned);
        match msg.get("t").and_then(Value::as_str) {
            Some("snapshot") => {
                self.queue = queue_of(msg);
                self.handoffs = ids(msg, "handoffs", "handoff_id");
                self.paused = msg
                    .pointer("/team/paused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.focus = msg
                    .pointer("/team/focus")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            }
            Some("approval_requested") => {
                if let Some(a) = msg.get("approval") {
                    if let Some(id) = a.get("request_id").and_then(Value::as_str) {
                        self.queue.insert(id.to_owned(), needs_attention(a));
                    }
                }
            }
            Some("approval_resolved") => {
                if let Some(id) = str_of("request_id") {
                    self.queue.remove(&id);
                }
            }
            Some("approval_handed_off") => {
                if let Some(id) = msg.pointer("/handoff/handoff_id").and_then(Value::as_str) {
                    self.handoffs.insert(id.to_owned());
                }
            }
            Some("handoff_cleared") => {
                if let Some(id) = str_of("handoff_id") {
                    self.handoffs.remove(&id);
                }
            }
            Some("team_updated") => {
                self.paused = msg
                    .pointer("/team/paused")
                    .and_then(Value::as_bool)
                    .unwrap_or(self.paused);
                self.focus = msg
                    .pointer("/team/focus")
                    .and_then(Value::as_bool)
                    .unwrap_or(self.focus);
            }
            _ => {}
        }
        before != self.badge(true)
    }

    pub fn badge(&self, connected: bool) -> Badge {
        let attention = self.queue.values().filter(|a| **a).count();
        let (pending, quiet) = if self.focus {
            (attention, self.queue.len() - attention)
        } else {
            (self.queue.len(), 0)
        };
        Badge {
            pending,
            quiet,
            // Thẻ "duyệt trong terminal" là lệnh quá dài, không có risk để lọc: luôn báo.
            in_terminal: self.handoffs.len(),
            paused: self.paused,
            focus: self.focus,
            connected,
        }
    }
}

/// Thời gian chờ trước lần kết nối lại thứ `attempt` (bắt đầu từ 0): 1 s, 2 s, 5 s, 10 s, rồi 30 s.
pub fn backoff(attempt: u32) -> Duration {
    const STEPS: [u64; 5] = [1, 2, 5, 10, 30];
    Duration::from_secs(STEPS[(attempt as usize).min(STEPS.len() - 1)])
}

/// Chạy mãi: kết nối, đếm, gửi lệnh; mất kết nối thì thử lại theo [`backoff`]. `info` được gọi lại mỗi lần kết nối
/// (hub tự khởi động lại thì token đổi). `on_badge` được gọi mỗi khi badge đổi.
pub async fn run<F>(info: F, mut commands: mpsc::Receiver<Command>, on_badge: impl Fn(Badge))
where
    F: Fn() -> Result<HubInfo, String> + Send + Clone + 'static,
{
    let mut attempt = 0u32;
    loop {
        // Lấy thông tin có thể phải chờ hub khởi động (chặn): chạy trên luồng blocking, không trên luồng async.
        let f = info.clone();
        let got = tokio::task::spawn_blocking(f)
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        let result = match got {
            Ok(info) => session(&info, &mut commands, &on_badge).await,
            Err(e) => Err(e),
        };
        match result {
            Ok(()) => attempt = 0,
            Err(e) => eprintln!("arcturus app: kết nối hub: {e}"),
        }
        on_badge(Badge::default());
        tokio::time::sleep(backoff(attempt)).await;
        attempt = attempt.saturating_add(1);
    }
}

async fn session(
    info: &HubInfo,
    commands: &mut mpsc::Receiver<Command>,
    on_badge: &impl Fn(Badge),
) -> Result<(), String> {
    let mut req = info
        .ws_url()
        .into_client_request()
        .map_err(|e| e.to_string())?;
    let auth = HeaderValue::from_str(&format!("Bearer {}", info.token))
        .map_err(|_| "token UI sai dạng".to_owned())?;
    req.headers_mut().insert("authorization", auth);
    let (mut ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .map_err(|e| e.to_string())?;
    let mut counter = Counter::default();
    on_badge(counter.badge(true));
    loop {
        tokio::select! {
            frame = ws.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&text) {
                        if counter.apply(&v) {
                            on_badge(counter.badge(true));
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => return Ok(()),
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(e.to_string()),
            },
            cmd = commands.recv() => {
                let Some(cmd) = cmd else { return Ok(()) };
                let t = match cmd {
                    Command::TeamPause => r#"{"t":"team_pause"}"#,
                    Command::TeamResume => r#"{"t":"team_resume"}"#,
                    Command::FocusSet(true) => r#"{"t":"focus_set","on":true}"#,
                    Command::FocusSet(false) => r#"{"t":"focus_set","on":false}"#,
                };
                ws.send(Message::text(t)).await.map_err(|e| e.to_string())?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn counter_follows_snapshot_and_deltas() {
        let mut c = Counter::default();
        assert!(c.apply(&json!({"t":"snapshot","agents":[],"queue":[{"request_id":"req_1"},{"request_id":"req_2"}],
            "handoffs":[{"handoff_id":"hof_1"}],"team":{"paused":false,"focus":false,"roster_max":5}})));
        assert_eq!(
            c.badge(true),
            Badge {
                pending: 2,
                in_terminal: 1,
                connected: true,
                ..Badge::default()
            }
        );
        // Trùng ngay sau snapshot: không đổi.
        assert!(!c.apply(&json!({"t":"approval_requested","approval":{"request_id":"req_2"}})));
        assert!(c.apply(&json!({"t":"approval_requested","approval":{"request_id":"req_3"}})));
        assert!(c.apply(&json!({"t":"approval_resolved","request_id":"req_1","decision":null,"decided_by":"timeout"})));
        assert!(!c.apply(&json!({"t":"approval_resolved","request_id":"req_x"})));
        assert!(c.apply(&json!({"t":"handoff_cleared","handoff_id":"hof_1"})));
        assert!(c.apply(&json!({"t":"team_updated","team":{"paused":true}})));
        assert!(!c.apply(&json!({"t":"agent_updated","agent":{"id":"a"}})));
        assert_eq!(
            c.badge(true),
            Badge {
                pending: 2,
                paused: true,
                connected: true,
                ..Badge::default()
            }
        );
        // Snapshot mới thay toàn bộ.
        assert!(c.apply(&json!({"t":"snapshot","queue":[],"handoffs":[],"team":{"paused":false}})));
        assert_eq!(c.badge(false), Badge::default());
    }

    #[test]
    fn focus_hides_low_and_medium_but_never_high_critical_or_unknown() {
        let mut c = Counter::default();
        c.apply(&json!({"t":"snapshot","handoffs":[{"handoff_id":"hof_1"}],
            "queue":[{"request_id":"req_l","risk":"low"},{"request_id":"req_m","risk":"medium"},
                     {"request_id":"req_h","risk":"high"},{"request_id":"req_c","risk":"critical"},
                     {"request_id":"req_x"}],
            "team":{"paused":false,"focus":false,"roster_max":5}}));
        assert_eq!(c.badge(true).pending, 5);
        assert_eq!(c.badge(true).quiet, 0);
        // Bật giờ tập trung: cờ đổi nên badge đổi.
        assert!(c.apply(
            &json!({"t":"team_updated","team":{"paused":false,"focus":true,"roster_max":5}})
        ));
        let b = c.badge(true);
        assert_eq!(
            (b.pending, b.quiet, b.in_terminal, b.focus),
            (3, 2, 1, true)
        );
        // Yêu cầu thấp/vừa mới tới trong giờ tập trung: ẩn; số báo không đổi nhưng `quiet` đổi.
        assert!(c.apply(
            &json!({"t":"approval_requested","approval":{"request_id":"req_l2","risk":"low"}})
        ));
        assert_eq!((c.badge(true).pending, c.badge(true).quiet), (3, 3));
        // Yêu cầu cao tới thì báo.
        assert!(c.apply(
            &json!({"t":"approval_requested","approval":{"request_id":"req_h2","risk":"high"}})
        ));
        assert_eq!(c.badge(true).pending, 4);
        // Tắt giờ tập trung: đếm lại tất cả.
        assert!(c.apply(
            &json!({"t":"team_updated","team":{"paused":false,"focus":false,"roster_max":5}})
        ));
        let b = c.badge(true);
        assert_eq!((b.pending, b.quiet, b.focus), (7, 0, false));
        // Giờ tập trung đi qua snapshot (kết nối lại).
        c.apply(
            &json!({"t":"snapshot","queue":[{"request_id":"req_l","risk":"low"}],"handoffs":[],
            "team":{"paused":false,"focus":true,"roster_max":5}}),
        );
        let b = c.badge(true);
        assert_eq!((b.pending, b.quiet), (0, 1));
    }

    #[test]
    fn backoff_caps_at_30s() {
        assert_eq!(backoff(0), Duration::from_secs(1));
        assert_eq!(backoff(3), Duration::from_secs(10));
        assert_eq!(backoff(99), Duration::from_secs(30));
    }
}
