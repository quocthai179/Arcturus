//! Persona cho nhân vật (`docs/PLAN.md` §3.7): khớp `match_cwd` trong `arcturus.toml`, không khớp thì đặt tên sao
//! chòm Boötes và chọn màu theo hash ổn định.

use globset::{Glob, GlobBuilder, GlobMatcher};
use serde::Deserialize;

use crate::protocol::Palette;

/// Tên mặc định cho agent không khớp persona, lấy lần lượt tên chưa dùng.
pub const STAR_NAMES: [&str; 10] = [
    "Muphrid",
    "Seginus",
    "Nekkar",
    "Xuange",
    "Genghe",
    "Merga",
    "Qigong",
    "Ceginus",
    "Alkalurops",
    "Alazal",
];

/// 6 bảng màu cố định (hair, outfit, acc) cho agent không khớp persona.
pub const DEFAULT_PALETTES: [(&str, &str, &str); 6] = [
    ("#1F1F24", "#4C8DF6", "#9AA3B5"),
    ("#5A3A22", "#2BB5A0", "#FFD166"),
    ("#2E2A5C", "#8B7CF6", "#FF7A59"),
    ("#244A3A", "#F2A65A", "#8B7CF6"),
    ("#3B2F2F", "#E5566D", "#7FD1B9"),
    ("#2A3B4C", "#F7C548", "#5B8DEF"),
];

/// Vai trò của agent phiên không khớp persona, và của subagent không có `agent_type`.
pub const DEFAULT_ROLE: &str = "thành viên";
pub const DEFAULT_SUBAGENT_ROLE: &str = "trợ lý";

/// Một mục `[[persona]]` trong `arcturus.toml`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PersonaSpec {
    pub match_cwd: String,
    pub name: String,
    pub role: String,
    pub palette: Palette,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("persona[{index}].match_cwd {pattern:?} không phải glob hợp lệ: {message}")]
pub struct PersonaError {
    pub index: usize,
    pub pattern: String,
    pub message: String,
}

/// Danh sách persona đã biên dịch glob. Chỉ đọc: tên đã gán cho từng phiên do `TeamState` giữ.
#[derive(Clone, Debug, Default)]
pub struct PersonaBook {
    entries: Vec<(GlobMatcher, PersonaSpec)>,
}

impl PersonaBook {
    /// Trên Windows so không phân biệt hoa thường (§3.7).
    pub fn new(specs: &[PersonaSpec]) -> Result<Self, PersonaError> {
        Self::with_case(specs, cfg!(windows))
    }

    pub fn with_case(specs: &[PersonaSpec], case_insensitive: bool) -> Result<Self, PersonaError> {
        let entries = specs
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let pattern = normalize_path(&spec.match_cwd);
                GlobBuilder::new(&pattern)
                    .case_insensitive(case_insensitive)
                    .literal_separator(true)
                    .build()
                    .map(|g: Glob| (g.compile_matcher(), spec.clone()))
                    .map_err(|e| PersonaError {
                        index,
                        pattern: spec.match_cwd.clone(),
                        message: e.kind().to_string(),
                    })
            })
            .collect::<Result<_, _>>()?;
        Ok(PersonaBook { entries })
    }

    /// Persona đầu tiên (theo thứ tự trong file) khớp `cwd` hoặc một thư mục cha của nó.
    pub fn find(&self, cwd: &str) -> Option<&PersonaSpec> {
        let path = normalize_path(cwd);
        let candidates: Vec<&str> = ancestors(&path).collect();
        self.entries
            .iter()
            .find(|(m, _)| candidates.iter().any(|c| m.is_match(c)))
            .map(|(_, spec)| spec)
    }
}

/// Đổi `\` thành `/`, bỏ `/` thừa ở cuối.
pub fn normalize_path(p: &str) -> String {
    let s = p.replace('\\', "/");
    let trimmed = s.trim_end_matches('/');
    if trimmed.is_empty() {
        s
    } else {
        trimmed.to_owned()
    }
}

/// `a/b/c` → `a/b/c`, `a/b`, `a`.
fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    let mut next = Some(path);
    std::iter::from_fn(move || {
        let cur = next?;
        next = cur.rfind('/').map(|i| &cur[..i]).filter(|p| !p.is_empty());
        Some(cur)
    })
}

/// FNV-1a 64 bit: ổn định giữa các lần chạy và các máy (không như `DefaultHasher`).
pub fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Bảng màu mặc định chọn theo hash của `key` (session_id, hoặc `session_id/agent_id` cho subagent).
pub fn default_palette(key: &str) -> Palette {
    let len = DEFAULT_PALETTES.len() as u64;
    let idx = usize::try_from(fnv1a(key) % len).unwrap_or(0);
    let (hair, outfit, acc) = DEFAULT_PALETTES[idx];
    Palette {
        hair: hair.to_owned(),
        outfit: outfit.to_owned(),
        acc: acc.to_owned(),
    }
}

/// Tên sao đầu tiên chưa có trong `used`; hết thì `"Agent <4 ký tự đầu session_id>"`.
pub fn next_star_name(used: impl Fn(&str) -> bool, session_id: &str) -> String {
    STAR_NAMES
        .iter()
        .find(|n| !used(n))
        .map(|n| (*n).to_owned())
        .unwrap_or_else(|| format!("Agent {}", session_id.chars().take(4).collect::<String>()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(glob: &str, name: &str) -> PersonaSpec {
        PersonaSpec {
            match_cwd: glob.to_owned(),
            name: name.to_owned(),
            role: "backend".to_owned(),
            palette: Palette {
                hair: "#000000".to_owned(),
                outfit: "#111111".to_owned(),
                acc: "#222222".to_owned(),
            },
        }
    }

    fn book(specs: &[PersonaSpec], ci: bool) -> PersonaBook {
        PersonaBook::with_case(specs, ci).unwrap()
    }

    #[test]
    fn persona_matches_by_cwd_glob() {
        let b = book(
            &[
                spec("**/.claude/worktrees/hub*", "Khoa"),
                spec("**/.claude/worktrees/ui*", "Linh"),
            ],
            false,
        );
        assert_eq!(
            b.find("/home/me/arcturus/.claude/worktrees/hub")
                .map(|p| p.name.as_str()),
            Some("Khoa")
        );
        assert_eq!(
            b.find("/home/me/arcturus/.claude/worktrees/ui-2")
                .map(|p| p.name.as_str()),
            Some("Linh")
        );
        assert_eq!(b.find("/home/me/arcturus"), None);
    }

    #[test]
    fn persona_matches_windows_backslash_path() {
        let specs = [spec("**/.claude/worktrees/hub*", "Khoa")];
        let ci = book(&specs, true);
        assert_eq!(
            ci.find(r"D:\code\Arcturus\.claude\worktrees\hub")
                .map(|p| p.name.as_str()),
            Some("Khoa")
        );
        // Windows: không phân biệt hoa thường.
        assert!(ci.find(r"D:\Code\ARCTURUS\.Claude\Worktrees\HUB").is_some());
        assert!(book(&specs, false)
            .find(r"D:\Code\ARCTURUS\.Claude\Worktrees\HUB")
            .is_none());
        // Mẫu viết bằng `\` cũng được chuẩn hóa.
        let back = book(&[spec(r"**\.claude\worktrees\hub*", "Khoa")], true);
        assert!(back
            .find(r"D:\code\Arcturus\.claude\worktrees\hub")
            .is_some());
    }

    #[test]
    fn persona_matches_subdirectory_of_worktree() {
        let b = book(&[spec("**/.claude/worktrees/hub*", "Khoa")], true);
        assert_eq!(
            b.find(r"D:\code\Arcturus\.claude\worktrees\hub\crates\hub\src")
                .map(|p| p.name.as_str()),
            Some("Khoa")
        );
        // `*` không vượt qua `/`: "hub-x/y" khớp nhờ thư mục cha "hub-x", không phải nhờ `*` ăn cả `/y`.
        assert!(b.find("/r/.claude/worktrees/hub-x/y").is_some());
        assert!(b.find("/r/.claude/worktrees/policy/hub").is_none());
    }

    #[test]
    fn first_matching_persona_wins() {
        let b = book(
            &[
                spec("**/worktrees/*", "Đầu"),
                spec("**/worktrees/hub*", "Sau"),
            ],
            false,
        );
        assert_eq!(
            b.find("/r/worktrees/hub").map(|p| p.name.as_str()),
            Some("Đầu")
        );
    }

    #[test]
    fn invalid_glob_reports_index() {
        let err =
            PersonaBook::with_case(&[spec("ok/*", "A"), spec("a/[b", "B")], false).unwrap_err();
        assert_eq!(err.index, 1);
        assert_eq!(err.pattern, "a/[b");
    }

    #[test]
    fn palette_hash_is_deterministic() {
        // Giá trị chuẩn của FNV-1a 64 bit.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        let p = default_palette("cd99a479-bb9f-4c58-8793-d22374df884e");
        assert_eq!(p, default_palette("cd99a479-bb9f-4c58-8793-d22374df884e"));
        let distinct: std::collections::BTreeSet<_> = (0..50)
            .map(|i| default_palette(&format!("s{i}")).outfit)
            .collect();
        assert!(distinct.len() > 1, "hash phải trải ra nhiều bảng màu");
    }

    #[test]
    fn star_names_in_order_then_fallback() {
        assert_eq!(next_star_name(|_| false, "abcdef"), "Muphrid");
        assert_eq!(next_star_name(|n| n == "Muphrid", "abcdef"), "Seginus");
        assert_eq!(next_star_name(|_| true, "abcdef"), "Agent abcd");
    }

    #[test]
    fn normalize_path_cases() {
        assert_eq!(normalize_path(r"D:\a\b\"), "D:/a/b");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(
            ancestors("D:/a/b").collect::<Vec<_>>(),
            ["D:/a/b", "D:/a", "D:"]
        );
        assert_eq!(ancestors("/a/b").collect::<Vec<_>>(), ["/a/b", "/a"]);
    }
}
