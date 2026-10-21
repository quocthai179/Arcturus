//! Schema sự kiện chung (`docs/PLAN.md` §3.1). Đây là nguồn sự thật: đổi trường thì cập nhật PLAN.md và
//! `docs/ARCHITECTURE.md` cùng commit; thay đổi phá tương thích thì tăng [`SCHEMA_VERSION`].

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const SCHEMA_VERSION: u32 = 1;

/// `"agt_<session_id>"` hoặc `"agt_<session_id>_<agent_id>"` (agent con).
pub type AgentId = String;
/// `"req_<ulid>"`.
pub type RequestId = String;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Event {
    pub v: u32,
    /// `"evt_<ulid>"`.
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub ts: OffsetDateTime,
    pub host: String,
    pub source: Source,
    pub session_id: String,
    pub agent: AgentRef,
    #[serde(rename = "type")]
    pub kind: EventKind,
    pub activity: Option<Activity>,
    pub tool: Option<ToolInfo>,
    /// Chỉ hub điền khi tạo yêu cầu duyệt (T3); `hook::to_event` luôn để `None`.
    pub approval: Option<ApprovalInfo>,
    pub metrics: Option<Metrics>,
    // Không có raw_ref: JSON hook gốc nằm ở cột events.raw_json, tra theo `id` (§3.5).
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    ClaudeCode,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentRef {
    pub id: AgentId,
    pub parent_id: Option<AgentId>,
    pub kind: AgentKind,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Session,
    Subagent,
    Teammate,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    #[serde(rename = "session.started")]
    SessionStarted,
    #[serde(rename = "prompt.submitted")]
    PromptSubmitted,
    #[serde(rename = "tool.pre")]
    ToolPre,
    #[serde(rename = "tool.post")]
    ToolPost,
    #[serde(rename = "tool.failed")]
    ToolFailed,
    #[serde(rename = "approval.requested")]
    ApprovalRequested,
    #[serde(rename = "approval.resolved")]
    ApprovalResolved,
    #[serde(rename = "subagent.started")]
    SubagentStarted,
    #[serde(rename = "subagent.stopped")]
    SubagentStopped,
    #[serde(rename = "notification")]
    Notification,
    #[serde(rename = "turn.stopped")]
    TurnStopped,
    #[serde(rename = "turn.failed")]
    TurnFailed,
    #[serde(rename = "session.ended")]
    SessionEnded,
    /// Sự kiện chưa hỗ trợ: giữ lại, không bao giờ làm hub trả lỗi.
    #[serde(rename = "unknown")]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Typing,
    Reading,
    Running,
    Thinking,
    Waiting,
    Idle,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mood {
    Idle,
    Working,
    Approval,
    Error,
    Done,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolInfo {
    /// `"Bash"`, `"Edit"`, `"Read"`…
    pub name: String,
    /// Lệnh/đường dẫn rút gọn ≤ 200 ký tự cho UI. Không dùng làm cơ sở duyệt (quy tắc bất biến 3).
    pub summary: String,
    /// Chỉ policy được điền.
    pub risk: Option<Risk>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ApprovalInfo {
    pub request_id: RequestId,
    pub deadline_ms: u64,
    pub decided_by: Option<DecidedBy>,
    pub decision: Option<Decision>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecidedBy {
    User,
    /// Hết giờ đếm ngược phủ quyết (T10, T14).
    Countdown,
    Policy,
    Timeout,
    Cancelled,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Metrics {
    pub context_pct: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::macros::datetime;

    #[test]
    fn event_kind_serde_names_match_plan() {
        let cases = [
            (EventKind::SessionStarted, "session.started"),
            (EventKind::PromptSubmitted, "prompt.submitted"),
            (EventKind::ToolPre, "tool.pre"),
            (EventKind::ToolPost, "tool.post"),
            (EventKind::ToolFailed, "tool.failed"),
            (EventKind::ApprovalRequested, "approval.requested"),
            (EventKind::ApprovalResolved, "approval.resolved"),
            (EventKind::SubagentStarted, "subagent.started"),
            (EventKind::SubagentStopped, "subagent.stopped"),
            (EventKind::Notification, "notification"),
            (EventKind::TurnStopped, "turn.stopped"),
            (EventKind::TurnFailed, "turn.failed"),
            (EventKind::SessionEnded, "session.ended"),
            (EventKind::Unknown, "unknown"),
        ];
        for (kind, name) in cases {
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(name));
            assert_eq!(
                serde_json::from_value::<EventKind>(json!(name)).unwrap(),
                kind
            );
        }
    }

    #[test]
    fn enum_wire_names() {
        assert_eq!(
            serde_json::to_value(Source::ClaudeCode).unwrap(),
            json!("claude-code")
        );
        assert_eq!(
            serde_json::to_value(AgentKind::Subagent).unwrap(),
            json!("subagent")
        );
        assert_eq!(
            serde_json::to_value(Activity::Running).unwrap(),
            json!("running")
        );
        assert_eq!(
            serde_json::to_value(Mood::Approval).unwrap(),
            json!("approval")
        );
        assert_eq!(
            serde_json::to_value(Risk::Critical).unwrap(),
            json!("critical")
        );
        assert_eq!(
            serde_json::to_value(DecidedBy::Countdown).unwrap(),
            json!("countdown")
        );
        assert_eq!(serde_json::to_value(Decision::Deny).unwrap(), json!("deny"));
        assert!(Risk::Low < Risk::Medium && Risk::High < Risk::Critical);
    }

    #[test]
    fn event_roundtrip_and_rfc3339_ts() {
        let ev = Event {
            v: SCHEMA_VERSION,
            id: "evt_01J0000000000000000000000".to_owned(),
            ts: datetime!(2026-10-21 09:15:02.123 UTC),
            host: "laptop".to_owned(),
            source: Source::ClaudeCode,
            session_id: "abc123".to_owned(),
            agent: AgentRef {
                id: "agt_abc123".to_owned(),
                parent_id: None,
                kind: AgentKind::Session,
            },
            kind: EventKind::ApprovalRequested,
            activity: Some(Activity::Waiting),
            tool: Some(ToolInfo {
                name: "Bash".to_owned(),
                summary: "npm test".to_owned(),
                risk: Some(Risk::Low),
            }),
            approval: Some(ApprovalInfo {
                request_id: "req_x".to_owned(),
                deadline_ms: 110_000,
                decided_by: Some(DecidedBy::User),
                decision: Some(Decision::Allow),
            }),
            metrics: Some(Metrics {
                context_pct: Some(0.5),
            }),
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["ts"], "2026-10-21T09:15:02.123Z");
        assert_eq!(v["type"], "approval.requested");
        assert!(v.get("raw_ref").is_none());
        let back: Event = serde_json::from_value(v).unwrap();
        assert_eq!(back, ev);
    }
}
