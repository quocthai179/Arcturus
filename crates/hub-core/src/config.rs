//! Cấu hình `arcturus.toml` (`docs/PLAN.md` §3.7). Thiếu file: mặc định. File hỏng: lỗi kèm số dòng, hub không khởi
//! động. Khóa lạ cũng là lỗi (bắt lỗi gõ nhầm thay vì lặng lẽ dùng mặc định).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::persona::{PersonaBook, PersonaSpec};
use crate::protocol::Palette;

/// Hook `PermissionRequest` hết giờ ở 120 s; hub phải trả trước đó.
pub const MAX_APPROVAL_TIMEOUT_SECS: u64 = 115;

/// Ngưỡng "lệnh quá dài để duyệt trên thẻ" không vượt quá giới hạn body của hook (1 MB).
pub const MAX_HANDOFF_THRESHOLD: usize = 1024 * 1024;

#[derive(Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub hub: HubConfig,
    pub team: TeamConfig,
    pub trust: TrustConfig,
    #[serde(rename = "persona")]
    pub personas: Vec<PersonaSpec>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct HubConfig {
    pub bind: String,
    pub approval_timeout_secs: u64,
    pub reconcile_interval_secs: u64,
    pub event_ttl_days: u32,
    /// Lệnh dài hơn chừng này byte UTF-8 (hoặc nhiều dòng hơn `handoff_max_lines`) không duyệt trên thẻ mà trả về
    /// terminal (quy tắc bất biến 3, T5b).
    pub handoff_max_bytes: usize,
    pub handoff_max_lines: usize,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct TeamConfig {
    pub main_roster_max: u8,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct TrustConfig {
    pub streak_threshold: u32,
    pub streak_window_days: u32,
    pub grant_days: u32,
}

impl Default for HubConfig {
    fn default() -> Self {
        HubConfig {
            bind: "127.0.0.1:7878".to_owned(),
            approval_timeout_secs: 110,
            reconcile_interval_secs: 30,
            event_ttl_days: 30,
            handoff_max_bytes: 4096,
            handoff_max_lines: 50,
        }
    }
}

impl Default for TeamConfig {
    fn default() -> Self {
        TeamConfig { main_roster_max: 5 }
    }
}

impl Default for TrustConfig {
    fn default() -> Self {
        TrustConfig {
            streak_threshold: 20,
            streak_window_days: 7,
            grant_days: 30,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("không đọc được {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{}:{line}: {message}", path.display())]
    Parse {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error("{}: {field}: {message}", path.display())]
    Invalid {
        path: PathBuf,
        field: String,
        message: String,
    },
}

impl Config {
    /// Đọc `path`; không có file thì trả mặc định.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::from_toml(&text, path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(source) => Err(ConfigError::Io {
                path: path.to_owned(),
                source,
            }),
        }
    }

    /// `path` chỉ dùng trong thông báo lỗi.
    pub fn from_toml(text: &str, path: &Path) -> Result<Config, ConfigError> {
        let cfg: Config = toml::from_str(text).map_err(|e| ConfigError::Parse {
            path: path.to_owned(),
            line: e.span().map_or(1, |s| line_of(text, s.start)),
            message: e.message().trim().to_owned(),
        })?;
        cfg.validate(path)?;
        Ok(cfg)
    }

    pub fn persona_book(&self) -> Result<PersonaBook, crate::persona::PersonaError> {
        PersonaBook::new(&self.personas)
    }

    pub fn bind_addr(&self) -> Option<SocketAddr> {
        self.hub.bind.parse().ok()
    }

    fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        let invalid = |field: &str, message: String| ConfigError::Invalid {
            path: path.to_owned(),
            field: field.to_owned(),
            message,
        };
        match self.bind_addr() {
            None => {
                return Err(invalid(
                    "hub.bind",
                    format!("{:?} không phải địa chỉ ip:cổng", self.hub.bind),
                ))
            }
            Some(a) if !a.ip().is_loopback() => {
                return Err(invalid(
                    "hub.bind",
                    format!("{a}: v0 chỉ cho phép địa chỉ loopback"),
                ))
            }
            Some(_) => {}
        }
        if !(1..=MAX_APPROVAL_TIMEOUT_SECS).contains(&self.hub.approval_timeout_secs) {
            return Err(invalid(
                "hub.approval_timeout_secs",
                format!(
                    "{} ngoài khoảng 1–{MAX_APPROVAL_TIMEOUT_SECS} (phải nhỏ hơn timeout 120 s của hook)",
                    self.hub.approval_timeout_secs
                ),
            ));
        }
        for (field, value) in [
            ("hub.handoff_max_bytes", self.hub.handoff_max_bytes),
            ("hub.handoff_max_lines", self.hub.handoff_max_lines),
        ] {
            if !(1..=MAX_HANDOFF_THRESHOLD).contains(&value) {
                return Err(invalid(
                    field,
                    format!("{value} ngoài khoảng 1–{MAX_HANDOFF_THRESHOLD}"),
                ));
            }
        }
        for (field, value) in [
            (
                "hub.reconcile_interval_secs",
                self.hub.reconcile_interval_secs,
            ),
            ("hub.event_ttl_days", u64::from(self.hub.event_ttl_days)),
            ("team.main_roster_max", u64::from(self.team.main_roster_max)),
            (
                "trust.streak_threshold",
                u64::from(self.trust.streak_threshold),
            ),
            (
                "trust.streak_window_days",
                u64::from(self.trust.streak_window_days),
            ),
            ("trust.grant_days", u64::from(self.trust.grant_days)),
        ] {
            if value == 0 {
                return Err(invalid(field, "phải lớn hơn 0".to_owned()));
            }
        }
        for (i, p) in self.personas.iter().enumerate() {
            if p.name.trim().is_empty() {
                return Err(invalid(
                    &format!("persona[{i}].name"),
                    "không được rỗng".to_owned(),
                ));
            }
            check_palette(&p.palette).map_err(|(k, v)| {
                invalid(
                    &format!("persona[{i}].palette.{k}"),
                    format!("{v:?} không phải màu #RRGGBB"),
                )
            })?;
        }
        self.persona_book()
            .map_err(|e| invalid(&format!("persona[{}].match_cwd", e.index), e.message))?;
        Ok(())
    }
}

/// Màu được chèn vào SVG nhân vật: chỉ nhận `#RRGGBB`.
fn check_palette(p: &Palette) -> Result<(), (&'static str, String)> {
    for (k, v) in [("hair", &p.hair), ("outfit", &p.outfit), ("acc", &p.acc)] {
        let ok =
            v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit());
        if !ok {
            return Err((k, v.clone()));
        }
    }
    Ok(())
}

/// Số dòng (từ 1) chứa byte `offset`.
fn line_of(text: &str, offset: usize) -> usize {
    text.get(..offset.min(text.len()))
        .map_or(1, |s| s.bytes().filter(|&b| b == b'\n').count() + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::from_toml(text, Path::new("arcturus.toml"))
    }

    #[test]
    fn missing_config_uses_defaults() {
        let dir = std::env::temp_dir().join(format!("arcturus-cfg-{}", ulid::Ulid::generate()));
        let cfg = Config::load(&dir.join("arcturus.toml")).unwrap();
        assert_eq!(cfg, Config::default());
        assert_eq!(cfg.hub.approval_timeout_secs, 110);
        assert_eq!(cfg.team.main_roster_max, 5);
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn example_config_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../arcturus.example.toml");
        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.personas.len(), 4);
        assert_eq!(cfg.personas[0].name, "Khoa");
        let book = cfg.persona_book().unwrap();
        assert_eq!(
            book.find("/x/.claude/worktrees/lab")
                .map(|p| p.name.as_str()),
            Some("Vy")
        );
    }

    #[test]
    fn invalid_config_fails_with_line_number() {
        let text = "[hub]\nbind = \"127.0.0.1:7878\"\napproval_timeout_secs = \"nhiều\"\n";
        match parse(text) {
            Err(ConfigError::Parse { line, message, .. }) => {
                assert_eq!(line, 3, "{message}");
            }
            other => panic!("cần lỗi Parse, có {other:?}"),
        }
        match parse("[hub\nbind = 1\n") {
            Err(ConfigError::Parse { line, .. }) => assert_eq!(line, 1),
            other => panic!("cần lỗi Parse, có {other:?}"),
        }
        let msg = parse(text).unwrap_err().to_string();
        assert!(msg.starts_with("arcturus.toml:3: "), "{msg}");
    }

    #[test]
    fn unknown_key_rejected_with_line() {
        let text = "[hub]\nbind = \"127.0.0.1:7878\"\n\n[team]\nmain_roster_mx = 5\n";
        match parse(text) {
            Err(ConfigError::Parse { line, message, .. }) => {
                assert_eq!(line, 5);
                assert!(message.contains("main_roster_mx"), "{message}");
            }
            other => panic!("cần lỗi Parse, có {other:?}"),
        }
    }

    #[test]
    fn handoff_thresholds_validated() {
        let cfg = Config::default();
        assert_eq!(
            (cfg.hub.handoff_max_bytes, cfg.hub.handoff_max_lines),
            (4096, 50)
        );
        let cfg = parse("[hub]\nhandoff_max_bytes = 100\nhandoff_max_lines = 3\n").unwrap();
        assert_eq!(
            (cfg.hub.handoff_max_bytes, cfg.hub.handoff_max_lines),
            (100, 3)
        );
        for (key, v) in [
            ("handoff_max_bytes", 0),
            ("handoff_max_lines", 0),
            ("handoff_max_bytes", MAX_HANDOFF_THRESHOLD + 1),
        ] {
            let err = parse(&format!("[hub]\n{key} = {v}\n")).unwrap_err();
            assert!(
                matches!(&err, ConfigError::Invalid { field, .. } if field == &format!("hub.{key}")),
                "{key}={v}: {err}"
            );
        }
    }

    #[test]
    fn approval_timeout_out_of_range_rejected() {
        for v in [0, 116, 120, 600] {
            let err = parse(&format!("[hub]\napproval_timeout_secs = {v}\n")).unwrap_err();
            assert!(
                matches!(&err, ConfigError::Invalid { field, .. } if field == "hub.approval_timeout_secs"),
                "{v}: {err}"
            );
        }
        assert!(parse("[hub]\napproval_timeout_secs = 115\n").is_ok());
    }

    #[test]
    fn non_loopback_bind_rejected() {
        for bind in ["0.0.0.0:7878", "192.168.1.5:7878", "khong-phai-dia-chi"] {
            let err = parse(&format!("[hub]\nbind = \"{bind}\"\n")).unwrap_err();
            assert!(
                matches!(&err, ConfigError::Invalid { field, .. } if field == "hub.bind"),
                "{bind}: {err}"
            );
        }
        assert!(parse("[hub]\nbind = \"[::1]:7878\"\n").is_ok());
    }

    #[test]
    fn persona_values_validated() {
        let persona = |glob: &str, hair: &str| {
            format!(
                "[[persona]]\nmatch_cwd = \"{glob}\"\nname = \"Khoa\"\nrole = \"backend\"\n\
                 palette = {{ hair = \"{hair}\", outfit = \"#4C8DF6\", acc = \"#9AA3B5\" }}\n"
            )
        };
        assert!(parse(&persona("**/hub*", "#1F1F24")).is_ok());
        let err = parse(&persona("**/hub*", "url(javascript:x)")).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Invalid { field, .. } if field == "persona[0].palette.hair"),
            "{err}"
        );
        let err = parse(&persona("a/[b", "#1F1F24")).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Invalid { field, .. } if field == "persona[0].match_cwd"),
            "{err}"
        );
        // Palette thiếu trường hoặc có trường lạ: lỗi parse có số dòng.
        let text = "[[persona]]\nmatch_cwd = \"x\"\nname = \"K\"\nrole = \"r\"\npalette = { hair = \"#000000\" }\n";
        assert!(matches!(
            parse(text),
            Err(ConfigError::Parse { line: 5, .. })
        ));
    }
}
