//! Trust grant cơ bản (`docs/PLAN.md` T10): đếm chuỗi duyệt liên tiếp của **người**, đề xuất nâng mức, tự hạ khi bị từ chối.
//!
//! - `subject`: tên persona (ổn định qua phiên), không phải session id. `pattern`: chính `group_key` (T9). Khóa của lệnh ghép
//!   không bao giờ được đếm hay cấp quyền.
//! - Chỉ quyết định của người (`decided_by = user`) cho yêu cầu risk ≤ Medium được đếm. Duyệt do đếm ngược, do policy hay do
//!   batch rule **không** đếm: chuỗi phải phản ánh việc người đã xem và đồng ý.
//! - Mức 6: [`Grant::AutoAllow`]. Mức 5: hỏi người kèm đếm ngược 10 giây (hub chỉ đặt đếm ngược khi yêu cầu `reversible` và đội
//!   không tạm dừng, §3.4). Mọi quyền chỉ tới Medium; Critical và High không bao giờ (quy tắc bất biến 2, `enforce` lọc lại).
//! - Bị từ chối khi đang có grant: 6 → 5, 5 → thu hồi (ghi `trust_log`). Chuỗi về 0 và ghi `last_incident_at`.
//! - Đề xuất nâng mức: đủ `streak_threshold` lần liên tiếp trong `streak_window_days`, không có sự cố trong cửa sổ đó; một lần
//!   cho mỗi cặp cho tới khi được trả lời; từ chối thì không đề xuất lại trong [`DECLINE_DAYS`] ngày.
//!
//! Module này thuần (không I/O): mỗi thao tác trả một [`Update`] cho hub phát thông điệp và ghi SQLite.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use time::OffsetDateTime;

use super::group::{group_key, is_compound_key};
use super::{Allowance, ApprovalCtx, Assessment, Grant};
use crate::config::TrustConfig;
use crate::schema::{Decision, Risk};

pub const LEVEL_AUTO: u8 = 6;
pub const LEVEL_COUNTDOWN: u8 = 5;
/// Mức ghi vào `trust_log` khi grant bị thu hồi.
pub const LEVEL_REVOKED: u8 = 4;
/// Đếm ngược phủ quyết của mức 5.
pub const COUNTDOWN: Duration = Duration::from_secs(10);
/// Không đề xuất lại chừng này ngày sau khi người từ chối.
pub const DECLINE_DAYS: i64 = 7;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustGrant {
    pub id: String,
    pub subject: String,
    pub pattern: String,
    pub level: u8,
    pub granted_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub reason: String,
    pub revoked_at: Option<OffsetDateTime>,
}

impl TrustGrant {
    fn active(&self, now: OffsetDateTime) -> bool {
        self.revoked_at.is_none() && self.expires_at > now
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Streak {
    pub subject: String,
    pub pattern: String,
    pub approved_in_row: u32,
    pub first_at: Option<OffsetDateTime>,
    pub last_at: Option<OffsetDateTime>,
    pub last_incident_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustLogEntry {
    pub id: String,
    pub ts: OffsetDateTime,
    pub subject: String,
    pub pattern: String,
    pub from_level: Option<u8>,
    pub to_level: Option<u8>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustSuggestion {
    pub id: String,
    pub subject: String,
    pub pattern: String,
    pub level: u8,
    pub approved_in_row: u32,
    pub window_days: u32,
    pub grant_days: u32,
    pub created_at: OffsetDateTime,
}

/// Việc hub cần báo cho người dùng.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrustEvent {
    Suggest(TrustSuggestion),
    Demoted {
        subject: String,
        pattern: String,
        from: u8,
        to: u8,
    },
    Revoked {
        subject: String,
        pattern: String,
        from: u8,
    },
}

/// Kết quả một thao tác: thông báo cần phát và bản ghi cần lưu.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Update {
    pub events: Vec<TrustEvent>,
    pub grants: Vec<TrustGrant>,
    pub streaks: Vec<Streak>,
    pub log: Vec<TrustLogEntry>,
}

impl Update {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
            && self.grants.is_empty()
            && self.streaks.is_empty()
            && self.log.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answered {
    Granted(TrustGrant),
    Declined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustError {
    /// Không có đề xuất với id này (đã trả lời, hoặc hết hiệu lực vì bị từ chối).
    UnknownSuggestion,
}

type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;
type Pair = (String, String);

#[derive(Default)]
struct Inner {
    grants: BTreeMap<String, TrustGrant>,
    streaks: BTreeMap<Pair, Streak>,
    suggestions: BTreeMap<String, TrustSuggestion>,
    pending: BTreeMap<Pair, String>,
    declined_until: BTreeMap<Pair, OffsetDateTime>,
}

pub struct TrustBook {
    cfg: TrustConfig,
    inner: Mutex<Inner>,
    clock: Clock,
}

fn id(prefix: &str) -> String {
    format!("{prefix}_{}", ulid::Ulid::generate())
}

impl TrustBook {
    pub fn new(cfg: &TrustConfig) -> Self {
        Self::with_clock(cfg, Arc::new(OffsetDateTime::now_utc))
    }

    /// Đồng hồ giả cho test.
    pub fn with_clock(cfg: &TrustConfig, clock: Clock) -> Self {
        TrustBook {
            cfg: cfg.clone(),
            inner: Mutex::new(Inner::default()),
            clock,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn now(&self) -> OffsetDateTime {
        (self.clock)()
    }

    fn window(&self) -> time::Duration {
        time::Duration::days(i64::from(self.cfg.streak_window_days))
    }

    /// Grant còn hiệu lực cao nhất của cặp (id, mức).
    pub fn grant_level(&self, subject: &str, pattern: &str) -> Option<(String, u8)> {
        let now = self.now();
        let inner = self.lock();
        Self::best(&inner, subject, pattern, now).map(|g| (g.id.clone(), g.level))
    }

    fn best<'a>(
        inner: &'a Inner,
        subject: &str,
        pattern: &str,
        now: OffsetDateTime,
    ) -> Option<&'a TrustGrant> {
        inner
            .grants
            .values()
            .filter(|g| g.subject == subject && g.pattern == pattern && g.active(now))
            .max_by_key(|g| g.level)
    }

    /// Nạp lại trạng thái đã lưu (khởi động hub). Grant hết hạn/đã thu hồi và mức lạ bị bỏ; trả số grant nạp được.
    pub fn restore(&self, grants: Vec<TrustGrant>, streaks: Vec<Streak>) -> usize {
        let now = self.now();
        let mut inner = self.lock();
        let mut n = 0;
        for g in grants {
            if g.active(now)
                && matches!(g.level, LEVEL_AUTO | LEVEL_COUNTDOWN)
                && !is_compound_key(&g.pattern)
            {
                inner.grants.insert(g.id.clone(), g);
                n += 1;
            }
        }
        for s in streaks {
            if !is_compound_key(&s.pattern) {
                inner
                    .streaks
                    .insert((s.subject.clone(), s.pattern.clone()), s);
            }
        }
        n
    }

    /// Chuỗi duyệt hiện tại của một cặp (chẩn đoán, test).
    pub fn streak(&self, subject: &str, pattern: &str) -> Option<Streak> {
        self.lock()
            .streaks
            .get(&(subject.to_owned(), pattern.to_owned()))
            .cloned()
    }

    pub fn pending_suggestions(&self) -> Vec<TrustSuggestion> {
        self.lock().suggestions.values().cloned().collect()
    }

    /// Quyết định của **người** cho một yêu cầu. Chỉ risk ≤ Medium và khóa không ghép mới được tính.
    pub fn record(&self, subject: &str, pattern: &str, decision: Decision, risk: Risk) -> Update {
        if risk > Risk::Medium || subject.is_empty() || is_compound_key(pattern) {
            return Update::default();
        }
        match decision {
            Decision::Deny => self.incident(subject, pattern, "người từ chối"),
            Decision::Allow => self.approved(subject, pattern),
        }
    }

    fn approved(&self, subject: &str, pattern: &str) -> Update {
        let now = self.now();
        let window = self.window();
        let pair = (subject.to_owned(), pattern.to_owned());
        let mut inner = self.lock();
        let streak = inner.streaks.entry(pair.clone()).or_insert_with(|| Streak {
            subject: subject.to_owned(),
            pattern: pattern.to_owned(),
            approved_in_row: 0,
            first_at: None,
            last_at: None,
            last_incident_at: None,
        });
        // Chuỗi cũ hơn cửa sổ thì bắt đầu lại: ngưỡng phải đạt trong `streak_window_days`.
        if streak.first_at.is_some_and(|f| now - f > window) {
            streak.approved_in_row = 0;
            streak.first_at = None;
        }
        streak.approved_in_row += 1;
        streak.last_at = Some(now);
        streak.first_at.get_or_insert(now);
        let streak = streak.clone();
        let mut update = Update {
            streaks: vec![streak.clone()],
            ..Update::default()
        };

        let quiet = streak.last_incident_at.is_none_or(|t| now - t > window);
        let has_top =
            Self::best(&inner, subject, pattern, now).is_some_and(|g| g.level >= LEVEL_AUTO);
        let declined = inner
            .declined_until
            .get(&pair)
            .is_some_and(|until| *until > now);
        if streak.approved_in_row >= self.cfg.streak_threshold
            && quiet
            && !has_top
            && !declined
            && !inner.pending.contains_key(&pair)
        {
            let s = TrustSuggestion {
                id: id("sug"),
                subject: subject.to_owned(),
                pattern: pattern.to_owned(),
                level: LEVEL_AUTO,
                approved_in_row: streak.approved_in_row,
                window_days: self.cfg.streak_window_days,
                grant_days: self.cfg.grant_days,
                created_at: now,
            };
            inner.pending.insert(pair, s.id.clone());
            inner.suggestions.insert(s.id.clone(), s.clone());
            update.events.push(TrustEvent::Suggest(s));
        }
        update
    }

    /// Sự cố (người từ chối, hoặc báo từ bước sau: hoàn tác): chuỗi về 0, hạ grant một mức.
    pub fn report_incident(&self, subject: &str, pattern: &str) -> Update {
        self.incident(subject, pattern, "sự cố")
    }

    fn incident(&self, subject: &str, pattern: &str, why: &str) -> Update {
        let now = self.now();
        let pair = (subject.to_owned(), pattern.to_owned());
        let mut inner = self.lock();
        let mut update = Update::default();
        let streak = inner.streaks.entry(pair.clone()).or_insert_with(|| Streak {
            subject: subject.to_owned(),
            pattern: pattern.to_owned(),
            approved_in_row: 0,
            first_at: None,
            last_at: None,
            last_incident_at: None,
        });
        streak.approved_in_row = 0;
        streak.first_at = None;
        streak.last_at = None;
        streak.last_incident_at = Some(now);
        update.streaks.push(streak.clone());
        // Đề xuất đang chờ không còn đúng (chuỗi đã đứt).
        if let Some(sid) = inner.pending.remove(&pair) {
            inner.suggestions.remove(&sid);
        }
        let Some(gid) = Self::best(&inner, subject, pattern, now).map(|g| g.id.clone()) else {
            return update;
        };
        // Mọi grant còn hiệu lực của cặp đều bị đụng tới; mức cao nhất quyết định kết quả.
        let from = inner.grants[&gid].level;
        for g in inner.grants.values_mut() {
            if g.subject == subject && g.pattern == pattern && g.active(now) {
                if g.level > LEVEL_COUNTDOWN {
                    g.level = LEVEL_COUNTDOWN;
                } else {
                    g.revoked_at = Some(now);
                }
                update.grants.push(g.clone());
            }
        }
        let to = if from > LEVEL_COUNTDOWN {
            LEVEL_COUNTDOWN
        } else {
            LEVEL_REVOKED
        };
        update.log.push(TrustLogEntry {
            id: id("trl"),
            ts: now,
            subject: subject.to_owned(),
            pattern: pattern.to_owned(),
            from_level: Some(from),
            to_level: Some(to),
            reason: why.to_owned(),
        });
        update.events.push(if to == LEVEL_REVOKED {
            TrustEvent::Revoked {
                subject: subject.to_owned(),
                pattern: pattern.to_owned(),
                from,
            }
        } else {
            TrustEvent::Demoted {
                subject: subject.to_owned(),
                pattern: pattern.to_owned(),
                from,
                to,
            }
        });
        update
    }

    /// Người trả lời một đề xuất. Chấp nhận: tạo grant mức 6 hạn `grant_days` (thay grant mức 5 nếu có). Từ chối: không đề
    /// xuất lại cặp đó trong [`DECLINE_DAYS`] ngày.
    pub fn answer(
        &self,
        suggestion_id: &str,
        accept: bool,
    ) -> Result<(Answered, Update), TrustError> {
        let now = self.now();
        let mut inner = self.lock();
        let s = inner
            .suggestions
            .remove(suggestion_id)
            .ok_or(TrustError::UnknownSuggestion)?;
        let pair = (s.subject.clone(), s.pattern.clone());
        inner.pending.remove(&pair);
        let mut update = Update::default();
        if !accept {
            inner
                .declined_until
                .insert(pair, now + time::Duration::days(DECLINE_DAYS));
            return Ok((Answered::Declined, update));
        }
        let previous = Self::best(&inner, &s.subject, &s.pattern, now).map(|g| g.level);
        for g in inner.grants.values_mut() {
            if g.subject == s.subject && g.pattern == s.pattern && g.active(now) {
                g.revoked_at = Some(now);
                update.grants.push(g.clone());
            }
        }
        let grant = TrustGrant {
            id: id("trg"),
            subject: s.subject.clone(),
            pattern: s.pattern.clone(),
            level: LEVEL_AUTO,
            granted_at: now,
            expires_at: now + time::Duration::days(i64::from(self.cfg.grant_days)),
            reason: format!("{} lần duyệt liên tiếp", s.approved_in_row),
            revoked_at: None,
        };
        inner.grants.insert(grant.id.clone(), grant.clone());
        update.grants.push(grant.clone());
        update.log.push(TrustLogEntry {
            id: id("trl"),
            ts: now,
            subject: s.subject,
            pattern: s.pattern,
            from_level: previous,
            to_level: Some(LEVEL_AUTO),
            reason: format!("người chấp nhận đề xuất {suggestion_id}"),
        });
        Ok((Answered::Granted(grant), update))
    }
}

impl Allowance for Arc<TrustBook> {
    fn grant(&self, ctx: &ApprovalCtx<'_>, a: &Assessment) -> Option<Grant> {
        // Chỉ tới Medium; Critical/High để người duyệt (`enforce` vẫn lọc lại ở hub).
        if a.risk > Risk::Medium || a.is_critical() || ctx.subject.is_empty() {
            return None;
        }
        let key = group_key(ctx.tool, ctx.tool_input);
        if is_compound_key(&key) {
            return None;
        }
        match self.grant_level(ctx.subject, &key)? {
            (gid, LEVEL_AUTO) => Some(Grant::AutoAllow {
                reason: format!("trust:{gid}"),
            }),
            (_, LEVEL_COUNTDOWN) => Some(Grant::Countdown(COUNTDOWN)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use serde_json::{json, Value};
    use time::macros::datetime;

    use super::super::danger::{DangerConfig, DangerRules};
    use super::super::{assess, enforce, Policy, PolicyOutcome, RulesPolicy};
    use super::*;

    const T0: OffsetDateTime = datetime!(2026-10-27 10:00 UTC);
    const KEY: &str = "Bash:npm test";

    fn cfg() -> TrustConfig {
        TrustConfig {
            streak_threshold: 3,
            streak_window_days: 7,
            grant_days: 30,
        }
    }

    /// Đồng hồ giả tính bằng giờ kể từ T0.
    fn book() -> (Arc<TrustBook>, Arc<AtomicI64>) {
        let hours = Arc::new(AtomicI64::new(0));
        let h = hours.clone();
        let b = Arc::new(TrustBook::with_clock(
            &cfg(),
            Arc::new(move || T0 + time::Duration::hours(h.load(Ordering::SeqCst))),
        ));
        (b, hours)
    }

    fn allow(b: &TrustBook, n: usize) -> Vec<Update> {
        (0..n)
            .map(|_| b.record("Khoa", KEY, Decision::Allow, Risk::Medium))
            .collect()
    }

    fn suggestions(updates: &[Update]) -> Vec<TrustSuggestion> {
        updates
            .iter()
            .flat_map(|u| u.events.iter())
            .filter_map(|e| match e {
                TrustEvent::Suggest(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    fn grant_it(b: &TrustBook) -> TrustGrant {
        let s = suggestions(&allow(b, 3)).remove(0);
        match b.answer(&s.id, true).unwrap().0 {
            Answered::Granted(g) => g,
            Answered::Declined => panic!("phải cấp"),
        }
    }

    #[test]
    fn suggests_after_threshold_within_window() {
        let (b, hours) = book();
        let ups = allow(&b, 2);
        assert!(suggestions(&ups).is_empty(), "mới 2 lần");
        let third = b.record("Khoa", KEY, Decision::Allow, Risk::Medium);
        let s = suggestions(&[third]);
        assert_eq!(s.len(), 1);
        assert_eq!(
            (s[0].subject.as_str(), s[0].pattern.as_str(), s[0].level),
            ("Khoa", KEY, 6)
        );
        assert_eq!(
            (s[0].approved_in_row, s[0].window_days, s[0].grant_days),
            (3, 7, 30)
        );
        // Một lần cho mỗi cặp cho tới khi được trả lời.
        assert!(suggestions(&allow(&b, 5)).is_empty());
        assert_eq!(b.pending_suggestions().len(), 1);
        // Cặp khác không bị ảnh hưởng.
        assert!(suggestions(&[b.record("Linh", KEY, Decision::Allow, Risk::Medium)]).is_empty());
        // Chuỗi chậm hơn cửa sổ thì không đạt: 2 lần, chờ 8 ngày, 2 lần nữa.
        let (b2, h2) = book();
        allow(&b2, 2);
        h2.store(8 * 24, Ordering::SeqCst);
        assert!(
            suggestions(&allow(&b2, 2)).is_empty(),
            "chuỗi cũ hơn 7 ngày bắt đầu lại"
        );
        assert_eq!(
            suggestions(&allow(&b2, 1)).len(),
            1,
            "3 lần mới trong cửa sổ"
        );
        let _ = hours;
    }

    #[test]
    fn no_suggestion_if_incident_in_window() {
        let (b, hours) = book();
        b.record("Khoa", KEY, Decision::Deny, Risk::Medium);
        hours.store(24, Ordering::SeqCst);
        assert!(
            suggestions(&allow(&b, 5)).is_empty(),
            "sự cố 1 ngày trước, trong cửa sổ 7 ngày"
        );
        hours.store(8 * 24, Ordering::SeqCst);
        assert_eq!(
            suggestions(&allow(&b, 3)).len(),
            1,
            "sự cố đã ra ngoài cửa sổ"
        );
    }

    #[test]
    fn accept_creates_grant_level_6() {
        let (b, _) = book();
        let g = grant_it(&b);
        assert_eq!(
            (g.subject.as_str(), g.pattern.as_str(), g.level),
            ("Khoa", KEY, 6)
        );
        assert_eq!(g.expires_at, T0 + time::Duration::days(30));
        assert!(g.id.starts_with("trg_") && g.revoked_at.is_none());
        assert_eq!(b.grant_level("Khoa", KEY), Some((g.id.clone(), 6)));
        assert!(b.pending_suggestions().is_empty());
        // Đã có mức 6 thì không đề xuất nữa.
        assert!(suggestions(&allow(&b, 10)).is_empty());
    }

    #[test]
    fn decline_blocks_new_suggestions_for_seven_days() {
        let (b, hours) = book();
        let s = suggestions(&allow(&b, 3)).remove(0);
        let (a, up) = b.answer(&s.id, false).unwrap();
        assert_eq!(a, Answered::Declined);
        assert!(up.is_empty());
        assert_eq!(b.grant_level("Khoa", KEY), None);
        hours.store(6 * 24, Ordering::SeqCst);
        assert!(
            suggestions(&allow(&b, 3)).is_empty(),
            "đang trong 7 ngày kể từ lúc từ chối"
        );
        hours.store(7 * 24 + 1, Ordering::SeqCst);
        assert_eq!(suggestions(&allow(&b, 3)).len(), 1);
    }

    #[test]
    fn answering_twice_or_unknown_id_is_an_error() {
        let (b, _) = book();
        let s = suggestions(&allow(&b, 3)).remove(0);
        assert!(b.answer(&s.id, true).is_ok());
        assert_eq!(
            b.answer(&s.id, true).unwrap_err(),
            TrustError::UnknownSuggestion
        );
        assert_eq!(
            b.answer("sug_khong_co", true).unwrap_err(),
            TrustError::UnknownSuggestion
        );
    }

    #[test]
    fn deny_under_grant_demotes_6_to_5_then_revokes() {
        let (b, _) = book();
        let g = grant_it(&b);
        let up = b.record("Khoa", KEY, Decision::Deny, Risk::Medium);
        assert_eq!(
            up.events,
            [TrustEvent::Demoted {
                subject: "Khoa".into(),
                pattern: KEY.into(),
                from: 6,
                to: 5
            }]
        );
        assert_eq!(b.grant_level("Khoa", KEY), Some((g.id.clone(), 5)));
        assert_eq!(
            (up.log[0].from_level, up.log[0].to_level),
            (Some(6), Some(5))
        );
        assert_eq!(up.streaks[0].approved_in_row, 0);
        // Từ chối nữa: thu hồi.
        let up = b.record("Khoa", KEY, Decision::Deny, Risk::Low);
        assert_eq!(
            up.events,
            [TrustEvent::Revoked {
                subject: "Khoa".into(),
                pattern: KEY.into(),
                from: 5
            }]
        );
        assert_eq!(b.grant_level("Khoa", KEY), None);
        assert!(up.grants[0].revoked_at.is_some());
        assert_eq!(
            (up.log[0].from_level, up.log[0].to_level),
            (Some(5), Some(4))
        );
        // Không có grant: từ chối chỉ đặt lại chuỗi, không có sự kiện hạ mức.
        assert!(b
            .record("Khoa", KEY, Decision::Deny, Risk::Medium)
            .events
            .is_empty());
    }

    #[test]
    fn grant_expires() {
        let (b, hours) = book();
        grant_it(&b);
        hours.store(30 * 24 - 1, Ordering::SeqCst);
        assert!(b.grant_level("Khoa", KEY).is_some());
        hours.store(30 * 24, Ordering::SeqCst);
        assert_eq!(b.grant_level("Khoa", KEY), None);
    }

    #[test]
    fn only_human_decisions_on_low_or_medium_simple_commands_count() {
        let (b, _) = book();
        for risk in [Risk::High, Risk::Critical] {
            assert!(
                b.record("Khoa", KEY, Decision::Allow, risk).is_empty(),
                "{risk:?}"
            );
            assert!(
                b.record("Khoa", KEY, Decision::Deny, risk).is_empty(),
                "{risk:?}"
            );
        }
        assert!(b
            .record(
                "Khoa",
                "Bash:npm test (ghép)",
                Decision::Allow,
                Risk::Medium
            )
            .is_empty());
        assert!(b.record("", KEY, Decision::Allow, Risk::Medium).is_empty());
        assert!(b.report_incident("Khoa", KEY).grants.is_empty());
    }

    #[test]
    fn restore_skips_expired_revoked_and_odd_levels() {
        let (b, _) = book();
        let g = |id: &str, level: u8, exp_days: i64, revoked: bool| TrustGrant {
            id: id.to_owned(),
            subject: "Khoa".to_owned(),
            pattern: KEY.to_owned(),
            level,
            granted_at: T0,
            expires_at: T0 + time::Duration::days(exp_days),
            reason: "x".to_owned(),
            revoked_at: revoked.then_some(T0),
        };
        let n = b.restore(
            vec![
                g("a", 6, 10, false),
                g("b", 6, -1, false),
                g("c", 6, 10, true),
                g("d", 9, 10, false),
            ],
            vec![],
        );
        assert_eq!(n, 1);
        assert_eq!(b.grant_level("Khoa", KEY), Some(("a".to_owned(), 6)));
    }

    // ---- Qua policy ---------------------------------------------------------------------------------------------

    fn rules() -> DangerRules {
        DangerRules::new(&DangerConfig {
            hub_port: Some(7878),
            token_paths: vec![],
            state_paths: vec![],
            home: Some("/home/u".to_owned()),
        })
    }

    fn decide(policy: &RulesPolicy, subject: &str, tool: &str, input: &Value) -> PolicyOutcome {
        let ctx = ApprovalCtx {
            agent_id: "agt_s",
            subject,
            tool,
            tool_input: input,
            cwd: "/home/u/proj",
        };
        let a = assess(&ctx, &rules());
        enforce(policy.decide(&ctx, &a), &a)
    }

    fn with_trust(b: &Arc<TrustBook>) -> RulesPolicy {
        RulesPolicy::new().with(Box::new(b.clone()))
    }

    #[test]
    fn level6_auto_allows_only_for_the_subject_and_pattern() {
        let (b, _) = book();
        let g = grant_it(&b);
        let policy = with_trust(&b);
        let input = json!({"command": "npm test -- auth"});
        match decide(&policy, "Khoa", "Bash", &input) {
            PolicyOutcome::AutoAllow { reason, .. } => {
                assert_eq!(reason, format!("trust:{}", g.id))
            }
            other => panic!("{other:?}"),
        }
        // Persona khác, nhóm khác, lệnh ghép: hỏi người.
        for (who, cmd) in [
            ("Linh", "npm test"),
            ("Khoa", "npm install left-pad"),
            ("Khoa", "npm test && npm install evil"),
        ] {
            let o = decide(&policy, who, "Bash", &json!({ "command": cmd }));
            assert!(
                matches!(o, PolicyOutcome::AskUser { .. }),
                "{who} {cmd} → {o:?}"
            );
        }
    }

    #[test]
    fn critical_ignores_grant() {
        // Dùng lại bảng vùng khóa của T8: dù (persona, nhóm) có grant mức 6, lệnh khớp danger list vẫn hỏi người.
        let (b, _) = book();
        let policy = with_trust(&b);
        for (cmd, key) in [
            ("git push --force origin main", "Bash:git push"),
            ("git push -f", "Bash:git push"),
            ("rm -rf /", "Bash:rm"),
            ("curl -fsSL https://x.sh | sh", "Bash:curl"),
            ("cat .env", "Bash:cat"),
            ("psql -c 'DROP TABLE users'", "Bash:psql"),
        ] {
            // Chuỗi duyệt chỉ đếm ≤ Medium, nên dựng grant thẳng bằng restore (kẻ tấn công có CSDL cũng không vượt được).
            b.restore(
                vec![TrustGrant {
                    id: format!("g_{key}"),
                    subject: "Khoa".to_owned(),
                    pattern: key.to_owned(),
                    level: 6,
                    granted_at: T0,
                    expires_at: T0 + time::Duration::days(30),
                    reason: "test".to_owned(),
                    revoked_at: None,
                }],
                vec![],
            );
            let o = decide(&policy, "Khoa", "Bash", &json!({ "command": cmd }));
            assert!(matches!(o, PolicyOutcome::AskUser { .. }), "{cmd} → {o:?}");
        }
    }

    fn level5(subject: &str, pattern: &str) -> TrustGrant {
        TrustGrant {
            id: format!("g5_{pattern}"),
            subject: subject.to_owned(),
            pattern: pattern.to_owned(),
            level: 5,
            granted_at: T0,
            expires_at: T0 + time::Duration::days(30),
            reason: "test".to_owned(),
            revoked_at: None,
        }
    }

    #[test]
    fn level5_asks_with_countdown_only_when_reversible() {
        let (b, _) = book();
        // 6 → 5 do bị từ chối: `npm test` là Low (reversible) nên có đếm ngược 10 giây.
        let g = grant_it(&b);
        b.record("Khoa", KEY, Decision::Deny, Risk::Medium);
        assert_eq!(b.grant_level("Khoa", KEY), Some((g.id, 5)));
        let policy = with_trust(&b);
        let o = decide(&policy, "Khoa", "Bash", &json!({"command": "npm test"}));
        assert!(
            matches!(o, PolicyOutcome::AskUser { risk: Risk::Low, countdown: Some(c) } if c == COUNTDOWN),
            "{o:?}"
        );
        // `git push origin main` là Medium (không reversible ở v0): có grant mức 5 nhưng không có đếm ngược.
        b.restore(vec![level5("Khoa", "Bash:git push")], vec![]);
        let o = decide(
            &policy,
            "Khoa",
            "Bash",
            &json!({"command": "git push origin main"}),
        );
        assert!(
            matches!(
                o,
                PolicyOutcome::AskUser {
                    risk: Risk::Medium,
                    countdown: None
                }
            ),
            "{o:?}"
        );
    }
}
