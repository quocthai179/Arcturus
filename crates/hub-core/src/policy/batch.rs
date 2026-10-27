//! Batch rule: "cho phép mẫu X trong N phút" (`docs/PLAN.md` T9).
//!
//! Mỗi quy tắc gắn với một [`group_key`](super::group::group_key). Trong thời hạn, yêu cầu **trùng khóa** và có risk ≤ Medium
//! được tự duyệt ([`Allowance`]); High và Critical không bao giờ (quy tắc bất biến 2, và `enforce` lọc lại ở hub). Khóa của
//! lệnh ghép không mở batch rule được ([`BatchError::Compound`]). Đội tạm dừng thì hub đổi mọi tự duyệt thành hỏi người.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use time::OffsetDateTime;

use super::group::{group_key, is_compound_key};
use super::{Allowance, ApprovalCtx, Assessment, Grant};
use crate::schema::Risk;

pub const MIN_MINUTES: u32 = 1;
pub const MAX_MINUTES: u32 = 240;
/// Tối đa quy tắc đang mở cùng lúc.
pub const MAX_RULES: usize = 64;
/// Độ dài tối đa của khóa (ký tự).
pub const MAX_KEY_CHARS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchRule {
    pub group_key: String,
    pub opened_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchError {
    /// `minutes` ngoài [`MIN_MINUTES`]..=[`MAX_MINUTES`].
    BadMinutes,
    /// Khóa rỗng, quá dài, hoặc chứa ký tự điều khiển.
    BadKey,
    /// Khóa của lệnh ghép.
    Compound,
    TooMany,
}

type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

pub struct BatchRules {
    rules: Mutex<BTreeMap<String, BatchRule>>,
    clock: Clock,
}

impl Default for BatchRules {
    fn default() -> Self {
        Self::with_clock(Arc::new(OffsetDateTime::now_utc))
    }
}

impl BatchRules {
    pub fn new() -> Self {
        Self::default()
    }

    /// Đồng hồ giả cho test.
    pub fn with_clock(clock: Clock) -> Self {
        BatchRules {
            rules: Mutex::new(BTreeMap::new()),
            clock,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, BatchRule>> {
        self.rules.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Giờ theo đồng hồ của bộ quy tắc (đồng hồ giả trong test).
    pub fn now(&self) -> OffsetDateTime {
        (self.clock)()
    }

    /// Mở (hoặc gia hạn: đặt lại thời hạn tính từ bây giờ) quy tắc cho `key`.
    pub fn open(&self, key: &str, minutes: u32) -> Result<BatchRule, BatchError> {
        if !(MIN_MINUTES..=MAX_MINUTES).contains(&minutes) {
            return Err(BatchError::BadMinutes);
        }
        if key.is_empty()
            || key.chars().count() > MAX_KEY_CHARS
            || key.chars().any(char::is_control)
        {
            return Err(BatchError::BadKey);
        }
        if is_compound_key(key) {
            return Err(BatchError::Compound);
        }
        let now = self.now();
        let mut rules = self.lock();
        rules.retain(|_, r| r.expires_at > now);
        if !rules.contains_key(key) && rules.len() >= MAX_RULES {
            return Err(BatchError::TooMany);
        }
        let rule = BatchRule {
            group_key: key.to_owned(),
            opened_at: now,
            expires_at: now + time::Duration::minutes(i64::from(minutes)),
        };
        rules.insert(key.to_owned(), rule.clone());
        Ok(rule)
    }

    /// Đóng quy tắc. `true` nếu có quy tắc còn hạn bị xóa.
    pub fn close(&self, key: &str) -> bool {
        let now = self.now();
        self.lock().remove(key).is_some_and(|r| r.expires_at > now)
    }

    /// Nạp lại quy tắc đã lưu (khởi động hub). Quy tắc hết hạn, khóa ghép, hoặc vượt giới hạn bị bỏ.
    pub fn restore(&self, rule: BatchRule) -> bool {
        let now = self.now();
        if rule.expires_at <= now
            || is_compound_key(&rule.group_key)
            || rule.group_key.is_empty()
            || rule.group_key.chars().count() > MAX_KEY_CHARS
            || rule.group_key.chars().any(char::is_control)
            // Hạn dài quá mức tối đa (CSDL bị sửa tay): không tin.
            || rule.expires_at > now + time::Duration::minutes(i64::from(MAX_MINUTES))
        {
            return false;
        }
        let mut rules = self.lock();
        if rules.len() >= MAX_RULES && !rules.contains_key(&rule.group_key) {
            return false;
        }
        rules.insert(rule.group_key.clone(), rule);
        true
    }

    /// Quy tắc còn hạn, theo khóa. Xóa luôn các quy tắc đã hết hạn.
    pub fn active(&self) -> Vec<BatchRule> {
        let now = self.now();
        let mut rules = self.lock();
        rules.retain(|_, r| r.expires_at > now);
        rules.values().cloned().collect()
    }

    /// Còn hạn không (không xóa gì).
    pub fn is_open(&self, key: &str) -> bool {
        let now = self.now();
        self.lock().get(key).is_some_and(|r| r.expires_at > now)
    }

    /// Hạn của quy tắc `key` nếu đang mở.
    pub fn expiry(&self, key: &str) -> Option<OffsetDateTime> {
        let now = self.now();
        self.lock()
            .get(key)
            .map(|r| r.expires_at)
            .filter(|e| *e > now)
    }
}

impl Allowance for Arc<BatchRules> {
    fn grant(&self, ctx: &ApprovalCtx<'_>, a: &Assessment) -> Option<Grant> {
        // Chỉ tới Medium; Critical/High để người duyệt (`enforce` vẫn lọc lại ở hub).
        if a.risk > Risk::Medium || a.is_critical() {
            return None;
        }
        let key = group_key(ctx.tool, ctx.tool_input);
        self.is_open(&key).then(|| Grant::AutoAllow {
            reason: format!("batch:{key}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use serde_json::{json, Value};
    use time::macros::datetime;

    use super::super::danger::{DangerConfig, DangerRules};
    use super::super::{assess, Policy, PolicyOutcome, RulesPolicy};
    use super::*;

    const T0: OffsetDateTime = datetime!(2026-10-27 10:00 UTC);

    /// Đồng hồ giả: giây kể từ T0.
    fn clock() -> (Arc<BatchRules>, Arc<AtomicI64>) {
        let secs = Arc::new(AtomicI64::new(0));
        let s = secs.clone();
        let rules = Arc::new(BatchRules::with_clock(Arc::new(move || {
            T0 + time::Duration::seconds(s.load(Ordering::SeqCst))
        })));
        (rules, secs)
    }

    fn rules() -> DangerRules {
        DangerRules::new(&DangerConfig {
            hub_port: Some(7878),
            token_paths: vec![],
            state_paths: vec![],
            home: Some("/home/u".to_owned()),
        })
    }

    fn decide(policy: &RulesPolicy, tool: &str, input: &Value) -> PolicyOutcome {
        let ctx = ApprovalCtx {
            agent_id: "agt_s",
            tool,
            tool_input: input,
            cwd: "/home/u/proj",
        };
        let a = assess(&ctx, &rules());
        policy.decide(&ctx, &a)
    }

    fn with_rules(batch: &Arc<BatchRules>) -> RulesPolicy {
        RulesPolicy::new().with(Box::new(batch.clone()))
    }

    fn is_auto(o: &PolicyOutcome) -> bool {
        matches!(o, PolicyOutcome::AutoAllow { .. })
    }

    #[test]
    fn batch_rule_auto_allows_until_expiry() {
        let (batch, secs) = clock();
        let policy = with_rules(&batch);
        let input = json!({"command": "cargo test -p hub"});
        assert!(!is_auto(&decide(&policy, "Bash", &input)), "chưa mở rule");
        batch.open("Bash:cargo test", 30).unwrap();
        let o = decide(&policy, "Bash", &input);
        assert!(is_auto(&o), "{o:?}");
        match o {
            PolicyOutcome::AutoAllow { reason, .. } => assert_eq!(reason, "batch:Bash:cargo test"),
            other => panic!("{other:?}"),
        }
        // Lệnh khác cùng nhóm cũng khớp; nhóm khác thì không.
        assert!(is_auto(&decide(
            &policy,
            "Bash",
            &json!({"command": "cargo test --release"})
        )));
        assert!(!is_auto(&decide(
            &policy,
            "Bash",
            &json!({"command": "cargo build"})
        )));
        secs.store(29 * 60, Ordering::SeqCst);
        assert!(is_auto(&decide(&policy, "Bash", &input)), "còn 1 phút");
        secs.store(30 * 60, Ordering::SeqCst);
        assert!(
            !is_auto(&decide(&policy, "Bash", &input)),
            "hết hạn đúng lúc"
        );
        assert!(batch.active().is_empty(), "rule hết hạn bị xóa");
    }

    #[test]
    fn batch_rule_never_applies_to_high_or_critical() {
        let (batch, _) = clock();
        let policy = with_rules(&batch);
        // Cố ý mở rule cho nhóm mà lệnh nguy hiểm cùng nhóm: `git push` (nhóm của `git push --force`).
        batch.open("Bash:git push", 60).unwrap();
        for cmd in ["git push --force origin main", "git push -f", "rm -rf /"] {
            let o = decide(&policy, "Bash", &json!({ "command": cmd }));
            assert!(!is_auto(&o), "{cmd} → {o:?}");
        }
        // Risk High (mã viết thẳng) cũng không.
        batch.open("Bash:python", 60).unwrap();
        let o = decide(&policy, "Bash", &json!({"command": "python -c 'print(1)'"}));
        assert!(!is_auto(&o), "{o:?}");
        // Ký tự ẩn → High.
        batch.open("Bash:cargo test\u{200B}", 60).ok();
        let o = decide(&policy, "Bash", &json!({"command": "cargo test\u{200B}"}));
        assert!(!is_auto(&o), "{o:?}");
    }

    #[test]
    fn compound_commands_are_not_covered_by_the_simple_rule() {
        let (batch, _) = clock();
        let policy = with_rules(&batch);
        batch.open("Bash:cargo test", 60).unwrap();
        for cmd in [
            "cargo test && npm install evil",
            "cargo test; echo x",
            "cargo test $(echo hi)",
        ] {
            let o = decide(&policy, "Bash", &json!({ "command": cmd }));
            assert!(!is_auto(&o), "{cmd} → {o:?}");
        }
        assert_eq!(
            batch.open("Bash:cargo test (ghép)", 60),
            Err(BatchError::Compound)
        );
    }

    #[test]
    fn batch_close_stops_auto_allow() {
        let (batch, _) = clock();
        let policy = with_rules(&batch);
        let input = json!({"command": "npm test"});
        batch.open("Bash:npm test", 60).unwrap();
        assert!(is_auto(&decide(&policy, "Bash", &input)));
        assert!(batch.close("Bash:npm test"));
        assert!(!is_auto(&decide(&policy, "Bash", &input)));
        assert!(!batch.close("Bash:npm test"), "đóng lần hai: không có gì");
    }

    #[test]
    fn open_validates_input_and_renews() {
        let (batch, secs) = clock();
        assert_eq!(batch.open("Bash:ls", 0), Err(BatchError::BadMinutes));
        assert_eq!(batch.open("Bash:ls", 241), Err(BatchError::BadMinutes));
        assert_eq!(batch.open("", 5), Err(BatchError::BadKey));
        assert_eq!(batch.open("Bash:l\ns", 5), Err(BatchError::BadKey));
        assert_eq!(batch.open(&"x".repeat(201), 5), Err(BatchError::BadKey));
        assert!(batch.open("Bash:ls", 1).is_ok());
        assert!(batch.open("Bash:ls", 240).is_ok());
        secs.store(120 * 60, Ordering::SeqCst);
        assert_eq!(
            batch.expiry("Bash:ls"),
            Some(T0 + time::Duration::minutes(240))
        );
        // Gia hạn đặt lại hạn tính từ bây giờ.
        batch.open("Bash:ls", 10).unwrap();
        assert_eq!(
            batch.expiry("Bash:ls"),
            Some(T0 + time::Duration::minutes(130))
        );
    }

    #[test]
    fn rule_count_is_capped_but_renewal_always_works() {
        let (batch, _) = clock();
        for i in 0..MAX_RULES {
            batch.open(&format!("Bash:cmd{i}"), 5).unwrap();
        }
        assert_eq!(batch.open("Bash:extra", 5), Err(BatchError::TooMany));
        assert!(batch.open("Bash:cmd0", 5).is_ok());
        assert_eq!(batch.active().len(), MAX_RULES);
    }

    #[test]
    fn restore_skips_expired_compound_and_implausible_rules() {
        let (batch, _) = clock();
        let rule = |k: &str, from_now: i64| BatchRule {
            group_key: k.to_owned(),
            opened_at: T0,
            expires_at: T0 + time::Duration::minutes(from_now),
        };
        assert!(batch.restore(rule("Bash:ls", 10)));
        assert!(!batch.restore(rule("Bash:old", -1)), "đã hết hạn");
        assert!(!batch.restore(rule("Bash:ls (ghép)", 10)), "khóa ghép");
        assert!(
            !batch.restore(rule("Bash:far", 24 * 60)),
            "hạn quá xa so với trần"
        );
        assert!(!batch.restore(rule("", 10)));
        assert_eq!(batch.active().len(), 1);
    }

    #[test]
    fn other_tools_batch_by_tool_but_still_only_up_to_medium() {
        let (batch, _) = clock();
        let policy = with_rules(&batch);
        batch.open("Edit:*", 30).unwrap();
        // Edit trong workspace là Medium: được tự duyệt.
        let inside =
            json!({"file_path": "/home/u/proj/a.rs", "old_string": "a", "new_string": "b"});
        assert!(is_auto(&decide(&policy, "Edit", &inside)));
        // Sửa file bí mật là vùng khóa: không.
        let secret =
            json!({"file_path": "/home/u/proj/.env", "old_string": "a", "new_string": "b"});
        assert!(!is_auto(&decide(&policy, "Edit", &secret)));
    }
}
