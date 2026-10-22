//! Thông điệp WebSocket (`docs/PLAN.md` §3.4). JSON dạng `{"t": "<loại>", ...}`.
//!
//! T4: phần trạng thái agent. T5a: thông điệp server phát ra từ luồng duyệt và đội (`ApprovalRequested`,
//! `ApprovalResolved`, `TeamUpdated`). T5b thêm `Snapshot`, `Feed`, `ClientMsg` và viết `docs/PROTOCOL.md`.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::schema::{
    Activity, AgentId, AgentKind, DecidedBy, Decision, Mood, RequestId, Risk, Source,
};

// Thông điệp chỉ sống ngắn (tạo rồi gửi broadcast), giữ hình dạng §3.4 thay vì `Box` từng biến thể.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    AgentUpdated {
        agent: AgentView,
    },
    AgentRemoved {
        id: AgentId,
    },
    ApprovalRequested {
        approval: ApprovalView,
    },
    /// `decision` là `None` khi `decided_by` là `timeout` hoặc `cancelled` (hub trả `{}`, quyết định D2 ở T5).
    ApprovalResolved {
        request_id: RequestId,
        decision: Option<Decision>,
        decided_by: DecidedBy,
    },
    TeamUpdated {
        team: TeamView,
    },
    /// Lệnh quá dài để duyệt trên thẻ: hub đã trả `{}`, người quyết định trong terminal. Thẻ chỉ để báo, không có nút
    /// duyệt (quy tắc bất biến 3, T5b).
    ApprovalHandedOff {
        handoff: HandoffView,
    },
    /// Thẻ báo "duyệt trong terminal" hết hiệu lực (agent đã sang bước khác hoặc phiên kết thúc).
    HandoffCleared {
        handoff_id: String,
    },
}

/// Thẻ báo cho lệnh trả về terminal. Không phải thẻ duyệt: UI không được hiện nút Allow/Deny cho nó.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HandoffView {
    /// `"hof_<ulid>"`; không bao giờ là `request_id` của yêu cầu đang chờ.
    pub handoff_id: String,
    pub agent_id: AgentId,
    pub agent_name: String,
    pub tool: String,
    /// Kích thước của lệnh đầy đủ (byte UTF-8) và số dòng.
    pub bytes: usize,
    pub lines: usize,
    /// Đầu và cuối lệnh (khoảng 300 ký tự mỗi phần), đã hiện ký tự ẩn như `command_normalized`.
    pub preview_head: String,
    pub preview_tail: String,
    /// Tính trên lệnh **đầy đủ**, không chỉ phần xem trước.
    pub suspicions: Vec<Suspicion>,
    pub message: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Thẻ duyệt (§3.4). `command_normalized` là cơ sở duyệt (quy tắc bất biến 3), không phải tóm tắt.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ApprovalView {
    pub request_id: RequestId,
    pub agent_id: AgentId,
    pub agent_name: String,
    pub tool: String,
    /// `tool_input` gốc dạng chuỗi: Bash là lệnh; tool khác là JSON, trường nội dung bị cắt (`truncated`).
    pub command_original: String,
    /// Sau chuẩn hóa: ký tự ẩn/điều khiển/bidi hiện thành `⟨U+XXXX⟩` (T8 thay bằng normalize đầy đủ).
    pub command_normalized: String,
    /// `[start, end)` theo chỉ số ký tự Unicode trong `command_normalized` (§3.4).
    pub danger_spans: Vec<(usize, usize)>,
    pub suspicions: Vec<Suspicion>,
    pub truncated: bool,
    pub risk: Risk,
    pub reversible: bool,
    pub group_key: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub deadline_at: OffsetDateTime,
    /// Đếm ngược phủ quyết; `None` khi đội tạm dừng.
    pub countdown_ms: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Suspicion {
    HiddenChars,
    Subshell,
    Eval,
    Encoded,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TeamView {
    pub paused: bool,
    pub focus: bool,
    pub roster_max: u8,
}

/// Ai gửi lệnh `Decide`: người bấm, hay UI tự gửi khi hết đếm ngược (§3.4).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecideOrigin {
    User,
    Countdown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentView {
    pub id: AgentId,
    pub parent_id: Option<AgentId>,
    pub kind: AgentKind,
    pub name: String,
    pub role: String,
    pub palette: Palette,
    pub mood: Mood,
    pub activity: Activity,
    /// Ví dụ `"Đang sửa LoginForm.tsx"`. Suy ra từ `ToolInfo::summary`: chỉ để hiển thị, không dùng để duyệt.
    pub act_text: String,
    pub host: String,
    pub source: Source,
    pub cwd: String,
    pub context_pct: Option<f32>,
    #[serde(with = "time::serde::rfc3339")]
    pub last_event_at: OffsetDateTime,
}

/// Màu nhân vật, dạng `#RRGGBB` (kiểm ở `config.rs` cho persona trong `arcturus.toml`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub hair: String,
    pub outfit: String,
    pub acc: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::macros::datetime;

    #[test]
    fn server_msg_wire_format() {
        let removed = ServerMsg::AgentRemoved {
            id: "agt_s1".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&removed).unwrap(),
            json!({"t": "agent_removed", "id": "agt_s1"})
        );

        let updated = ServerMsg::AgentUpdated {
            agent: AgentView {
                id: "agt_s1".to_owned(),
                parent_id: None,
                kind: AgentKind::Session,
                name: "Khoa".to_owned(),
                role: "backend".to_owned(),
                palette: Palette {
                    hair: "#1F1F24".to_owned(),
                    outfit: "#4C8DF6".to_owned(),
                    acc: "#9AA3B5".to_owned(),
                },
                mood: Mood::Working,
                activity: Activity::Typing,
                act_text: "Đang sửa main.rs".to_owned(),
                host: "laptop".to_owned(),
                source: Source::ClaudeCode,
                cwd: "D:/code/Arcturus".to_owned(),
                context_pct: None,
                last_event_at: datetime!(2026-10-22 09:00:00 UTC),
            },
        };
        let v = serde_json::to_value(&updated).unwrap();
        assert_eq!(v["t"], "agent_updated");
        assert_eq!(v["agent"]["mood"], "working");
        assert_eq!(v["agent"]["last_event_at"], "2026-10-22T09:00:00Z");
        let back: ServerMsg = serde_json::from_value(v).unwrap();
        assert_eq!(back, updated);
    }

    #[test]
    fn approval_and_team_messages_wire_format() {
        let requested = ServerMsg::ApprovalRequested {
            approval: ApprovalView {
                request_id: "req_1".to_owned(),
                agent_id: "agt_s1".to_owned(),
                agent_name: "Khoa".to_owned(),
                tool: "Bash".to_owned(),
                command_original: "echo a\u{202E}b".to_owned(),
                command_normalized: "echo a⟨U+202E⟩b".to_owned(),
                danger_spans: vec![(0, 4)],
                suspicions: vec![Suspicion::HiddenChars],
                truncated: false,
                risk: Risk::Medium,
                reversible: false,
                group_key: "Bash".to_owned(),
                created_at: datetime!(2026-10-22 09:00:00 UTC),
                deadline_at: datetime!(2026-10-22 09:01:50 UTC),
                countdown_ms: None,
            },
        };
        let v = serde_json::to_value(&requested).unwrap();
        assert_eq!(v["t"], "approval_requested");
        assert_eq!(v["approval"]["danger_spans"], json!([[0, 4]]));
        assert_eq!(v["approval"]["suspicions"], json!(["hidden_chars"]));
        assert_eq!(v["approval"]["deadline_at"], "2026-10-22T09:01:50Z");
        assert_eq!(serde_json::from_value::<ServerMsg>(v).unwrap(), requested);

        // D2: hết giờ/hủy không có quyết định.
        let timed_out = ServerMsg::ApprovalResolved {
            request_id: "req_1".to_owned(),
            decision: None,
            decided_by: DecidedBy::Timeout,
        };
        assert_eq!(
            serde_json::to_value(&timed_out).unwrap(),
            json!({"t": "approval_resolved", "request_id": "req_1", "decision": null, "decided_by": "timeout"})
        );
        let allowed = ServerMsg::ApprovalResolved {
            request_id: "req_1".to_owned(),
            decision: Some(Decision::Allow),
            decided_by: DecidedBy::User,
        };
        assert_eq!(
            serde_json::to_value(&allowed).unwrap()["decision"],
            json!("allow")
        );

        let team = ServerMsg::TeamUpdated {
            team: TeamView {
                paused: true,
                focus: false,
                roster_max: 5,
            },
        };
        assert_eq!(
            serde_json::to_value(&team).unwrap(),
            json!({"t": "team_updated", "team": {"paused": true, "focus": false, "roster_max": 5}})
        );
        assert_eq!(
            serde_json::to_value(DecideOrigin::Countdown).unwrap(),
            json!("countdown")
        );
    }

    #[test]
    fn handoff_messages_wire_format() {
        let m = ServerMsg::ApprovalHandedOff {
            handoff: HandoffView {
                handoff_id: "hof_1".to_owned(),
                agent_id: "agt_s1".to_owned(),
                agent_name: "Khoa".to_owned(),
                tool: "Bash".to_owned(),
                bytes: 5000,
                lines: 80,
                preview_head: "head".to_owned(),
                preview_tail: "tail".to_owned(),
                suspicions: vec![Suspicion::HiddenChars],
                message: "m".to_owned(),
                created_at: datetime!(2026-10-22 09:00:00 UTC),
            },
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["t"], "approval_handed_off");
        assert_eq!(v["handoff"]["bytes"], 5000);
        assert!(
            v["handoff"].get("request_id").is_none(),
            "không có id để Decide"
        );
        assert_eq!(serde_json::from_value::<ServerMsg>(v).unwrap(), m);
        assert_eq!(
            serde_json::to_value(ServerMsg::HandoffCleared {
                handoff_id: "hof_1".to_owned()
            })
            .unwrap(),
            json!({"t": "handoff_cleared", "handoff_id": "hof_1"})
        );
    }
}
