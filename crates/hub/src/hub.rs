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
use hub_core::protocol::{ServerMsg, TeamView};
use hub_core::state::TeamState;
use time::OffsetDateTime;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;

use crate::approvals::{
    AlwaysAsk, ApprovalStore, Approvals, ApprovalsConfig, NoStore, PermissionOutcome, Policy,
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
    /// Khóa ngoài cùng cho thay đổi cờ đội (`set_paused`, `set_focus`); lấy trước khóa của `Approvals` và `state`.
    team: Mutex<()>,
}

/// Các task nền của hub; drop không dừng chúng (dừng khi `Hub` bị drop và kênh đóng).
pub struct HubTasks {
    pub events: JoinHandle<()>,
    pub tick: JoinHandle<()>,
}

impl Hub {
    /// Hub chạy thật: policy luôn hỏi người (T8 thay), chưa có store (T6 thay).
    pub fn start(cfg: Config, personas: PersonaBook, host: String) -> (Arc<Hub>, HubTasks) {
        let approvals_cfg = ApprovalsConfig {
            timeout: Duration::from_secs(cfg.hub.approval_timeout_secs),
            handoff_max_bytes: cfg.hub.handoff_max_bytes,
            handoff_max_lines: cfg.hub.handoff_max_lines,
            held_input_max_bytes: cfg.hub.held_input_max_bytes,
            ..ApprovalsConfig::default()
        };
        Self::start_with(
            cfg,
            personas,
            host,
            approvals_cfg,
            Box::new(AlwaysAsk),
            Box::new(NoStore),
        )
    }

    /// Dựng hub và khởi động vòng sự kiện + task tick (cần đang ở trong runtime tokio).
    pub fn start_with(
        cfg: Config,
        personas: PersonaBook,
        host: String,
        approvals_cfg: ApprovalsConfig,
        policy: Box<dyn Policy>,
        store: Box<dyn ApprovalStore>,
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
            team: Mutex::new(()),
        });
        let tasks = HubTasks {
            events: tokio::spawn(event_loop(Arc::downgrade(&hub), events_rx)),
            tick: tokio::spawn(tick_loop(Arc::downgrade(&hub), wake_rx)),
        };
        (hub, tasks)
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
