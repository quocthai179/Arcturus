//! Trạng thái chung của hub (`docs/PLAN.md` §4): cấu hình, persona, trạng thái agent, hàng đợi duyệt và kênh broadcast
//! `ServerMsg` cho UI.
//!
//! Quy tắc khóa: không bao giờ giữ đồng thời khóa `state` và khóa của [`Approvals`]; không giữ khóa nào qua `.await`.
//! Thứ tự xử lý một sự kiện: luồng duyệt trước (hủy/thay yêu cầu), trạng thái agent sau, nên khi `SessionEnd` UI nhận
//! `ApprovalResolved` trước `AgentRemoved`.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use hub_core::config::Config;
use hub_core::hook::{self, HookInput};
use hub_core::persona::PersonaBook;
use hub_core::policy::batch::{BatchError, BatchRule, BatchRules};
use hub_core::protocol::{ServerMsg, TeamView};
use hub_core::schema::Event;
use hub_core::state::TeamState;
use hub_core::store::{BatchRow, EventRing};
use time::OffsetDateTime;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;

use hub_core::policy::danger::DangerConfig;

use crate::approvals::{
    ApprovalStore, Approvals, ApprovalsConfig, NoStore, PermissionOutcome, Policy, RulesPolicy,
};

/// Sức chứa kênh broadcast cho UI (§4). Client chậm hơn thì nhận `Snapshot` mới (T5b).
pub const BROADCAST_CAPACITY: usize = 256;

/// Sức chứa hàng đợi `/hooks/event` (§3.5).
pub const EVENT_QUEUE: usize = 1024;

pub struct Hub {
    pub cfg: Config,
    pub host: String,
    pub personas: PersonaBook,
    pub approvals: Approvals,
    state: Mutex<TeamState>,
    tx: broadcast::Sender<ServerMsg>,
    events: mpsc::Sender<HookInput>,
    /// Thời điểm cần `tick` sớm nhất (`TeamState::next_wakeup`); `None` thì task tick ngủ, không hẹn giờ.
    wake: watch::Sender<Option<OffsetDateTime>>,
    /// Sự kiện gần nhất trong RAM (tối đa 2.000 / 2 MiB, T6). Khóa riêng, không giữ cùng khóa nào khác.
    recent: Mutex<EventRing>,
    /// Khóa ngoài cùng cho thay đổi cờ đội (`set_paused`, `set_focus`); lấy trước khóa của `Approvals` và `state`.
    team: Mutex<()>,
    /// Batch rule đang mở (T9). `RulesPolicy` giữ một bản `Arc` để tự duyệt theo nhóm.
    batch: Arc<BatchRules>,
}

/// Các task nền của hub; drop không dừng chúng (dừng khi `Hub` bị drop và kênh đóng).
pub struct HubTasks {
    pub events: JoinHandle<()>,
    pub tick: JoinHandle<()>,
}

impl Hub {
    /// Hub với policy thật ([`RulesPolicy`]: danger list, chưa có batch/trust nên mọi yêu cầu hỏi người) và không có
    /// store (test; mọi AutoAllow rơi về hỏi người). Luật danger biết cổng trong `cfg.hub.bind`.
    pub fn start(cfg: Config, personas: PersonaBook, host: String) -> (Arc<Hub>, HubTasks) {
        let danger = DangerConfig {
            hub_port: cfg.bind_addr().map(|a| a.port()),
            ..DangerConfig::default()
        };
        Self::start_full(cfg, personas, host, danger, Box::new(NoStore))
    }

    /// Hub chạy thật (`main.rs`): `danger` mang cổng đang nghe, đường dẫn token UI, cấu hình, cơ sở dữ liệu; `store`
    /// là writer SQLite (`hub_core::store::StoreHandle`).
    pub fn start_full(
        cfg: Config,
        personas: PersonaBook,
        host: String,
        danger: DangerConfig,
        store: Box<dyn ApprovalStore>,
    ) -> (Arc<Hub>, HubTasks) {
        let approvals_cfg = approvals_config(&cfg, danger);
        let batch = Arc::new(BatchRules::new());
        let policy = RulesPolicy::new().with(Box::new(batch.clone()));
        Self::start_with_batch(
            cfg,
            personas,
            host,
            approvals_cfg,
            Box::new(policy),
            store,
            batch,
        )
    }

    /// Dựng hub và khởi động vòng sự kiện + task tick (cần đang ở trong runtime tokio). Batch rule mở qua hub chỉ có hiệu
    /// lực nếu `policy` tự cắm [`BatchRules`] (xem [`Hub::start_with_batch`]).
    pub fn start_with(
        cfg: Config,
        personas: PersonaBook,
        host: String,
        approvals_cfg: ApprovalsConfig,
        policy: Box<dyn Policy>,
        store: Box<dyn ApprovalStore>,
    ) -> (Arc<Hub>, HubTasks) {
        Self::start_with_batch(
            cfg,
            personas,
            host,
            approvals_cfg,
            policy,
            store,
            Arc::new(BatchRules::new()),
        )
    }

    /// Như [`Hub::start_with`], với bộ batch rule do người gọi cấp (cùng bộ đã cắm vào `policy`; test truyền đồng hồ giả).
    pub fn start_with_batch(
        cfg: Config,
        personas: PersonaBook,
        host: String,
        approvals_cfg: ApprovalsConfig,
        policy: Box<dyn Policy>,
        store: Box<dyn ApprovalStore>,
        batch: Arc<BatchRules>,
    ) -> (Arc<Hub>, HubTasks) {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (events, events_rx) = mpsc::channel(EVENT_QUEUE);
        let (wake, wake_rx) = watch::channel(None);
        let hub = Arc::new(Hub {
            approvals: Approvals::new(approvals_cfg, policy, store, tx.clone()),
            cfg,
            host,
            personas,
            state: Mutex::new(TeamState::default()),
            tx,
            events,
            wake,
            recent: Mutex::new(EventRing::default()),
            team: Mutex::new(()),
            batch,
        });
        let tasks = HubTasks {
            events: tokio::spawn(event_loop(Arc::downgrade(&hub), events_rx)),
            tick: tokio::spawn(tick_loop(Arc::downgrade(&hub), wake_rx)),
        };
        (hub, tasks)
    }

    pub fn batch(&self) -> &Arc<BatchRules> {
        &self.batch
    }

    /// `BatchOpen` (T9): mở hoặc gia hạn quy tắc cho `group_key` trong `minutes` phút. Lưu vào store, hẹn một lần để báo khi
    /// hết hạn (task một lần, không polling) và báo `Feed` cho mọi client.
    pub fn batch_open(
        self: &Arc<Self>,
        group_key: &str,
        minutes: u32,
    ) -> Result<BatchRule, BatchError> {
        let rule = self.batch.open(group_key, minutes)?;
        self.approvals.batch_set(BatchRow {
            group_key: rule.group_key.clone(),
            opened_at: rule.opened_at,
            expires_at: rule.expires_at,
        });
        self.schedule_batch_expiry(&rule);
        tracing::info!(group = ?rule.group_key, minutes, "batch rule mở");
        self.broadcast_feed(format!(
            "Cho phép \"{}\" trong {minutes} phút (chỉ yêu cầu rủi ro thấp/vừa)",
            rule.group_key
        ));
        Ok(rule)
    }

    /// `BatchClose` (T9). `true` nếu có quy tắc đang mở bị đóng.
    pub fn batch_close(&self, group_key: &str) -> bool {
        let closed = self.batch.close(group_key);
        self.approvals.batch_delete(group_key);
        if closed {
            tracing::info!(group = ?group_key, "batch rule đóng");
            self.broadcast_feed(format!("Đã tắt cho phép \"{group_key}\""));
        }
        closed
    }

    /// Nạp lại quy tắc đã lưu khi khởi động (bỏ qua hàng hết hạn hoặc không hợp lệ). Trả số quy tắc nạp được.
    pub fn restore_batch(self: &Arc<Self>, rows: Vec<BatchRow>) -> usize {
        let mut n = 0;
        for row in rows {
            let rule = BatchRule {
                group_key: row.group_key,
                opened_at: row.opened_at,
                expires_at: row.expires_at,
            };
            if self.batch.restore(rule.clone()) {
                self.schedule_batch_expiry(&rule);
                n += 1;
            } else {
                // Hàng không dùng được (hết hạn, khóa ghép, hạn quá xa): dọn khỏi store.
                self.approvals.batch_delete(&rule.group_key);
            }
        }
        n
    }

    /// Hẹn báo hết hạn: một task ngủ tới hạn (không polling). Quy tắc được gia hạn có hạn mới hơn, nên task của hạn cũ bỏ
    /// qua; task của hạn mới sẽ xử lý.
    fn schedule_batch_expiry(self: &Arc<Self>, rule: &BatchRule) {
        let weak = Arc::downgrade(self);
        let rule = rule.clone();
        let wait = (rule.expires_at - self.batch_now())
            .try_into()
            .unwrap_or(Duration::ZERO)
            + Duration::from_millis(50);
        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            let Some(hub) = weak.upgrade() else { return };
            hub.batch_expired(&rule);
        });
    }

    fn batch_now(&self) -> OffsetDateTime {
        self.batch.now()
    }

    fn batch_expired(&self, rule: &BatchRule) {
        // Còn hạn: đã được gia hạn (task của hạn mới sẽ báo), hoặc đánh thức sớm hơn hạn.
        if self.batch.expiry(&rule.group_key).is_some() {
            return;
        }
        // Dọn hàng đã hết hạn khỏi bộ nhớ và store.
        let _ = self.batch.active();
        self.approvals.batch_delete(&rule.group_key);
        tracing::info!(group = ?rule.group_key, "batch rule hết hạn");
        self.broadcast_feed(format!("Hết thời hạn cho phép \"{}\"", rule.group_key));
    }

    /// `Feed` cho mọi client.
    pub fn broadcast_feed(&self, text: String) {
        self.publish(vec![ServerMsg::Feed {
            text,
            ts: OffsetDateTime::now_utc(),
        }]);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        self.tx.subscribe()
    }

    /// `/hooks/event`: đưa vào hàng đợi, không chờ. Hàng đợi đầy thì bỏ (mất `SessionEnd` chỉ làm yêu cầu đang chờ
    /// hết giờ và trả `{}`, an toàn).
    pub fn enqueue_event(&self, input: HookInput) {
        if let Err(err) = self.events.try_send(input) {
            tracing::warn!(%err, "bỏ sự kiện hook: hàng đợi đầy hoặc đã đóng");
        }
    }

    /// Xử lý một sự kiện (vòng sự kiện gọi, test gọi trực tiếp).
    pub fn ingest(&self, input: &HookInput) {
        let now = OffsetDateTime::now_utc();
        let ev = hook::to_event(input, &self.host, now);
        self.remember(&ev, &input.raw);
        self.approvals.on_event(&ev, input);
        let msgs = self.lock_state().apply(&ev, &self.personas, now);
        self.publish(msgs);
        self.update_wake();
    }

    /// `/hooks/permission` (PLAN T3): cập nhật nhân vật sang chờ duyệt (bước 3), giữ yêu cầu, rồi đưa nhân vật về suy
    /// nghĩ khi đã có quyết định (bước 9). Hết giờ/hủy/đầy giữ nguyên "chờ duyệt": terminal vẫn đang hỏi người.
    pub async fn permission(&self, input: HookInput) -> PermissionOutcome {
        let now = OffsetDateTime::now_utc();
        let ev = hook::to_event(&input, &self.host, now);
        // Ghi trước khi giữ yêu cầu: `request` nhả JSON gốc ngay để không giữ nó suốt 110 s.
        self.remember(&ev, &input.raw);
        let (msgs, name) = {
            let mut st = self.lock_state();
            let msgs = st.apply(&ev, &self.personas, now);
            let name = st.name_of(&ev.agent.id).unwrap_or_default().to_owned();
            (msgs, name)
        };
        self.publish(msgs);
        self.update_wake();

        let outcome = self.approvals.request(input, &ev, &name).await;
        let msgs = match outcome {
            PermissionOutcome::Decided(_) | PermissionOutcome::AutoAllowed => {
                self.lock_state().approval_finished(&ev.agent.id)
            }
            PermissionOutcome::HandedOff { .. } => {
                self.lock_state().approval_in_terminal(&ev.agent.id)
            }
            _ => Vec::new(),
        };
        self.publish(msgs);
        outcome
    }

    /// Lưu sự kiện vào store (không chặn, kênh đầy thì bỏ) và ring buffer.
    fn remember(&self, ev: &Event, raw: &serde_json::Value) {
        self.approvals.record_event(ev, raw);
        self.recent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(ev.clone());
    }

    /// `n` sự kiện gần nhất trong RAM, cũ trước (T19 standup, gỡ lỗi).
    pub fn recent_events(&self, n: usize) -> Vec<Event> {
        self.recent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recent(n)
    }

    /// `TeamPause` / `TeamResume` (T5b nối với WebSocket): cờ duy nhất nằm trong `Approvals` (D3).
    pub fn set_paused(&self, paused: bool) {
        let _team = self.lock_team();
        self.approvals.set_paused(paused);
        self.publish(vec![ServerMsg::TeamUpdated {
            team: self.team_view(),
        }]);
    }

    /// `FocusSet` (T14).
    pub fn set_focus(&self, on: bool) {
        let _team = self.lock_team();
        self.lock_state().focus = on;
        self.publish(vec![ServerMsg::TeamUpdated {
            team: self.team_view(),
        }]);
    }

    /// Tuần tự hóa "đổi cờ đội + broadcast": hai client bấm tạm dừng/tiếp tục cùng lúc không làm `TeamUpdated` cuối cùng
    /// mang giá trị cũ (audit T5b finding 6). Chỉ giữ quanh các thao tác đồng bộ, không qua `.await`.
    fn lock_team(&self) -> MutexGuard<'_, ()> {
        self.team.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn team_view(&self) -> TeamView {
        let paused = self.approvals.is_paused();
        let focus = self.lock_state().focus;
        TeamView {
            paused,
            focus,
            roster_max: self.cfg.team.main_roster_max,
        }
    }

    /// Mọi agent hiện có (cho `Snapshot`, T5b).
    pub fn agent_views(&self) -> Vec<hub_core::protocol::AgentView> {
        self.lock_state().views()
    }

    /// Tắt hub: trả `{}` cho mọi yêu cầu đang chờ.
    pub fn shutdown(&self) -> usize {
        self.approvals.shutdown()
    }

    fn tick(&self) {
        let msgs = self.lock_state().tick(OffsetDateTime::now_utc());
        self.publish(msgs);
        self.update_wake();
    }

    fn update_wake(&self) {
        let next = self.lock_state().next_wakeup();
        self.wake.send_if_modified(|cur| {
            if *cur == next {
                false
            } else {
                *cur = next;
                true
            }
        });
    }

    fn publish(&self, msgs: Vec<ServerMsg>) {
        for m in msgs {
            // Không có client nào thì `send` lỗi: bình thường, bỏ qua.
            let _ = self.tx.send(m);
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, TeamState> {
        // Poison chỉ khi một thread panic lúc giữ khóa; mỗi thao tác trên TeamState là đơn bước nên dữ liệu vẫn dùng được.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Tham số luồng duyệt lấy từ `[hub]` của cấu hình.
pub fn approvals_config(cfg: &Config, danger: DangerConfig) -> ApprovalsConfig {
    ApprovalsConfig {
        timeout: Duration::from_secs(cfg.hub.approval_timeout_secs),
        handoff_max_bytes: cfg.hub.handoff_max_bytes,
        handoff_max_lines: cfg.hub.handoff_max_lines,
        held_input_max_bytes: cfg.hub.held_input_max_bytes,
        min_review: Duration::from_millis(cfg.hub.min_review_ms),
        danger,
        ..ApprovalsConfig::default()
    }
}

/// Đọc `/hooks/event` theo thứ tự đến. Dừng khi hub bị drop.
async fn event_loop(hub: std::sync::Weak<Hub>, mut rx: mpsc::Receiver<HookInput>) {
    while let Some(input) = rx.recv().await {
        let Some(hub) = hub.upgrade() else {
            break;
        };
        hub.ingest(&input);
    }
}

/// Ngủ tới `next_wakeup` rồi `tick`. Không có agent nào `done` thì chỉ chờ watch đổi (không timer, bất biến 6).
async fn tick_loop(hub: std::sync::Weak<Hub>, mut wake: watch::Receiver<Option<OffsetDateTime>>) {
    loop {
        let next = *wake.borrow_and_update();
        match next {
            None => {
                if wake.changed().await.is_err() {
                    break;
                }
            }
            Some(at) => {
                let wait = (at - OffsetDateTime::now_utc()).max(time::Duration::ZERO);
                let wait = Duration::try_from(wait).unwrap_or(Duration::ZERO);
                tokio::select! {
                    () = tokio::time::sleep(wait) => {
                        let Some(hub) = hub.upgrade() else { break };
                        hub.tick();
                    }
                    changed = wake.changed() => if changed.is_err() { break },
                }
            }
        }
    }
}
