//! Liệt kê fixture `.json`.

use std::path::{Path, PathBuf};

/// Đếm file `.json` trong `dir` và các thư mục con. Thư mục chưa tồn tại thì trả 0.
pub fn count_json_files(dir: &Path) -> std::io::Result<usize> {
    Ok(json_files(dir)?.len())
}

/// Danh sách file `.json` (đường dẫn tương đối so với `dir`), sắp xếp. Thư mục chưa tồn tại thì rỗng.
pub fn json_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                if let Ok(rel) = path.strip_prefix(dir) {
                    out.push(rel.to_path_buf());
                }
            }
        }
    }
    out.sort();
    Ok(out)
}
