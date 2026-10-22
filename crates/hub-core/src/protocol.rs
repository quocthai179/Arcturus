//! Thông điệp WebSocket (`docs/PLAN.md` §3.4). JSON dạng `{"t": "<loại>", ...}`.
//!
//! T4 mới có phần trạng thái agent (`AgentUpdated`, `AgentRemoved`, `AgentView`, `Palette`); T5 thêm các biến thể
//! còn lại, `ClientMsg` và viết `docs/PROTOCOL.md`.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::schema::{Activity, AgentId, AgentKind, Mood, Source};

// Thông điệp chỉ sống ngắn (tạo rồi gửi broadcast), giữ hình dạng §3.4 thay vì `Box` từng biến thể.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    AgentUpdated { agent: AgentView },
    AgentRemoved { id: AgentId },
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
}
