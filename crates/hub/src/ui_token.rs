//! Token cho UI (`/ws`, `/approvals/*`), tách khỏi token của hook (quyết định D1 ở T5).
//!
//! Lý do: Bash tool của Claude Code thừa hưởng `ARCTURUS_TOKEN` (`docs/HOOK_FINDINGS.md` F6), nên agent nào cũng đọc
//! được token của hook. Token UI do hub tự sinh mỗi lần chạy (không lấy từ biến môi trường), ghi vào file chỉ người dùng
//! đọc được (`<data_dir>/arcturus/ui-token`); app Tauri đọc file đó. Không bao giờ log token.
//!
//! File vẫn đọc được bởi agent đã có quyền chạy mã dưới cùng người dùng (F6). `--ui-token-stdout` (T5d · ARC-44) trao
//! token qua stdout cho tiến trình đã khởi động hub (app, T12), không ghi file nào.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Cách trao token UI cho app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Ghi file (mặc định): `--ui-token-file` hoặc [`default_path`].
    File(PathBuf),
    /// In đúng một dòng `<token>\n` ra stdout rồi flush; không ghi file.
    Stdout,
}

/// Trao token theo `delivery`. `out` là stdout (tham số để test được).
pub fn deliver(delivery: &Delivery, token: &str, out: &mut impl Write) -> io::Result<()> {
    match delivery {
        Delivery::File(path) => write(path, token),
        Delivery::Stdout => {
            out.write_all(token.as_bytes())?;
            out.write_all(b"\n")?;
            out.flush()
        }
    }
}

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
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(dir)?;
    }
    // Ghi file tạm cạnh đích rồi đổi tên: app không bao giờ đọc phải token ghi dở. File tạm luôn được TẠO MỚI
    // (`create_new`): file hay symlink cài sẵn ở đó bị xóa trước, không bao giờ bị ghi xuyên qua hay giữ quyền cũ
    // (audit T5b finding 7).
    let tmp = path.with_extension("tmp");
    match std::fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
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
    fn deliver_stdout_writes_exactly_one_line() {
        let mut out = Vec::new();
        deliver(&Delivery::Stdout, "abc123", &mut out).unwrap();
        assert_eq!(out, b"abc123\n");
    }

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
    fn stale_tmp_file_is_replaced_not_reused() {
        let dir =
            std::env::temp_dir().join(format!("arcturus-ui-token-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ui-token");
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, "cài sẵn, rất dài để thấy rõ nếu bị ghi đè một phần").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        write(&path, "tok").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "tok");
        assert!(!tmp.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "không giữ quyền 0644 của file cài sẵn");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn planted_symlink_tmp_is_not_followed() {
        let dir =
            std::env::temp_dir().join(format!("arcturus-ui-token-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&dir).unwrap();
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, "không được đổi").unwrap();
        let path = dir.join("ui-token");
        std::os::unix::fs::symlink(&victim, path.with_extension("tmp")).unwrap();
        write(&path, "tok").unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "không được đổi");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "tok");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_path_is_under_data_dir() {
        if let Some(p) = default_path() {
            assert!(p.ends_with(Path::new("arcturus").join("ui-token")));
        }
    }
}
