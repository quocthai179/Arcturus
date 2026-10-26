//! Ứng dụng desktop Arcturus (docs/PLAN.md T12): tray + popup Lite. Là CLIENT của hub, không chứa logic nghiệp vụ.
//!
//! Khởi động: chỉ tạo tray; một thread nền kết nối hub (hoặc khởi động hub làm tiến trình con, `hub_proc`), rồi một
//! kết nối WebSocket nhẹ (`link`) cập nhật badge. Webview nhận token qua `invoke('hub_info')`.

// Bản release trên Windows không mở cửa sổ console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod http;
mod hub_proc;
mod link;
mod tray;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, RunEvent, State};
use tokio::sync::mpsc;

use hub_proc::{HubChild, HubInfo};
use link::{Badge, Command};
use tray::TrayState;

/// Trạng thái chung của app (ngoài tray).
struct AppState {
    /// Kết nối hiện tại, hoặc lý do chưa kết nối được (hiện cho người dùng).
    hub: Mutex<Result<HubInfo, String>>,
    /// Hub do app khởi động (đóng pipe stdin là hub tắt êm).
    child: Mutex<Option<HubChild>>,
    commands: mpsc::Sender<Command>,
}

impl AppState {
    fn info(&self) -> Result<HubInfo, String> {
        self.hub.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Kết nối lại: còn hub con thì giữ thông tin cũ; không thì tìm hub đang chạy hoặc khởi động hub mới.
    fn refresh(&self) -> Result<HubInfo, String> {
        {
            let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
            if child.as_mut().is_some_and(HubChild::running) {
                return self.info();
            }
            *child = None;
        }
        let result = hub_proc::connect();
        let info = result
            .as_ref()
            .map(|(i, _)| i.clone())
            .map_err(Clone::clone);
        if let Ok((_, child)) = result {
            *self.child.lock().unwrap_or_else(|e| e.into_inner()) = child;
        }
        *self.hub.lock().unwrap_or_else(|e| e.into_inner()) = info.clone();
        info
    }

    fn stop_hub(&self) {
        if let Some(mut child) = self.child.lock().unwrap_or_else(|e| e.into_inner()).take() {
            child.stop(Duration::from_secs(3));
        }
    }
}

#[derive(Serialize)]
struct HubInfoDto {
    ws_url: String,
    token: String,
}

/// Thông tin kết nối cho webview. Token chỉ đi qua đây (không nằm trong URL của trang, không có file khi app tự khởi
/// động hub).
#[tauri::command]
fn hub_info(state: State<'_, Arc<AppState>>) -> Result<HubInfoDto, String> {
    let info = state.info()?;
    Ok(HubInfoDto {
        ws_url: info.ws_url(),
        token: info.token,
    })
}

/// `request_id` do hub sinh: `req_` + ULID. Kiểm trước khi ghép vào đường dẫn HTTP.
fn valid_request_id(id: &str) -> bool {
    id.strip_prefix("req_").is_some_and(|r| {
        !r.is_empty() && r.len() <= 64 && r.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

/// `GET /approvals/{id}/input` từ phía Rust (route không có CORS, cần header Bearer). Trả JSON đã định dạng.
#[tauri::command]
async fn approval_input(
    state: State<'_, Arc<AppState>>,
    request_id: String,
) -> Result<String, String> {
    if !valid_request_id(&request_id) {
        return Err("Mã yêu cầu không hợp lệ".to_owned());
    }
    let info = state.info()?;
    let path = format!("/approvals/{request_id}/input");
    let resp = tauri::async_runtime::spawn_blocking(move || {
        http::get(info.addr, &path, Some(&info.token), Duration::from_secs(5))
    })
    .await
    .map_err(|e| e.to_string())??;
    match resp.status {
        200 => {
            let text = String::from_utf8_lossy(&resp.body).into_owned();
            Ok(serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| serde_json::to_string_pretty(&v).ok())
                .unwrap_or(text))
        }
        404 => Err("Yêu cầu không còn chờ".to_owned()),
        401 | 403 => Err("Hub từ chối token UI".to_owned()),
        s => Err(format!("Hub trả lỗi {s}")),
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        tray::MENU_OPEN_QUEUE => tray::toggle_popup(app, None),
        tray::MENU_OPEN_OFFICE => tray::open_office(app),
        tray::MENU_TOGGLE_PAUSE => {
            let (Some(state), Some(t)) = (
                app.try_state::<Arc<AppState>>(),
                app.try_state::<TrayState>(),
            ) else {
                return;
            };
            let cmd = if t.paused() {
                Command::TeamResume
            } else {
                Command::TeamPause
            };
            let _ = state.commands.try_send(cmd);
        }
        tray::MENU_TOGGLE_FOCUS => {
            let (Some(state), Some(t)) = (
                app.try_state::<Arc<AppState>>(),
                app.try_state::<TrayState>(),
            ) else {
                return;
            };
            let _ = state.commands.try_send(Command::FocusSet(!t.focused()));
        }
        tray::MENU_QUIT => {
            if let Some(state) = app.try_state::<Arc<AppState>>() {
                state.stop_hub();
            }
            app.exit(0);
        }
        _ => {}
    }
}

fn start_link(app: AppHandle, state: Arc<AppState>, commands: mpsc::Receiver<Command>) {
    tauri::async_runtime::spawn(async move {
        let getter_state = state.clone();
        let info = move || getter_state.refresh();
        let badge_app = app.clone();
        let badge_state = state.clone();
        link::run(info, commands, move |b: Badge| {
            let error = badge_state.info().err();
            if let Some(t) = badge_app.try_state::<TrayState>() {
                t.show_badge(b, error.as_deref());
            }
        })
        .await;
    });
}

fn main() {
    let (commands_tx, commands_rx) = mpsc::channel(16);
    let state = Arc::new(AppState {
        hub: Mutex::new(Err("đang kết nối hub…".to_owned())),
        child: Mutex::new(None),
        commands: commands_tx,
    });
    let link_state = state.clone();
    let exit_state = state.clone();
    let mut commands_rx = Some(commands_rx);

    let app = tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![hub_info, approval_input])
        .on_menu_event(|app, event| on_menu(app, tray::menu_id(&event)))
        .setup(move |app| {
            let t = tray::build(app.handle())?;
            app.manage(t);
            if let Some(rx) = commands_rx.take() {
                start_link(app.handle().clone(), link_state.clone(), rx);
            }
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(e) => {
            eprintln!("arcturus app: không khởi động được: {e}");
            std::process::exit(1);
        }
    };
    app.run(move |_app, event| match event {
        // Đóng popup/văn phòng không thoát app: app sống trong tray. Chỉ "Thoát" (exit với mã) mới thoát.
        RunEvent::ExitRequested {
            api, code: None, ..
        } => api.prevent_exit(),
        RunEvent::Exit => exit_state.stop_hub(),
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_validation() {
        assert!(valid_request_id("req_01JABCDEF0123456789XYZ"));
        assert!(!valid_request_id("req_"));
        assert!(!valid_request_id("hof_01J"));
        assert!(!valid_request_id("req_../../ws"));
        assert!(!valid_request_id("req_a/b"));
        assert!(!valid_request_id(&format!("req_{}", "a".repeat(65))));
    }
}
