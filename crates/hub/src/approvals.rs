//! Luồng duyệt (`docs/PLAN.md` T3): giữ request `/hooks/permission` tới khi trưởng nhóm quyết định, hết giờ hoặc bị hủy.
//!
//! Quy tắc bất biến 1: chỉ có hai đường ra `allow`: người bấm allow (hoặc đếm ngược hợp lệ, §3.4), và policy AutoAllow
//! đã được store xác nhận ghi kiểm toán. Mọi nhánh khác (hết giờ, hủy, phiên kết thúc, client đóng kết nối, tắt hub,
//! lỗi ghi) trả `{}` để Claude Code tiếp tục hỏi trong terminal.
//!
//! Phần chưa có được thay bằng kiểu tạm trong file này: [`Policy`] (T8), [`ApprovalStore`] (T6), [`ApprovalNotice`]
//! (T5 đổi thành `ServerMsg`). Trạng thái agent (T4) chưa được cập nhật ở đây.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use hub_core::hook::HookInput;
use hub_core::schema::{AgentId, DecidedBy, Decision, Event, EventKind, RequestId, Risk};
use serde_json::{json, Value};
use time::OffsetDateTime;
use tokio::sync::{broadcast, oneshot};
use tokio::time::Instant;

/// Hub tự trả `{}` sau chừng này, trước timeout 120 s của hook (`hooks/arcturus-dogfood.json`).
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(110);

/// AutoAllow chờ store xác nhận đã ghi tối đa chừng này (PLAN T3 bước 6).
pub const DEFAULT_STORE_ACK_TIMEOUT: Duration = Duration::from_secs(1);

/// Giới hạn yêu cầu đang giữ: toàn hub và mỗi phiên (audit T3, finding 2).
pub const DEFAULT_MAX_PENDING: usize = 64;
pub const DEFAULT_MAX_PENDING_PER_SESSION: usize = 16;

/// Sức chứa kênh thông báo cho UI (`docs/PLAN.md` §4).
pub const NOTICE_CAPACITY: usize = 256;

// ---------------------------------------------------------------------------------------------------------------
// Policy tạm (T8 thay bằng `hub-core/src/policy/`)
// ---------------------------------------------------------------------------------------------------------------

/// Ngữ cảnh một yêu cầu duyệt, đưa cho policy.
pub struct ApprovalCtx<'a> {
    pub agent_id: &'a str,
    pub tool: &'a str,
    pub tool_input: &'a Value,
    pub cwd: &'a str,
}

/// Policy không có nhánh tự từ chối trong v0.
#[derive(Clone, Debug, PartialEq)]
pub enum PolicyOutcome {
    AutoAllow {
        reason: String,
        risk: Risk,
    },
    AskUser {
        risk: Risk,
        countdown: Option<Duration>,
    },
}

pub trait Policy: Send + Sync {
    fn decide(&self, ctx: &ApprovalCtx<'_>) -> PolicyOutcome;
}

/// Policy của T3: luôn hỏi người.
pub struct AlwaysAsk;

impl Policy for AlwaysAsk {
    fn decide(&self, _ctx: &ApprovalCtx<'_>) -> PolicyOutcome {
        PolicyOutcome::AskUser {
            risk: Risk::Medium,
            countdown: None,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Store tạm (T6 thay bằng writer SQLite)
// ---------------------------------------------------------------------------------------------------------------

/// Cột `approvals.status` (§3.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Allowed,
    Denied,
    TimedOut,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalRecord {
    pub request_id: RequestId,
    pub session_id: String,
    pub agent_id: AgentId,
    pub tool: String,
    pub status: ApprovalStatus,
    pub decided_by: Option<DecidedBy>,
    pub at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreError(pub String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lỗi ghi store: {}", self.0)
    }
}

impl std::error::Error for StoreError {}

/// Kết quả commit một bản ghi. Sender bị drop cũng tính là chưa ghi.
pub type StoreAck = oneshot::Receiver<Result<(), StoreError>>;

pub trait ApprovalStore: Send + Sync {
    /// Gửi bản ghi cho writer. Người gọi không cần chờ thì bỏ receiver.
    fn record(&self, rec: ApprovalRecord) -> StoreAck;
}

/// Chưa có SQLite (T6): không ghi được gì, nên mọi AutoAllow rơi về hỏi người.
pub struct NoStore;

impl ApprovalStore for NoStore {
    fn record(&self, _rec: ApprovalRecord) -> StoreAck {
        let (_tx, rx) = oneshot::channel();
        rx
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Kiểu của luồng duyệt
// ---------------------------------------------------------------------------------------------------------------

/// Một yêu cầu đang chờ. T5 dựng `ApprovalView` từ đây (kèm normalize của T8).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingApproval {
    pub request_id: RequestId,
    pub session_id: String,
    pub agent_id: AgentId,
    pub tool: String,
    /// `tool_input` gốc, chưa chuẩn hóa: chỉ để hiển thị, không bao giờ log.
    pub tool_input: Value,
    pub cwd: String,
    pub risk: Risk,
    pub created_at: OffsetDateTime,
    pub deadline_at: OffsetDateTime,
    /// Đếm ngược phủ quyết (§3.4). Luôn `None` khi đội tạm dừng.
    pub countdown: Option<Duration>,
}

/// Thông báo cho UI. T5 ánh xạ sang `ServerMsg::ApprovalRequested` / `ApprovalResolved`.
#[derive(Clone, Debug, PartialEq)]
pub enum ApprovalNotice {
    Requested(PendingApproval),
    /// `decision` là `None` khi hết giờ hoặc bị hủy (hub trả `{}`).
    Resolved {
        request_id: RequestId,
        decision: Option<Decision>,
        decided_by: DecidedBy,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decided {
    pub decision: Decision,
    pub by: DecidedBy,
    pub message: Option<String>,
}

/// Kết quả xử lý một `/hooks/permission`.
#[derive(Clone, Debug, PartialEq)]
pub enum PermissionOutcome {
    /// Policy AutoAllow, store đã xác nhận ghi.
    AutoAllowed,
    Decided(Decided),
    /// Phiên kết thúc, được trả lời ở terminal, client đóng kết nối, hoặc hub đang tắt.
    Cancelled,
    TimedOut,
    /// Body không phải `PermissionRequest` (hook cấu hình sai): không quyết định.
    NotPermissionRequest,
    /// Hàng đợi đầy ([`ApprovalsConfig::max_pending`]): không giữ, để terminal hỏi.
    Overloaded,
}

/// Ai gửi lệnh `Decide` (§3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecideOrigin {
    User,
    Countdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldReason {
    NoCountdown,
    NotElapsed,
    Paused,
    /// Đếm ngược chỉ được allow.
    CountdownCannotDeny,
}

/// Kết quả `Decide`. T5 đổi `Expired` thành `Feed` "Yêu cầu đã hết hạn", `Held` thành "Cần trưởng nhóm bấm duyệt".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecideResult {
    Resolved,
    Expired,
    Held(HeldReason),
}

/// JSON trả cho Claude Code. Đây là chỗ duy nhất sinh `allow`; mọi nhánh khác là `{}`.
pub fn response_for(outcome: &PermissionOutcome) -> Value {
    match outcome {
        PermissionOutcome::AutoAllowed => decision_json(json!({ "behavior": "allow" })),
        PermissionOutcome::Decided(Decided {
            decision: Decision::Allow,
            ..
        }) => decision_json(json!({ "behavior": "allow" })),
        PermissionOutcome::Decided(Decided {
            decision: Decision::Deny,
            message,
            ..
        }) => {
            let message = match message.as_deref().map(str::trim) {
                Some(m) if !m.is_empty() => format!("Trưởng nhóm từ chối: {m}"),
                _ => "Trưởng nhóm từ chối".to_owned(),
            };
            decision_json(json!({ "behavior": "deny", "message": message }))
        }
        PermissionOutcome::Cancelled
        | PermissionOutcome::TimedOut
        | PermissionOutcome::NotPermissionRequest
        | PermissionOutcome::Overloaded => json!({}),
    }
}

fn new_request_id() -> RequestId {
    format!("req_{}", ulid::Ulid::generate())
}

fn decision_json(decision: Value) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": decision,
        }
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Hàng đợi duyệt
// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ApprovalsConfig {
    pub timeout: Duration,
    pub store_ack_timeout: Duration,
    /// Tối đa yêu cầu đang giữ (giữ RAM trong ngân sách, quy tắc bất biến 6). Vượt thì trả `{}` ngay.
    pub max_pending: usize,
    pub max_pending_per_session: usize,
}

impl Default for ApprovalsConfig {
    fn default() -> Self {
        ApprovalsConfig {
            timeout: DEFAULT_APPROVAL_TIMEOUT,
            store_ack_timeout: DEFAULT_STORE_ACK_TIMEOUT,
            max_pending: DEFAULT_MAX_PENDING,
            max_pending_per_session: DEFAULT_MAX_PENDING_PER_SESSION,
        }
    }
}

struct Entry {
    info: PendingApproval,
    started: Instant,
    reply: oneshot::Sender<Decided>,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<RequestId, Entry>,
    paused: bool,
    closed: bool,
}

/// Hàng đợi yêu cầu duyệt (`Hub::pending` trong §4).
///
/// Dùng `std::sync::Mutex` (không giữ qua `.await`) để guard hủy trong `Drop` khóa được khi client đóng kết nối.
pub struct Approvals {
    cfg: ApprovalsConfig,
    policy: Box<dyn Policy>,
    store: Box<dyn ApprovalStore>,
    inner: Mutex<Inner>,
    notices: broadcast::Sender<ApprovalNotice>,
}

impl Approvals {
    pub fn new(
        cfg: ApprovalsConfig,
        policy: Box<dyn Policy>,
        store: Box<dyn ApprovalStore>,
    ) -> Self {
        let (notices, _) = broadcast::channel(NOTICE_CAPACITY);
        Approvals {
            cfg,
            policy,
            store,
            inner: Mutex::new(Inner::default()),
            notices,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ApprovalNotice> {
        self.notices.subscribe()
    }

    /// Các yêu cầu đang chờ, cũ trước (cho `Snapshot` ở T5).
    pub fn snapshot(&self) -> Vec<PendingApproval> {
        let mut out: Vec<_> = self
            .lock()
            .entries
            .values()
            .map(|e| e.info.clone())
            .collect();
        out.sort_by_key(|p| p.created_at);
        out
    }

    pub fn is_paused(&self) -> bool {
        self.lock().paused
    }

    /// `TeamPause`/`TeamResume` (T5 gọi). Tạm dừng xóa đếm ngược của mọi yêu cầu đang chờ; tiếp tục không khôi phục.
    pub fn set_paused(&self, paused: bool) {
        let mut inner = self.lock();
        inner.paused = paused;
        if paused {
            for e in inner.entries.values_mut() {
                e.info.countdown = None;
            }
        }
    }

    /// Xử lý một `PermissionRequest`. `ev` là `hook::to_event(input)`. Nhận `input` theo giá trị để nhả JSON thô
    /// trước khi chờ (tới 110 s): chỉ giữ lại các trường cần dùng.
    pub async fn request(&self, input: HookInput, ev: &Event) -> PermissionOutcome {
        if ev.kind != EventKind::ApprovalRequested {
            tracing::warn!(
                event = ?input.event_name(),
                "/hooks/permission nhận sự kiện không phải PermissionRequest, không quyết định"
            );
            return PermissionOutcome::NotPermissionRequest;
        }
        let tool = input.tool_name().unwrap_or("").to_owned();
        let cwd = input.cwd().unwrap_or("").to_owned();
        let mut raw = input.raw;
        let tool_input = raw
            .get_mut("tool_input")
            .map(Value::take)
            .unwrap_or(Value::Null);
        drop(raw);

        let outcome = self.policy.decide(&ApprovalCtx {
            agent_id: &ev.agent.id,
            tool: &tool,
            tool_input: &tool_input,
            cwd: &cwd,
        });
        let (risk, countdown) = match outcome {
            PolicyOutcome::AutoAllow { reason, risk } => {
                // Lượt AutoAllow có id riêng: nếu rơi về hỏi người, bản ghi `allowed/policy` (có thể commit muộn)
                // không trùng id với yêu cầu người sẽ quyết định.
                let auto_id = new_request_id();
                if self.is_paused() {
                    tracing::info!(request_id = %auto_id, "đội tạm dừng: AutoAllow chuyển thành hỏi người");
                } else if self.auto_allow_recorded(&auto_id, ev, &tool).await {
                    tracing::info!(request_id = %auto_id, tool = ?tool, reason = ?reason, "policy tự duyệt");
                    return PermissionOutcome::AutoAllowed;
                }
                (risk, None)
            }
            PolicyOutcome::AskUser { risk, countdown } => (risk, countdown),
        };

        let request_id = new_request_id();
        let (reply, mut rx) = oneshot::channel();
        let info = {
            let mut inner = self.lock();
            if inner.closed {
                return PermissionOutcome::Cancelled;
            }
            if inner.entries.len() >= self.cfg.max_pending
                || inner
                    .entries
                    .values()
                    .filter(|e| e.info.session_id == ev.session_id)
                    .count()
                    >= self.cfg.max_pending_per_session
            {
                tracing::warn!(
                    session = ?ev.session_id,
                    pending = inner.entries.len(),
                    "hàng đợi duyệt đầy, trả {{}} để Claude Code hỏi trong terminal"
                );
                return PermissionOutcome::Overloaded;
            }
            let info = PendingApproval {
                request_id: request_id.clone(),
                session_id: ev.session_id.clone(),
                agent_id: ev.agent.id.clone(),
                tool,
                tool_input,
                cwd,
                risk,
                created_at: ev.ts,
                deadline_at: ev.ts + self.cfg.timeout,
                countdown: if inner.paused { None } else { countdown },
            };
            inner.entries.insert(
                request_id.clone(),
                Entry {
                    info: info.clone(),
                    started: Instant::now(),
                    reply,
                },
            );
            info
        };
        self.record(&info, ApprovalStatus::Pending, None);
        // `?`: chuỗi từ agent không tin cậy, Debug escape xuống dòng (chặn giả dòng log).
        tracing::info!(%request_id, session = ?info.session_id, tool = ?info.tool, "đang giữ yêu cầu duyệt");
        let _ = self.notices.send(ApprovalNotice::Requested(info));

        // Future bị drop (client đóng kết nối) thì guard hủy yêu cầu; mọi nhánh dưới tự dọn nên tắt guard.
        let mut guard = CancelOnDrop {
            approvals: self,
            request_id: Some(request_id.clone()),
        };
        let outcome = tokio::select! {
            biased;
            r = &mut rx => match r {
                Ok(d) => PermissionOutcome::Decided(d),
                Err(_) => PermissionOutcome::Cancelled,
            },
            () = tokio::time::sleep(self.cfg.timeout) => match self.take(&request_id) {
                Some(entry) => {
                    tracing::info!(%request_id, "hết giờ chờ duyệt, trả {{}}");
                    self.finish(entry, ApprovalStatus::TimedOut, DecidedBy::Timeout, None);
                    PermissionOutcome::TimedOut
                }
                // `decide` hoặc hủy vừa lấy yêu cầu: kết quả đã (hoặc sắp) nằm trong `rx`.
                None => match rx.await {
                    Ok(d) => PermissionOutcome::Decided(d),
                    Err(_) => PermissionOutcome::Cancelled,
                },
            },
        };
        guard.request_id = None;
        outcome
    }

    /// Ghi bản ghi `allowed/policy` và chờ xác nhận. `false` khi lỗi, kênh đóng, quá giờ, hoặc đội vừa tạm dừng;
    /// khi đó ghi thêm `cancelled` cho cùng id (bản `allowed` có thể vẫn được commit muộn; T6 giữ thứ tự ghi).
    async fn auto_allow_recorded(&self, request_id: &str, ev: &Event, tool: &str) -> bool {
        let rec = |status, decided_by| ApprovalRecord {
            request_id: request_id.to_owned(),
            session_id: ev.session_id.clone(),
            agent_id: ev.agent.id.clone(),
            tool: tool.to_owned(),
            status,
            decided_by,
            at: OffsetDateTime::now_utc(),
        };
        let ack = self
            .store
            .record(rec(ApprovalStatus::Allowed, Some(DecidedBy::Policy)));
        let ok = match tokio::time::timeout(self.cfg.store_ack_timeout, ack).await {
            Ok(Ok(Ok(()))) => true,
            Ok(Ok(Err(err))) => {
                tracing::error!(request_id, %err, "không ghi được kiểm toán AutoAllow, chuyển sang hỏi người");
                false
            }
            Ok(Err(_)) => {
                tracing::error!(
                    request_id,
                    "store không xác nhận AutoAllow (kênh đóng), chuyển sang hỏi người"
                );
                false
            }
            Err(_) => {
                tracing::error!(
                    request_id,
                    "store không xác nhận AutoAllow kịp, chuyển sang hỏi người"
                );
                false
            }
        };
        let ok = if ok && self.is_paused() {
            tracing::info!(
                request_id,
                "đội tạm dừng trong lúc ghi: AutoAllow chuyển thành hỏi người"
            );
            false
        } else {
            ok
        };
        if !ok {
            drop(
                self.store
                    .record(rec(ApprovalStatus::Cancelled, Some(DecidedBy::Cancelled))),
            );
        }
        ok
    }

    /// Lệnh `Decide` từ UI (T5 gọi).
    pub fn decide(
        &self,
        request_id: &str,
        decision: Decision,
        origin: DecideOrigin,
        message: Option<String>,
    ) -> DecideResult {
        let entry = {
            let mut inner = self.lock();
            let Some(e) = inner.entries.get(request_id) else {
                return DecideResult::Expired;
            };
            if origin == DecideOrigin::Countdown {
                let held = if decision != Decision::Allow {
                    Some(HeldReason::CountdownCannotDeny)
                } else if inner.paused {
                    Some(HeldReason::Paused)
                } else {
                    match e.info.countdown {
                        None => Some(HeldReason::NoCountdown),
                        Some(c) if e.started.elapsed() < c => Some(HeldReason::NotElapsed),
                        Some(_) => None,
                    }
                };
                if let Some(reason) = held {
                    tracing::info!(
                        request_id,
                        ?reason,
                        "giữ nguyên yêu cầu: đếm ngược không hợp lệ"
                    );
                    return DecideResult::Held(reason);
                }
            }
            match inner.entries.remove(request_id) {
                Some(e) => e,
                None => return DecideResult::Expired,
            }
        };
        let by = match origin {
            DecideOrigin::User => DecidedBy::User,
            DecideOrigin::Countdown => DecidedBy::Countdown,
        };
        let status = match decision {
            Decision::Allow => ApprovalStatus::Allowed,
            Decision::Deny => ApprovalStatus::Denied,
        };
        let Entry { info, reply, .. } = entry;
        match reply.send(Decided {
            decision,
            by,
            message,
        }) {
            Ok(()) => {
                tracing::info!(request_id, ?decision, ?by, "đã quyết định");
                self.record(&info, status, Some(by));
                let _ = self.notices.send(ApprovalNotice::Resolved {
                    request_id: info.request_id,
                    decision: Some(decision),
                    decided_by: by,
                });
                DecideResult::Resolved
            }
            // Handler đã đi (không xảy ra khi guard chạy đúng): coi như hủy.
            Err(_) => {
                self.record(&info, ApprovalStatus::Cancelled, Some(DecidedBy::Cancelled));
                DecideResult::Expired
            }
        }
    }

    /// Hủy yêu cầu đang chờ theo sự kiện từ `/hooks/event`. Trả số yêu cầu bị hủy.
    ///
    /// - `SessionEnd`: mọi yêu cầu của phiên.
    /// - Đã được trả lời ở terminal (Q1, Q1b): sự kiện kế tiếp của **cùng agent** cho thấy hộp thoại đã đóng:
    ///   `PostToolUse`/`PostToolUseFailure` của đúng tool và `tool_input` đó, hoặc `Stop`, `StopFailure`,
    ///   `UserPromptSubmit`, `SubagentStop`. `Notification` (tới sau `PermissionRequest`) và sự kiện của tool khác
    ///   chạy song song không hủy.
    pub fn on_event(&self, ev: &Event, input: &HookInput) -> usize {
        match ev.kind {
            EventKind::SessionEnded => {
                self.cancel_where(|p| p.session_id == ev.session_id, "phiên kết thúc")
            }
            EventKind::ToolPost | EventKind::ToolFailed => {
                let (Some(tool), Some(tool_input)) = (input.tool_name(), input.tool_input()) else {
                    return 0;
                };
                self.cancel_where(
                    |p| p.agent_id == ev.agent.id && p.tool == tool && &p.tool_input == tool_input,
                    "tool đã chạy xong: yêu cầu được trả lời ở nơi khác",
                )
            }
            EventKind::TurnStopped
            | EventKind::TurnFailed
            | EventKind::PromptSubmitted
            | EventKind::SubagentStopped => self.cancel_where(
                |p| p.agent_id == ev.agent.id,
                "agent đã sang bước khác: yêu cầu được trả lời ở nơi khác",
            ),
            _ => 0,
        }
    }

    /// Tắt hub: trả `{}` cho mọi yêu cầu đang chờ, từ chối yêu cầu mới. Trả số yêu cầu bị hủy.
    pub fn shutdown(&self) -> usize {
        let drained: Vec<Entry> = {
            let mut inner = self.lock();
            inner.closed = true;
            inner.entries.drain().map(|(_, e)| e).collect()
        };
        let n = drained.len();
        for entry in drained {
            self.finish(entry, ApprovalStatus::Cancelled, DecidedBy::Cancelled, None);
        }
        if n > 0 {
            tracing::info!(n, "tắt hub: trả {{}} cho các yêu cầu đang chờ");
        }
        n
    }

    fn cancel_where(&self, pred: impl Fn(&PendingApproval) -> bool, why: &str) -> usize {
        let taken: Vec<Entry> = {
            let mut inner = self.lock();
            let ids: Vec<RequestId> = inner
                .entries
                .iter()
                .filter(|(_, e)| pred(&e.info))
                .map(|(id, _)| id.clone())
                .collect();
            ids.iter()
                .filter_map(|id| inner.entries.remove(id))
                .collect()
        };
        let n = taken.len();
        for entry in taken {
            tracing::info!(request_id = %entry.info.request_id, why, "hủy yêu cầu duyệt, trả {{}}");
            self.finish(entry, ApprovalStatus::Cancelled, DecidedBy::Cancelled, None);
        }
        n
    }

    fn take(&self, request_id: &str) -> Option<Entry> {
        self.lock().entries.remove(request_id)
    }

    /// Ghi trạng thái cuối, báo UI, rồi drop `reply` (handler đang chờ rơi vào nhánh hủy và trả `{}`).
    fn finish(
        &self,
        entry: Entry,
        status: ApprovalStatus,
        by: DecidedBy,
        decision: Option<Decision>,
    ) {
        let Entry { info, reply, .. } = entry;
        self.record(&info, status, Some(by));
        let _ = self.notices.send(ApprovalNotice::Resolved {
            request_id: info.request_id,
            decision,
            decided_by: by,
        });
        drop(reply);
    }

    /// Ghi không chờ xác nhận (chỉ AutoAllow cần chờ).
    fn record(&self, info: &PendingApproval, status: ApprovalStatus, by: Option<DecidedBy>) {
        drop(self.store.record(ApprovalRecord {
            request_id: info.request_id.clone(),
            session_id: info.session_id.clone(),
            agent_id: info.agent_id.clone(),
            tool: info.tool.clone(),
            status,
            decided_by: by,
            at: OffsetDateTime::now_utc(),
        }));
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Mutex bị poison chỉ khi một thread panic lúc đang giữ; dữ liệu vẫn nhất quán vì mọi thao tác là đơn bước.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Hủy yêu cầu nếu future của handler bị drop giữa chừng (client đóng kết nối, Q1b).
struct CancelOnDrop<'a> {
    approvals: &'a Approvals,
    request_id: Option<RequestId>,
}

impl Drop for CancelOnDrop<'_> {
    fn drop(&mut self) {
        let Some(id) = self.request_id.take() else {
            return;
        };
        if let Some(entry) = self.approvals.take(&id) {
            tracing::info!(request_id = %id, "kết nối hook đóng trước khi có quyết định, hủy yêu cầu");
            self.approvals
                .finish(entry, ApprovalStatus::Cancelled, DecidedBy::Cancelled, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decided(decision: Decision, message: Option<&str>) -> PermissionOutcome {
        PermissionOutcome::Decided(Decided {
            decision,
            by: DecidedBy::User,
            message: message.map(str::to_owned),
        })
    }

    fn behavior(v: &Value) -> Option<&str> {
        v.pointer("/hookSpecificOutput/decision/behavior")
            .and_then(Value::as_str)
    }

    #[test]
    fn response_for_allow_paths() {
        for o in [
            PermissionOutcome::AutoAllowed,
            decided(Decision::Allow, None),
        ] {
            let v = response_for(&o);
            assert_eq!(behavior(&v), Some("allow"), "{o:?}");
            assert_eq!(
                v.pointer("/hookSpecificOutput/hookEventName"),
                Some(&json!("PermissionRequest"))
            );
        }
    }

    #[test]
    fn response_for_deny_has_message() {
        let v = response_for(&decided(Decision::Deny, Some("  dùng nhánh khác ")));
        assert_eq!(behavior(&v), Some("deny"));
        assert_eq!(
            v.pointer("/hookSpecificOutput/decision/message"),
            Some(&json!("Trưởng nhóm từ chối: dùng nhánh khác"))
        );
        let v = response_for(&decided(Decision::Deny, Some("   ")));
        assert_eq!(
            v.pointer("/hookSpecificOutput/decision/message"),
            Some(&json!("Trưởng nhóm từ chối"))
        );
    }

    #[test]
    fn response_for_every_non_decision_is_empty_object() {
        for o in [
            PermissionOutcome::Cancelled,
            PermissionOutcome::TimedOut,
            PermissionOutcome::NotPermissionRequest,
            PermissionOutcome::Overloaded,
        ] {
            assert_eq!(response_for(&o), json!({}), "{o:?}");
        }
    }
}
