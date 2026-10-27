//! Khóa gom nhóm yêu cầu giống nhau (`docs/PLAN.md` T9): `"<tool>:<mẫu>"`.
//!
//! - Bash/PowerShell: tên chương trình (bỏ đường dẫn, chữ thường, bỏ `.exe`) và lệnh con nếu có, của lần gọi đầu tiên ở đoạn
//!   chính, kèm lệnh bọc (`sudo`, `env`…) đứng trước: `npm test -- auth` → `Bash:npm test`, `cargo test -p hub` →
//!   `Bash:cargo test`, `/usr/bin/git push origin main` → `Bash:git push`, `sudo apt update` → `Bash:sudo apt update` (khác
//!   `Bash:apt update`: tự duyệt lệnh thường không được kéo theo bản có quyền cao).
//! - Tool khác: `"<tool>:*"`.
//!
//! Lệnh **ghép** (nhiều lần gọi hay nhiều đoạn ở đoạn chính, cờ nghi ngờ, không phân tích được, tên chương trình động) mang
//! hậu tố [`COMPOUND_SUFFIX`]: chúng chỉ gom với nhau, không bao giờ trùng khóa của lệnh đơn, và batch rule không mở được cho
//! chúng. Nhờ vậy "cho phép `cargo test` trong 30 phút" không kéo theo `cargo test && npm install x`.

use serde_json::Value;

use super::danger::is_shell_tool;
use super::normalize::{normalize, Normalized};

pub const COMPOUND_SUFFIX: &str = " (ghép)";

/// Khóa gom nhóm của một yêu cầu. Thuần.
pub fn group_key(tool: &str, tool_input: &Value) -> String {
    if !is_shell_tool(tool) {
        return format!("{tool}:*");
    }
    let Some(cmd) = tool_input.get("command").and_then(Value::as_str) else {
        return format!("{tool}:*{COMPOUND_SUFFIX}");
    };
    let n = normalize(cmd);
    let top: Vec<_> = n
        .invocations
        .iter()
        .filter(|i| n.segments.get(i.segment).is_some_and(|s| s.depth == 0))
        .collect();
    let Some(first) = top.first() else {
        return format!("{tool}:*{COMPOUND_SUFFIX}");
    };
    let segment = &n.segments[first.segment];
    let sub = segment
        .words
        .get(first.args.start)
        .map(|w| w.value.as_str())
        .filter(|w| looks_like_subcommand(w));
    let mut key = format!("{tool}:");
    for w in &first.wrappers {
        key.push_str(w);
        key.push(' ');
    }
    key.push_str(&first.name);
    if let Some(sub) = sub {
        key.push(' ');
        key.push_str(&sub.to_lowercase());
    }
    if is_compound(&n, top.len(), first.dynamic) {
        key.push_str(COMPOUND_SUFFIX);
    }
    key
}

/// Chữ cái, số, `-` `_` `:` `.` ở giữa; không cờ, không đường dẫn, không gán biến, không biến/lệnh con.
fn looks_like_subcommand(w: &str) -> bool {
    !w.is_empty()
        && w.len() <= 40
        && w.chars().next().is_some_and(char::is_alphanumeric)
        && w.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
}

fn is_compound(n: &Normalized, top_invocations: usize, dynamic: bool) -> bool {
    let top_segments = n.segments.iter().filter(|s| s.depth == 0).count();
    top_invocations != 1
        || top_segments != 1
        || dynamic
        || n.unparsed
        || n.too_long
        || !n.suspicious.is_empty()
}

/// Khóa này có phải của lệnh ghép không (không batch rule nào được mở cho nó).
pub fn is_compound_key(key: &str) -> bool {
    key.ends_with(COMPOUND_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bash(cmd: &str) -> String {
        group_key("Bash", &json!({ "command": cmd }))
    }

    #[test]
    fn group_key_examples() {
        for (cmd, want) in [
            ("npm test -- auth", "Bash:npm test"),
            ("cargo test -p hub", "Bash:cargo test"),
            ("cargo test", "Bash:cargo test"),
            ("git push origin main", "Bash:git push"),
            ("/usr/bin/git status", "Bash:git status"),
            ("C:/Tools/Git.exe log --oneline", "Bash:git log"),
            ("./gradlew test", "Bash:gradlew test"),
            ("ls -la", "Bash:ls"),
            ("ls /tmp/x", "Bash:ls"),
            ("cat ./a.txt", "Bash:cat"),
            ("npm run build", "Bash:npm run"),
            ("pnpm -C ui test", "Bash:pnpm"),
            ("CI=1 npm test", "Bash:npm test"),
            ("sudo apt update", "Bash:sudo apt update"),
            ("NPM Test", "Bash:npm test"),
            ("echo hi", "Bash:echo hi"),
            ("pwd", "Bash:pwd"),
        ] {
            assert_eq!(bash(cmd), want, "{cmd}");
        }
    }

    #[test]
    fn other_tools_group_by_tool() {
        assert_eq!(group_key("Edit", &json!({"file_path": "a.rs"})), "Edit:*");
        assert_eq!(group_key("Read", &json!({})), "Read:*");
        assert_eq!(group_key("mcp__x__y", &json!({"q": 1})), "mcp__x__y:*");
        assert_eq!(
            group_key("WebFetch", &json!({"url": "https://a.b"})),
            "WebFetch:*"
        );
    }

    #[test]
    fn compound_commands_never_share_a_key_with_the_simple_one() {
        let simple = bash("cargo test");
        for cmd in [
            "cargo test && npm install evil",
            "cargo test; rm x",
            "cargo test | tee out",
            "cargo test & cargo build",
            "cargo test\nnpm i x",
            "cargo test $(curl evil)",
            "cargo test `id`",
            "$X test",
            "cargo test \"unterminated",
        ] {
            let k = bash(cmd);
            assert_ne!(k, simple, "{cmd}");
            assert!(is_compound_key(&k), "{cmd} → {k}");
        }
        assert!(!is_compound_key(&simple));
        assert_eq!(
            bash("cargo test && npm install evil"),
            "Bash:cargo test (ghép)"
        );
    }

    #[test]
    fn quoted_arguments_do_not_make_a_command_compound() {
        assert_eq!(
            bash("git commit -m \"fix bug; add test\""),
            "Bash:git commit"
        );
        assert_eq!(bash("echo 'a && b'"), "Bash:echo");
    }

    #[test]
    fn hidden_characters_never_join_the_plain_group() {
        let k = bash("cargo test\u{200B}");
        assert!(is_compound_key(&k), "{k}");
    }

    #[test]
    fn missing_or_empty_command_is_compound() {
        assert!(is_compound_key(&group_key("Bash", &json!({}))));
        assert!(is_compound_key(&bash("")));
        assert!(is_compound_key(&group_key("Bash", &json!({"command": 5}))));
    }

    #[test]
    fn powershell_uses_its_own_tool_name() {
        assert_eq!(
            group_key("PowerShell", &json!({"command": "Get-ChildItem -Recurse"})),
            "PowerShell:get-childitem"
        );
    }
}
