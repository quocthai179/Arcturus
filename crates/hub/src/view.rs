//! Dựng thẻ duyệt `ApprovalView` (`docs/PLAN.md` §3.4) từ một yêu cầu đang chờ.
//!
//! - Bash và mọi trường đường dẫn **không bao giờ bị cắt** (quy tắc bất biến 3). Trường nội dung của Edit/Write/
//!   MultiEdit/NotebookEdit bị cắt còn [`CONTENT_MAX_CHARS`] ký tự mỗi trường; bản đầy đủ lấy qua
//!   `GET /approvals/{id}/input` (T5b).
//! - `command_normalized`: ký tự điều khiển (trừ xuống dòng, tab), zero-width, bidi hiện thành `⟨U+XXXX⟩` và gắn
//!   `Suspicion::HiddenChars`. T8 thay bằng normalize đầy đủ (subshell, eval, encoded, `danger_spans`, `risk`).

use hub_core::protocol::{ApprovalView, Suspicion};
use serde_json::Value;

use crate::approvals::PendingApproval;

/// Tối đa ký tự Unicode cho mỗi trường nội dung trong thẻ duyệt.
pub const CONTENT_MAX_CHARS: usize = 2000;

/// Trường nội dung bị cắt (§3.4). Các trường khác (đường dẫn, lệnh) giữ nguyên.
const CONTENT_FIELDS: [&str; 5] = ["content", "old_string", "new_string", "edits", "new_source"];

pub fn approval_view(p: &PendingApproval, agent_name: &str) -> ApprovalView {
    let (command_original, truncated) = command_text(&p.tool, &p.tool_input);
    let (command_normalized, hidden) = escape_hidden(&command_original);
    ApprovalView {
        request_id: p.request_id.clone(),
        agent_id: p.agent_id.clone(),
        agent_name: agent_name.to_owned(),
        tool: p.tool.clone(),
        command_original,
        command_normalized,
        danger_spans: Vec::new(),
        suspicions: if hidden {
            vec![Suspicion::HiddenChars]
        } else {
            Vec::new()
        },
        truncated,
        risk: p.risk,
        reversible: false,
        group_key: p.tool.clone(),
        created_at: p.created_at,
        deadline_at: p.deadline_at,
        countdown_ms: p
            .countdown
            .map(|c| u64::try_from(c.as_millis()).unwrap_or(u64::MAX)),
    }
}

/// `tool_input` dạng chuỗi cho thẻ duyệt, và có cắt trường nội dung nào không.
pub fn command_text(tool: &str, tool_input: &Value) -> (String, bool) {
    if tool == "Bash" {
        if let Some(cmd) = tool_input.get("command").and_then(Value::as_str) {
            return (cmd.to_owned(), false);
        }
    }
    let mut v = tool_input.clone();
    let mut truncated = false;
    if let Some(obj) = v.as_object_mut() {
        for key in CONTENT_FIELDS {
            let Some(field) = obj.get_mut(key) else {
                continue;
            };
            // Mảng `edits` được đo theo dạng JSON; ngắn thì giữ nguyên cấu trúc.
            let text = match &*field {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let total = text.chars().count();
            if total > CONTENT_MAX_CHARS {
                let head: String = text.chars().take(CONTENT_MAX_CHARS).collect();
                *field = Value::String(format!("{head}\n… [đã cắt, tổng {total} ký tự]"));
                truncated = true;
            }
        }
    }
    let text = serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string());
    (text, truncated)
}

/// Thay ký tự ẩn bằng `⟨U+XXXX⟩`. Trả `true` nếu có thay.
pub fn escape_hidden(s: &str) -> (String, bool) {
    let mut out = String::with_capacity(s.len());
    let mut found = false;
    for c in s.chars() {
        if is_hidden(c) {
            found = true;
            out.push_str(&format!("⟨U+{:04X}⟩", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    (out, found)
}

fn is_hidden(c: char) -> bool {
    match c {
        '\n' | '\t' => false,
        c if c.is_control() => true,
        // Soft hyphen, Mongolian vowel separator, zero-width, LRM/RLM, bidi embedding/override/isolate,
        // word joiner và invisible operators, BOM.
        '\u{00AD}'
        | '\u{180E}'
        | '\u{200B}'..='\u{200F}'
        | '\u{2028}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{2069}'
        | '\u{FEFF}' => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hub_core::schema::Risk;
    use serde_json::json;
    use std::time::Duration;
    use time::macros::datetime;

    fn pending(tool: &str, input: Value) -> PendingApproval {
        PendingApproval {
            request_id: "req_1".to_owned(),
            session_id: "s1".to_owned(),
            agent_id: "agt_s1".to_owned(),
            tool: tool.to_owned(),
            tool_input: input,
            cwd: "D:/code".to_owned(),
            risk: Risk::Medium,
            created_at: datetime!(2026-10-22 09:00:00 UTC),
            deadline_at: datetime!(2026-10-22 09:01:50 UTC),
            countdown: None,
        }
    }

    #[test]
    fn bash_command_is_never_truncated() {
        let long = format!("echo {}", "x".repeat(10_000));
        let v = approval_view(
            &pending("Bash", json!({ "command": long, "description": "d" })),
            "Khoa",
        );
        assert_eq!(v.command_original, long);
        assert!(!v.truncated);
        assert_eq!(v.agent_name, "Khoa");
        assert_eq!(v.group_key, "Bash");
        assert_eq!(v.countdown_ms, None);
    }

    #[test]
    fn edit_content_truncated_but_path_kept() {
        let path = format!("D:/repo/{}/file.rs", "d".repeat(3000));
        let input =
            json!({ "file_path": path, "old_string": "a".repeat(2500), "new_string": "ngắn" });
        let (text, truncated) = command_text("Edit", &input);
        assert!(truncated);
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["file_path"], json!(path), "đường dẫn không bị cắt");
        assert_eq!(back["new_string"], json!("ngắn"));
        let old = back["old_string"].as_str().unwrap();
        assert!(old.starts_with(&"a".repeat(CONTENT_MAX_CHARS)));
        assert!(old.ends_with("… [đã cắt, tổng 2500 ký tự]"));

        let (_, short) = command_text("Write", &json!({ "file_path": "/a", "content": "ok" }));
        assert!(!short);
    }

    #[test]
    fn multi_edit_array_truncated_when_long() {
        let edits: Vec<Value> = (0..100)
            .map(|i| json!({ "old_string": format!("old {i} {}", "z".repeat(50)), "new_string": "n" }))
            .collect();
        let (text, truncated) =
            command_text("MultiEdit", &json!({ "file_path": "/a", "edits": edits }));
        assert!(truncated);
        assert!(text.contains("đã cắt"));
        let (text, truncated) = command_text(
            "MultiEdit",
            &json!({ "file_path": "/a", "edits": [{ "old_string": "a", "new_string": "b" }] }),
        );
        assert!(!truncated);
        assert!(text.contains("\"old_string\": \"a\""));
    }

    #[test]
    fn hidden_chars_escaped_and_flagged() {
        let cmd = "echo safe\u{202E}fdp.exe\u{200B} && ls\tx\nnext";
        let v = approval_view(&pending("Bash", json!({ "command": cmd })), "Khoa");
        assert_eq!(v.command_original, cmd, "bản gốc giữ nguyên");
        assert_eq!(
            v.command_normalized,
            "echo safe⟨U+202E⟩fdp.exe⟨U+200B⟩ && ls\tx\nnext"
        );
        assert_eq!(v.suspicions, vec![Suspicion::HiddenChars]);

        let (s, found) = escape_hidden("thường, tiếng Việt, emoji 🦀");
        assert!(!found);
        assert_eq!(s, "thường, tiếng Việt, emoji 🦀");
        assert_eq!(escape_hidden("\u{1b}[31m").0, "⟨U+001B⟩[31m");
        assert_eq!(escape_hidden("a\u{FEFF}").0, "a⟨U+FEFF⟩");
    }

    #[test]
    fn countdown_in_millis() {
        let mut p = pending("Bash", json!({ "command": "ls" }));
        p.countdown = Some(Duration::from_secs(5));
        assert_eq!(approval_view(&p, "K").countdown_ms, Some(5000));
    }
}
