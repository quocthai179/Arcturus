//! `replay scrub`: xóa thông tin nhạy cảm khỏi JSON hook đã ghi băng trước khi đưa vào `fixtures/hooks/`.
//!
//! - Tên người dùng trong đường dẫn home (`…\Users\<tên>`, `/c/Users/<tên>`, `/home/<tên>`, slug `C--Users-<tên>`,
//!   kể cả `\\` nhân đôi, gạch chéo lẫn lộn, mọi kiểu hoa thường) → `user`. Phần còn lại của đường dẫn giữ nguyên.
//! - Giá trị bí mật biết trước (token hub, tên máy) → `<redacted>`, không phân biệt hoa thường; `<máy>\<tên>` → `<redacted>\user`.
//! - Chuỗi trông như khóa (`sk-ant-…`, `ghp_…`, `AKIA…`, JWT, chuỗi hex ≥ 40 ký tự, khóa PEM) → `<redacted>`.
//! - Giá trị chuỗi của khóa có tên nhạy cảm (`token`, `secret`, `password`, `cookie`, `api_key`…) → `<redacted>`.
//! - Áp cho cả khóa lẫn giá trị của object JSON.

use serde_json::{Map, Value};

pub const REDACTED: &str = "<redacted>";

/// Tên thay cho tên người dùng thật.
const USER_PLACEHOLDER: &str = "user";

/// Bí mật ngắn hơn mức này không được thay theo giá trị (tránh xóa nhầm chuỗi phổ biến).
const MIN_SECRET_LEN: usize = 8;

/// Chuỗi hex liền nhau dài từ mức này coi là bí mật (token hub là 48 hex; agent_id là 17 hex).
const MIN_HEX_SECRET_LEN: usize = 40;

const KEY_PREFIXES: [&str; 18] = [
    "sk-ant-",
    "sk-proj-",
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "AKIA",
    "ASIA",
    "AIza",
    "npm_",
    "pypi-",
    "eyJ",
];

const SENSITIVE_KEYS: [&str; 13] = [
    "token",
    "secret",
    "password",
    "passwd",
    "authorization",
    "api_key",
    "apikey",
    "api-key",
    "credential",
    "cookie",
    "private_key",
    "access_key",
    "session_key",
];

pub struct Scrubber {
    /// Tên người dùng cần thay trong đường dẫn home (so khớp ASCII không phân biệt hoa thường).
    usernames: Vec<String>,
    secrets: Vec<String>,
}

impl Scrubber {
    /// `home`: thư mục home (tên người dùng lấy từ thành phần cuối). `usernames`: thêm tên khác (ví dụ `USERNAME`).
    pub fn new(
        home: Option<&str>,
        usernames: impl IntoIterator<Item = String>,
        secrets: impl IntoIterator<Item = String>,
    ) -> Self {
        let mut names: Vec<String> = home
            .and_then(|h| h.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next())
            .map(str::to_owned)
            .into_iter()
            .chain(usernames)
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case(USER_PLACEHOLDER))
            .collect();
        names.sort();
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        let secrets = secrets
            .into_iter()
            .map(|s| s.trim().to_owned())
            .filter(|s| s.chars().count() >= MIN_SECRET_LEN)
            .collect();
        Scrubber {
            usernames: names,
            secrets,
        }
    }

    pub fn scrub_value(&self, value: &mut Value) {
        match value {
            Value::String(s) => *s = self.scrub_str(s),
            Value::Array(items) => items.iter_mut().for_each(|v| self.scrub_value(v)),
            Value::Object(map) => {
                let old = std::mem::take(map);
                let mut new = Map::with_capacity(old.len());
                for (key, mut v) in old {
                    if is_sensitive_key(&key) && v.is_string() {
                        v = Value::String(REDACTED.to_owned());
                    } else {
                        self.scrub_value(&mut v);
                    }
                    new.insert(self.scrub_str(&key), v);
                }
                *map = new;
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    pub fn scrub_str(&self, s: &str) -> String {
        if s.contains("-----BEGIN") && s.contains("PRIVATE KEY") {
            return REDACTED.to_owned();
        }
        let mut out = s.to_owned();
        for secret in &self.secrets {
            out = replace_ignore_ascii_case(&out, secret, REDACTED);
        }
        for name in &self.usernames {
            // `<máy>\<tên>` (đầu ra của whoami) sau khi tên máy đã thành <redacted>.
            out = replace_ignore_ascii_case(
                &out,
                &format!("{REDACTED}\\{name}"),
                &format!("{REDACTED}\\{USER_PLACEHOLDER}"),
            );
            out = replace_home_user(&out, name);
        }
        redact_hex_runs(&redact_key_like(&out))
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|s| k.contains(s))
}

fn is_sep(b: u8) -> bool {
    matches!(b, b'\\' | b'/' | b'-')
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Thay `<sep>users|home<sep…><name>` (không phân biệt hoa thường, `<name>` có ranh giới từ) bằng `…user`.
fn replace_home_user(s: &str, name: &str) -> String {
    // So sánh trên byte: i có thể rơi giữa một ký tự nhiều byte (tiếng Việt), không cắt &str ở đó.
    let bytes = s.as_bytes();
    let lower = s.to_ascii_lowercase().into_bytes();
    let lname = name.to_ascii_lowercase().into_bytes();
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    let mut i = 0;
    while i < bytes.len() {
        let keyword_len = [&b"users"[..], &b"home"[..]]
            .iter()
            .find(|k| lower[i..].starts_with(k))
            .map(|k| k.len());
        let preceded_by_sep = i > 0 && is_sep(bytes[i - 1]);
        if let (Some(klen), true) = (keyword_len, preceded_by_sep) {
            let mut j = i + klen;
            let sep_start = j;
            while j < bytes.len() && is_sep(bytes[j]) {
                j += 1;
            }
            let end = j + lname.len();
            if j > sep_start
                && lower[j..].starts_with(&lname)
                && (end == bytes.len() || !is_word(bytes[end]))
            {
                out.push_str(&s[last..j]);
                out.push_str(USER_PLACEHOLDER);
                last = end;
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&s[last..]);
    out
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
        out.push_str(&haystack[last..idx]);
        out.push_str(replacement);
        last = idx + pattern.len();
    }
    out.push_str(&haystack[last..]);
    out
}

/// Thay các chuỗi bắt đầu bằng tiền tố khóa phổ biến (và phần đuôi `[A-Za-z0-9_.-]` ≥ 10 ký tự) bằng [`REDACTED`].
fn redact_key_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    'outer: while !rest.is_empty() {
        for prefix in KEY_PREFIXES {
            if let Some(tail) = rest.strip_prefix(prefix) {
                let body_len = tail
                    .bytes()
                    .take_while(|b| is_word(*b) || *b == b'-' || *b == b'.')
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

/// Thay các đoạn hex liền nhau dài ≥ [`MIN_HEX_SECRET_LEN`] (có ranh giới từ) bằng [`REDACTED`].
fn redact_hex_runs(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_hexdigit() && (i == 0 || !is_word(b[i - 1])) {
            let mut j = i;
            while j < b.len() && b[j].is_ascii_hexdigit() {
                j += 1;
            }
            if j - i >= MIN_HEX_SECRET_LEN && (j == b.len() || !is_word(b[j])) {
                out.push_str(&s[last..i]);
                out.push_str(REDACTED);
                last = j;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out.push_str(&s[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scrubbed(home: Option<&str>, secrets: &[&str], mut v: Value) -> Value {
        Scrubber::new(home, Vec::new(), secrets.iter().map(|s| s.to_string())).scrub_value(&mut v);
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
            "c:/users/user/.claude/projects/p/abc.jsonl"
        );
        assert_eq!(v["tool_input"]["command"], "cat C:\\Users\\user\\notes.txt");
    }

    #[test]
    fn other_home_path_forms_replaced() {
        let v = scrubbed(
            Some("C:\\Users\\PC"),
            &[],
            json!({
                "git_bash": "/c/Users/PC/project",
                "doubled": "C:\\\\Users\\\\PC\\\\x.json",
                "mixed": "C:\\Users/PC/x",
                "end": "C:\\Users\\PC"
            }),
        );
        assert_eq!(v["git_bash"], "/c/Users/user/project");
        assert_eq!(v["doubled"], "C:\\\\Users\\\\user\\\\x.json");
        assert_eq!(v["mixed"], "C:\\Users/user/x");
        assert_eq!(v["end"], "C:\\Users\\user");
    }

    #[test]
    fn username_needs_word_boundary() {
        let v = scrubbed(
            Some("C:\\Users\\PC"),
            &[],
            json!({ "p": "C:\\Users\\PCAdmin\\x" }),
        );
        assert_eq!(v["p"], "C:\\Users\\PCAdmin\\x");
    }

    #[test]
    fn home_slug_in_project_dir_replaced() {
        let v = scrubbed(
            Some("C:\\Users\\PC"),
            &[],
            json!({ "transcript_path": "C:\\Users\\PC\\.claude\\projects\\C--Users-PC-AppData-x\\s.jsonl" }),
        );
        assert_eq!(
            v["transcript_path"],
            "C:\\Users\\user\\.claude\\projects\\C--Users-user-AppData-x\\s.jsonl"
        );
        let v = scrubbed(
            Some("/home/thai"),
            &[],
            json!({ "p": "/home/thai/.claude/projects/-home-thai-x/s.jsonl" }),
        );
        assert_eq!(v["p"], "/home/user/.claude/projects/-home-user-x/s.jsonl");
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
            json!({ "cwd": "D:\\code\\Arcturus", "t": "PC game", "vi": "đường dẫn C:\\Users\\PC\\tệp" }),
        );
        assert_eq!(v["cwd"], "D:\\code\\Arcturus");
        assert_eq!(v["t"], "PC game");
        assert_eq!(v["vi"], "đường dẫn C:\\Users\\user\\tệp");
    }

    #[test]
    fn object_keys_scrubbed() {
        let v = scrubbed(
            Some("C:\\Users\\PC"),
            &[],
            json!({ "C:\\Users\\PC\\a.txt": 1 }),
        );
        assert_eq!(v["C:\\Users\\user\\a.txt"], 1);
        assert!(v.get("C:\\Users\\PC\\a.txt").is_none());
    }

    #[test]
    fn known_secret_value_redacted_everywhere_any_case() {
        let tok = "abcdefXY0123456789";
        let v = scrubbed(
            None,
            &[tok],
            json!({ "c": format!("Bearer {tok} và {}", tok.to_ascii_lowercase()) }),
        );
        assert_eq!(v["c"], "Bearer <redacted> và <redacted>");
    }

    #[test]
    fn whoami_hostname_and_user_redacted() {
        let mut v = json!({ "out": "desktop-48dlrpq\\pc\n" });
        Scrubber::new(
            Some("C:\\Users\\PC"),
            vec!["PC".to_owned()],
            vec!["DESKTOP-48DLRPQ".to_owned()],
        )
        .scrub_value(&mut v);
        assert_eq!(v["out"], "<redacted>\\user\n");
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
            json!({ "api_key": "k", "Authorization": "Bearer z", "X-Api-Key": "q", "cookie": "c",
                    "total_tokens": 42, "nested": [{ "password": "p" }] }),
        );
        assert_eq!(v["api_key"], REDACTED);
        assert_eq!(v["Authorization"], REDACTED);
        assert_eq!(v["X-Api-Key"], REDACTED);
        assert_eq!(v["cookie"], REDACTED);
        assert_eq!(v["total_tokens"], 42);
        assert_eq!(v["nested"][0]["password"], REDACTED);
    }

    #[test]
    fn key_like_strings_redacted() {
        let v = scrubbed(
            None,
            &[],
            json!({ "c": "export K=sk-ant-api03-AbCdEf0123456789_xyz; echo ghp_ABCDEFGHIJ0123 ok glpat-abcdefghij12" }),
        );
        assert_eq!(v["c"], "export K=<redacted>; echo <redacted> ok <redacted>");
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig_abc-123";
        let v = scrubbed(None, &[], json!({ "c": format!("token {jwt} end") }));
        assert_eq!(v["c"], "token <redacted> end");
    }

    #[test]
    fn long_hex_redacted_short_hex_kept() {
        let tok = "0123456789abcdef0123456789abcdef0123456789abcdef";
        let v = scrubbed(
            None,
            &[],
            json!({ "c": format!("x={tok}"), "agent_id": "ad7e4b71226dc3e83" }),
        );
        assert_eq!(v["c"], "x=<redacted>");
        assert_eq!(v["agent_id"], "ad7e4b71226dc3e83");
    }

    #[test]
    fn pem_private_key_redacted() {
        let v = scrubbed(
            None,
            &[],
            json!({ "c": "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----" }),
        );
        assert_eq!(v["c"], REDACTED);
    }

    #[test]
    fn short_prefix_match_kept() {
        // "AKIA" theo sau ít hơn 10 ký tự: không phải khóa AWS.
        let v = scrubbed(None, &[], json!({ "c": "AKIAxy và tiếng Việt" }));
        assert_eq!(v["c"], "AKIAxy và tiếng Việt");
    }
}
