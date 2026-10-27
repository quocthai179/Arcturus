//! Lưu trữ SQLite (`docs/PLAN.md` §3.5, T6 · ARC-27): sự kiện (xóa sau `event_ttl_days`) và nhật ký duyệt.
//!
//! - Một thread ghi duy nhất nhận lệnh theo đúng thứ tự gửi qua một kênh (`allowed` rồi `cancelled` cho cùng một id
//!   luôn được ghi theo thứ tự đó). Mỗi lượt gom tới [`BATCH_MAX`] lệnh vào một transaction.
//! - Kênh đầy (quá [`EVENT_QUEUE_MAX`] sự kiện hoặc [`EVENT_QUEUE_MAX_BYTES`] byte đang chờ): bỏ bản ghi `events`, đếm
//!   số bị bỏ, log warn. Bản ghi `approvals` **không bao giờ** bị bỏ (số lượng đã bị chặn bởi hàng đợi duyệt).
//! - Xác nhận ghi (cho AutoAllow, PLAN T3 bước 6) chỉ được gửi **sau khi** transaction đã commit.
//! - Dọn TTL lúc mở và mỗi [`PURGE_INTERVAL`] (không polling: thread ngủ trong `recv_timeout` tới lượt dọn kế tiếp).
//! - Thời gian lưu dạng UTC `YYYY-MM-DDTHH:MM:SS.mmmZ` (độ dài cố định, so sánh chuỗi đúng thứ tự thời gian).
//!
//! Ngân sách bộ nhớ (quy tắc bất biến 6): `cache_size` 512 KiB mỗi kết nối, JSON hook gốc lớn hơn
//! [`RAW_JSON_MAX_BYTES`] không được lưu (cột `raw_json` là NULL), [`EventRing`] tối đa 2.000 sự kiện / 2 MiB.

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use time::format_description::BorrowedFormatItem;
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

use crate::schema::{AgentId, DecidedBy, Event, RequestId, Risk};

/// Phiên bản schema mới nhất (`PRAGMA user_version`).
pub const DB_VERSION: i32 = 2;

/// Tối đa sự kiện chờ ghi trong kênh (§3.5).
pub const EVENT_QUEUE_MAX: usize = 1024;
/// Tối đa byte sự kiện chờ ghi (JSON hook gốc có thể tới 1 MB mỗi sự kiện).
pub const EVENT_QUEUE_MAX_BYTES: usize = 4 * 1024 * 1024;
/// JSON hook gốc lớn hơn chừng này (ví dụ `PostToolUse` của Read file lớn) không được lưu.
pub const RAW_JSON_MAX_BYTES: usize = 256 * 1024;
/// Lệnh tối đa mỗi transaction.
pub const BATCH_MAX: usize = 256;
/// Chu kỳ dọn sự kiện quá hạn (§3.5).
pub const PURGE_INTERVAL: Duration = Duration::from_secs(6 * 3600);
/// Chờ khóa ghi tối đa (một kết nối khác đang ghi).
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const MIGRATIONS: [(i32, &str); 2] = [
    (
    1,
    "CREATE TABLE events (
       id TEXT PRIMARY KEY, ts TEXT NOT NULL, session_id TEXT NOT NULL,
       kind TEXT NOT NULL, event_json TEXT NOT NULL, raw_json TEXT
     );
     CREATE INDEX events_ts ON events(ts);
     CREATE INDEX events_session ON events(session_id, ts);
     CREATE TABLE approvals (
       request_id TEXT PRIMARY KEY, session_id TEXT NOT NULL, agent_id TEXT NOT NULL,
       tool TEXT NOT NULL, command_original TEXT NOT NULL, command_normalized TEXT NOT NULL,
       risk TEXT NOT NULL, group_key TEXT NOT NULL,
       status TEXT NOT NULL CHECK (status IN ('pending','allowed','denied','timed_out','cancelled','handed_off')),
       decided_by TEXT, created_at TEXT NOT NULL, decided_at TEXT, latency_ms INTEGER, channel TEXT
     );
     CREATE INDEX approvals_created ON approvals(created_at);",
    ),
    (
        2,
        // T9: quy tắc "cho phép mẫu X trong N phút". Bảng của T10 (trust_grants, streaks, trust_log) thêm ở migration sau.
        "CREATE TABLE batch_rules (
           group_key TEXT PRIMARY KEY, opened_at TEXT NOT NULL, expires_at TEXT NOT NULL
         );",
    ),
];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cơ sở dữ liệu ở phiên bản {found}, mới hơn hub này (tối đa {DB_VERSION})")]
    TooNew { found: i32 },
    #[error("thread ghi đã dừng")]
    Closed,
}

// ---------------------------------------------------------------------------------------------------------------
// Bản ghi
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

impl ApprovalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Allowed => "allowed",
            ApprovalStatus::Denied => "denied",
            ApprovalStatus::TimedOut => "timed_out",
            ApprovalStatus::Cancelled => "cancelled",
            ApprovalStatus::HandedOff => "handed_off",
        }
    }
}

/// Phần chỉ có ở bản ghi đầu tiên của một yêu cầu (tạo dòng): thẻ người duyệt đã thấy.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalDetail {
    /// Đúng như trên thẻ (`ApprovalView`): nội dung Edit/Write đã cắt; lệnh trả về terminal chỉ có phần xem trước.
    pub command_original: String,
    pub command_normalized: String,
    pub risk: Risk,
    pub group_key: String,
    pub created_at: OffsetDateTime,
}

/// Một thay đổi trạng thái của yêu cầu duyệt. Bản ghi đầu tiên của một id mang `detail` (tạo dòng); các bản sau chỉ
/// cập nhật trạng thái (`latency_ms` tính từ `created_at` đã lưu).
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalRecord {
    pub request_id: RequestId,
    pub session_id: String,
    pub agent_id: AgentId,
    pub tool: String,
    pub status: ApprovalStatus,
    pub decided_by: Option<DecidedBy>,
    pub at: OffsetDateTime,
    pub detail: Option<ApprovalDetail>,
}

/// Một dòng `events`, đã serialize sẵn ở phía gửi.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    pub id: String,
    pub ts: OffsetDateTime,
    pub session_id: String,
    pub kind: String,
    pub event_json: String,
    /// `None` khi JSON gốc lớn hơn [`RAW_JSON_MAX_BYTES`].
    pub raw_json: Option<String>,
}

impl EventRecord {
    pub fn new(ev: &Event, raw: &Value) -> Self {
        let kind = serde_json::to_value(ev.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown".to_owned());
        EventRecord {
            id: ev.id.clone(),
            ts: ev.ts,
            session_id: ev.session_id.clone(),
            kind,
            event_json: serde_json::to_string(ev).unwrap_or_default(),
            // Đo trước không cấp phát: body 1 MB (Write, PostToolUse của Read) không bị serialize dở tới 256 KiB rồi bỏ
            // (nhiều yêu cầu đồng thời từng làm đỉnh RSS tăng vài MB).
            raw_json: if json_len_at_least(raw, RAW_JSON_MAX_BYTES) > RAW_JSON_MAX_BYTES {
                None
            } else {
                to_json_capped(raw, RAW_JSON_MAX_BYTES)
            },
        }
    }

    fn size(&self) -> usize {
        self.id.len()
            + self.session_id.len()
            + self.kind.len()
            + self.event_json.len()
            + self.raw_json.as_ref().map_or(0, String::len)
            + 64
    }
}

/// Cận dưới độ dài JSON của `v` (chưa tính ký tự thoát), dừng đếm khi đã vượt `max`. Không cấp phát.
fn json_len_at_least(v: &Value, max: usize) -> usize {
    fn walk(v: &Value, acc: &mut usize, max: usize) {
        if *acc > max {
            return;
        }
        match v {
            Value::Null | Value::Bool(_) | Value::Number(_) => *acc += 4,
            Value::String(s) => *acc += s.len() + 2,
            Value::Array(a) => {
                *acc += 2;
                for x in a {
                    walk(x, acc, max);
                    *acc += 1;
                }
            }
            Value::Object(o) => {
                *acc += 2;
                for (k, x) in o {
                    *acc += k.len() + 4;
                    walk(x, acc, max);
                }
            }
        }
    }
    let mut acc = 0;
    walk(v, &mut acc, max);
    acc
}

/// Serialize nhưng dừng khi vượt `max` byte (không cấp phát quá `max`).
fn to_json_capped(v: &Value, max: usize) -> Option<String> {
    struct Capped(Vec<u8>, usize);
    impl io::Write for Capped {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.0.len() + buf.len() > self.1 {
                return Err(io::Error::other("vượt giới hạn"));
            }
            self.0.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut w = Capped(Vec::new(), max);
    serde_json::to_writer(&mut w, v).ok()?;
    String::from_utf8(w.0).ok()
}

const TS_FORMAT: &[BorrowedFormatItem<'static>] = time::macros::format_description!(
    "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
);

/// Dạng lưu của thời gian: UTC, mili giây, độ dài cố định.
pub fn format_ts(t: OffsetDateTime) -> String {
    t.to_offset(UtcOffset::UTC)
        .format(TS_FORMAT)
        .unwrap_or_default()
}

pub fn parse_ts(s: &str) -> Option<OffsetDateTime> {
    PrimitiveDateTime::parse(s, TS_FORMAT)
        .ok()
        .map(PrimitiveDateTime::assume_utc)
}

fn risk_str(r: Risk) -> &'static str {
    match r {
        Risk::Low => "low",
        Risk::Medium => "medium",
        Risk::High => "high",
        Risk::Critical => "critical",
    }
}

fn decided_by_str(d: DecidedBy) -> &'static str {
    match d {
        DecidedBy::User => "user",
        DecidedBy::Countdown => "countdown",
        DecidedBy::Policy => "policy",
        DecidedBy::Timeout => "timeout",
        DecidedBy::Cancelled => "cancelled",
    }
}

/// Cột `channel`: quyết định tới qua đâu (v0: UI qua WebSocket, hoặc policy).
fn channel_of(d: Option<DecidedBy>) -> Option<&'static str> {
    match d {
        Some(DecidedBy::User | DecidedBy::Countdown) => Some("ws"),
        Some(DecidedBy::Policy) => Some("policy"),
        _ => None,
    }
}

/// Một batch rule đã lưu (T9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchRow {
    pub group_key: String,
    pub opened_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

// ---------------------------------------------------------------------------------------------------------------
// Mở, migration
// ---------------------------------------------------------------------------------------------------------------

/// Đường dẫn mặc định: `<data_dir>/arcturus/arcturus.db` (người gọi truyền `data_dir`).
pub fn default_path(data_dir: &Path) -> PathBuf {
    data_dir.join("arcturus").join("arcturus.db")
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn prepare_dir(path: &Path) -> Result<(), StoreError> {
    let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) else {
        return Ok(());
    };
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir).map_err(io_err(dir))
}

fn tune(conn: &Connection) -> Result<(), StoreError> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.execute_batch(
        "PRAGMA synchronous=NORMAL; PRAGMA cache_size=-512; PRAGMA temp_store=MEMORY;",
    )?;
    Ok(())
}

/// Mở (tạo nếu chưa có) và đưa schema lên [`DB_VERSION`].
fn open_rw(path: &Path) -> Result<Connection, StoreError> {
    prepare_dir(path)?;
    let mut conn = Connection::open(path)?;
    // Nhật ký duyệt có thể chứa lệnh và đường dẫn: chỉ người dùng đọc được (Unix).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(io_err(path))?;
    }
    let mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        tracing::warn!(mode, "SQLite không bật được WAL");
    }
    tune(&conn)?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let found: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if found > DB_VERSION {
        return Err(StoreError::TooNew { found });
    }
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > found) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "đã áp migration SQLite");
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Thread ghi
// ---------------------------------------------------------------------------------------------------------------

/// Xác nhận một bản ghi đã commit (hoặc lỗi). Gọi đúng một lần, trên thread ghi: phải không chặn.
pub type Ack = Box<dyn FnOnce(Result<(), StoreError>) + Send>;

enum Cmd {
    Event(EventRecord),
    Approval(ApprovalRecord, Option<Ack>),
    BatchSet(BatchRow),
    BatchDelete(String),
    Purge(OffsetDateTime),
    Flush(mpsc::Sender<()>),
    Close,
}

#[derive(Default)]
struct Shared {
    queued_events: AtomicUsize,
    queued_bytes: AtomicUsize,
    dropped: AtomicU64,
    dropping: AtomicBool,
}

/// Chủ sở hữu thread ghi. `close` (hoặc drop) ghi hết những gì đã gửi rồi dừng thread.
pub struct Store {
    handle: StoreHandle,
    thread: Option<JoinHandle<()>>,
    path: PathBuf,
}

impl Store {
    /// Mở cơ sở dữ liệu, chạy migration, dọn sự kiện cũ hơn `ttl_days` rồi khởi động thread ghi.
    pub fn open(path: &Path, ttl_days: u32) -> Result<Store, StoreError> {
        let conn = open_rw(path)?;
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared::default());
        let ttl = time::Duration::days(i64::from(ttl_days.max(1)));
        let worker_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("arcturus-store".to_owned())
            .spawn(move || Writer::new(conn, worker_shared, ttl).run(rx))
            .map_err(io_err(path))?;
        Ok(Store {
            handle: StoreHandle { tx, shared },
            thread: Some(thread),
            path: path.to_path_buf(),
        })
    }

    pub fn handle(&self) -> StoreHandle {
        self.handle.clone()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Ghi hết lệnh đang chờ rồi dừng thread ghi. Lệnh gửi sau đó trả [`StoreError::Closed`].
    pub fn close(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.handle.tx.send(Cmd::Close);
        if let Some(t) = self.thread.take() {
            if t.join().is_err() {
                tracing::error!("thread ghi SQLite panic");
            }
        }
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Đầu gửi tới thread ghi; rẻ để clone, không bao giờ chặn.
#[derive(Clone)]
pub struct StoreHandle {
    tx: mpsc::Sender<Cmd>,
    shared: Arc<Shared>,
}

impl StoreHandle {
    /// Gửi một sự kiện. `false`: bị bỏ vì kênh đầy hoặc thread ghi đã dừng.
    pub fn event(&self, rec: EventRecord) -> bool {
        let size = rec.size();
        let s = &self.shared;
        if s.queued_events.load(Ordering::Relaxed) >= EVENT_QUEUE_MAX
            || s.queued_bytes.load(Ordering::Relaxed) + size > EVENT_QUEUE_MAX_BYTES
        {
            let n = s.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            if !s.dropping.swap(true, Ordering::Relaxed) {
                tracing::warn!(dropped_total = n, "kênh ghi SQLite đầy: bỏ bản ghi sự kiện");
            }
            return false;
        }
        s.queued_events.fetch_add(1, Ordering::Relaxed);
        s.queued_bytes.fetch_add(size, Ordering::Relaxed);
        if self.tx.send(Cmd::Event(rec)).is_err() {
            s.queued_events.fetch_sub(1, Ordering::Relaxed);
            s.queued_bytes.fetch_sub(size, Ordering::Relaxed);
            return false;
        }
        if s.dropping.swap(false, Ordering::Relaxed) {
            tracing::info!(
                dropped_total = s.dropped.load(Ordering::Relaxed),
                "kênh ghi SQLite đã thông"
            );
        }
        true
    }

    /// Gửi một bản ghi duyệt (không bao giờ bị bỏ). `ack` được gọi sau khi commit, hoặc ngay với
    /// [`StoreError::Closed`] nếu thread ghi đã dừng.
    pub fn approval(&self, rec: ApprovalRecord, ack: Option<Ack>) {
        if let Err(mpsc::SendError(Cmd::Approval(rec, ack))) = self.tx.send(Cmd::Approval(rec, ack))
        {
            tracing::error!(request_id = %rec.request_id, "thread ghi SQLite đã dừng: mất bản ghi duyệt");
            if let Some(ack) = ack {
                ack(Err(StoreError::Closed));
            }
        }
    }

    /// Lưu (hoặc thay) một batch rule (T9). Không chặn; thread ghi đã dừng thì chỉ log.
    pub fn batch_set(&self, row: BatchRow) {
        if self.tx.send(Cmd::BatchSet(row)).is_err() {
            tracing::warn!("thread ghi SQLite đã dừng: mất bản ghi batch rule");
        }
    }

    /// Xóa batch rule của `group_key`.
    pub fn batch_delete(&self, group_key: &str) {
        if self
            .tx
            .send(Cmd::BatchDelete(group_key.to_owned()))
            .is_err()
        {
            tracing::warn!("thread ghi SQLite đã dừng: mất lệnh xóa batch rule");
        }
    }

    /// Xóa sự kiện có `ts` trước `before` (thread ghi tự gọi theo TTL; dùng trực tiếp trong test).
    pub fn purge_before(&self, before: OffsetDateTime) {
        let _ = self.tx.send(Cmd::Purge(before));
    }

    /// Chặn tới khi mọi lệnh gửi trước đó đã được xử lý. Không gọi trong task async.
    pub fn flush(&self) -> Result<(), StoreError> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Cmd::Flush(tx))
            .map_err(|_| StoreError::Closed)?;
        rx.recv().map_err(|_| StoreError::Closed)
    }

    /// Số sự kiện đã bị bỏ vì kênh đầy.
    pub fn dropped_events(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }
}

struct Writer {
    conn: Connection,
    shared: Arc<Shared>,
    ttl: time::Duration,
}

impl Writer {
    fn new(conn: Connection, shared: Arc<Shared>, ttl: time::Duration) -> Self {
        Writer { conn, shared, ttl }
    }

    fn run(mut self, rx: mpsc::Receiver<Cmd>) {
        let mut next_purge = Instant::now();
        loop {
            let now = Instant::now();
            if now >= next_purge {
                let before = OffsetDateTime::now_utc() - self.ttl;
                if let Err(err) = self.purge(before) {
                    tracing::warn!(%err, "không dọn được sự kiện quá hạn");
                }
                next_purge = now + PURGE_INTERVAL;
            }
            let first = match rx.recv_timeout(next_purge.saturating_duration_since(now)) {
                Ok(c) => c,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            let mut batch = vec![first];
            while batch.len() < BATCH_MAX {
                match rx.try_recv() {
                    Ok(c) => batch.push(c),
                    Err(_) => break,
                }
            }
            if self.process(batch) {
                break;
            }
        }
    }

    /// Ghi một lượt trong một transaction. Trả `true` khi gặp `Close`.
    fn process(&mut self, batch: Vec<Cmd>) -> bool {
        let mut close = false;
        let mut flushes = Vec::new();
        let mut writes: Vec<Cmd> = Vec::with_capacity(batch.len());
        for cmd in batch {
            match cmd {
                Cmd::Event(rec) => {
                    self.shared.queued_events.fetch_sub(1, Ordering::Relaxed);
                    self.shared
                        .queued_bytes
                        .fetch_sub(rec.size(), Ordering::Relaxed);
                    writes.push(Cmd::Event(rec));
                }
                Cmd::Flush(done) => flushes.push(done),
                Cmd::Close => close = true,
                other => writes.push(other),
            }
        }
        self.write_batch(writes);
        for done in flushes {
            let _ = done.send(());
        }
        close
    }

    fn write_batch(&mut self, writes: Vec<Cmd>) {
        if writes.is_empty() {
            return;
        }
        let mut results: Vec<rusqlite::Result<()>> = Vec::with_capacity(writes.len());
        let committed = (|| -> rusqlite::Result<()> {
            let tx = self.conn.transaction()?;
            for w in &writes {
                results.push(apply(&tx, w));
            }
            tx.commit()
        })();
        match committed {
            Ok(()) => {
                for (w, r) in writes.into_iter().zip(results) {
                    finish(w, r.map_err(StoreError::from));
                }
            }
            Err(err) => {
                // Cả lượt không commit được (đĩa đầy, khóa quá lâu…): thử từng lệnh riêng để lỗi của một lệnh không
                // kéo theo cả lượt.
                tracing::warn!(%err, n = writes.len(), "lượt ghi SQLite lỗi, thử từng bản ghi");
                for w in writes {
                    let r = apply(&self.conn, &w);
                    finish(w, r.map_err(StoreError::from));
                }
            }
        }
    }

    fn purge(&self, before: OffsetDateTime) -> rusqlite::Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM events WHERE ts < ?1",
            params![format_ts(before)],
        )?;
        if n > 0 {
            tracing::info!(n, "đã xóa sự kiện quá hạn");
        }
        Ok(n)
    }
}

fn apply(conn: &Connection, cmd: &Cmd) -> rusqlite::Result<()> {
    match cmd {
        Cmd::Event(e) => {
            conn.execute(
                "INSERT OR IGNORE INTO events (id, ts, session_id, kind, event_json, raw_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    e.id,
                    format_ts(e.ts),
                    e.session_id,
                    e.kind,
                    e.event_json,
                    e.raw_json
                ],
            )?;
            Ok(())
        }
        Cmd::Approval(rec, _) => write_approval(conn, rec),
        Cmd::BatchSet(r) => {
            conn.execute(
                "INSERT OR REPLACE INTO batch_rules (group_key, opened_at, expires_at) VALUES (?1, ?2, ?3)",
                params![r.group_key, format_ts(r.opened_at), format_ts(r.expires_at)],
            )?;
            Ok(())
        }
        Cmd::BatchDelete(key) => {
            conn.execute("DELETE FROM batch_rules WHERE group_key = ?1", params![key])?;
            Ok(())
        }
        Cmd::Purge(before) => {
            conn.execute(
                "DELETE FROM events WHERE ts < ?1",
                params![format_ts(*before)],
            )?;
            Ok(())
        }
        Cmd::Flush(_) | Cmd::Close => Ok(()),
    }
}

fn finish(cmd: Cmd, result: Result<(), StoreError>) {
    match cmd {
        Cmd::Approval(rec, ack) => {
            if let Err(err) = &result {
                tracing::error!(request_id = %rec.request_id, %err, "không ghi được bản ghi duyệt");
            }
            if let Some(ack) = ack {
                ack(result);
            }
        }
        Cmd::Event(e) => {
            if let Err(err) = result {
                tracing::warn!(id = %e.id, %err, "không ghi được sự kiện");
            }
        }
        _ => {}
    }
}

fn latency_ms(created: OffsetDateTime, at: OffsetDateTime) -> i64 {
    i64::try_from((at - created).whole_milliseconds())
        .unwrap_or(i64::MAX)
        .max(0)
}

fn write_approval(conn: &Connection, rec: &ApprovalRecord) -> rusqlite::Result<()> {
    let terminal = rec.status != ApprovalStatus::Pending;
    let decided_by = rec.decided_by.map(decided_by_str);
    let channel = channel_of(rec.decided_by);
    let at = format_ts(rec.at);
    if let Some(d) = &rec.detail {
        let (decided_at, latency) = if terminal {
            (Some(at.as_str()), Some(latency_ms(d.created_at, rec.at)))
        } else {
            (None, None)
        };
        conn.execute(
            "INSERT INTO approvals (request_id, session_id, agent_id, tool, command_original, command_normalized,
               risk, group_key, status, decided_by, created_at, decided_at, latency_ms, channel)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(request_id) DO UPDATE SET status = excluded.status, decided_by = excluded.decided_by,
               decided_at = excluded.decided_at, latency_ms = excluded.latency_ms, channel = excluded.channel",
            params![
                rec.request_id,
                rec.session_id,
                rec.agent_id,
                rec.tool,
                d.command_original,
                d.command_normalized,
                risk_str(d.risk),
                d.group_key,
                rec.status.as_str(),
                decided_by,
                format_ts(d.created_at),
                decided_at,
                latency,
                channel,
            ],
        )?;
        return Ok(());
    }
    let created: Option<String> = conn
        .query_row(
            "SELECT created_at FROM approvals WHERE request_id = ?1",
            params![rec.request_id],
            |r| r.get(0),
        )
        .optional()?;
    match created {
        Some(c) => {
            let latency = parse_ts(&c).map(|c| latency_ms(c, rec.at));
            conn.execute(
                "UPDATE approvals SET status = ?2, decided_by = ?3, decided_at = ?4, latency_ms = ?5, channel = ?6
                 WHERE request_id = ?1",
                params![
                    rec.request_id,
                    rec.status.as_str(),
                    decided_by,
                    terminal.then_some(at.as_str()),
                    if terminal { latency } else { None },
                    channel,
                ],
            )?;
        }
        // Không có dòng tạo trước (không xảy ra theo luồng duyệt): vẫn giữ dấu vết, phần thẻ để trống.
        None => {
            conn.execute(
                "INSERT INTO approvals (request_id, session_id, agent_id, tool, command_original, command_normalized,
                   risk, group_key, status, decided_by, created_at, decided_at, latency_ms, channel)
                 VALUES (?1, ?2, ?3, ?4, '', '', 'unknown', ?4, ?5, ?6, ?7, ?8, 0, ?9)",
                params![
                    rec.request_id,
                    rec.session_id,
                    rec.agent_id,
                    rec.tool,
                    rec.status.as_str(),
                    decided_by,
                    at,
                    terminal.then_some(at.as_str()),
                    channel,
                ],
            )?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Đọc
// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRow {
    pub request_id: String,
    pub session_id: String,
    pub agent_id: String,
    pub tool: String,
    pub command_original: String,
    pub command_normalized: String,
    pub risk: String,
    pub group_key: String,
    pub status: String,
    pub decided_by: Option<String>,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub latency_ms: Option<i64>,
    pub channel: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRow {
    pub id: String,
    pub ts: String,
    pub session_id: String,
    pub kind: String,
    pub event_json: String,
    pub raw_json: Option<String>,
}

/// Kết nối đọc riêng (chỉ đọc; WAL cho phép đọc song song với thread ghi). Mở khi cần, không giữ thường trực.
pub struct StoreReader {
    conn: Mutex<Connection>,
}

const APPROVAL_COLS: &str =
    "request_id, session_id, agent_id, tool, command_original, command_normalized, risk,
  group_key, status, decided_by, created_at, decided_at, latency_ms, channel";

fn approval_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ApprovalRow> {
    Ok(ApprovalRow {
        request_id: r.get(0)?,
        session_id: r.get(1)?,
        agent_id: r.get(2)?,
        tool: r.get(3)?,
        command_original: r.get(4)?,
        command_normalized: r.get(5)?,
        risk: r.get(6)?,
        group_key: r.get(7)?,
        status: r.get(8)?,
        decided_by: r.get(9)?,
        created_at: r.get(10)?,
        decided_at: r.get(11)?,
        latency_ms: r.get(12)?,
        channel: r.get(13)?,
    })
}

fn event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<EventRow> {
    Ok(EventRow {
        id: r.get(0)?,
        ts: r.get(1)?,
        session_id: r.get(2)?,
        kind: r.get(3)?,
        event_json: r.get(4)?,
        raw_json: r.get(5)?,
    })
}

impl StoreReader {
    pub fn open(path: &Path) -> Result<StoreReader, StoreError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        tune(&conn)?;
        Ok(StoreReader {
            conn: Mutex::new(conn),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, StoreError> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        f(&conn).map_err(StoreError::from)
    }

    pub fn user_version(&self) -> Result<i32, StoreError> {
        self.with(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
    }

    pub fn approval(&self, request_id: &str) -> Result<Option<ApprovalRow>, StoreError> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {APPROVAL_COLS} FROM approvals WHERE request_id = ?1"),
                params![request_id],
                approval_row,
            )
            .optional()
        })
    }

    /// Mới nhất trước.
    pub fn recent_approvals(&self, limit: usize) -> Result<Vec<ApprovalRow>, StoreError> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {APPROVAL_COLS} FROM approvals ORDER BY created_at DESC LIMIT ?1"
            ))?;
            let rows = st.query_map(params![limit_i64(limit)], approval_row)?;
            rows.collect()
        })
    }

    /// Batch rule đã lưu còn hạn tại `now` (hàng hỏng bị bỏ qua).
    pub fn batch_rules(&self, now: OffsetDateTime) -> Result<Vec<BatchRow>, StoreError> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT group_key, opened_at, expires_at FROM batch_rules WHERE expires_at > ?1 ORDER BY group_key",
            )?;
            let rows = st.query_map(params![format_ts(now)], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (group_key, opened, expires) = row?;
                if let (Some(opened_at), Some(expires_at)) = (parse_ts(&opened), parse_ts(&expires)) {
                    out.push(BatchRow {
                        group_key,
                        opened_at,
                        expires_at,
                    });
                }
            }
            Ok(out)
        })
    }

    pub fn event_count(&self) -> Result<u64, StoreError> {
        self.with(|c| c.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0)))
            .map(|n| u64::try_from(n).unwrap_or(0))
    }

    pub fn approval_count(&self) -> Result<u64, StoreError> {
        self.with(|c| c.query_row("SELECT COUNT(*) FROM approvals", [], |r| r.get::<_, i64>(0)))
            .map(|n| u64::try_from(n).unwrap_or(0))
    }

    /// Sự kiện từ `since` trở đi (không kèm `raw_json`), cũ trước, tối đa `limit` (T19).
    pub fn events_since(
        &self,
        since: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<EventRow>, StoreError> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, ts, session_id, kind, event_json, NULL FROM events
                 WHERE ts >= ?1 ORDER BY ts, rowid LIMIT ?2",
            )?;
            let rows = st.query_map(params![format_ts(since), limit_i64(limit)], event_row)?;
            rows.collect()
        })
    }

    /// Sự kiện của một phiên, cũ trước (cùng mili giây: theo thứ tự ghi, vì chỉ có một thread ghi).
    pub fn session_events(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, StoreError> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, ts, session_id, kind, event_json, raw_json FROM events
                 WHERE session_id = ?1 ORDER BY ts, rowid LIMIT ?2",
            )?;
            let rows = st.query_map(params![session_id, limit_i64(limit)], event_row)?;
            rows.collect()
        })
    }
}

fn limit_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

// ---------------------------------------------------------------------------------------------------------------
// Ring buffer trong RAM
// ---------------------------------------------------------------------------------------------------------------

/// Tối đa sự kiện trong [`EventRing`] (`docs/BUDGET.md`).
pub const RING_MAX_ITEMS: usize = 2000;
/// Tối đa byte ước lượng trong [`EventRing`] (sự kiện có `cwd`, id do agent gửi, không bị chặn độ dài).
pub const RING_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Sự kiện gần nhất trong RAM (không có JSON gốc), cũ bị bỏ trước khi vượt số lượng hoặc byte.
pub struct EventRing {
    items: VecDeque<(Event, usize)>,
    bytes: usize,
    max_items: usize,
    max_bytes: usize,
}

impl Default for EventRing {
    fn default() -> Self {
        EventRing::new(RING_MAX_ITEMS, RING_MAX_BYTES)
    }
}

impl EventRing {
    pub fn new(max_items: usize, max_bytes: usize) -> Self {
        EventRing {
            items: VecDeque::new(),
            bytes: 0,
            max_items,
            max_bytes,
        }
    }

    pub fn push(&mut self, ev: Event) {
        let size = event_size(&ev);
        if size > self.max_bytes || self.max_items == 0 {
            return;
        }
        while self.items.len() >= self.max_items || self.bytes + size > self.max_bytes {
            match self.items.pop_front() {
                Some((_, s)) => self.bytes -= s,
                None => break,
            }
        }
        self.bytes += size;
        self.items.push_back((ev, size));
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// `n` sự kiện mới nhất, cũ trước.
    pub fn recent(&self, n: usize) -> Vec<Event> {
        let skip = self.items.len().saturating_sub(n);
        self.items
            .iter()
            .skip(skip)
            .map(|(e, _)| e.clone())
            .collect()
    }
}

fn event_size(ev: &Event) -> usize {
    std::mem::size_of::<Event>()
        + ev.id.len()
        + ev.host.len()
        + ev.session_id.len()
        + ev.cwd.as_ref().map_or(0, String::len)
        + ev.agent.id.len()
        + ev.agent.parent_id.as_ref().map_or(0, String::len)
        + ev.agent.agent_type.as_ref().map_or(0, String::len)
        + ev.tool
            .as_ref()
            .map_or(0, |t| t.name.len() + t.summary.len())
        + ev.approval.as_ref().map_or(0, |a| a.request_id.len())
        + 128
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AgentKind, AgentRef, EventKind, Source};
    use serde_json::json;
    use time::macros::datetime;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("arcturus-store-{}", ulid::Ulid::generate()));
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn db(&self) -> PathBuf {
            self.0.join("arcturus.db")
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn event(id: &str, session: &str, ts: OffsetDateTime) -> Event {
        Event {
            v: 1,
            id: id.to_owned(),
            ts,
            host: "laptop".to_owned(),
            source: Source::ClaudeCode,
            session_id: session.to_owned(),
            cwd: Some("/w".to_owned()),
            agent: AgentRef {
                id: format!("agt_{session}"),
                parent_id: None,
                kind: AgentKind::Session,
                agent_type: None,
            },
            kind: EventKind::ToolPre,
            activity: None,
            tool: None,
            approval: None,
            metrics: None,
        }
    }

    fn pending(id: &str, at: OffsetDateTime) -> ApprovalRecord {
        ApprovalRecord {
            request_id: id.to_owned(),
            session_id: "s1".to_owned(),
            agent_id: "agt_s1".to_owned(),
            tool: "Bash".to_owned(),
            status: ApprovalStatus::Pending,
            decided_by: None,
            at,
            detail: Some(ApprovalDetail {
                command_original: "rm -rf build".to_owned(),
                command_normalized: "rm -rf build".to_owned(),
                risk: Risk::Critical,
                group_key: "Bash".to_owned(),
                created_at: at,
            }),
        }
    }

    fn update(
        id: &str,
        status: ApprovalStatus,
        by: DecidedBy,
        at: OffsetDateTime,
    ) -> ApprovalRecord {
        ApprovalRecord {
            status,
            decided_by: Some(by),
            at,
            detail: None,
            ..pending(id, at)
        }
    }

    #[test]
    fn migrations_apply_from_empty_and_are_idempotent() {
        let dir = TempDir::new();
        Store::open(&dir.db(), 30).unwrap().close();
        let r = StoreReader::open(&dir.db()).unwrap();
        assert_eq!(r.user_version().unwrap(), DB_VERSION);
        drop(r);
        // Mở lại: không áp lại migration, dữ liệu và phiên bản giữ nguyên.
        let s = Store::open(&dir.db(), 30).unwrap();
        s.handle()
            .approval(pending("req_1", OffsetDateTime::now_utc()), None);
        s.close();
        Store::open(&dir.db(), 30).unwrap().close();
        let r = StoreReader::open(&dir.db()).unwrap();
        assert_eq!(r.user_version().unwrap(), DB_VERSION);
        assert_eq!(r.approval_count().unwrap(), 1);
        let mode: String = r
            .with(|c| c.query_row("PRAGMA journal_mode", [], |row| row.get(0)))
            .unwrap();
        assert_eq!(mode.to_ascii_lowercase(), "wal");
    }

    #[test]
    fn newer_database_is_refused() {
        let dir = TempDir::new();
        let c = Connection::open(dir.db()).unwrap();
        c.pragma_update(None, "user_version", DB_VERSION + 1)
            .unwrap();
        drop(c);
        assert!(matches!(
            Store::open(&dir.db(), 30),
            Err(StoreError::TooNew { .. })
        ));
    }

    #[test]
    fn approval_lifecycle_persisted() {
        let dir = TempDir::new();
        let s = Store::open(&dir.db(), 30).unwrap();
        let h = s.handle();
        let t0 = datetime!(2026-10-23 09:00:00.250 UTC);
        h.approval(pending("req_1", t0), None);
        h.flush().unwrap();
        let r = StoreReader::open(&dir.db()).unwrap();
        let row = r.approval("req_1").unwrap().unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.decided_at, None);
        assert_eq!(row.created_at, "2026-10-23T09:00:00.250Z");
        h.approval(
            update(
                "req_1",
                ApprovalStatus::Allowed,
                DecidedBy::User,
                t0 + Duration::from_millis(1500),
            ),
            None,
        );
        h.flush().unwrap();
        let row = r.approval("req_1").unwrap().unwrap();
        assert_eq!(row.status, "allowed");
        assert_eq!(row.decided_by.as_deref(), Some("user"));
        assert_eq!(row.latency_ms, Some(1500));
        assert_eq!(row.decided_at.as_deref(), Some("2026-10-23T09:00:01.750Z"));
        assert_eq!(row.channel.as_deref(), Some("ws"));
        assert_eq!(row.risk, "critical");
        assert_eq!(row.command_original, "rm -rf build");

        // AutoAllow (có detail, trạng thái cuối ngay) rồi rơi về hỏi người: cancelled ghi đè theo đúng thứ tự.
        let mut auto = pending("req_auto", t0);
        auto.status = ApprovalStatus::Allowed;
        auto.decided_by = Some(DecidedBy::Policy);
        h.approval(auto, None);
        h.approval(
            update(
                "req_auto",
                ApprovalStatus::Cancelled,
                DecidedBy::Cancelled,
                t0 + Duration::from_millis(5),
            ),
            None,
        );
        h.flush().unwrap();
        let row = r.approval("req_auto").unwrap().unwrap();
        assert_eq!(
            (row.status.as_str(), row.decided_by.as_deref()),
            ("cancelled", Some("cancelled"))
        );
        assert_eq!(row.latency_ms, Some(5));
        assert_eq!(row.channel, None);
        s.close();
    }

    #[test]
    fn ack_after_commit_and_error_when_closed() {
        let dir = TempDir::new();
        let s = Store::open(&dir.db(), 30).unwrap();
        let h = s.handle();
        let (tx, rx) = mpsc::channel();
        let tx2 = tx.clone();
        h.approval(
            pending("req_1", OffsetDateTime::now_utc()),
            Some(Box::new(move |r| {
                let _ = tx.send(r.is_ok());
            })),
        );
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap());
        // Đã commit khi ack tới: kết nối khác đọc thấy.
        let r = StoreReader::open(&dir.db()).unwrap();
        assert!(r.approval("req_1").unwrap().is_some());
        s.close();
        h.approval(
            pending("req_2", OffsetDateTime::now_utc()),
            Some(Box::new(move |r| {
                let _ = tx2.send(matches!(r, Err(StoreError::Closed)));
            })),
        );
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap());
        assert!(!h.event(EventRecord::new(
            &event("evt_x", "s", OffsetDateTime::now_utc()),
            &json!({})
        )));
        assert!(h.flush().is_err());
    }

    #[test]
    fn events_older_than_ttl_are_deleted() {
        let dir = TempDir::new();
        let s = Store::open(&dir.db(), 30).unwrap();
        let h = s.handle();
        let now = OffsetDateTime::now_utc();
        for (id, age_days) in [
            ("evt_old", 40),
            ("evt_edge", 31),
            ("evt_new", 1),
            ("evt_now", 0),
        ] {
            h.event(EventRecord::new(
                &event(id, "s1", now - time::Duration::days(age_days)),
                &json!({ "hook_event_name": "PreToolUse" }),
            ));
        }
        h.flush().unwrap();
        let r = StoreReader::open(&dir.db()).unwrap();
        assert_eq!(r.event_count().unwrap(), 4);
        h.purge_before(now - time::Duration::days(30));
        h.flush().unwrap();
        let ids: Vec<String> = r
            .session_events("s1", 10)
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, vec!["evt_new", "evt_now"]);
        s.close();

        // Lúc mở, thread ghi tự dọn theo TTL: TTL 1 ngày xóa nốt `evt_new`.
        let s = Store::open(&dir.db(), 1).unwrap();
        s.handle().flush().unwrap();
        assert_eq!(r.event_count().unwrap(), 1);
        s.close();
    }

    #[test]
    fn full_channel_drops_events_never_approvals() {
        let dir = TempDir::new();
        let s = Store::open(&dir.db(), 30).unwrap();
        let h = s.handle();
        h.flush().unwrap();
        // Kết nối khác giữ khóa ghi: thread ghi kẹt ở lượt đầu, kênh dồn lại.
        let blocker = Connection::open(dir.db()).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
        let now = OffsetDateTime::now_utc();
        let mut accepted = 0usize;
        for i in 0..3000 {
            if h.event(EventRecord::new(
                &event(&format!("evt_{i:05}"), "s1", now),
                &json!({}),
            )) {
                accepted += 1;
            }
            if i % 60 == 0 {
                h.approval(pending(&format!("req_{i:05}"), now), None);
            }
        }
        assert!(accepted < 3000, "phải có sự kiện bị bỏ");
        assert!(accepted <= EVENT_QUEUE_MAX + BATCH_MAX, "{accepted}");
        assert_eq!(h.dropped_events() as usize, 3000 - accepted);
        blocker.execute_batch("ROLLBACK").unwrap();
        h.flush().unwrap();
        let r = StoreReader::open(&dir.db()).unwrap();
        assert_eq!(
            r.approval_count().unwrap(),
            50,
            "không bỏ bản ghi duyệt nào"
        );
        assert_eq!(r.event_count().unwrap() as usize, accepted);
        // Kênh thông lại: nhận sự kiện mới.
        assert!(h.event(EventRecord::new(&event("evt_after", "s1", now), &json!({}))));
        s.close();
    }

    #[test]
    fn survives_restart() {
        let dir = TempDir::new();
        let t = datetime!(2026-10-23 10:00:00 UTC);
        {
            let s = Store::open(&dir.db(), 3650).unwrap();
            let h = s.handle();
            let mut ev = event("evt_1", "s1", t);
            ev.kind = EventKind::ApprovalRequested;
            h.event(EventRecord::new(
                &ev,
                &json!({ "hook_event_name": "PermissionRequest", "x": "ý" }),
            ));
            h.approval(pending("req_1", t), None);
            // Không flush: `close` phải ghi hết.
            s.close();
        }
        let s = Store::open(&dir.db(), 3650).unwrap();
        let r = StoreReader::open(&dir.db()).unwrap();
        let evs = r.session_events("s1", 10).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, "approval.requested");
        assert_eq!(evs[0].ts, "2026-10-23T10:00:00.000Z");
        let back: Event = serde_json::from_str(&evs[0].event_json).unwrap();
        assert_eq!(back.id, "evt_1");
        assert_eq!(
            serde_json::from_str::<Value>(evs[0].raw_json.as_deref().unwrap()).unwrap()["x"],
            json!("ý")
        );
        assert_eq!(r.approval("req_1").unwrap().unwrap().status, "pending");
        s.close();
    }

    #[test]
    fn large_raw_json_is_not_stored() {
        let big = json!({ "tool_response": "x".repeat(RAW_JSON_MAX_BYTES) });
        let rec = EventRecord::new(&event("evt_1", "s", OffsetDateTime::now_utc()), &big);
        assert_eq!(rec.raw_json, None);
        assert!(!rec.event_json.is_empty());
        let small = EventRecord::new(
            &event("evt_2", "s", OffsetDateTime::now_utc()),
            &json!({"a": 1}),
        );
        assert_eq!(small.raw_json.as_deref(), Some(r#"{"a":1}"#));
    }

    #[test]
    fn json_len_estimate_is_a_lower_bound() {
        for v in [
            json!(null),
            json!({"a": [1, 2, {"b": "x\"y"}], "ý": true}),
            json!("x".repeat(1000)),
        ] {
            let real = serde_json::to_string(&v).unwrap().len();
            assert!(json_len_at_least(&v, usize::MAX) <= real + 8, "{v}");
        }
        assert!(json_len_at_least(&json!({"c": "x".repeat(10_000)}), 100) > 100);
    }

    #[test]
    fn ts_format_is_fixed_width_and_sortable() {
        let a = format_ts(datetime!(2026-10-23 09:00:00 UTC));
        let b = format_ts(datetime!(2026-10-23 09:00:00.5 UTC));
        let c = format_ts(datetime!(2026-10-23 16:00:01 +07:00));
        assert_eq!(a, "2026-10-23T09:00:00.000Z");
        assert_eq!(b, "2026-10-23T09:00:00.500Z");
        assert_eq!(c, "2026-10-23T09:00:01.000Z");
        assert!(a < b && b < c);
        assert_eq!(parse_ts(&b), Some(datetime!(2026-10-23 09:00:00.5 UTC)));
    }

    #[test]
    fn batch_rules_persist_replace_delete_and_skip_expired() {
        let dir = TempDir::new();
        let t0 = datetime!(2026-10-27 10:00 UTC);
        let row = |k: &str, mins: i64| BatchRow {
            group_key: k.to_owned(),
            opened_at: t0,
            expires_at: t0 + time::Duration::minutes(mins),
        };
        let s = Store::open(&dir.db(), 30).unwrap();
        let h = s.handle();
        h.batch_set(row("Bash:cargo test", 30));
        h.batch_set(row("Bash:ls", 5));
        h.batch_set(row("Bash:ls", 10)); // thay
        h.batch_set(row("Bash:old", 1));
        h.batch_set(row("Bash:gone", 60));
        h.batch_delete("Bash:gone");
        h.flush().unwrap();
        s.close();
        // Sống qua lần khởi động lại; hàng hết hạn không được trả về.
        let r = StoreReader::open(&dir.db()).unwrap();
        let rows = r.batch_rules(t0 + time::Duration::minutes(2)).unwrap();
        let keys: Vec<&str> = rows.iter().map(|b| b.group_key.as_str()).collect();
        assert_eq!(keys, ["Bash:cargo test", "Bash:ls"]);
        assert_eq!(rows[1].expires_at, t0 + time::Duration::minutes(10));
        assert!(r
            .batch_rules(t0 + time::Duration::minutes(31))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn upgrade_from_v1_database_adds_batch_rules_and_keeps_data() {
        let dir = TempDir::new();
        // Dựng CSDL ở phiên bản 1 (chỉ migration 1), ghi một sự kiện, rồi mở bằng hub hiện tại.
        {
            let mut conn = Connection::open(dir.db()).unwrap();
            let tx = conn.transaction().unwrap();
            tx.execute_batch(MIGRATIONS[0].1).unwrap();
            tx.pragma_update(None, "user_version", 1).unwrap();
            tx.execute(
                "INSERT INTO events (id, ts, session_id, kind, event_json) VALUES ('e1', '2026-10-27T10:00:00.000Z', 's', 'k', '{}')",
                [],
            )
            .unwrap();
            tx.commit().unwrap();
        }
        let s = Store::open(&dir.db(), 30).unwrap();
        s.close();
        let r = StoreReader::open(&dir.db()).unwrap();
        assert_eq!(r.user_version().unwrap(), DB_VERSION);
        assert_eq!(r.event_count().unwrap(), 1, "dữ liệu cũ còn nguyên");
        assert!(r
            .batch_rules(datetime!(2026-10-27 10:00 UTC))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn ring_buffer_caps_items_and_bytes() {
        let now = OffsetDateTime::now_utc();
        let mut ring = EventRing::new(3, 1_000_000);
        for i in 0..5 {
            ring.push(event(&format!("evt_{i}"), "s", now));
        }
        let ids: Vec<String> = ring.recent(10).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, vec!["evt_2", "evt_3", "evt_4"]);
        assert_eq!(ring.recent(1)[0].id, "evt_4");

        let mut ring = EventRing::new(2000, 64 * 1024);
        for i in 0..100 {
            let mut e = event(&format!("evt_{i}"), "s", now);
            e.cwd = Some("d".repeat(8000));
            ring.push(e);
        }
        assert!(ring.bytes() <= 64 * 1024);
        assert!(ring.len() < 10);
        // Một sự kiện lớn hơn cả trần thì không giữ.
        let mut huge = event("evt_huge", "s", now);
        huge.cwd = Some("d".repeat(100_000));
        ring.push(huge);
        assert_ne!(ring.recent(1)[0].id, "evt_huge");
    }
}
