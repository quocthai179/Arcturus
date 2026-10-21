//! `replay scrub`: xóa thông tin nhạy cảm khỏi JSON hook đã ghi băng trước khi đưa vào `fixtures/hooks/`.
//!
//! - Thư mục home → `C:\Users\user` / `C:/Users/user` (giữ dạng đường dẫn Windows để test persona trên Windows)
//!   hoặc `/home/user`.
//! - Giá trị bí mật biết trước (ví dụ `ARCTURUS_TOKEN`) → `<redacted>`.
//! - Chuỗi trông như khóa API (`sk-ant-…`, `ghp_…`, `AKIA…`) → `<redacted>`.
//! - Giá trị chuỗi của khóa có tên nhạy cảm (`token`, `secret`, `password`, `authorization`, `api_key`) → `<redacted>`.

use serde_json::Value;

pub const REDACTED: &str = "<redacted>";

/// Bí mật ngắn hơn mức này không được thay theo giá trị (tránh xóa nhầm chuỗi phổ biến).
const MIN_SECRET_LEN: usize = 8;

const KEY_PREFIXES: [&str; 8] = [
    "sk-ant-",
    "sk-proj-",
    "ghp_",
    "gho_",
    "ghs_",
    "github_pat_",
    "xoxb-",
    "AKIA",
];

const SENSITIVE_KEYS: [&str; 6] = [
    "token",
    "secret",
    "password",
    "passwd",
    "authorization",
    "api_key",
];

pub struct Scrubber {
    /// (mẫu, thay bằng), so khớp không phân biệt hoa thường.
    homes: Vec<(String, &'static str)>,
    secrets: Vec<String>,
}

impl Scrubber {
    pub fn new(home: Option<&str>, secrets: impl IntoIterator<Item = String>) -> Self {
        let mut homes = Vec::new();
        if let Some(h) = home.map(|h| h.trim_end_matches(['/', '\\'])) {
            if !h.is_empty() {
                let back = h.replace('/', "\\");
                let fwd = h.replace('\\', "/");
                let windows = has_drive_letter(h);
                if windows {
                    homes.push((back, "C:\\Users\\user"));
                    homes.push((fwd, "C:/Users/user"));
                } else {
                    homes.push((fwd, "/home/user"));
                }
            }
        }
        let secrets = secrets
            .into_iter()
            .map(|s| s.trim().to_owned())
            .filter(|s| s.chars().count() >= MIN_SECRET_LEN)
            .collect();
        Scrubber { homes, secrets }
    }

    pub fn scrub_value(&self, value: &mut Value) {
        match value {
            Value::String(s) => *s = self.scrub_str(s),
            Value::Array(items) => items.iter_mut().for_each(|v| self.scrub_value(v)),
            Value::Object(map) => {
                for (key, v) in map.iter_mut() {
                    if is_sensitive_key(key) && v.is_string() {
                        *v = Value::String(REDACTED.to_owned());
                    } else {
                        self.scrub_value(v);
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    pub fn scrub_str(&self, s: &str) -> String {
        let mut out = s.to_owned();
        for secret in &self.secrets {
            out = out.replace(secret.as_str(), REDACTED);
        }
        for (pattern, replacement) in &self.homes {
            out = replace_ignore_ascii_case(&out, pattern, replacement);
        }
        redact_key_like(&out)
    }
}

fn has_drive_letter(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|s| k.contains(s))
}

/// Thay mọi lần xuất hiện của `pattern` (so khớp ASCII không phân biệt hoa thường) bằng `replacement`.
fn replace_ignore_ascii_case(haystack: &str, pattern: &str, replacement: &str) -> String {
    if pattern.is_empty() {
        return haystack.to_owned();
    }
    let lower_hay = haystack.to_ascii_lowercase();
    let lower_pat = pattern.to_ascii_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut last = 0;
    // to_ascii_lowercase giữ nguyên độ dài byte, nên chỉ số trong lower_hay dùng được cho haystack.
    for (idx, _) in lower_hay.match_indices(&lower_pat) {
        if idx < last {
            continue;
        }
        out.push_str(&haystack[last..idx]);
        out.push_str(replacement);
        last = idx + pattern.len();
    }
    out.push_str(&haystack[last..]);
    out
}

/// Thay các chuỗi bắt đầu bằng tiền tố khóa API phổ biến (và phần đuôi `[A-Za-z0-9_-]`) bằng [`REDACTED`].
fn redact_key_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    'outer: while !rest.is_empty() {
        for prefix in KEY_PREFIXES {
            if let Some(tail) = rest.strip_prefix(prefix) {
                let body_len = tail
                    .bytes()
                    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
                    .count();
                if body_len >= 10 {
                    out.push_str(REDACTED);
                    rest = &tail[body_len..];
                    continue 'outer;
                }
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scrubbed(home: Option<&str>, secrets: &[&str], mut v: Value) -> Value {
        Scrubber::new(home, secrets.iter().map(|s| s.to_string())).scrub_value(&mut v);
        v
    }

    #[test]
    fn windows_home_replaced_in_both_slash_styles_and_any_case() {
        let v = scrubbed(
            Some("C:\\Users\\PC"),
            &[],
            json!({
                "cwd": "C:\\Users\\PC\\code\\x",
                "transcript_path": "c:/users/pc/.claude/projects/p/abc.jsonl",
                "tool_input": { "command": "cat C:\\Users\\PC\\notes.txt" }
            }),
        );
        assert_eq!(v["cwd"], "C:\\Users\\user\\code\\x");
        assert_eq!(
            v["transcript_path"],
            "C:/Users/user/.claude/projects/p/abc.jsonl"
        );
        assert_eq!(v["tool_input"]["command"], "cat C:\\Users\\user\\notes.txt");
    }

    #[test]
    fn unix_home_replaced() {
        let v = scrubbed(
            Some("/home/thai/"),
            &[],
            json!({ "cwd": "/home/thai/arcturus" }),
        );
        assert_eq!(v["cwd"], "/home/user/arcturus");
    }

    #[test]
    fn paths_outside_home_untouched() {
        let v = scrubbed(
            Some("C:\\Users\\PC"),
            &[],
            json!({ "cwd": "D:\\code\\Arcturus" }),
        );
        assert_eq!(v["cwd"], "D:\\code\\Arcturus");
    }

    #[test]
    fn known_secret_value_redacted_everywhere() {
        let tok = "abcdef0123456789abcdef0123456789";
        let v = scrubbed(
            None,
            &[tok],
            json!({ "tool_input": { "command": format!("curl -H 'Authorization: Bearer {tok}' x") } }),
        );
        assert_eq!(
            v["tool_input"]["command"],
            "curl -H 'Authorization: Bearer <redacted>' x"
        );
    }

    #[test]
    fn short_secret_ignored() {
        let v = scrubbed(None, &["abc"], json!({ "x": "abc abc" }));
        assert_eq!(v["x"], "abc abc");
    }

    #[test]
    fn sensitive_keys_redacted_only_for_strings() {
        let v = scrubbed(
            None,
            &[],
            json!({ "api_key": "k", "Authorization": "Bearer z", "total_tokens": 42, "nested": [{ "password": "p" }] }),
        );
        assert_eq!(v["api_key"], REDACTED);
        assert_eq!(v["Authorization"], REDACTED);
        assert_eq!(v["total_tokens"], 42);
        assert_eq!(v["nested"][0]["password"], REDACTED);
    }

    #[test]
    fn key_like_strings_redacted() {
        let v = scrubbed(
            None,
            &[],
            json!({ "c": "export K=sk-ant-api03-AbCdEf0123456789_xyz; echo ghp_ABCDEFGHIJ0123 ok" }),
        );
        assert_eq!(v["c"], "export K=<redacted>; echo <redacted> ok");
    }

    #[test]
    fn short_prefix_match_kept() {
        // "AKIA" theo sau ít hơn 10 ký tự: không phải khóa AWS.
        let v = scrubbed(None, &[], json!({ "c": "AKIAxy và tiếng Việt" }));
        assert_eq!(v["c"], "AKIAxy và tiếng Việt");
    }
}
