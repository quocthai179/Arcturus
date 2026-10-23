//! Luồng duyệt (`docs/PLAN.md` T3): giữ request `/hooks/permission` tới khi trưởng nhóm quyết định, hết giờ hoặc bị hủy.
//!
//! Quy tắc bất biến 1: chỉ có hai đường ra `allow`: người bấm allow (hoặc đếm ngược hợp lệ, §3.4), và policy AutoAllow
//! đã được store xác nhận ghi kiểm toán. Mọi nhánh khác (hết giờ, hủy, phiên kết thúc, client đóng kết nối, tắt hub,
//! lỗi ghi) trả `{}` để Claude Code tiếp tục hỏi trong terminal.
//!
//! Phần chưa có được thay bằng kiểu tạm trong file này: [`Policy`] (T8), [`ApprovalStore`] (T6). Thông báo cho UI
//! đi qua kênh broadcast `ServerMsg` chung của hub (T5): thẻ duyệt là `ApprovalView` đã cắt nội dung, `tool_input`
//! đầy đủ chỉ nằm trong hàng đợi (lấy qua [`Approvals::tool_input`]). Trạng thái agent do `crate::hub` cập nhật.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use hub_core::hook::HookInput;
use hub_core::persona::fnv1a;
pub use hub_core::protocol::DecideOrigin;
use hub_core::protocol::{ApprovalView, HandoffView, ServerMsg};
use hub_core::schema::{AgentId, DecidedBy, Decision, Event, EventKind, RequestId, Risk};
use serde_json::{json, Value};
use time::OffsetDateTime;
use tokio::sync::{broadcast, oneshot};
use tokio::time::Instant;

use crate::view::{approval_view, handoff_view, is_too_long, non_content_text, untruncated_text};

/// Hub tự trả `{}` sau chừng này, trước timeout 120 s của hook (`hooks/arcturus-dogfood.json`).
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(110);

/// AutoAllow chờ store xác nhận đã ghi tối đa chừng này (PLAN T3 bước 6).
pub const DEFAULT_STORE_ACK_TIMEOUT: Duration = Duration::from_secs(1);

/// Giới hạn yêu cầu đang giữ: toàn hub và mỗi phiên (audit T3, finding 2).
pub const DEFAULT_MAX_PENDING: usize = 64;
pub const DEFAULT_MAX_PENDING_PER_SESSION: usize = 16;

/// Trần tổng byte `tool_input` đang giữ (T5c · ARC-43, quy tắc bất biến 6): 64 Write ~1 MB từng đẩy hub lên 93 MB.
/// Khớp `Config::default()`.
pub const DEFAULT_HELD_INPUT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Độ dài tối đa (byte) của các trường do agent gửi được sao chép vào hàng đợi, thẻ duyệt và trạng thái (audit ARC-43
/// finding 1). Vượt thì trả `{}`: Claude Code hỏi trong terminal.
pub const MAX_TOOL_NAME_BYTES: usize = 256;
pub const MAX_ID_BYTES: usize = 256;
pub const MAX_CWD_BYTES: usize = 32 * 1024;

/// Sàn thời gian duyệt (T5d · ARC-44, F6 biện pháp B4): `allow` tới sớm hơn chừng này kể từ lúc thẻ được phát
/// (broadcast) bị giữ lại. Khớp `Config::default()`.
pub const DEFAULT_MIN_REVIEW: Duration = Duration::from_millis(1000);

/// Ngưỡng mặc định "quá dài để duyệt trên thẻ" (khớp `Config::default()`).
pub const DEFAULT_HANDOFF_MAX_BYTES: usize = 4096;
pub const DEFAULT_HANDOFF_MAX_LINES: usize = 50;

/// Tối đa thẻ "duyệt trong terminal" giữ cùng lúc.
pub const MAX_HANDOFFS: usize = 64;

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
    /// Lệnh quá dài: trả về terminal, hub không quyết định (T5b, L2).
    HandedOff,
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
    ///
    /// Có thể được gọi khi `Approvals` đang giữ khóa: phải không chặn (chỉ đẩy vào kênh), không gọi ngược `Approvals`.
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
    /// Lệnh quá dài để duyệt trên thẻ: không giữ, người duyệt trong terminal (quy tắc bất biến 3).
    HandedOff {
        handoff_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldReason {
    NoCountdown,
    NotElapsed,
    Paused,
    /// Đếm ngược chỉ được allow.
    CountdownCannotDeny,
    /// Allow thẻ đã cắt nội dung khi chưa mở bản đầy đủ.
    NotReviewed,
    /// Allow (của người hay đếm ngược) tới sớm hơn [`ApprovalsConfig::min_review`] kể từ lúc thẻ được phát.
    TooFast,
}

/// Kết quả `Decide`. T5 đổi `Expired` thành `Feed` "Yêu cầu đã hết hạn", `Held` thành "Cần trưởng nhóm bấm duyệt".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecideResult {
    Resolved,
    Expired,
    Held(HeldReason),
    /// Id là thẻ "duyệt trong terminal": hub không bao giờ quyết định cho nó.
    HandedOff,
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
        | PermissionOutcome::Overloaded
        | PermissionOutcome::HandedOff { .. } => json!({}),
    }
}

/// Hash ổn định của `tool_input` (serde_json sắp khóa object theo thứ tự, nên cùng nội dung cho cùng chuỗi).
fn input_hash(tool_input: &Value) -> u64 {
    fnv1a(&tool_input.to_string())
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
    /// Trần tổng byte `tool_input` đang giữ (ước lượng bằng [`held_size`]). Vượt thì trả `{}` ngay.
    pub held_input_max_bytes: usize,
    /// Sàn thời gian duyệt cho mọi `allow`, tính từ lúc thẻ được phát (`Duration::ZERO` là tắt). Không áp cho deny.
    pub min_review: Duration,
    /// Ngưỡng trả về terminal (`[hub] handoff_max_bytes`, `handoff_max_lines`).
    pub handoff_max_bytes: usize,
    pub handoff_max_lines: usize,
}

impl Default for ApprovalsConfig {
    fn default() -> Self {
        ApprovalsConfig {
            timeout: DEFAULT_APPROVAL_TIMEOUT,
            store_ack_timeout: DEFAULT_STORE_ACK_TIMEOUT,
            max_pending: DEFAULT_MAX_PENDING,
            max_pending_per_session: DEFAULT_MAX_PENDING_PER_SESSION,
            held_input_max_bytes: DEFAULT_HELD_INPUT_MAX_BYTES,
            min_review: DEFAULT_MIN_REVIEW,
            handoff_max_bytes: DEFAULT_HANDOFF_MAX_BYTES,
            handoff_max_lines: DEFAULT_HANDOFF_MAX_LINES,
        }
    }
}

/// Thẻ "duyệt trong terminal" đang hiện. Không giữ `tool_input` (có thể tới 1 MB), chỉ giữ hash để khớp `PostToolUse`.
struct Handoff {
    view: HandoffView,
    session_id: String,
    agent_id: AgentId,
    tool: String,
    input_hash: u64,
}

struct Entry {
    info: PendingApproval,
    /// Thẻ duyệt dựng một lần lúc tạo (không dựng lại dưới khóa, audit T5a finding 2, 6).
    view: ApprovalView,
    started: Instant,
    reply: oneshot::Sender<Decided>,
    /// Bản đầy đủ đã được mở qua `GET /approvals/{id}/input` (bắt buộc trước khi allow thẻ `truncated`).
    reviewed: bool,
    /// Phần của entry này trong [`Inner::held_bytes`].
    held_bytes: usize,
}

#[derive(Default)]
struct Inner {
    /// Chỉ thêm/bớt qua [`Inner::insert_entry`], [`Inner::remove_entry`], [`Inner::drain_entries`] để `held_bytes`
    /// không bao giờ lệch.
    entries: HashMap<RequestId, Entry>,
    /// Tổng [`Entry::held_bytes`] của mọi entry đang giữ.
    held_bytes: usize,
    /// Byte đã đặt chỗ cho body `/hooks/permission` đang nhận ([`Approvals::reserve_body`]).
    inflight_bytes: usize,
    /// Cũ trước; tối đa [`MAX_HANDOFFS`], đầy thì bỏ thẻ cũ nhất.
    handoffs: VecDeque<Handoff>,
    paused: bool,
    closed: bool,
}

impl Inner {
    fn insert_entry(&mut self, id: RequestId, entry: Entry) {
        self.held_bytes += entry.held_bytes;
        if let Some(old) = self.entries.insert(id, entry) {
            // Không xảy ra (id là ULID mới); vẫn giữ bộ đếm đúng.
            self.held_bytes = self.held_bytes.saturating_sub(old.held_bytes);
        }
    }

    fn remove_entry(&mut self, id: &str) -> Option<Entry> {
        let e = self.entries.remove(id)?;
        self.held_bytes = self.held_bytes.saturating_sub(e.held_bytes);
        Some(e)
    }

    fn drain_entries(&mut self) -> Vec<Entry> {
        self.held_bytes = 0;
        self.entries.drain().map(|(_, e)| e).collect()
    }
}

/// Chỗ đặt cho một body `/hooks/permission` đang nhận và parse: trả lại khi drop.
pub struct BodyReservation<'a> {
    approvals: &'a Approvals,
    bytes: usize,
}

impl BodyReservation<'_> {
    /// Nới chỗ đặt lên `bytes` (ví dụ ước lượng sau khi đếm nút JSON). `false`: không còn chỗ, chỗ cũ giữ nguyên.
    pub fn grow_to(&mut self, bytes: usize) -> bool {
        if bytes <= self.bytes {
            return true;
        }
        let mut inner = self.approvals.lock();
        let extra = bytes - self.bytes;
        let used = inner.held_bytes.saturating_add(inner.inflight_bytes);
        if used.saturating_add(extra) > self.approvals.cfg.held_input_max_bytes {
            return false;
        }
        inner.inflight_bytes += extra;
        self.bytes = bytes;
        true
    }
}

impl Drop for BodyReservation<'_> {
    fn drop(&mut self) {
        let mut inner = self.approvals.lock();
        inner.inflight_bytes = inner.inflight_bytes.saturating_sub(self.bytes);
    }
}

/// Phần đầu ước lượng của mỗi lần cấp phát trên heap (bộ cấp phát của hệ điều hành).
const ALLOC_OVERHEAD: usize = 16;
/// Ước lượng cho mỗi phần tử `BTreeMap` (nút B-tree chia đều cho phần tử).
const MAP_ENTRY_OVERHEAD: usize = 100;

/// Ước lượng byte RAM một giá trị JSON đã parse chiếm: theo `capacity` của chuỗi và mảng (không chỉ `len`), cộng phần
/// đầu mỗi lần cấp phát và mỗi phần tử object (audit ARC-43 finding 3). Không serialize lại (đầu vào có thể tới 1 MB).
pub fn held_size(v: &Value) -> usize {
    let node = std::mem::size_of::<Value>();
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) => node,
        Value::String(s) => node + s.capacity() + ALLOC_OVERHEAD,
        Value::Array(items) => {
            node + ALLOC_OVERHEAD
                + (items.capacity() - items.len()) * node
                + items.iter().map(held_size).sum::<usize>()
        }
        Value::Object(map) => {
            node + map
                .iter()
                .map(|(k, v)| MAP_ENTRY_OVERHEAD + k.capacity() + ALLOC_OVERHEAD + held_size(v))
                .sum::<usize>()
        }
    }
}

/// Byte RAM của thẻ duyệt giữ cùng entry (thẻ có thể lớn hơn `tool_input`: ký tự ẩn hiện thành `⟨U+XXXX⟩` dài gấp 4).
fn view_size(v: &ApprovalView) -> usize {
    let s = |x: &String| x.capacity() + ALLOC_OVERHEAD;
    std::mem::size_of::<ApprovalView>()
        + s(&v.request_id)
        + s(&v.agent_id)
        + s(&v.agent_name)
        + s(&v.tool)
        + s(&v.command_original)
        + s(&v.command_normalized)
        + s(&v.group_key)
        + v.tool_options
            .iter()
            .map(|(k, x)| MAP_ENTRY_OVERHEAD + s(k) + s(x))
            .sum::<usize>()
        + v.danger_spans.capacity() * std::mem::size_of::<(usize, usize)>()
        + v.suspicions.capacity() * 8
}

/// Hàng đợi yêu cầu duyệt (`Hub::pending` trong §4).
///
/// Dùng `std::sync::Mutex` (không giữ qua `.await`) để guard hủy trong `Drop` khóa được khi client đóng kết nối.
pub struct Approvals {
    cfg: ApprovalsConfig,
    policy: Box<dyn Policy>,
    store: Box<dyn ApprovalStore>,
    inner: Mutex<Inner>,
    tx: broadcast::Sender<ServerMsg>,
}

impl Approvals {
    /// `tx`: kênh broadcast chung của hub (`ApprovalRequested`, `ApprovalResolved`).
    pub fn new(
        cfg: ApprovalsConfig,
        policy: Box<dyn Policy>,
        store: Box<dyn ApprovalStore>,
        tx: broadcast::Sender<ServerMsg>,
    ) -> Self {
        Approvals {
            cfg,
            policy,
            store,
            inner: Mutex::new(Inner::default()),
            tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        self.tx.subscribe()
    }

    /// Các yêu cầu đang chờ, cũ trước.
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

    /// Thẻ duyệt của mọi yêu cầu đang chờ, cũ trước (cho `Snapshot`, T5b).
    pub fn views(&self) -> Vec<ApprovalView> {
        let mut out: Vec<_> = self
            .lock()
            .entries
            .values()
            .map(|e| e.view.clone())
            .collect();
        out.sort_by_key(|v| v.created_at);
        out
    }

    /// Tổng byte `tool_input` đang giữ (ước lượng, xem [`held_size`]).
    pub fn held_bytes(&self) -> usize {
        self.lock().held_bytes
    }

    /// Đặt chỗ `declared` byte (thường là `Content-Length`) cho một body `/hooks/permission` TRƯỚC khi đọc nó, trong
    /// cùng ngân sách [`ApprovalsConfig::held_input_max_bytes`] với các yêu cầu đang giữ. `None`: không còn chỗ, handler
    /// trả `{}` ngay mà không đọc body (nhiều body ~1 MB về cùng lúc từng đẩy RSS lên ~50 MB, đo ở T5c).
    pub fn reserve_body(&self, declared: usize) -> Option<BodyReservation<'_>> {
        let mut inner = self.lock();
        let used = inner.held_bytes.saturating_add(inner.inflight_bytes);
        if used.saturating_add(declared) > self.cfg.held_input_max_bytes {
            return None;
        }
        inner.inflight_bytes += declared;
        Some(BodyReservation {
            approvals: self,
            bytes: declared,
        })
    }

    /// Tổng byte đang đặt chỗ cho body đang nhận.
    pub fn inflight_body_bytes(&self) -> usize {
        self.lock().inflight_bytes
    }

    pub fn pending(&self, request_id: &str) -> Option<PendingApproval> {
        self.lock().entries.get(request_id).map(|e| e.info.clone())
    }

    /// `tool_input` đầy đủ của yêu cầu đang chờ (chỉ đọc).
    pub fn tool_input(&self, request_id: &str) -> Option<Value> {
        self.lock()
            .entries
            .get(request_id)
            .map(|e| e.info.tool_input.clone())
    }

    /// `GET /approvals/{id}/input`: trả `tool_input` đầy đủ và đánh dấu đã mở bản đầy đủ (mở khóa allow cho thẻ đã cắt).
    pub fn tool_input_for_review(&self, request_id: &str) -> Option<Value> {
        let mut inner = self.lock();
        let e = inner.entries.get_mut(request_id)?;
        e.reviewed = true;
        Some(e.info.tool_input.clone())
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
                e.view.countdown_ms = None;
            }
        }
    }

    /// Xử lý một `PermissionRequest`. `ev` là `hook::to_event(input)`, `agent_name` là tên nhân vật trên thẻ duyệt.
    /// Nhận `input` theo giá trị để nhả JSON thô trước khi chờ (tới 110 s): chỉ giữ lại các trường cần dùng.
    pub async fn request(
        &self,
        input: HookInput,
        ev: &Event,
        agent_name: &str,
    ) -> PermissionOutcome {
        if ev.kind != EventKind::ApprovalRequested {
            tracing::warn!(
                event = ?input.event_name(),
                "/hooks/permission nhận sự kiện không phải PermissionRequest, không quyết định"
            );
            return PermissionOutcome::NotPermissionRequest;
        }
        // Trường do agent gửi được sao chép nhiều lần (sự kiện, hàng đợi, thẻ, trạng thái): chặn độ dài trước mọi thứ
        // khác (audit ARC-43 finding 1). Quá dài thì `{}`, không bao giờ allow.
        let too_long = [
            (
                "tool_name",
                input.tool_name().map_or(0, str::len),
                MAX_TOOL_NAME_BYTES,
            ),
            (
                "session_id",
                input.session_id().map_or(0, str::len),
                MAX_ID_BYTES,
            ),
            (
                "agent_id",
                input.agent_id().map_or(0, str::len),
                MAX_ID_BYTES,
            ),
            (
                "agent_type",
                input.agent_type().map_or(0, str::len),
                MAX_ID_BYTES,
            ),
            ("cwd", input.cwd().map_or(0, str::len), MAX_CWD_BYTES),
        ]
        .into_iter()
        .find(|(_, len, max)| len > max);
        if let Some((field, len, max)) = too_long {
            tracing::warn!(
                field,
                len,
                max,
                "trường hook quá dài, trả {{}} để Claude Code hỏi trong terminal"
            );
            return PermissionOutcome::Overloaded;
        }
        let tool = input.tool_name().unwrap_or("").to_owned();
        let cwd = input.cwd().unwrap_or("").to_owned();
        let mut raw = input.raw;
        let tool_input = raw
            .get_mut("tool_input")
            .map(Value::take)
            .unwrap_or(Value::Null);
        drop(raw);

        // Lệnh quá dài để duyệt trên thẻ: trả về terminal TRƯỚC khi hỏi policy (không bao giờ tự duyệt, không đếm
        // ngược, không cắt rồi cho duyệt; quy tắc bất biến 3).
        if let Some(text) = untruncated_text(&tool, &tool_input) {
            if is_too_long(
                &text,
                self.cfg.handoff_max_bytes,
                self.cfg.handoff_max_lines,
            ) {
                return self.hand_off(ev, tool, tool_input, cwd, &text, agent_name);
            }
        }
        // Edit/Write/…: nội dung được cắt trên thẻ, nhưng đường dẫn và khóa phụ thì không; phần đó quá dài cũng trả về
        // terminal (audit ARC-43 finding 3).
        if let Some(text) = non_content_text(&tool, &tool_input) {
            if is_too_long(
                &text,
                self.cfg.handoff_max_bytes,
                self.cfg.handoff_max_lines,
            ) {
                return self.hand_off(ev, tool, tool_input, cwd, &text, agent_name);
            }
        }

        // Kiểm rẻ trước khi dựng thẻ (dựng thẻ tốn bộ nhớ gấp nhiều lần `tool_input`, audit ARC-43 finding 2): hàng đợi
        // đầy hay hết ngân sách byte thì `{}` ngay. Kiểm lại chính xác (gồm thẻ) khi chèn.
        let input_bytes = held_size(&tool_input)
            + 3 * (tool.capacity() + ev.session_id.len() + ev.agent.id.len())
            + cwd.capacity();
        if let Some(early) = self.admission_precheck(&ev.session_id, input_bytes) {
            return early;
        }

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
        let mut info = PendingApproval {
            request_id: request_id.clone(),
            session_id: ev.session_id.clone(),
            agent_id: ev.agent.id.clone(),
            tool,
            tool_input,
            cwd,
            risk,
            created_at: ev.ts,
            deadline_at: ev.ts + self.cfg.timeout,
            countdown,
        };
        // Dựng thẻ (cắt, escape) ngoài khóa.
        let mut view = approval_view(&info, agent_name);
        let held_bytes = input_bytes + view_size(&view);
        let rec = self.record_of(&info, ApprovalStatus::Pending, None);
        {
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
            if inner.held_bytes.saturating_add(held_bytes) > self.cfg.held_input_max_bytes {
                tracing::warn!(
                    session = ?ev.session_id,
                    held = inner.held_bytes,
                    request = held_bytes,
                    max = self.cfg.held_input_max_bytes,
                    "hàng đợi duyệt đầy bộ nhớ, trả {{}} để Claude Code hỏi trong terminal"
                );
                return PermissionOutcome::Overloaded;
            }
            if inner.paused {
                info.countdown = None;
                view.countdown_ms = None;
            }
            // `?`: chuỗi từ agent không tin cậy, Debug escape xuống dòng (chặn giả dòng log).
            tracing::info!(%request_id, session = ?info.session_id, tool = ?info.tool, "đang giữ yêu cầu duyệt");
            // Ghi `pending` và gửi khi còn giữ khóa: bản ghi/thông báo kết thúc của yêu cầu này (hủy, quyết định) chỉ có
            // thể tới sau (audit T5a finding 5). Cả hai chỉ đẩy vào kênh, không chờ.
            drop(self.store.record(rec));
            let _ = self.tx.send(ServerMsg::ApprovalRequested {
                approval: view.clone(),
            });
            inner.insert_entry(
                request_id.clone(),
                Entry {
                    info,
                    view,
                    started: Instant::now(),
                    reviewed: false,
                    reply,
                    held_bytes,
                },
            );
        }

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

    /// Trả lệnh quá dài về terminal: không giữ yêu cầu, ghi `handed_off`, broadcast thẻ báo (không có nút duyệt).
    /// Kiểm hàng đợi trước khi dựng thẻ: đóng, đầy theo số lượng (toàn hub, mỗi phiên), hoặc `bytes` làm tổng vượt ngân
    /// sách. `None`: được đi tiếp (kiểm lại chính xác khi chèn).
    fn admission_precheck(&self, session_id: &str, bytes: usize) -> Option<PermissionOutcome> {
        let inner = self.lock();
        if inner.closed {
            return Some(PermissionOutcome::Cancelled);
        }
        let full = inner.entries.len() >= self.cfg.max_pending
            || inner
                .entries
                .values()
                .filter(|e| e.info.session_id == session_id)
                .count()
                >= self.cfg.max_pending_per_session;
        let used = inner.held_bytes.saturating_add(inner.inflight_bytes);
        if full || used.saturating_add(bytes) > self.cfg.held_input_max_bytes {
            tracing::warn!(
                session = ?session_id,
                pending = inner.entries.len(),
                held = inner.held_bytes,
                request = bytes,
                "hàng đợi duyệt đầy, trả {{}} để Claude Code hỏi trong terminal"
            );
            return Some(PermissionOutcome::Overloaded);
        }
        None
    }

    fn hand_off(
        &self,
        ev: &Event,
        tool: String,
        tool_input: Value,
        cwd: String,
        text: &str,
        agent_name: &str,
    ) -> PermissionOutcome {
        let handoff_id = format!("hof_{}", ulid::Ulid::generate());
        let info = PendingApproval {
            request_id: handoff_id.clone(),
            session_id: ev.session_id.clone(),
            agent_id: ev.agent.id.clone(),
            tool,
            tool_input,
            cwd,
            risk: Risk::Medium,
            created_at: ev.ts,
            deadline_at: ev.ts,
            countdown: None,
        };
        // Dựng thẻ và hash ngoài khóa; `tool_input` được nhả ngay sau đó.
        let view = handoff_view(&handoff_id, &info, agent_name, text, ev.ts);
        let input_hash = input_hash(&info.tool_input);
        let rec = self.record_of(&info, ApprovalStatus::HandedOff, None);
        tracing::info!(
            handoff_id = %handoff_id,
            session = ?info.session_id,
            tool = ?info.tool,
            bytes = view.bytes,
            lines = view.lines,
            "lệnh quá dài để duyệt trên thẻ, trả {{}} để duyệt trong terminal"
        );
        let PendingApproval {
            session_id,
            agent_id,
            tool,
            ..
        } = info;
        let mut inner = self.lock();
        if inner.closed {
            return PermissionOutcome::Cancelled;
        }
        drop(self.store.record(rec));
        while inner.handoffs.len() >= MAX_HANDOFFS {
            if let Some(old) = inner.handoffs.pop_front() {
                let _ = self.tx.send(ServerMsg::HandoffCleared {
                    handoff_id: old.view.handoff_id,
                });
            }
        }
        let _ = self.tx.send(ServerMsg::ApprovalHandedOff {
            handoff: view.clone(),
        });
        inner.handoffs.push_back(Handoff {
            view,
            session_id,
            agent_id,
            tool,
            input_hash,
        });
        PermissionOutcome::HandedOff { handoff_id }
    }

    /// Thẻ "duyệt trong terminal" đang hiện, cũ trước (cho `Snapshot`).
    pub fn handoffs(&self) -> Vec<HandoffView> {
        self.lock()
            .handoffs
            .iter()
            .map(|h| h.view.clone())
            .collect()
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
                if inner
                    .handoffs
                    .iter()
                    .any(|h| h.view.handoff_id == request_id)
                {
                    tracing::warn!(
                        request_id,
                        "Decide cho thẻ duyệt-trong-terminal: bỏ qua, hub không quyết định lệnh này"
                    );
                    return DecideResult::HandedOff;
                }
                return DecideResult::Expired;
            };
            if let Some(reason) = allow_gate(&self.cfg, inner.paused, e, decision, origin) {
                tracing::info!(
                    request_id,
                    ?reason,
                    "giữ nguyên yêu cầu: chưa đủ điều kiện để nhận quyết định"
                );
                return DecideResult::Held(reason);
            }
            match inner.remove_entry(request_id) {
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
                let _ = self.tx.send(ServerMsg::ApprovalResolved {
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
    ///
    /// Cùng luật đó xóa thẻ "duyệt trong terminal" (`HandoffCleared`); số trả về gồm cả thẻ bị xóa.
    pub fn on_event(&self, ev: &Event, input: &HookInput) -> usize {
        match ev.kind {
            EventKind::SessionEnded => {
                self.cancel_where(|p| p.session_id == ev.session_id, "phiên kết thúc")
                    + self.clear_handoffs_where(|h| h.session_id == ev.session_id)
            }
            EventKind::ToolPost | EventKind::ToolFailed => {
                let (Some(tool), Some(tool_input)) = (input.tool_name(), input.tool_input()) else {
                    return 0;
                };
                let hash = input_hash(tool_input);
                self.cancel_where(
                    |p| p.agent_id == ev.agent.id && p.tool == tool && &p.tool_input == tool_input,
                    "tool đã chạy xong: yêu cầu được trả lời ở nơi khác",
                ) + self.clear_handoffs_where(|h| {
                    h.agent_id == ev.agent.id && h.tool == tool && h.input_hash == hash
                })
            }
            EventKind::TurnStopped
            | EventKind::TurnFailed
            | EventKind::PromptSubmitted
            | EventKind::SubagentStopped => {
                self.cancel_where(
                    |p| p.agent_id == ev.agent.id,
                    "agent đã sang bước khác: yêu cầu được trả lời ở nơi khác",
                ) + self.clear_handoffs_where(|h| h.agent_id == ev.agent.id)
            }
            _ => 0,
        }
    }

    fn clear_handoffs_where(&self, pred: impl Fn(&Handoff) -> bool) -> usize {
        let mut inner = self.lock();
        let before = inner.handoffs.len();
        let mut cleared = Vec::new();
        inner.handoffs.retain(|h| {
            if pred(h) {
                cleared.push(h.view.handoff_id.clone());
                false
            } else {
                true
            }
        });
        // Gửi dưới khóa, như `ApprovalHandedOff`: thẻ không thể "sống lại" sau khi đã xóa.
        for handoff_id in cleared {
            tracing::info!(%handoff_id, "xóa thẻ duyệt-trong-terminal");
            let _ = self.tx.send(ServerMsg::HandoffCleared { handoff_id });
        }
        before - inner.handoffs.len()
    }

    /// Tắt hub: trả `{}` cho mọi yêu cầu đang chờ, từ chối yêu cầu mới. Trả số yêu cầu bị hủy.
    pub fn shutdown(&self) -> usize {
        let drained: Vec<Entry> = {
            let mut inner = self.lock();
            inner.closed = true;
            inner.drain_entries()
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
            ids.iter().filter_map(|id| inner.remove_entry(id)).collect()
        };
        let n = taken.len();
        for entry in taken {
            tracing::info!(request_id = %entry.info.request_id, why, "hủy yêu cầu duyệt, trả {{}}");
            self.finish(entry, ApprovalStatus::Cancelled, DecidedBy::Cancelled, None);
        }
        n
    }

    fn take(&self, request_id: &str) -> Option<Entry> {
        self.lock().remove_entry(request_id)
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
        let _ = self.tx.send(ServerMsg::ApprovalResolved {
            request_id: info.request_id,
            decision,
            decided_by: by,
        });
        drop(reply);
    }

    /// Ghi không chờ xác nhận (chỉ AutoAllow cần chờ).
    fn record(&self, info: &PendingApproval, status: ApprovalStatus, by: Option<DecidedBy>) {
        drop(self.store.record(self.record_of(info, status, by)));
    }

    fn record_of(
        &self,
        info: &PendingApproval,
        status: ApprovalStatus,
        by: Option<DecidedBy>,
    ) -> ApprovalRecord {
        ApprovalRecord {
            request_id: info.request_id.clone(),
            session_id: info.session_id.clone(),
            agent_id: info.agent_id.clone(),
            tool: info.tool.clone(),
            status,
            decided_by: by,
            at: OffsetDateTime::now_utc(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Mutex bị poison chỉ khi một thread panic lúc đang giữ; dữ liệu vẫn nhất quán vì mọi thao tác là đơn bước.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Mọi điều kiện để một quyết định cho yêu cầu `e` được nhận (audit ARC-44 finding 8). Đường duyệt gộp hay batch (T9)
/// PHẢI gọi hàm này cho từng yêu cầu. `None`: được nhận; `Some`: giữ nguyên yêu cầu (không bao giờ tính là từ chối).
fn allow_gate(
    cfg: &ApprovalsConfig,
    paused: bool,
    e: &Entry,
    decision: Decision,
    origin: DecideOrigin,
) -> Option<HeldReason> {
    if origin == DecideOrigin::Countdown {
        if decision != Decision::Allow {
            return Some(HeldReason::CountdownCannotDeny);
        }
        if paused {
            return Some(HeldReason::Paused);
        }
        match e.info.countdown {
            None => return Some(HeldReason::NoCountdown),
            Some(c) if e.started.elapsed() < c => return Some(HeldReason::NotElapsed),
            Some(_) => {}
        }
    }
    if decision != Decision::Allow {
        return None;
    }
    // Sàn thời gian duyệt (F6, B4) áp cho MỌI allow, kể cả đếm ngược (audit ARC-44 finding 4): `origin: countdown` do
    // client khai, hub không biết có ai nhìn thấy thẻ. Deny luôn được nhận.
    if e.started.elapsed() < cfg.min_review {
        return Some(HeldReason::TooFast);
    }
    // Thẻ đã cắt nội dung: chỉ cho allow sau khi bản đầy đủ đã được mở (audit T5b finding 3, bất biến 3).
    if e.view.truncated && !e.reviewed {
        return Some(HeldReason::NotReviewed);
    }
    None
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
