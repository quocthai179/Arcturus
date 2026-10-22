//! Dựng thẻ duyệt `ApprovalView` (`docs/PLAN.md` §3.4) từ một yêu cầu đang chờ.
//!
//! - Bash và mọi trường đường dẫn **không bao giờ bị cắt** (quy tắc bất biến 3). Chỉ với Edit/Write/MultiEdit/
//!   NotebookEdit, trường nội dung bị cắt còn [`CONTENT_MAX_CHARS`] ký tự mỗi trường (MultiEdit: từng `old_string`/
//!   `new_string` của từng edit, giữ nguyên mảng và `replace_all`); bản đầy đủ lấy qua `GET /approvals/{id}/input`
//!   (T5b). Tool khác (kể cả MCP có trường `content`) hiện đầy đủ.
//! - `command_normalized`: ký tự vô hình hoặc giả khoảng trắng ([`is_hidden`]) hiện thành `⟨U+XXXX⟩` (chuỗi lặp:
//!   `⟨U+XXXX×N⟩`). `Suspicion::HiddenChars` dựa trên mọi chuỗi trong `tool_input` gốc, không chỉ phần hiển thị.
//!   T8 thay bằng normalize đầy đủ theo thuộc tính Unicode (subshell, eval, encoded, `danger_spans`, `risk`).

use std::collections::BTreeMap;

use hub_core::protocol::{ApprovalView, HandoffView, Suspicion};
use serde_json::{Map, Value};
use time::OffsetDateTime;

use crate::approvals::PendingApproval;

/// Số ký tự xem trước ở đầu và ở cuối thẻ "duyệt trong terminal".
pub const PREVIEW_CHARS: usize = 300;

/// Tối đa ký tự Unicode cho mỗi trường nội dung trong thẻ duyệt.
pub const CONTENT_MAX_CHARS: usize = 2000;

/// Tool có trường nội dung được phép cắt (§3.4).
const TRUNCATED_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Trường nội dung bị cắt. Các trường khác (đường dẫn, cờ) giữ nguyên.
const CONTENT_FIELDS: [&str; 4] = ["content", "old_string", "new_string", "new_source"];

pub fn approval_view(p: &PendingApproval, agent_name: &str) -> ApprovalView {
    let (command_original, truncated) = command_text(&p.tool, &p.tool_input);
    let (command_normalized, shown_hidden) = escape_hidden(&command_original);
    let hidden = shown_hidden || value_has_hidden(&p.tool_input);
    ApprovalView {
        request_id: p.request_id.clone(),
        agent_id: p.agent_id.clone(),
        agent_name: agent_name.to_owned(),
        tool: p.tool.clone(),
        tool_options: tool_options(&p.tool, &p.tool_input),
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

/// Nội dung thẻ sẽ hiện đầy đủ (không cắt) cho tool này, để đo ngưỡng "quá dài". `None` với Edit/Write/MultiEdit/
/// NotebookEdit: nội dung của chúng được cắt kèm `truncated` và bản đầy đủ (quyết định L1 ở T5b).
pub fn untruncated_text(tool: &str, tool_input: &Value) -> Option<String> {
    if TRUNCATED_TOOLS.contains(&tool) {
        None
    } else {
        Some(command_text(tool, tool_input).0)
    }
}

/// Quá `max_bytes` byte UTF-8 hoặc quá `max_lines` dòng.
pub fn is_too_long(text: &str, max_bytes: usize, max_lines: usize) -> bool {
    text.len() > max_bytes || text.lines().count() > max_lines
}

/// Thẻ báo cho lệnh trả về terminal. Cờ tính trên `text` đầy đủ và `tool_input` gốc; xem trước đã hiện ký tự ẩn.
pub fn handoff_view(
    handoff_id: &str,
    p: &PendingApproval,
    agent_name: &str,
    text: &str,
    now: OffsetDateTime,
) -> HandoffView {
    let total = text.chars().count();
    let (head, tail) = if total <= 2 * PREVIEW_CHARS {
        (text.to_owned(), String::new())
    } else {
        (
            text.chars().take(PREVIEW_CHARS).collect(),
            text.chars().skip(total - PREVIEW_CHARS).collect(),
        )
    };
    let hidden = text.chars().any(is_hidden) || value_has_hidden(&p.tool_input);
    let who = if agent_name.is_empty() {
        "agent"
    } else {
        agent_name
    };
    HandoffView {
        handoff_id: handoff_id.to_owned(),
        agent_id: p.agent_id.clone(),
        agent_name: agent_name.to_owned(),
        tool: p.tool.clone(),
        bytes: text.len(),
        lines: text.lines().count(),
        preview_head: escape_hidden(&head).0,
        preview_tail: escape_hidden(&tail).0,
        suspicions: if hidden {
            vec![Suspicion::HiddenChars]
        } else {
            Vec::new()
        },
        tool_options: tool_options(&p.tool, &p.tool_input),
        message: format!(
            "Lệnh quá dài để duyệt ở đây — hãy mở terminal của {who} để xem đủ và quyết định."
        ),
        created_at: now,
    }
}

/// Bash: các trường ngoài `command`/`description` (đổi cách lệnh chạy: `dangerouslyDisableSandbox`, `run_in_background`,
/// `timeout`…), giá trị JSON đã hiện ký tự ẩn. Tool khác: rỗng. Khóa có ký tự ẩn cũng được escape.
pub fn tool_options(tool: &str, tool_input: &Value) -> BTreeMap<String, String> {
    if tool != "Bash" {
        return BTreeMap::new();
    }
    // `command` không phải chuỗi thì cả `tool_input` đã hiện dạng JSON trong `command_*`.
    if !tool_input.get("command").is_some_and(Value::is_string) {
        return BTreeMap::new();
    }
    tool_input
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter(|(k, _)| k.as_str() != "command" && k.as_str() != "description")
                .map(|(k, v)| (escape_hidden(k).0, escape_hidden(&v.to_string()).0))
                .collect()
        })
        .unwrap_or_default()
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
    if TRUNCATED_TOOLS.contains(&tool) {
        if let Some(obj) = v.as_object_mut() {
            truncate_fields(obj, &mut truncated);
            if let Some(Value::Array(edits)) = obj.get_mut("edits") {
                for edit in edits.iter_mut().filter_map(Value::as_object_mut) {
                    truncate_fields(edit, &mut truncated);
                }
            }
        }
    }
    let text = serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string());
    (text, truncated)
}

fn truncate_fields(obj: &mut Map<String, Value>, truncated: &mut bool) {
    for key in CONTENT_FIELDS {
        let Some(Value::String(s)) = obj.get_mut(key) else {
            continue;
        };
        let total = s.chars().count();
        if total > CONTENT_MAX_CHARS {
            let head: String = s.chars().take(CONTENT_MAX_CHARS).collect();
            *s = format!("{head}\n… [đã cắt, tổng {total} ký tự]");
            *truncated = true;
        }
    }
}

/// Có ký tự ẩn trong bất kỳ khóa hay chuỗi nào của `v` (trước khi serde đổi ký tự điều khiển thành `\uXXXX`).
fn value_has_hidden(v: &Value) -> bool {
    match v {
        Value::String(s) => s.chars().any(is_hidden),
        Value::Array(a) => a.iter().any(value_has_hidden),
        Value::Object(o) => o
            .iter()
            .any(|(k, v)| k.chars().any(is_hidden) || value_has_hidden(v)),
        _ => false,
    }
}

/// Thay ký tự ẩn bằng `⟨U+XXXX⟩`, chuỗi lặp cùng ký tự thành `⟨U+XXXX×N⟩` (không phình gấp 12 lần). Trả `true`
/// nếu có thay.
pub fn escape_hidden(s: &str) -> (String, bool) {
    let mut out = String::with_capacity(s.len());
    let mut found = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if !is_hidden(c) {
            out.push(c);
            continue;
        }
        found = true;
        let mut run = 1usize;
        while chars.next_if_eq(&c).is_some() {
            run += 1;
        }
        if run == 1 {
            out.push_str(&format!("⟨U+{:04X}⟩", u32::from(c)));
        } else {
            out.push_str(&format!("⟨U+{:04X}×{run}⟩", u32::from(c)));
        }
    }
    (out, found)
}

/// Ký tự không hiện ra (hoặc trông như khoảng trắng nhưng shell không tách từ) trên thẻ duyệt.
///
/// Gồm: Cc (trừ `\n`, `\t`); toàn bộ Default_Ignorable_Code_Point; các Cf khác; Zl/Zp; Co (vùng riêng); khoảng trắng
/// không phải ASCII (Zs trừ U+0020) và U+2800. Chưa gồm Cn (chưa gán): cần bảng Unicode, để T8.
pub fn is_hidden(c: char) -> bool {
    matches!(
        c,
        '\u{0000}'..='\u{0008}'
            | '\u{000B}'..='\u{001F}'
            | '\u{007F}'..='\u{009F}'
            // Default_Ignorable_Code_Point (DerivedCoreProperties)
            | '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'..='\u{1160}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF0}'..='\u{FFF8}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
            // Cf khác (Arabic/Syriac format, interlinear annotation, Egyptian hieroglyph format, Kaithi/Brahmi)
            | '\u{0600}'..='\u{0605}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            // Zl, Zp
            | '\u{2028}'..='\u{2029}'
            // Co
            | '\u{E000}'..='\u{F8FF}'
            | '\u{F0000}'..='\u{FFFFD}'
            | '\u{100000}'..='\u{10FFFD}'
            // Zs ngoài U+0020, và braille blank
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{2800}'
    )
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
    fn bash_options_shown_separately_from_command() {
        // Audit T5b finding 2: trường đổi cách chạy lệnh phải hiện trên thẻ, không chỉ `command`.
        let input = json!({
            "command": "ls", "description": "liệt kê",
            "dangerouslyDisableSandbox": true, "run_in_background": true, "timeout": 600000,
            "x\u{200B}": "a\u{202E}b",
        });
        let v = approval_view(&pending("Bash", input.clone()), "K");
        assert_eq!(v.command_normalized, "ls");
        assert_eq!(
            v.tool_options,
            BTreeMap::from([
                ("dangerouslyDisableSandbox".to_owned(), "true".to_owned()),
                ("run_in_background".to_owned(), "true".to_owned()),
                ("timeout".to_owned(), "600000".to_owned()),
                ("x⟨U+200B⟩".to_owned(), "\"a⟨U+202E⟩b\"".to_owned()),
            ])
        );
        assert_eq!(v.suspicions, vec![Suspicion::HiddenChars]);
        let h = handoff_view(
            "hof_1",
            &pending("Bash", input),
            "K",
            "ls",
            datetime!(2026-10-22 09:00:00 UTC),
        );
        assert_eq!(h.tool_options.len(), 4);
        // Chỉ `command` + `description`: không có tùy chọn; tool khác: rỗng (JSON đầy đủ đã hiện).
        assert!(approval_view(
            &pending("Bash", json!({ "command": "ls", "description": "d" })),
            "K"
        )
        .tool_options
        .is_empty());
        assert!(tool_options("mcp__x", &json!({ "a": 1 })).is_empty());
        assert!(tool_options("Bash", &json!({ "command": 5, "timeout": 1 })).is_empty());
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
    fn multi_edit_truncates_each_edit_keeps_structure() {
        // 100 edit ngắn: không cắt gì, mọi edit đều hiện (audit T5a finding 4).
        let edits: Vec<Value> = (0..100)
            .map(|i| json!({ "old_string": format!("old {i} {}", "z".repeat(50)), "new_string": "n" }))
            .collect();
        let (text, truncated) =
            command_text("MultiEdit", &json!({ "file_path": "/a", "edits": edits }));
        assert!(!truncated);
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["edits"].as_array().unwrap().len(), 100);

        // Một edit dài: chỉ trường đó bị cắt; mảng, edit khác và `replace_all` giữ nguyên.
        let input = json!({ "file_path": "/a", "edits": [
            { "old_string": "a", "new_string": "b" },
            { "old_string": "x".repeat(3000), "new_string": "y", "replace_all": true },
        ]});
        let (text, truncated) = command_text("MultiEdit", &input);
        assert!(truncated);
        let back: Value = serde_json::from_str(&text).unwrap();
        let edits = back["edits"].as_array().unwrap();
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0]["old_string"], json!("a"));
        assert_eq!(edits[1]["replace_all"], json!(true));
        assert!(edits[1]["old_string"]
            .as_str()
            .unwrap()
            .ends_with("… [đã cắt, tổng 3000 ký tự]"));
    }

    #[test]
    fn only_edit_tools_are_truncated() {
        // Tool MCP có trường `content`: đó là nội dung chính, phải hiện đầy đủ.
        let long = "m".repeat(5000);
        let (text, truncated) = command_text(
            "mcp__github__create_or_update_file",
            &json!({ "path": "README.md", "content": long }),
        );
        assert!(!truncated);
        assert!(text.contains(&long));
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
    fn every_invisible_group_is_escaped() {
        // Mỗi nhóm audit T5a finding 1 liệt kê, cộng các nhóm đã có từ trước.
        for c in [
            '\u{0001}',
            '\u{007F}',
            '\u{0085}', // Cc (C0, DEL, C1)
            '\u{00AD}',
            '\u{034F}',
            '\u{061C}',
            '\u{115F}',
            '\u{1160}',
            '\u{17B4}',
            '\u{180B}',
            '\u{180E}',
            '\u{200B}',
            '\u{200D}',
            '\u{200F}',
            '\u{202A}',
            '\u{202E}',
            '\u{2060}',
            '\u{2066}',
            '\u{2069}',
            '\u{3164}',
            '\u{FE00}',
            '\u{FE0F}',
            '\u{FEFF}',
            '\u{FFA0}',
            '\u{1BCA0}',
            '\u{1D173}',
            '\u{E0001}',
            '\u{E0041}',
            '\u{E007F}',
            '\u{E0100}',
            '\u{E01EF}', // Default_Ignorable
            '\u{0600}',
            '\u{06DD}',
            '\u{FFF9}',
            '\u{13430}', // Cf khác
            '\u{2028}',
            '\u{2029}', // Zl, Zp
            '\u{E000}',
            '\u{F0000}', // Co
            '\u{00A0}',
            '\u{1680}',
            '\u{2000}',
            '\u{200A}',
            '\u{202F}',
            '\u{205F}',
            '\u{3000}',
            '\u{2800}', // giả khoảng trắng
        ] {
            assert!(is_hidden(c), "U+{:04X}", u32::from(c));
            let (s, found) = escape_hidden(&format!("a{c}b"));
            assert!(found);
            assert_eq!(s, format!("a⟨U+{:04X}⟩b", u32::from(c)));
        }
        for c in [' ', '\n', '\t', 'a', 'ế', '🦀', '\u{0301}', '中'] {
            assert!(!is_hidden(c), "U+{:04X}", u32::from(c));
        }
    }

    #[test]
    fn hidden_runs_are_compressed() {
        let smuggled: String = "lệnh"
            .chars()
            .chain(std::iter::repeat_n('\u{E0041}', 500))
            .collect();
        let (s, found) = escape_hidden(&smuggled);
        assert!(found);
        assert_eq!(s, "lệnh⟨U+E0041×500⟩");
        let (s, _) = escape_hidden("a\u{200B}\u{200B}\u{200C}b");
        assert_eq!(s, "a⟨U+200B×2⟩⟨U+200C⟩b");
    }

    #[test]
    fn hidden_chars_in_json_tools_are_flagged() {
        // Serde đổi ký tự điều khiển thành `\u001b` (ASCII) trong bản hiển thị, nhưng vẫn phải gắn cờ (finding 7).
        let v = approval_view(
            &pending(
                "Write",
                json!({ "file_path": "/a", "content": "x\u{1b}[2Jy" }),
            ),
            "K",
        );
        assert!(!v.command_normalized.contains('\u{1b}'));
        assert_eq!(v.suspicions, vec![Suspicion::HiddenChars]);
        // Ký tự ẩn trong khóa JSON cũng tính.
        let v = approval_view(&pending("mcp__x", json!({ "pa\u{200B}th": "/a" })), "K");
        assert_eq!(v.suspicions, vec![Suspicion::HiddenChars]);
        let v = approval_view(&pending("Read", json!({ "file_path": "/a" })), "K");
        assert!(v.suspicions.is_empty());
    }

    #[test]
    fn too_long_threshold_boundaries() {
        let at_bytes = "a".repeat(4096);
        assert!(!is_too_long(&at_bytes, 4096, 50));
        assert!(is_too_long(&format!("{at_bytes}a"), 4096, 50));
        // Byte UTF-8, không phải ký tự: 2.048 ký tự "é" = 4.096 byte (đạt), thêm một là vượt.
        let accents = "é".repeat(2048);
        assert_eq!(accents.len(), 4096);
        assert!(!is_too_long(&accents, 4096, 50));
        assert!(is_too_long(&format!("{accents}é"), 4096, 50));
        let fifty = vec!["x"; 50].join("\n");
        assert!(!is_too_long(&fifty, 4096, 50));
        assert!(
            !is_too_long(&format!("{fifty}\n"), 4096, 50),
            "xuống dòng cuối không thêm dòng"
        );
        assert!(is_too_long(&format!("{fifty}\nx"), 4096, 50));
    }

    #[test]
    fn untruncated_text_excludes_edit_tools() {
        assert_eq!(
            untruncated_text("Bash", &json!({ "command": "ls" })).as_deref(),
            Some("ls")
        );
        assert!(untruncated_text("mcp__x", &json!({ "a": 1 })).is_some());
        for t in ["Edit", "Write", "MultiEdit", "NotebookEdit"] {
            assert_eq!(untruncated_text(t, &json!({ "content": "x" })), None, "{t}");
        }
    }

    #[test]
    fn handoff_preview_and_flags_on_full_text() {
        // Ký tự ẩn nằm giữa, ngoài phần xem trước: vẫn phải gắn cờ.
        let cmd = format!("{}\u{202E}{}", "a".repeat(5000), "b".repeat(5000));
        let p = pending("Bash", json!({ "command": cmd }));
        let h = handoff_view(
            "hof_1",
            &p,
            "Khoa",
            &cmd,
            datetime!(2026-10-22 09:00:00 UTC),
        );
        assert_eq!(h.preview_head, "a".repeat(PREVIEW_CHARS));
        assert_eq!(h.preview_tail, "b".repeat(PREVIEW_CHARS));
        assert_eq!(h.suspicions, vec![Suspicion::HiddenChars]);
        assert_eq!(h.bytes, cmd.len());
        assert_eq!(h.lines, 1);
        assert!(h.message.contains("terminal của Khoa"));

        // Lệnh ngắn hơn 2 phần xem trước (vượt ngưỡng do số dòng): hiện hết ở `preview_head`, ký tự ẩn đã escape.
        let short = format!("{}\u{200B}", "l\n".repeat(60));
        let h = handoff_view("hof_2", &p, "", &short, datetime!(2026-10-22 09:00:00 UTC));
        assert!(h.preview_head.ends_with("⟨U+200B⟩"));
        assert_eq!(h.preview_tail, "");
        assert_eq!(h.lines, 61);
        assert!(h.message.contains("terminal của agent"));
    }

    #[test]
    fn countdown_in_millis() {
        let mut p = pending("Bash", json!({ "command": "ls" }));
        p.countdown = Some(Duration::from_secs(5));
        assert_eq!(approval_view(&p, "K").countdown_ms, Some(5000));
    }
}
