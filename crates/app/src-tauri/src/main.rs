//! Ứng dụng desktop Arcturus. T0 chỉ mở một cửa sổ hiển thị `ui/`; tray và popup Lite ở T12.

// Bản release trên Windows không mở cửa sổ console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = tauri::Builder::default().run(tauri::generate_context!()) {
        eprintln!("arcturus app: không khởi động được: {e}");
        std::process::exit(1);
    }
}
