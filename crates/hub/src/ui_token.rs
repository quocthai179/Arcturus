//! Token cho UI (`/ws`, `/approvals/*`), tách khỏi token của hook (quyết định D1 ở T5).
//!
//! Lý do: Bash tool của Claude Code thừa hưởng `ARCTURUS_TOKEN` (`docs/HOOK_FINDINGS.md` F6), nên agent nào cũng đọc
//! được token của hook. Token UI do hub tự sinh mỗi lần chạy (không lấy từ biến môi trường), ghi vào file chỉ người dùng
//! đọc được (`<data_dir>/arcturus/ui-token`); app Tauri đọc file đó. Không bao giờ log token.

use std::io;
use std::path::{Path, PathBuf};

/// Số byte ngẫu nhiên (hex thành 64 ký tự).
pub const UI_TOKEN_BYTES: usize = 32;

/// Token mới từ nguồn ngẫu nhiên của hệ điều hành.
pub fn generate() -> Result<String, getrandom::Error> {
    let mut buf = [0u8; UI_TOKEN_BYTES];
    getrandom::fill(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// `<data_dir>/arcturus/ui-token` (Windows: `%APPDATA%\arcturus\ui-token`).
pub fn default_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("arcturus").join("ui-token"))
}

/// Ghi token, thay file cũ. Unix: quyền 0600 ngay khi tạo. Windows: dựa vào quyền mặc định của thư mục hồ sơ người
/// dùng (`%APPDATA%`), chỉ người dùng và SYSTEM/Administrators đọc được (quyết định P2).
pub fn write(path: &Path, token: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Ghi file tạm cạnh đích rồi đổi tên: app không bao giờ đọc phải token ghi dở.
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(token.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_hex_and_strong() {
        let a = generate().unwrap();
        let b = generate().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 2 * UI_TOKEN_BYTES);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        // Đủ mạnh theo cùng luật với token của hook.
        assert!(crate::auth::token_from_env(Some(a)).is_ok());
    }

    #[test]
    fn write_replaces_file() {
        let dir =
            std::env::temp_dir().join(format!("arcturus-ui-token-{}", ulid::Ulid::generate()));
        let path = dir.join("nested").join("ui-token");
        write(&path, "first").unwrap();
        write(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        assert!(!path.with_extension("tmp").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_path_is_under_data_dir() {
        if let Some(p) = default_path() {
            assert!(p.ends_with(Path::new("arcturus").join("ui-token")));
        }
    }
}
