//! Cửa sổ trợ lý "Pet" (`docs/PLAN.md` §7, `docs/adr/0001-pet-v1.md`). Là client của hub như popup: chỉ hiển thị và gửi
//! lệnh có sẵn. Tắt mặc định; bật/tắt từ tray. Đóng pet thì hủy webview (không giữ WebGL khi ẩn).
//!
//! Cửa sổ vừa khít nhân vật, không click xuyên (click xuyên cần theo dõi con trỏ toàn cục = polling, trái quy tắc 6).
//! Model do người dùng chọn: kiểm định dạng GLB/VRM và kích thước, chép vào thư mục dữ liệu của app, rồi webview đọc
//! bytes qua lệnh `pet_model` (không mở asset protocol cho ổ đĩa).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{
    AppHandle, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

pub const PET: &str = "pet";
/// Kích thước logic của cửa sổ (đủ cho nhân vật từ đầu gối trở lên và bong bóng thoại phía trên).
pub const PET_W: f64 = 340.0;
pub const PET_H: f64 = 560.0;
/// Trần kích thước file model. Model VRoid thường 10–30 MB; PLAN §7.6 khuyên ≤ 15 MB cho model đóng gói.
pub const MODEL_MAX_BYTES: u64 = 64 * 1024 * 1024;
const MODEL_FILE: &str = "model.vrm";

/// Tùy chọn của pet, lưu ở `<app_config_dir>/pet.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    /// Vị trí góc trên trái (pixel vật lý) lần cuối.
    pub x: Option<i32>,
    pub y: Option<i32>,
    /// Thông tin model đã chọn (file nằm ở `<app_data_dir>/pet/model.vrm`).
    pub model: Option<ModelInfo>,
    /// Đọc lời pet bằng giọng của hệ điều hành (P9). Tắt mặc định.
    pub voice: bool,
}

/// Thông tin đọc từ phần meta của model, để người dùng tự kiểm giấy phép. Mọi chuỗi là dữ liệu không tin cậy.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelInfo {
    pub file_name: String,
    pub bytes: u64,
    /// "0" (VRM 0.x) hoặc "1" (VRM 1.0).
    pub vrm_version: String,
    pub name: String,
    pub authors: Vec<String>,
    pub license: String,
    pub allow_redistribution: Option<bool>,
    pub commercial_usage: String,
}

/// Trạng thái pet trong app (vị trí mới nhất, thời điểm mở để đo).
#[derive(Default)]
pub struct PetState {
    settings: Mutex<Settings>,
    opened_at: Mutex<Option<Instant>>,
}

impl PetState {
    pub fn load(app: &AppHandle) -> Self {
        let settings = settings_path(app)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default();
        Self {
            settings: Mutex::new(settings),
            opened_at: Mutex::new(None),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn update(&self, app: &AppHandle, f: impl FnOnce(&mut Settings)) {
        let snapshot = {
            let mut s = self.settings.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut s);
            s.clone()
        };
        if let Err(e) = save_settings(app, &snapshot) {
            eprintln!("arcturus app: không lưu được tùy chọn pet: {e}");
        }
    }

    /// Mili giây từ lúc bắt đầu mở cửa sổ pet (lấy đi).
    pub fn take_open_ms(&self) -> Option<u128> {
        self.opened_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .map(|t| t.elapsed().as_millis())
    }
}

/// Thư mục dữ liệu pet thay thế (`ARCTURUS_PET_DIR`): chế độ đo và test dùng thư mục tạm để không ghi đè tùy chọn và
/// model thật của người dùng.
fn dir_override() -> Option<PathBuf> {
    std::env::var_os("ARCTURUS_PET_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    if let Some(d) = dir_override() {
        return Some(d.join("pet.json"));
    }
    app.path().app_config_dir().ok().map(|d| d.join("pet.json"))
}

fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(d) = dir_override() {
        return Ok(d.join("model"));
    }
    app.path()
        .app_data_dir()
        .map(|d| d.join("pet"))
        .map_err(|e| format!("không tìm được thư mục dữ liệu của app: {e}"))
}

fn save_settings(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let path = settings_path(app).ok_or("không tìm được thư mục cấu hình")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    // Ghi file tạm rồi đổi tên: mất điện giữa chừng không làm hỏng tùy chọn cũ.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Bật pet khi khởi động nếu lần trước đang bật.
pub fn restore(app: &AppHandle) {
    if app.state::<PetState>().settings().enabled {
        if let Err(e) = open(app) {
            eprintln!("arcturus app: không mở được pet: {e}");
        }
    }
}

pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(PET).is_some()
}

/// Bật/tắt từ tray. Trả trạng thái mới (đang hiện hay không).
pub fn toggle(app: &AppHandle) -> bool {
    if is_open(app) {
        close(app);
        false
    } else {
        match open(app) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("arcturus app: không mở được pet: {e}");
                false
            }
        }
    }
}

pub fn close(app: &AppHandle) {
    let state = app.state::<PetState>();
    state.update(app, |s| s.enabled = false);
    if let Some(w) = app.get_webview_window(PET) {
        let _ = w.destroy();
    }
    crate::tray::set_pet_shown(app, false);
}

pub fn open(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(PET) {
        return w.set_focus();
    }
    let state = app.state::<PetState>();
    *state.opened_at.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    let window = WebviewWindowBuilder::new(app, PET, WebviewUrl::App("index.html?view=pet".into()))
        .title("Arcturus — Trợ lý")
        .inner_size(PET_W, PET_H)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    let saved = state.settings();
    let pos = initial_position(app, &window, saved.x.zip(saved.y));
    if let Some(p) = pos {
        let _ = window.set_position(p);
    }
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Moved(p) = event {
            if let Some(s) = handle.try_state::<PetState>() {
                let mut g = s.settings.lock().unwrap_or_else(|e| e.into_inner());
                g.x = Some(p.x);
                g.y = Some(p.y);
            }
        }
        if let WindowEvent::Destroyed = event {
            // Lưu vị trí cuối (Moved bắn liên tục khi kéo nên chỉ ghi file ở đây).
            if let Some(s) = handle.try_state::<PetState>() {
                s.update(&handle, |_| {});
            }
        }
    });
    window.show()?;
    state.update(app, |s| s.enabled = true);
    crate::tray::set_pet_shown(app, true);
    crate::measure::log("pet_shown", "");
    Ok(())
}

/// Vị trí đã lưu nếu còn nằm trong một màn hình đang có; nếu không thì góc dưới phải vùng làm việc của màn hình chính.
fn initial_position<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    saved: Option<(i32, i32)>,
) -> Option<PhysicalPosition<i32>> {
    let size = window.outer_size().ok()?;
    let monitors: Vec<Area> = app
        .available_monitors()
        .ok()?
        .iter()
        .map(|m| {
            let wa = m.work_area();
            Area {
                x: wa.position.x,
                y: wa.position.y,
                w: i64::from(wa.size.width),
                h: i64::from(wa.size.height),
            }
        })
        .collect();
    let primary = app.primary_monitor().ok().flatten().map(|m| {
        let wa = m.work_area();
        Area {
            x: wa.position.x,
            y: wa.position.y,
            w: i64::from(wa.size.width),
            h: i64::from(wa.size.height),
        }
    });
    let (x, y) = place(
        saved,
        &monitors,
        primary,
        i64::from(size.width),
        i64::from(size.height),
    )?;
    Some(PhysicalPosition::new(x, y))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i64,
    pub h: i64,
}

/// Hàm thuần chọn vị trí: giữ vị trí đã lưu nếu ít nhất 1/3 khung còn nằm trong một màn hình; nếu không, góc dưới phải
/// của màn hình chính (lề 16 px). Không có màn hình nào: `None` (để hệ điều hành tự đặt).
pub fn place(
    saved: Option<(i32, i32)>,
    monitors: &[Area],
    primary: Option<Area>,
    w: i64,
    h: i64,
) -> Option<(i32, i32)> {
    if let Some((x, y)) = saved {
        let (x, y) = (i64::from(x), i64::from(y));
        let visible = monitors.iter().any(|m| {
            let ix = (x + w).min(i64::from(m.x) + m.w) - x.max(i64::from(m.x));
            let iy = (y + h).min(i64::from(m.y) + m.h) - y.max(i64::from(m.y));
            ix > 0 && iy > 0 && ix * iy * 3 >= w * h
        });
        if visible {
            return Some((x as i32, y as i32));
        }
    }
    let m = primary.or_else(|| monitors.first().copied())?;
    let x = i64::from(m.x) + (m.w - w - 16).max(0);
    let y = i64::from(m.y) + (m.h - h - 16).max(0);
    Some((x as i32, y as i32))
}

// ---------------------------------------------------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------------------------------------------------

/// Đọc phần JSON của file GLB và kiểm đây là VRM. Không tin nội dung: chỉ đọc vài trường meta để hiển thị.
pub fn inspect_glb(bytes: &[u8]) -> Result<ModelInfo, String> {
    if bytes.len() < 20 {
        return Err("File quá nhỏ, không phải VRM".to_owned());
    }
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    if &bytes[0..4] != b"glTF" {
        return Err("Không phải file glTF nhị phân (.vrm/.glb)".to_owned());
    }
    if u32_at(4) != 2 {
        return Err("Chỉ hỗ trợ glTF 2.0".to_owned());
    }
    if u64::from(u32_at(8)) != bytes.len() as u64 {
        return Err("Độ dài file không khớp phần đầu (file hỏng hoặc bị cắt)".to_owned());
    }
    let json_len = u32_at(12) as usize;
    if &bytes[16..20] != b"JSON" || 20 + json_len > bytes.len() {
        return Err("Thiếu phần JSON của glTF".to_owned());
    }
    let doc: Value = serde_json::from_slice(&bytes[20..20 + json_len])
        .map_err(|_| "Phần JSON của glTF không hợp lệ".to_owned())?;
    let ext = doc.get("extensions");
    let text = |v: Option<&Value>| -> String {
        v.and_then(Value::as_str)
            .map(|s| s.chars().filter(|c| !c.is_control()).take(120).collect())
            .unwrap_or_default()
    };
    let mut info = ModelInfo {
        bytes: bytes.len() as u64,
        ..ModelInfo::default()
    };
    if let Some(meta) = ext
        .and_then(|e| e.get("VRMC_vrm"))
        .and_then(|v| v.get("meta"))
    {
        info.vrm_version = "1".to_owned();
        info.name = text(meta.get("name"));
        info.authors = meta
            .get("authors")
            .and_then(Value::as_array)
            .map(|a| a.iter().take(8).map(|v| text(Some(v))).collect())
            .unwrap_or_default();
        info.license = text(meta.get("licenseUrl"));
        info.allow_redistribution = meta.get("allowRedistribution").and_then(Value::as_bool);
        info.commercial_usage = text(meta.get("commercialUsage"));
    } else if let Some(meta) = ext.and_then(|e| e.get("VRM")).and_then(|v| v.get("meta")) {
        info.vrm_version = "0".to_owned();
        info.name = text(meta.get("title"));
        let author = text(meta.get("author"));
        info.authors = if author.is_empty() {
            vec![]
        } else {
            vec![author]
        };
        info.license = text(meta.get("licenseName"));
        let other = text(meta.get("otherLicenseUrl"));
        if !other.is_empty() {
            info.license = format!("{} ({other})", info.license);
        }
        info.commercial_usage = text(meta.get("commercialUssageName"));
    } else {
        return Err("File glTF này không có phần mở rộng VRM".to_owned());
    }
    Ok(info)
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().chars().take(120).collect())
        .unwrap_or_default()
}

/// Kiểm và chép model người dùng chọn vào thư mục dữ liệu của app.
pub fn import_model(app: &AppHandle, source: &Path) -> Result<ModelInfo, String> {
    let meta = std::fs::metadata(source).map_err(|e| format!("Không đọc được file: {e}"))?;
    if !meta.is_file() {
        return Err("Đây không phải file".to_owned());
    }
    if meta.len() > MODEL_MAX_BYTES {
        return Err(format!(
            "Model quá lớn ({} MB, tối đa {} MB)",
            meta.len() / 1_048_576,
            MODEL_MAX_BYTES / 1_048_576
        ));
    }
    let bytes = std::fs::read(source).map_err(|e| format!("Không đọc được file: {e}"))?;
    let mut info = inspect_glb(&bytes)?;
    info.file_name = file_name_of(source);
    let dir = model_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("model.vrm.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("Không chép được model: {e}"))?;
    std::fs::rename(&tmp, dir.join(MODEL_FILE))
        .map_err(|e| format!("Không chép được model: {e}"))?;
    let saved = info.clone();
    app.state::<PetState>()
        .update(app, move |s| s.model = Some(saved));
    Ok(info)
}

/// Bytes của model đã chọn (kiểm lại trước khi đưa cho webview).
pub fn read_model(app: &AppHandle) -> Result<Vec<u8>, String> {
    let path = model_dir(app)?.join(MODEL_FILE);
    let meta = std::fs::metadata(&path).map_err(|_| "Chưa chọn model".to_owned())?;
    if meta.len() > MODEL_MAX_BYTES {
        return Err("Model quá lớn".to_owned());
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("Không đọc được model: {e}"))?;
    inspect_glb(&bytes)?;
    Ok(bytes)
}

pub fn remove_model(app: &AppHandle) -> Result<(), String> {
    let path = model_dir(app)?.join(MODEL_FILE);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Không xóa được model: {e}"))?;
    }
    app.state::<PetState>().update(app, |s| s.model = None);
    Ok(())
}

pub fn set_voice(app: &AppHandle, on: bool) {
    app.state::<PetState>().update(app, |s| s.voice = on);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glb(json: &str) -> Vec<u8> {
        let mut j = json.as_bytes().to_vec();
        while !j.len().is_multiple_of(4) {
            j.push(b' ');
        }
        let total = 12 + 8 + j.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(j.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&j);
        out
    }

    #[test]
    fn inspects_vrm1_meta() {
        let b = glb(
            r#"{"asset":{"version":"2.0"},"extensions":{"VRMC_vrm":{"meta":{"name":"Seed-san","authors":["VirtualCast"],"licenseUrl":"https://vrm.dev/licenses/1.0/","allowRedistribution":false,"commercialUsage":"personalNonProfit"}}}}"#,
        );
        let i = inspect_glb(&b).unwrap();
        assert_eq!(i.vrm_version, "1");
        assert_eq!(i.name, "Seed-san");
        assert_eq!(i.authors, vec!["VirtualCast".to_owned()]);
        assert_eq!(i.allow_redistribution, Some(false));
        assert_eq!(i.commercial_usage, "personalNonProfit");
    }

    #[test]
    fn inspects_vrm0_meta() {
        let b = glb(
            r#"{"extensions":{"VRM":{"meta":{"title":"Alicia","author":"DWANGO","licenseName":"Other","otherLicenseUrl":"https://example/x","commercialUssageName":"Disallow"}}}}"#,
        );
        let i = inspect_glb(&b).unwrap();
        assert_eq!(i.vrm_version, "0");
        assert_eq!(i.name, "Alicia");
        assert_eq!(i.license, "Other (https://example/x)");
        assert_eq!(i.commercial_usage, "Disallow");
    }

    #[test]
    fn rejects_non_vrm_and_broken_files() {
        assert!(inspect_glb(b"hello").is_err());
        assert!(
            inspect_glb(&glb(r#"{"asset":{}}"#)).is_err(),
            "glTF thường không có VRM"
        );
        let mut b = glb(r#"{"extensions":{"VRM":{"meta":{}}}}"#);
        b.truncate(b.len() - 2);
        assert!(inspect_glb(&b).is_err(), "độ dài không khớp");
        let mut b = glb(r#"{"extensions":{"VRM":{"meta":{}}}}"#);
        b[0] = b'x';
        assert!(inspect_glb(&b).is_err(), "sai magic");
        let mut b = glb(r#"{"extensions":{"VRM":{"meta":{}}}}"#);
        b[4] = 1;
        assert!(inspect_glb(&b).is_err(), "glTF 1.0");
        assert!(inspect_glb(&glb("{not json")).is_err());
    }

    #[test]
    fn meta_strings_are_sanitized_and_capped() {
        let long = "a".repeat(500);
        let b = glb(&format!(
            r#"{{"extensions":{{"VRMC_vrm":{{"meta":{{"name":"x\u0007y{long}"}}}}}}}}"#
        ));
        let i = inspect_glb(&b).unwrap();
        assert!(i.name.starts_with("xy"));
        assert_eq!(i.name.chars().count(), 120);
    }

    #[test]
    fn place_keeps_visible_saved_position() {
        let m = Area {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        assert_eq!(
            place(Some((100, 100)), &[m], Some(m), 340, 560),
            Some((100, 100))
        );
    }

    #[test]
    fn place_falls_back_when_saved_is_off_screen() {
        let m = Area {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        // Màn hình thứ hai đã rút ra: vị trí cũ ở x=3000.
        assert_eq!(
            place(Some((3000, 200)), &[m], Some(m), 340, 560),
            Some((1920 - 340 - 16, 1040 - 560 - 16))
        );
        // Gần như ra ngoài (chỉ còn một mép nhỏ trong màn hình).
        assert_eq!(
            place(Some((1900, 900)), &[m], Some(m), 340, 560),
            Some((1564, 464))
        );
    }

    #[test]
    fn place_handles_second_monitor_and_negative_coords() {
        let a = Area {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        let b = Area {
            x: -1280,
            y: 0,
            w: 1280,
            h: 984,
        };
        assert_eq!(
            place(Some((-900, 300)), &[a, b], Some(a), 340, 560),
            Some((-900, 300))
        );
        assert_eq!(place(None, &[], None, 340, 560), None);
    }

    #[test]
    fn settings_default_is_off_and_tolerates_unknown_fields() {
        let s: Settings = serde_json::from_str(r#"{"enabled":true,"future":1}"#).unwrap();
        assert!(s.enabled && !s.voice && s.model.is_none());
        assert!(!Settings::default().enabled);
    }
}
