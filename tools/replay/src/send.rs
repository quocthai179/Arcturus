//! Gửi fixture tới hub (`docs/PLAN.md` T15).
//!
//! `replay <dir> --url http://127.0.0.1:7878 --sessions N --rate R --duration D [--permission-mode allow|skip]`:
//! đọc fixture, thay `session_id` bằng `replay-<i>`, gửi tới `/hooks/event` (và `/hooks/permission` khi bật). Chế độ
//! `allow` đóng vai người duyệt qua WebSocket (cần token UI) để các yêu cầu giữ được trả lời. In p50/p95/p99 độ trễ và
//! số lỗi; `--assert-p99-ms` làm lệnh thoát ≠0 nếu p99 của `/hooks/event` vượt ngưỡng.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::files::json_files;

/// Chặn thời gian chờ một yêu cầu duyệt (hub tự trả `{}` ở 110 giây).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(130);

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum PermissionMode {
    /// Gửi cả `PermissionRequest` và duyệt `allow` qua WebSocket (đóng vai người dùng).
    Allow,
    /// Bỏ qua `PermissionRequest`, chỉ gửi sự kiện.
    Skip,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub dir: PathBuf,
    /// Gốc HTTP của hub, ví dụ `http://127.0.0.1:7878`.
    pub url: String,
    pub sessions: usize,
    /// Số yêu cầu mỗi giây, chia đều cho các phiên.
    pub rate: f64,
    pub duration: Duration,
    pub permission_mode: PermissionMode,
    /// Chờ bao lâu rồi mới duyệt `allow` (phải lớn hơn `min_review_ms` của hub, mặc định 1.000 ms).
    pub allow_delay: Duration,
    pub hook_token: String,
    pub ui_token: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Event,
    Permission,
}

#[derive(Clone, Debug)]
pub struct Fixture {
    /// Đường dẫn tương đối, dùng để báo lỗi.
    pub name: String,
    pub kind: Kind,
    pub value: Value,
}

/// Đọc mọi fixture trong `dir`. `PermissionRequest` đi `/hooks/permission`, còn lại đi `/hooks/event`.
pub fn load_fixtures(dir: &Path) -> anyhow::Result<Vec<Fixture>> {
    let mut out = Vec::new();
    for rel in json_files(dir).with_context(|| format!("không đọc được {}", dir.display()))?
    {
        let path = dir.join(&rel);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("không đọc được {}", path.display()))?;
        let value: Value = serde_json::from_str(&text)
            .with_context(|| format!("{} không phải JSON hợp lệ", path.display()))?;
        let kind =
            if value.get("hook_event_name").and_then(Value::as_str) == Some("PermissionRequest") {
                Kind::Permission
            } else {
                Kind::Event
            };
        out.push(Fixture {
            name: rel.display().to_string(),
            kind,
            value,
        });
    }
    Ok(out)
}

/// Bản sao của fixture với `session_id` của phiên giả `index` (`replay-<index>`).
pub fn patched(fixture: &Value, index: usize) -> Value {
    let mut v = fixture.clone();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("session_id".to_owned(), json!(format!("replay-{index}")));
    }
    v
}

/// Yêu cầu thứ `n` của kế hoạch: phiên và fixture nào.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub session: usize,
    pub fixture: usize,
}

/// Kế hoạch `total` yêu cầu, xoay vòng giữa các phiên; mỗi phiên đi hết danh sách fixture theo thứ tự, lệch pha giữa các
/// phiên để chúng không gửi cùng một sự kiện cùng lúc. `usable` là số fixture dùng được (đã lọc theo chế độ).
pub fn plan(usable: usize, sessions: usize, total: usize) -> Vec<Step> {
    if usable == 0 || sessions == 0 {
        return Vec::new();
    }
    (0..total)
        .map(|n| {
            let session = n % sessions;
            let round = n / sessions;
            Step {
                session,
                fixture: (round + session * 7) % usable,
            }
        })
        .collect()
}

/// Phân vị theo thứ hạng gần nhất trên mảng đã sắp xếp tăng dần. Mảng rỗng thì 0.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    /// `/hooks/event` trả `204`.
    Accepted,
    Allow,
    Deny,
    /// `200 {}`: không quyết định, Claude Code hỏi trong terminal.
    Empty,
    /// Mã HTTP khác kỳ vọng.
    HttpError(u16),
    /// Không kết nối được hoặc hết giờ.
    NetError,
}

impl Outcome {
    pub fn is_error(self) -> bool {
        matches!(self, Outcome::HttpError(_) | Outcome::NetError)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub kind: Kind,
    pub ms: f64,
    pub outcome: Outcome,
}

#[derive(Debug, Default)]
pub struct Report {
    pub samples: Vec<Sample>,
}

impl Report {
    fn of(&self, kind: Kind) -> Vec<&Sample> {
        self.samples.iter().filter(|s| s.kind == kind).collect()
    }

    /// Độ trễ (ms) đã sắp xếp của `kind`, không tính mẫu lỗi.
    pub fn latencies(&self, kind: Kind) -> Vec<f64> {
        let mut v: Vec<f64> = self
            .of(kind)
            .into_iter()
            .filter(|s| !s.outcome.is_error())
            .map(|s| s.ms)
            .collect();
        v.sort_by(f64::total_cmp);
        v
    }

    pub fn errors(&self, kind: Kind) -> usize {
        self.of(kind)
            .iter()
            .filter(|s| s.outcome.is_error())
            .count()
    }

    pub fn count(&self, kind: Kind, outcome: Outcome) -> usize {
        self.of(kind)
            .iter()
            .filter(|s| s.outcome == outcome)
            .count()
    }

    pub fn p99(&self, kind: Kind) -> f64 {
        percentile(&self.latencies(kind), 99.0)
    }

    /// Bảng tóm tắt in ra màn hình.
    pub fn summary(&self) -> String {
        let mut out = String::new();
        for (kind, label) in [
            (Kind::Event, "/hooks/event"),
            (Kind::Permission, "/hooks/permission"),
        ] {
            let n = self.of(kind).len();
            if n == 0 {
                continue;
            }
            let l = self.latencies(kind);
            let _ = write!(
                out,
                "{label}: {n} yêu cầu, {} lỗi, p50 {:.2} ms, p95 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
                self.errors(kind),
                percentile(&l, 50.0),
                percentile(&l, 95.0),
                percentile(&l, 99.0),
                l.last().copied().unwrap_or(0.0),
            );
            if kind == Kind::Permission {
                let _ = write!(
                    out,
                    " (allow {}, deny {}, {{}} {})",
                    self.count(kind, Outcome::Allow),
                    self.count(kind, Outcome::Deny),
                    self.count(kind, Outcome::Empty),
                );
            }
            out.push('\n');
        }
        out
    }

    /// `Err` nếu `/hooks/event` có lỗi, hoặc (khi `limit_ms` có) p99 của nó vượt ngưỡng.
    pub fn check(&self, limit_ms: Option<f64>) -> Result<(), String> {
        let errors = self.errors(Kind::Event);
        if errors > 0 {
            return Err(format!("{errors} yêu cầu /hooks/event lỗi"));
        }
        if let Some(limit) = limit_ms {
            let p99 = self.p99(Kind::Event);
            if p99 > limit {
                return Err(format!(
                    "p99 của /hooks/event là {p99:.2} ms, vượt ngưỡng {limit:.2} ms"
                ));
            }
        }
        Ok(())
    }
}

/// Chạy kế hoạch: gửi theo tốc độ `rate`, chờ mọi yêu cầu xong, trả báo cáo.
pub async fn run(opts: Options) -> anyhow::Result<Report> {
    if opts.sessions == 0 {
        bail!("--sessions phải ≥ 1");
    }
    if !(opts.rate.is_finite() && opts.rate > 0.0) {
        bail!("--rate phải > 0");
    }
    let fixtures: Vec<Fixture> = load_fixtures(&opts.dir)?
        .into_iter()
        .filter(|f| opts.permission_mode == PermissionMode::Allow || f.kind == Kind::Event)
        .collect();
    if fixtures.is_empty() {
        bail!(
            "không có fixture nào dùng được trong {}",
            opts.dir.display()
        );
    }
    let total = ((opts.rate * opts.duration.as_secs_f64()).round() as usize).max(1);
    let steps = plan(fixtures.len(), opts.sessions, total);

    // Người duyệt giả: kết nối trước khi gửi để không lỡ yêu cầu nào (Snapshot cũng được duyệt).
    let _responder = if opts.permission_mode == PermissionMode::Allow {
        let token = opts.ui_token.clone().context(
            "--permission-mode allow cần token UI (--ui-token-file hoặc ARCTURUS_UI_TOKEN)",
        )?;
        Some(spawn_responder(&opts.url, token, opts.allow_delay).await?)
    } else {
        None
    };

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()?;
    let mut tasks: JoinSet<Sample> = JoinSet::new();
    let mut tick = tokio::time::interval(Duration::from_secs_f64(1.0 / opts.rate));
    for step in steps {
        tick.tick().await;
        let f = &fixtures[step.fixture];
        let body = patched(&f.value, step.session);
        let (client, url, token, kind) = (
            client.clone(),
            opts.url.clone(),
            opts.hook_token.clone(),
            f.kind,
        );
        tasks.spawn(async move { send_one(&client, &url, &token, kind, &body).await });
    }
    let mut report = Report::default();
    while let Some(done) = tasks.join_next().await {
        report.samples.push(done.context("task gửi bị hủy")?);
    }
    Ok(report)
}

async fn send_one(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    kind: Kind,
    body: &Value,
) -> Sample {
    let path = match kind {
        Kind::Event => "/hooks/event",
        Kind::Permission => "/hooks/permission",
    };
    let started = Instant::now();
    let resp = client
        .post(format!("{url}{path}"))
        .bearer_auth(token)
        .json(body)
        .send()
        .await;
    let outcome = match resp {
        Err(_) => Outcome::NetError,
        Ok(r) => {
            let status = r.status().as_u16();
            match (kind, status) {
                (Kind::Event, 204) => Outcome::Accepted,
                (Kind::Permission, 200) => match r.json::<Value>().await {
                    Ok(v) => decision_of(&v),
                    Err(_) => Outcome::NetError,
                },
                (_, s) => Outcome::HttpError(s),
            }
        }
    };
    Sample {
        kind,
        ms: started.elapsed().as_secs_f64() * 1000.0,
        outcome,
    }
}

/// `{"hookSpecificOutput":{"decision":{"behavior":"allow"|"deny"}}}` hoặc `{}`.
pub fn decision_of(v: &Value) -> Outcome {
    match v
        .pointer("/hookSpecificOutput/decision/behavior")
        .and_then(Value::as_str)
    {
        Some("allow") => Outcome::Allow,
        Some("deny") => Outcome::Deny,
        _ => Outcome::Empty,
    }
}

/// Người duyệt giả; hủy khi bị drop.
struct Responder {
    _stop: oneshot::Sender<()>,
}

async fn spawn_responder(
    url: &str,
    ui_token: String,
    delay: Duration,
) -> anyhow::Result<Responder> {
    let ws_url = url.replacen("http", "ws", 1) + "/ws";
    let mut req = ws_url
        .as_str()
        .into_client_request()
        .context("địa chỉ WebSocket sai")?;
    req.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {ui_token}")).context("token UI sai dạng")?,
    );
    let (ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .context("không kết nối được WebSocket của hub (token UI đúng chưa?)")?;
    let (mut tx, mut rx) = ws.split();
    let (stop, mut stopped) = oneshot::channel::<()>();
    let (decide_tx, mut decide_rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        let mut seen = BTreeSet::new();
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                Some(msg) = decide_rx.recv() => {
                    if tx.send(Message::text(msg)).await.is_err() {
                        break;
                    }
                }
                m = rx.next() => {
                    let Some(Ok(Message::Text(text))) = m else {
                        if m.is_none() { break; }
                        continue;
                    };
                    let Ok(v) = serde_json::from_str::<Value>(text.as_str()) else { continue };
                    for id in pending_ids(&v) {
                        if seen.insert(id.clone()) {
                            let out = decide_tx.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(delay).await;
                                let _ = out.send(
                                    json!({"t":"decide","request_id":id,"decision":"allow","origin":"user"}).to_string(),
                                );
                            });
                        }
                    }
                }
            }
        }
    });
    Ok(Responder { _stop: stop })
}

/// `request_id` của các yêu cầu mới trong một thông điệp của hub (`approval_requested` hoặc `snapshot`).
pub fn pending_ids(msg: &Value) -> Vec<String> {
    match msg.get("t").and_then(Value::as_str) {
        Some("approval_requested") => msg
            .pointer("/approval/request_id")
            .and_then(Value::as_str)
            .map(|s| vec![s.to_owned()])
            .unwrap_or_default(),
        Some("snapshot") => msg
            .get("queue")
            .and_then(Value::as_array)
            .map(|q| {
                q.iter()
                    .filter_map(|a| {
                        a.get("request_id")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_nearest_rank() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&v, 50.0), 50.0);
        assert_eq!(percentile(&v, 95.0), 95.0);
        assert_eq!(percentile(&v, 99.0), 99.0);
        assert_eq!(percentile(&v, 100.0), 100.0);
        assert_eq!(percentile(&[7.0], 99.0), 7.0);
        assert_eq!(percentile(&[], 99.0), 0.0);
    }

    #[test]
    fn patched_replaces_only_session_id() {
        let v = json!({"hook_event_name": "Stop", "session_id": "real", "cwd": "/w"});
        let p = patched(&v, 3);
        assert_eq!(p["session_id"], "replay-3");
        assert_eq!(p["cwd"], "/w");
        assert_eq!(v["session_id"], "real", "bản gốc không đổi");
    }

    #[test]
    fn plan_round_robins_sessions_and_is_deterministic() {
        let steps = plan(5, 3, 9);
        assert_eq!(steps.len(), 9);
        let sessions: Vec<usize> = steps.iter().map(|s| s.session).collect();
        assert_eq!(sessions, [0, 1, 2, 0, 1, 2, 0, 1, 2]);
        assert!(steps.iter().all(|s| s.fixture < 5));
        assert_eq!(steps, plan(5, 3, 9));
        // Mỗi phiên đi hết danh sách fixture theo thứ tự.
        let s0: Vec<usize> = steps
            .iter()
            .filter(|s| s.session == 0)
            .map(|s| s.fixture)
            .collect();
        assert_eq!(s0, [0, 1, 2]);
        // Lệch pha giữa các phiên.
        assert_ne!(steps[0].fixture, steps[1].fixture);
    }

    #[test]
    fn plan_edge_cases() {
        assert!(plan(0, 3, 9).is_empty());
        assert!(plan(3, 0, 9).is_empty());
        assert!(plan(3, 2, 0).is_empty());
    }

    #[test]
    fn decision_of_reads_hook_output() {
        let allow = json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}});
        let deny = json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny"}}});
        assert_eq!(decision_of(&allow), Outcome::Allow);
        assert_eq!(decision_of(&deny), Outcome::Deny);
        assert_eq!(decision_of(&json!({})), Outcome::Empty);
    }

    #[test]
    fn pending_ids_from_snapshot_and_delta() {
        let snap = json!({"t":"snapshot","queue":[{"request_id":"req_1"},{"request_id":"req_2"}],"handoffs":[]});
        assert_eq!(pending_ids(&snap), ["req_1", "req_2"]);
        let delta = json!({"t":"approval_requested","approval":{"request_id":"req_3"}});
        assert_eq!(pending_ids(&delta), ["req_3"]);
        assert!(pending_ids(&json!({"t":"feed","text":"x"})).is_empty());
    }

    fn sample(kind: Kind, ms: f64, outcome: Outcome) -> Sample {
        Sample { kind, ms, outcome }
    }

    #[test]
    fn report_counts_errors_and_ignores_them_in_latency() {
        let r = Report {
            samples: vec![
                sample(Kind::Event, 1.0, Outcome::Accepted),
                sample(Kind::Event, 2.0, Outcome::Accepted),
                sample(Kind::Event, 900.0, Outcome::NetError),
                sample(Kind::Permission, 1100.0, Outcome::Allow),
                sample(Kind::Permission, 110_000.0, Outcome::Empty),
            ],
        };
        assert_eq!(r.errors(Kind::Event), 1);
        assert_eq!(r.latencies(Kind::Event), [1.0, 2.0]);
        assert_eq!(r.count(Kind::Permission, Outcome::Allow), 1);
        let s = r.summary();
        assert!(s.contains("/hooks/event: 3 yêu cầu, 1 lỗi"), "{s}");
        assert!(s.contains("allow 1, deny 0, {} 1"), "{s}");
    }

    #[test]
    fn check_fails_on_event_errors_or_p99_over_limit() {
        let ok = Report {
            samples: vec![
                sample(Kind::Event, 1.0, Outcome::Accepted),
                sample(Kind::Event, 4.0, Outcome::Accepted),
            ],
        };
        assert!(ok.check(None).is_ok());
        assert!(ok.check(Some(5.0)).is_ok());
        let err = ok.check(Some(3.0)).unwrap_err();
        assert!(err.contains("p99") && err.contains("vượt"), "{err}");
        let bad = Report {
            samples: vec![sample(Kind::Event, 1.0, Outcome::HttpError(401))],
        };
        assert!(bad.check(None).unwrap_err().contains("lỗi"));
        // Lỗi ở /hooks/permission không làm hỏng kiểm p99 của sự kiện.
        let perm = Report {
            samples: vec![sample(Kind::Permission, 1.0, Outcome::NetError)],
        };
        assert!(perm.check(Some(5.0)).is_ok());
    }

    #[test]
    fn committed_fixtures_load_and_classify() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/hooks");
        let fixtures = load_fixtures(&dir).unwrap();
        assert!(fixtures.len() >= 30);
        assert!(fixtures.iter().any(|f| f.kind == Kind::Permission));
        assert!(fixtures.iter().any(|f| f.kind == Kind::Event));
        for f in &fixtures {
            let is_perm = f.value["hook_event_name"] == "PermissionRequest";
            assert_eq!(f.kind == Kind::Permission, is_perm, "{}", f.name);
        }
    }
}
