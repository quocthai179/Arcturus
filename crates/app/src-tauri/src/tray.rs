//! Tray, badge và popup Lite (docs/PLAN.md T12).
//!
//! - Không mở cửa sổ khi khởi động; chỉ có tray.
//! - Click trái tray (Windows, macOS): chưa có popup thì tạo (380×560, không viền, luôn ở trên, gần khay); có rồi thì
//!   **destroy** (không hide, để RSS về mức Lite). Mất focus cũng destroy. Linux (appindicator) không báo click: mở
//!   bằng mục menu "Mở hàng đợi duyệt".
//! - Prewarm (T16): con trỏ dừng ≥150 ms trên icon thì tạo sẵn popup **ẩn** để lúc bấm chỉ còn `show()`; không bấm trong 3 s
//!   sau khi con trỏ rời thì destroy. Đường nguội (bàn phím, cảm ứng, Linux) vẫn tạo mới khi bấm.
//! - Badge: macOS dùng tiêu đề tray (số); Windows/Linux đổi giữa icon thường / icon có chấm, kèm tooltip.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent, Wry,
};

use crate::link::Badge;

pub const TRAY_ID: &str = "arcturus";
pub const POPUP: &str = "popup";
pub const OFFICE: &str = "office";
pub const SETUP: &str = "setup";
const POPUP_W: f64 = 380.0;
const POPUP_H: f64 = 560.0;
/// Mất focus ngay sau khi mở (trước khi cửa sổ kịp nhận focus) không tính là đóng.
const FOCUS_GRACE: Duration = Duration::from_millis(400);
/// Click vào tray làm popup mất focus rồi mới tới sự kiện click: không mở lại popup vừa đóng bởi chính cú click đó.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

/// Con trỏ phải dừng trên icon chừng này thì mới tạo sẵn popup (quét ngang qua khay không tốn webview).
const HOVER_DWELL: Duration = Duration::from_millis(150);
/// Popup tạo sẵn mà không ai bấm: destroy sau chừng này kể từ lúc con trỏ rời icon.
const PREWARM_TTL: Duration = Duration::from_secs(3);
/// Trần cứng: popup tạo sẵn không ai hiện thì destroy sau chừng này kể từ lúc tạo, dù có `Leave` hay không.
const PREWARM_MAX: Duration = Duration::from_secs(8);

pub const MENU_OPEN_QUEUE: &str = "open_queue";
pub const MENU_OPEN_OFFICE: &str = "open_office";
pub const MENU_OPEN_SETUP: &str = "open_setup";
pub const MENU_TOGGLE_PAUSE: &str = "toggle_pause";
pub const MENU_TOGGLE_FOCUS: &str = "toggle_focus";
pub const MENU_QUIT: &str = "quit";
pub const MENU_TOGGLE_PET: &str = "toggle_pet";

/// Phần trạng thái của tray trong app.
pub struct TrayState {
    pub tray: TrayIcon<Wry>,
    pub pause_item: MenuItem<Wry>,
    pub focus_item: MenuItem<Wry>,
    pub pet_item: MenuItem<Wry>,
    icon_plain: Image<'static>,
    icon_dot: Image<'static>,
    popup: Mutex<PopupTimes>,
    last_badge: Mutex<Option<Badge>>,
    /// Tăng mỗi lần con trỏ vào/ra icon hoặc có click: các luồng chờ của lần cũ thấy số khác thì bỏ việc.
    hover_gen: AtomicU64,
    /// Sau một cú bấm thì không tạo sẵn lại cho tới lần rê chuột kế tiếp.
    hover_armed: AtomicBool,
    /// Đã có một lượt hẹn tạo sẵn đang chờ hết `HOVER_DWELL` (`Move` dày đặc không sinh thêm luồng).
    hover_pending: AtomicBool,
    /// Số thứ tự lần tạo sẵn gần nhất, để luồng giới hạn cũ không dọn nhầm popup mới.
    prewarm_id: AtomicU64,
}

#[derive(Default)]
struct PopupTimes {
    opened: Option<Instant>,
    closed: Option<Instant>,
    /// Lúc người dùng bấm mở popup, để đo "popup mở" (T16). Lấy đi một lần khi UI báo xong.
    started: Option<Instant>,
    /// UI đã báo vẽ xong khung đầu tiên trước khi người dùng bấm (popup tạo sẵn).
    ready_early: bool,
}

impl PopupTimes {
    /// UI báo xong: nếu đã có lúc bấm thì trả số mili giây từ lúc bấm, chưa bấm (tạo sẵn) thì nhớ lại.
    fn ui_ready(&mut self) -> Option<u128> {
        if let Some(t) = self.started.take() {
            return Some(t.elapsed().as_millis());
        }
        self.ready_early = true;
        None
    }

    /// Popup tạo sẵn vừa được hiện: nếu UI đã vẽ xong từ trước thì thời gian mở tính tới lúc này.
    fn shown(&mut self) -> Option<u128> {
        if std::mem::take(&mut self.ready_early) {
            return self.started.take().map(|t| t.elapsed().as_millis());
        }
        None
    }
}

/// Vẽ chấm tròn màu `approval` (#FFB020) góc trên phải của icon RGBA.
pub fn draw_dot(rgba: &mut [u8], width: u32, height: u32) {
    let (w, h) = (width as i64, height as i64);
    let r = (w.min(h) / 4).max(2);
    let (cx, cy) = (w - r - 1, r + 1);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x - cx, y - cy);
            let d2 = dx * dx + dy * dy;
            if d2 > (r + 1) * (r + 1) {
                continue;
            }
            let i = ((y * w + x) * 4) as usize;
            let Some(px) = rgba.get_mut(i..i + 4) else {
                continue;
            };
            // Viền tối một pixel cho dễ thấy trên nền sáng lẫn tối.
            let color = if d2 > r * r {
                [0x24, 0x1B, 0x4D, 0xFF]
            } else {
                [0xFF, 0xB0, 0x20, 0xFF]
            };
            px.copy_from_slice(&color);
        }
    }
}

pub fn tooltip(b: &Badge, error: Option<&str>) -> String {
    if !b.connected {
        return match error {
            Some(e) => format!("Arcturus: {e}"),
            None => "Arcturus: đang kết nối hub…".to_owned(),
        };
    }
    let mut t = match (b.pending, b.focus) {
        (0, true) => "Arcturus: không có yêu cầu nào cần chú ý".to_owned(),
        (0, false) => "Arcturus: không có yêu cầu nào".to_owned(),
        (n, _) => format!("Arcturus: {n} yêu cầu đang chờ"),
    };
    if b.in_terminal > 0 {
        t.push_str(&format!(", {} đang duyệt trong terminal", b.in_terminal));
    }
    if b.focus {
        t.push_str(" (giờ tập trung");
        if b.quiet > 0 {
            t.push_str(&format!(": ẩn {} yêu cầu rủi ro thấp/vừa", b.quiet));
        }
        t.push(')');
    }
    if b.paused {
        t.push_str(" (đội đang tạm dừng)");
    }
    t
}

pub fn build(app: &AppHandle) -> tauri::Result<TrayState> {
    let base = Image::from_bytes(include_bytes!("../icons/32x32.png"))?.to_owned();
    let mut dot = base.rgba().to_vec();
    draw_dot(&mut dot, base.width(), base.height());
    let icon_dot = Image::new_owned(dot, base.width(), base.height());

    let open_queue = MenuItem::with_id(
        app,
        MENU_OPEN_QUEUE,
        "Mở hàng đợi duyệt",
        true,
        None::<&str>,
    )?;
    let open_office = MenuItem::with_id(app, MENU_OPEN_OFFICE, "Mở văn phòng", true, None::<&str>)?;
    let pause_item = MenuItem::with_id(
        app,
        MENU_TOGGLE_PAUSE,
        "Tạm dừng cả đội",
        false,
        None::<&str>,
    )?;
    let focus_item = MenuItem::with_id(
        app,
        MENU_TOGGLE_FOCUS,
        "Bật giờ tập trung",
        false,
        None::<&str>,
    )?;
    let pet_item = MenuItem::with_id(app, MENU_TOGGLE_PET, "Hiện trợ lý", true, None::<&str>)?;
    let setup_item = MenuItem::with_id(
        app,
        MENU_OPEN_SETUP,
        "Cài đặt và tự kiểm tra…",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Thoát", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open_queue,
            &open_office,
            &pause_item,
            &focus_item,
            &pet_item,
            &setup_item,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(base.clone())
        .tooltip(tooltip(&Badge::default(), None))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } => {
                hover_clicked(tray.app_handle());
                toggle_popup(tray.app_handle(), Some(rect));
            }
            TrayIconEvent::Enter { rect, .. } => hover_enter(tray.app_handle(), rect, true),
            TrayIconEvent::Move { rect, .. } => hover_enter(tray.app_handle(), rect, false),
            TrayIconEvent::Leave { .. } => hover_leave(tray.app_handle()),
            _ => {}
        })
        .build(app)?;

    Ok(TrayState {
        tray,
        pause_item,
        focus_item,
        pet_item,
        icon_plain: base,
        icon_dot,
        popup: Mutex::new(PopupTimes::default()),
        last_badge: Mutex::new(None),
        hover_gen: AtomicU64::new(0),
        hover_armed: AtomicBool::new(true),
        hover_pending: AtomicBool::new(false),
        prewarm_id: AtomicU64::new(0),
    })
}

impl TrayState {
    /// Cập nhật badge (bỏ qua nếu không đổi). `error`: lý do không kết nối được hub, hiện trong tooltip.
    pub fn show_badge(&self, b: Badge, error: Option<&str>) {
        {
            let mut last = self.last_badge.lock().unwrap_or_else(|e| e.into_inner());
            if *last == Some(b) && b.connected {
                return;
            }
            *last = Some(b);
        }
        let waiting = b.pending + b.in_terminal;
        let icon = if waiting > 0 {
            &self.icon_dot
        } else {
            &self.icon_plain
        };
        let _ = self.tray.set_icon(Some(icon.clone()));
        let _ = self.tray.set_tooltip(Some(tooltip(&b, error)));
        if cfg!(target_os = "macos") {
            let _ = self.tray.set_title(if waiting > 0 {
                Some(waiting.to_string())
            } else {
                None
            });
        }
        let _ = self.pause_item.set_enabled(b.connected);
        let _ = self.focus_item.set_enabled(b.connected);
        let _ = self.focus_item.set_text(if b.focus {
            "Tắt giờ tập trung"
        } else {
            "Bật giờ tập trung"
        });
        let _ = self.pause_item.set_text(if b.paused {
            "Tiếp tục cả đội"
        } else {
            "Tạm dừng cả đội"
        });
    }

    pub fn paused(&self) -> bool {
        self.last_badge
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|b| b.paused)
    }

    pub fn focused(&self) -> bool {
        self.last_badge
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|b| b.focus)
    }

    fn mark_started(&self) {
        self.popup.lock().unwrap_or_else(|e| e.into_inner()).started = Some(Instant::now());
    }

    /// UI báo popup đã vẽ xong khung đầu tiên (T16); trả mili giây từ lúc bấm nếu đã biết.
    pub fn popup_ui_ready(&self) -> Option<u128> {
        self.popup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ui_ready()
    }

    fn popup_shown_ready_ms(&self) -> Option<u128> {
        self.popup.lock().unwrap_or_else(|e| e.into_inner()).shown()
    }

    /// Mili giây từ lúc bắt đầu mở popup (không lấy đi).
    fn popup_started_ms(&self) -> Option<u128> {
        self.popup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .started
            .map(|t| t.elapsed().as_millis())
    }

    fn mark_opened(&self) {
        self.popup.lock().unwrap_or_else(|e| e.into_inner()).opened = Some(Instant::now());
    }

    fn mark_closed(&self) {
        self.popup.lock().unwrap_or_else(|e| e.into_inner()).closed = Some(Instant::now());
    }

    fn just_closed(&self) -> bool {
        self.popup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .closed
            .is_some_and(|t| t.elapsed() < REOPEN_GUARD)
    }

    fn in_focus_grace(&self) -> bool {
        self.popup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .opened
            .is_some_and(|t| t.elapsed() < FOCUS_GRACE)
    }
}

/// Chữ của mục tray bật/tắt pet (`pet.rs`).
pub fn set_pet_shown(app: &AppHandle, shown: bool) {
    if let Some(t) = app.try_state::<TrayState>() {
        let _ = t.pet_item.set_text(if shown {
            "Ẩn trợ lý"
        } else {
            "Hiện trợ lý"
        });
    }
}

/// Mở popup hàng đợi (không đóng nếu đang mở): pet gọi khi người dùng bấm vào nhân vật.
pub fn show_queue(app: &AppHandle) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if let Some(w) = app.get_webview_window(POPUP) {
        if matches!(w.is_visible(), Ok(true)) {
            let _ = w.set_focus();
            return;
        }
    }
    if app.get_webview_window(POPUP).is_some() {
        toggle_popup(app, None);
        return;
    }
    if let Err(e) = open_popup(app, &state, None) {
        eprintln!("arcturus app: không mở được popup: {e}");
    }
}

/// Đóng popup nếu đang mở, mở nếu chưa có (hoặc mới tạo sẵn, còn ẩn).
pub fn toggle_popup(app: &AppHandle, anchor: Option<Rect>) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if let Some(w) = app.get_webview_window(POPUP) {
        if matches!(w.is_visible(), Ok(false)) {
            // Popup tạo sẵn khi con trỏ dừng trên icon: chỉ cần hiện.
            state.mark_started();
            if let Some(pos) = popup_position(app, anchor) {
                let _ = w.set_position(pos);
            }
            if let Err(e) = show_popup(&w, &state) {
                eprintln!("arcturus app: không hiện được popup: {e}");
            }
            return;
        }
        state.mark_closed();
        let _ = w.destroy();
        return;
    }
    if state.just_closed() {
        return;
    }
    if let Err(e) = open_popup(app, &state, anchor) {
        eprintln!("arcturus app: không mở được popup: {e}");
    }
}

/// Tạo popup **ẩn**, đặt vị trí và gắn xử lý mất focus. Chưa hiện.
fn build_popup(app: &AppHandle, anchor: Option<Rect>) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(app, POPUP, WebviewUrl::App("index.html".into()))
        .title("Arcturus")
        .inner_size(POPUP_W, POPUP_H)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    if let Some(pos) = popup_position(app, anchor) {
        let _ = window.set_position(pos);
    }
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            let Some(state) = handle.try_state::<TrayState>() else {
                return;
            };
            if state.in_focus_grace() {
                return;
            }
            let Some(w) = handle.get_webview_window(POPUP) else {
                return;
            };
            // Popup tạo sẵn (chưa hiện) chưa từng có focus để mất.
            if matches!(w.is_visible(), Ok(false)) {
                return;
            }
            state.mark_closed();
            let _ = w.destroy();
        }
    });
    Ok(window)
}

fn show_popup(window: &WebviewWindow, state: &TrayState) -> tauri::Result<()> {
    state.mark_opened();
    window.show()?;
    window.set_focus()?;
    if let Some(ms) = state.popup_started_ms() {
        crate::measure::log("popup_shown", &format!("ms={ms}"));
    }
    if let Some(ms) = state.popup_shown_ready_ms() {
        crate::measure::log("popup_ready", &format!("ms={ms}"));
    }
    Ok(())
}

fn open_popup(app: &AppHandle, state: &TrayState, anchor: Option<Rect>) -> tauri::Result<()> {
    state.mark_started();
    let window = build_popup(app, anchor)?;
    if let Some(ms) = state.popup_started_ms() {
        crate::measure::log("popup_built", &format!("ms={ms}"));
    }
    show_popup(&window, state)
}

/// Tạo sẵn popup ẩn (con trỏ đang dừng trên icon). Gọi trên thread chính. Không làm gì nếu đã có popup.
pub fn prewarm_popup(app: &AppHandle, anchor: Option<Rect>) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if app.get_webview_window(POPUP).is_some() || state.just_closed() {
        return;
    }
    let t = Instant::now();
    match build_popup(app, anchor) {
        Ok(_) => {
            crate::measure::log(
                "popup_prewarmed",
                &format!("ms={}", t.elapsed().as_millis()),
            );
            schedule_prewarm_cap(app);
        }
        Err(e) => eprintln!("arcturus app: không tạo sẵn được popup: {e}"),
    }
}

fn bump_hover(app: &AppHandle) -> u64 {
    app.try_state::<TrayState>()
        .map_or(0, |s| s.hover_gen.fetch_add(1, Ordering::SeqCst) + 1)
}

fn hover_current(app: &AppHandle) -> u64 {
    app.try_state::<TrayState>()
        .map_or(0, |s| s.hover_gen.load(Ordering::SeqCst))
}

/// Có nên hẹn tạo sẵn popup không: con trỏ chưa bị "vô hiệu" bởi một cú bấm, chưa có lượt hẹn nào đang chờ và chưa có popup.
fn hover_wants_prewarm(armed: bool, pending: bool, popup_exists: bool) -> bool {
    armed && !pending && !popup_exists
}

/// Con trỏ vào (`fresh`) hoặc di chuyển trên icon. Windows chỉ phát `Leave` sau ít nhất một `Move`, nên rê vào rồi rời nhanh có
/// thể không có `Leave` và lần rê sau chỉ ra `Move`: `Move` cũng phải kích hoạt được prewarm.
pub(crate) fn hover_enter(app: &AppHandle, rect: Rect, fresh: bool) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if fresh {
        state.hover_armed.store(true, Ordering::SeqCst);
    }
    let popup_exists = app.get_webview_window(POPUP).is_some();
    if !hover_wants_prewarm(
        state.hover_armed.load(Ordering::SeqCst),
        state.hover_pending.load(Ordering::SeqCst),
        popup_exists,
    ) {
        return;
    }
    state.hover_pending.store(true, Ordering::SeqCst);
    let gen = bump_hover(app);
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("prewarm".to_owned())
        .spawn(move || {
            std::thread::sleep(HOVER_DWELL);
            if let Some(state) = app.try_state::<TrayState>() {
                state.hover_pending.store(false, Ordering::SeqCst);
                if !state.hover_armed.load(Ordering::SeqCst) {
                    return;
                }
            }
            if hover_current(&app) != gen {
                return;
            }
            let a = app.clone();
            let _ = app.run_on_main_thread(move || prewarm_popup(&a, Some(rect)));
        });
    if let Err(e) = spawned {
        eprintln!("arcturus app: không chạy được luồng prewarm: {e}");
        state.hover_pending.store(false, Ordering::SeqCst);
    }
}

/// Click: popup vừa hiện hoặc đóng theo ý người dùng, con trỏ vẫn nằm trên icon nên không tạo sẵn lại cho tới lần rê sau.
fn hover_clicked(app: &AppHandle) {
    bump_hover(app);
    if let Some(state) = app.try_state::<TrayState>() {
        state.hover_armed.store(false, Ordering::SeqCst);
    }
}

pub(crate) fn hover_leave(app: &AppHandle) {
    if let Some(state) = app.try_state::<TrayState>() {
        state.hover_armed.store(true, Ordering::SeqCst);
    }
    let gen = bump_hover(app);
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("prewarm-ttl".to_owned())
        .spawn(move || {
            std::thread::sleep(PREWARM_TTL);
            if hover_current(&app) != gen {
                return;
            }
            let a = app.clone();
            let _ = app.run_on_main_thread(move || drop_unused_prewarm(&a));
        });
    if let Err(e) = spawned {
        eprintln!("arcturus app: không chạy được luồng dọn prewarm: {e}");
    }
}

/// Dọn nếu popup tạo sẵn vẫn chưa ai hiện sau `PREWARM_MAX`, kể cả khi hệ thống không phát `Leave`.
fn schedule_prewarm_cap(app: &AppHandle) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let id = state.prewarm_id.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("prewarm-cap".to_owned())
        .spawn(move || {
            std::thread::sleep(PREWARM_MAX);
            let current = app
                .try_state::<TrayState>()
                .map_or(0, |s| s.prewarm_id.load(Ordering::SeqCst));
            if current != id {
                return;
            }
            let a = app.clone();
            let _ = app.run_on_main_thread(move || drop_unused_prewarm(&a));
        });
    if let Err(e) = spawned {
        eprintln!("arcturus app: không chạy được luồng giới hạn prewarm: {e}");
    }
}

/// Destroy popup tạo sẵn mà không ai hiện (RSS về mức Lite). Popup đang hiện thì giữ nguyên.
pub fn drop_unused_prewarm(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(POPUP) {
        if matches!(w.is_visible(), Ok(false)) {
            let _ = w.destroy();
            crate::measure::log("popup_prewarm_dropped", "");
        }
    }
}

/// Đặt popup sát icon tray: phía trên nếu khay ở nửa dưới màn hình (Windows), phía dưới nếu ở trên (macOS, Linux).
/// Không có vị trí icon: góc trên phải màn hình chính.
fn popup_position(app: &AppHandle, anchor: Option<Rect>) -> Option<PhysicalPosition<i32>> {
    let monitor = match &anchor {
        Some(r) => {
            let p = r.position.to_physical::<f64>(1.0);
            app.monitor_from_point(p.x, p.y).ok().flatten()
        }
        None => None,
    }
    .or_else(|| app.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let (mx, my) = (
        f64::from(monitor.position().x),
        f64::from(monitor.position().y),
    );
    let (mw, mh) = (
        f64::from(monitor.size().width),
        f64::from(monitor.size().height),
    );
    let (w, h, gap) = (POPUP_W * scale, POPUP_H * scale, 8.0 * scale);
    let (x, y) = match anchor {
        Some(r) => {
            let p = r.position.to_physical::<f64>(scale);
            let s = r.size.to_physical::<f64>(scale);
            let x = p.x + s.width / 2.0 - w / 2.0;
            let y = if p.y > my + mh / 2.0 {
                p.y - h - gap
            } else {
                p.y + s.height + gap
            };
            (x, y)
        }
        None => (mx + mw - w - 2.0 * gap, my + 5.0 * gap),
    };
    let x = x.clamp(mx, (mx + mw - w).max(mx));
    let y = y.clamp(my, (my + mh - h).max(my));
    Some(PhysicalPosition::new(x.round() as i32, y.round() as i32))
}

/// Mở (hoặc đưa lên trước) cửa sổ văn phòng (T13 làm nội dung).
pub fn open_office(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(OFFICE) {
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(
        app,
        OFFICE,
        WebviewUrl::App("index.html?view=office".into()),
    )
    .title("Arcturus — Văn phòng")
    .inner_size(900.0, 600.0)
    .build();
    if let Err(e) = built {
        eprintln!("arcturus app: không mở được văn phòng: {e}");
    }
}

/// Mở (hoặc đưa lên trước) cửa sổ cài đặt hook và tự kiểm tra (`setup.rs`). Đóng là hủy webview như văn phòng.
pub fn open_setup(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETUP) {
        let _ = w.set_focus();
        return;
    }
    let built =
        WebviewWindowBuilder::new(app, SETUP, WebviewUrl::App("index.html?view=setup".into()))
            .title("Arcturus — Cài đặt")
            .inner_size(720.0, 640.0)
            .build();
    if let Err(e) = built {
        eprintln!("arcturus app: không mở được cửa sổ cài đặt: {e}");
    }
}

pub fn menu_id(event: &MenuEvent) -> &str {
    event.id().as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prewarm_only_when_armed_idle_and_no_popup() {
        assert!(hover_wants_prewarm(true, false, false));
        assert!(
            !hover_wants_prewarm(false, false, false),
            "vừa bấm: chờ lần rê sau"
        );
        assert!(
            !hover_wants_prewarm(true, true, false),
            "đã có lượt hẹn đang chờ"
        );
        assert!(!hover_wants_prewarm(true, false, true), "đã có popup");
    }

    #[test]
    fn ready_after_click_reports_time_since_click() {
        let mut p = PopupTimes {
            started: Some(Instant::now()),
            ..PopupTimes::default()
        };
        assert!(p.ui_ready().is_some());
        assert!(p.started.is_none(), "chỉ báo một lần cho mỗi lần bấm");
        assert!(!p.ready_early);
    }

    #[test]
    fn ready_before_click_is_reported_when_shown() {
        let mut p = PopupTimes::default();
        assert_eq!(p.ui_ready(), None, "chưa bấm: chỉ nhớ lại");
        assert!(p.ready_early);
        p.started = Some(Instant::now());
        assert!(
            p.shown().is_some(),
            "đã vẽ xong từ trước: tính tới lúc hiện"
        );
        assert_eq!(p.shown(), None, "một lần");
    }

    #[test]
    fn shown_without_early_ready_waits_for_ui() {
        let mut p = PopupTimes {
            started: Some(Instant::now()),
            ..PopupTimes::default()
        };
        assert_eq!(p.shown(), None);
        assert!(p.started.is_some(), "UI chưa vẽ xong: vẫn chờ báo");
    }

    #[test]
    fn dot_drawn_top_right_only() {
        let (w, h) = (32u32, 32u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        draw_dot(&mut px, w, h);
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2], px[i + 3]]
        };
        assert_eq!(at(23, 9), [0xFF, 0xB0, 0x20, 0xFF], "tâm chấm");
        assert_eq!(at(2, 2), [0, 0, 0, 0], "góc trên trái không đổi");
        assert_eq!(at(16, 30), [0, 0, 0, 0], "đáy không đổi");
        // Icon lệch kích thước cũng không panic.
        let mut tiny = vec![0u8; 3 * 3 * 4];
        draw_dot(&mut tiny, 3, 3);
    }

    #[test]
    fn tooltip_texts() {
        let b = Badge {
            pending: 3,
            in_terminal: 1,
            paused: true,
            connected: true,
            ..Badge::default()
        };
        assert_eq!(
            tooltip(&b, None),
            "Arcturus: 3 yêu cầu đang chờ, 1 đang duyệt trong terminal (đội đang tạm dừng)"
        );
        assert_eq!(
            tooltip(
                &Badge {
                    connected: true,
                    ..Badge::default()
                },
                None
            ),
            "Arcturus: không có yêu cầu nào"
        );
        // Giờ tập trung: nói rõ có yêu cầu bị ẩn, để chấm báo không làm người dùng tưởng hàng đợi trống.
        let focus = Badge {
            quiet: 2,
            focus: true,
            connected: true,
            ..Badge::default()
        };
        assert_eq!(
            tooltip(&focus, None),
            "Arcturus: không có yêu cầu nào cần chú ý (giờ tập trung: ẩn 2 yêu cầu rủi ro thấp/vừa)"
        );
        assert_eq!(
            tooltip(
                &Badge {
                    pending: 1,
                    quiet: 0,
                    ..focus
                },
                None
            ),
            "Arcturus: 1 yêu cầu đang chờ (giờ tập trung)"
        );
        assert_eq!(
            tooltip(&Badge::default(), Some("hub không khởi động được")),
            "Arcturus: hub không khởi động được"
        );
    }
}
