//! Ứng dụng desktop Arcturus (docs/PLAN.md T12): tray + popup Lite. Là CLIENT của hub, không chứa logic nghiệp vụ.
//!
//! Khởi động: chỉ tạo tray; một thread nền kết nối hub (hoặc khởi động hub làm tiến trình con, `hub_proc`), rồi một
//! kết nối WebSocket nhẹ (`link`) cập nhật badge. Webview nhận token qua `invoke('hub_info')`.

// Bản release trên Windows không mở cửa sổ console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod http;
mod hub_proc;
mod link;
mod measure;
mod pet;
mod setup;
mod tray;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, RunEvent, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::mpsc;

use hub_proc::{HubChild, HubInfo};
use link::{Badge, Command};
use pet::{ModelInfo, PetState, Settings};
use tray::TrayState;

/// Trạng thái chung của app (ngoài tray).
struct AppState {
    /// Kết nối hiện tại, hoặc lý do chưa kết nối được (hiện cho người dùng).
    hub: Mutex<Result<HubInfo, String>>,
    /// Hub do app khởi động (đóng pipe stdin là hub tắt êm).
    child: Mutex<Option<HubChild>>,
    commands: mpsc::Sender<Command>,
    /// Token hook do app quản lý (`setup::hook_token`), hoặc lý do không có.
    hook_token: Result<String, String>,
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
        let result = hub_proc::connect(self.hook_token.as_deref().ok());
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

/// UI báo popup đã dựng xong khung đầu tiên (T16). Chỉ có tác dụng ở chế độ đo; ngoài ra là lệnh rỗng.
#[tauri::command]
fn popup_ready(tray: State<'_, TrayState>) {
    if !measure::enabled() {
        return;
    }
    if let Some(ms) = tray.popup_ui_ready() {
        measure::log("popup_ready", &format!("ms={ms}"));
    }
}

/// `GET /standup?hours=` (T19) từ phía Rust (route không có CORS, cần header Bearer). Trả phần văn bản template.
#[tauri::command]
async fn standup(state: State<'_, Arc<AppState>>, hours: u32) -> Result<String, String> {
    let hours = hours.clamp(1, 168);
    let info = state.info()?;
    let path = format!("/standup?hours={hours}");
    let resp = tauri::async_runtime::spawn_blocking(move || {
        http::get(info.addr, &path, Some(&info.token), Duration::from_secs(10))
    })
    .await
    .map_err(|e| e.to_string())??;
    match resp.status {
        200 => serde_json::from_slice::<serde_json::Value>(&resp.body)
            .ok()
            .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(str::to_owned))
            .ok_or_else(|| "Standup trả dữ liệu lạ".to_owned()),
        503 => Err("Hub chưa có cơ sở dữ liệu để tóm tắt".to_owned()),
        401 | 403 => Err("Hub từ chối token UI".to_owned()),
        s => Err(format!("Hub trả lỗi {s}")),
    }
}

/// Lệnh chỉ dành cho cửa sổ pet.
fn pet_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == pet::PET {
        Ok(())
    } else {
        Err("Chỉ cửa sổ trợ lý được gọi lệnh này".to_owned())
    }
}

#[tauri::command]
fn pet_settings(window: WebviewWindow, pet: State<'_, PetState>) -> Result<Settings, String> {
    pet_only(&window)?;
    Ok(pet.settings())
}

/// Bytes của model (ArrayBuffer ở phía JS). Không có model: lỗi "Chưa chọn model" (UI vẽ nhân vật 2D).
#[tauri::command]
async fn pet_model(window: WebviewWindow, app: AppHandle) -> Result<Response, String> {
    pet_only(&window)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || pet::read_model(&app))
        .await
        .map_err(|e| e.to_string())??;
    Ok(Response::new(bytes))
}

/// Hộp thoại chọn file `.vrm`, kiểm rồi chép vào thư mục dữ liệu của app. `None` khi người dùng hủy.
#[tauri::command]
async fn pet_choose_model(
    window: WebviewWindow,
    app: AppHandle,
) -> Result<Option<ModelInfo>, String> {
    pet_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_title("Chọn model VRM cho trợ lý")
            .add_filter("VRM", &["vrm"])
            .blocking_pick_file();
        let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
            return Ok(None);
        };
        pet::import_model(&app, &path).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn pet_remove_model(window: WebviewWindow, app: AppHandle) -> Result<(), String> {
    pet_only(&window)?;
    pet::remove_model(&app)
}

#[tauri::command]
fn pet_set_voice(window: WebviewWindow, app: AppHandle, on: bool) -> Result<(), String> {
    pet_only(&window)?;
    pet::set_voice(&app, on);
    Ok(())
}

#[tauri::command]
fn pet_set_chatter(
    window: WebviewWindow,
    app: AppHandle,
    level: pet::Chatter,
) -> Result<(), String> {
    pet_only(&window)?;
    pet::set_chatter(&app, level);
    Ok(())
}

/// Đổi cỡ cửa sổ pet (gọi từ phía Rust: capability của webview không có quyền đổi kích thước cửa sổ).
#[tauri::command]
fn pet_set_size(window: WebviewWindow, app: AppHandle, size: pet::PetSize) -> Result<(), String> {
    pet_only(&window)?;
    pet::set_size(&app, &window, size)
}

/// Kéo cửa sổ pet: bắt đầu, dời theo con trỏ, kết thúc (gọi từ phía Rust để capability của webview chỉ cần
/// `core:default`). Xem `pet::drag_begin`.
#[tauri::command]
fn pet_drag_begin(window: WebviewWindow, app: AppHandle) -> Result<(), String> {
    pet_only(&window)?;
    pet::drag_begin(&app, &window)
}

#[tauri::command]
fn pet_drag_move(window: WebviewWindow, app: AppHandle) -> Result<(), String> {
    pet_only(&window)?;
    pet::drag_move(&app, &window)
}

#[tauri::command]
fn pet_drag_end(window: WebviewWindow, app: AppHandle) -> Result<(), String> {
    pet_only(&window)?;
    pet::drag_end(&app);
    Ok(())
}

#[tauri::command]
fn pet_hide(window: WebviewWindow, app: AppHandle) -> Result<(), String> {
    pet_only(&window)?;
    pet::close(&app);
    Ok(())
}

/// Bấm vào nhân vật: mở popup hàng đợi (nơi duyệt có đủ điều kiện và đếm ngược).
#[tauri::command]
fn pet_open_queue(window: WebviewWindow, app: AppHandle) -> Result<(), String> {
    pet_only(&window)?;
    tray::show_queue(&app);
    Ok(())
}

#[tauri::command]
fn pet_open_office(window: WebviewWindow, app: AppHandle) -> Result<(), String> {
    pet_only(&window)?;
    tray::open_office(&app);
    Ok(())
}

/// UI pet đã vẽ khung đầu tiên (`ok`: model 3D hay bản 2D). Chỉ có tác dụng ở chế độ đo.
#[tauri::command]
fn pet_ready(pet: State<'_, PetState>, kind: String) {
    if let Some(ms) = pet.take_open_ms() {
        let kind: String = kind
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(16)
            .collect();
        measure::log("pet_ready", &format!("ms={ms} kind={kind}"));
    }
}

/// Lệnh chỉ dành cho cửa sổ cài đặt (ghi `settings.json` của Claude Code).
fn setup_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == tray::SETUP {
        Ok(())
    } else {
        Err("Chỉ cửa sổ cài đặt được gọi lệnh này".to_owned())
    }
}

/// Đường dẫn settings, cổng hub và token hook cho các lệnh `setup_*`.
fn setup_ctx(state: &AppState) -> Result<(std::path::PathBuf, u16, String), String> {
    let path = setup::settings_path().ok_or("không tìm được thư mục người dùng")?;
    let token = state.hook_token.clone()?;
    Ok((path, hub_proc::hub_addr().port(), token))
}

#[tauri::command]
fn setup_status(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
) -> Result<setup::Status, String> {
    setup_only(&window)?;
    let (path, port, token) = setup_ctx(&state)?;
    Ok(setup::status(&path, port, &token))
}

#[tauri::command]
fn setup_preview(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
    action: setup::Action,
) -> Result<setup::Preview, String> {
    setup_only(&window)?;
    let (path, port, token) = setup_ctx(&state)?;
    setup::preview(&path, action, port, &token)
}

/// Ghi thay đổi người dùng vừa xem trước (`digest` của lúc xem trước; file đổi giữa chừng thì không ghi).
#[tauri::command]
fn setup_apply(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
    action: setup::Action,
    digest: String,
) -> Result<setup::Applied, String> {
    setup_only(&window)?;
    let (path, port, token) = setup_ctx(&state)?;
    setup::apply(&path, action, port, &token, &digest)
}

#[tauri::command]
async fn setup_selftest(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
) -> Result<setup::SelfTest, String> {
    setup_only(&window)?;
    let (path, _, token) = setup_ctx(&state)?;
    // Hub con đã tắt (crash) thì khởi động lại trước khi thử, như link làm khi mất kết nối.
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _ = st.refresh();
        setup::selftest(&path, hub_proc::hub_addr(), &token)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn setup_selftest_cleanup(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
    nonce: String,
) -> Result<(), String> {
    setup_only(&window)?;
    let (_, _, token) = setup_ctx(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        setup::selftest_cleanup(hub_proc::hub_addr(), &token, &nonce)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Lần chạy đầu mà hook chưa cài: tự mở cửa sổ cài đặt một lần (đánh dấu bằng file `setup-seen`).
fn first_run_setup(app: &AppHandle, state: &AppState) {
    let Ok((path, port, token)) = setup_ctx(state) else {
        return;
    };
    let Ok(dir) = app.path().app_config_dir() else {
        return;
    };
    let marker = dir.join("setup-seen");
    if marker.exists() || setup::status(&path, port, &token).up_to_date {
        return;
    }
    if std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&marker, b"1").is_ok() {
        tray::open_setup(app);
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        tray::MENU_OPEN_QUEUE => tray::toggle_popup(app, None),
        tray::MENU_OPEN_OFFICE => tray::open_office(app),
        tray::MENU_OPEN_SETUP => tray::open_setup(app),
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
        tray::MENU_TOGGLE_PET => {
            pet::toggle(app);
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
    let hook_token = dirs::data_dir()
        .map(|d| d.join("arcturus"))
        .ok_or_else(|| "không tìm được thư mục dữ liệu người dùng".to_owned())
        .and_then(|d| setup::hook_token(&d, std::env::var(setup::TOKEN_ENV).ok()));
    if let Err(e) = &hook_token {
        eprintln!("arcturus app: {e}");
    }
    let state = Arc::new(AppState {
        hub: Mutex::new(Err("đang kết nối hub…".to_owned())),
        child: Mutex::new(None),
        commands: commands_tx,
        hook_token,
    });
    let setup_state = state.clone();
    let link_state = state.clone();
    let exit_state = state.clone();
    let mut commands_rx = Some(commands_rx);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            hub_info,
            approval_input,
            popup_ready,
            standup,
            pet_settings,
            pet_model,
            pet_choose_model,
            pet_remove_model,
            pet_set_voice,
            pet_set_chatter,
            pet_set_size,
            pet_drag_begin,
            pet_drag_move,
            pet_drag_end,
            pet_hide,
            pet_open_queue,
            pet_open_office,
            pet_ready,
            setup_status,
            setup_preview,
            setup_apply,
            setup_selftest,
            setup_selftest_cleanup
        ])
        .on_menu_event(|app, event| on_menu(app, tray::menu_id(&event)))
        .setup(move |app| {
            let t = tray::build(app.handle())?;
            app.manage(t);
            app.manage(PetState::load(app.handle()));
            pet::restore(app.handle());
            first_run_setup(app.handle(), &setup_state);
            if let Some(rx) = commands_rx.take() {
                start_link(app.handle().clone(), link_state.clone(), rx);
            }
            // Chế độ đo popup (T16): chỉ bật khi script đo đặt ARCTURUS_MEASURE_POPUP.
            if let Some(cfg) = measure::config_from(|k| std::env::var(k).ok()) {
                measure::start(app.handle().clone(), cfg);
            } else if let Some(cfg) = measure::pet_config_from(|k| std::env::var(k).ok()) {
                measure::start_pet(app.handle().clone(), cfg);
            } else if std::env::var_os("ARCTURUS_MEASURE_LOG").is_some_and(|v| !v.is_empty()) {
                measure::enable_logging();
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
