//! Tray, badge và popup Lite (docs/PLAN.md T12).
//!
//! - Không mở cửa sổ khi khởi động; chỉ có tray.
//! - Click trái tray (Windows, macOS): chưa có popup thì tạo (380×560, không viền, luôn ở trên, gần khay); có rồi thì
//!   **destroy** (không hide, để RSS về mức Lite). Mất focus cũng destroy. Linux (appindicator) không báo click: mở
//!   bằng mục menu "Mở hàng đợi duyệt".
//! - Badge: macOS dùng tiêu đề tray (số); Windows/Linux đổi giữa icon thường / icon có chấm, kèm tooltip.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindowBuilder, WindowEvent, Wry,
};

use crate::link::Badge;

pub const TRAY_ID: &str = "arcturus";
pub const POPUP: &str = "popup";
pub const OFFICE: &str = "office";
const POPUP_W: f64 = 380.0;
const POPUP_H: f64 = 560.0;
/// Mất focus ngay sau khi mở (trước khi cửa sổ kịp nhận focus) không tính là đóng.
const FOCUS_GRACE: Duration = Duration::from_millis(400);
/// Click vào tray làm popup mất focus rồi mới tới sự kiện click: không mở lại popup vừa đóng bởi chính cú click đó.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

pub const MENU_OPEN_QUEUE: &str = "open_queue";
pub const MENU_OPEN_OFFICE: &str = "open_office";
pub const MENU_TOGGLE_PAUSE: &str = "toggle_pause";
pub const MENU_TOGGLE_FOCUS: &str = "toggle_focus";
pub const MENU_QUIT: &str = "quit";

/// Phần trạng thái của tray trong app.
pub struct TrayState {
    pub tray: TrayIcon<Wry>,
    pub pause_item: MenuItem<Wry>,
    pub focus_item: MenuItem<Wry>,
    icon_plain: Image<'static>,
    icon_dot: Image<'static>,
    popup: Mutex<PopupTimes>,
    last_badge: Mutex<Option<Badge>>,
}

#[derive(Default)]
struct PopupTimes {
    opened: Option<Instant>,
    closed: Option<Instant>,
    /// Lúc bắt đầu mở cửa sổ (trước khi tạo webview), để đo "popup mở" (T16). Lấy đi một lần khi UI báo xong.
    started: Option<Instant>,
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
    let quit = MenuItem::with_id(app, MENU_QUIT, "Thoát", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open_queue, &open_office, &pause_item, &focus_item, &quit],
    )?;

    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(base.clone())
        .tooltip(tooltip(&Badge::default(), None))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_popup(tray.app_handle(), Some(rect));
            }
        })
        .build(app)?;

    Ok(TrayState {
        tray,
        pause_item,
        focus_item,
        icon_plain: base,
        icon_dot,
        popup: Mutex::new(PopupTimes::default()),
        last_badge: Mutex::new(None),
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

    /// Mili giây từ lúc bắt đầu mở popup tới giờ; trả một lần cho mỗi lần mở.
    pub fn take_popup_elapsed_ms(&self) -> Option<u128> {
        self.popup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .started
            .take()
            .map(|t| t.elapsed().as_millis())
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

/// Đóng popup nếu đang mở, mở nếu chưa có.
pub fn toggle_popup(app: &AppHandle, anchor: Option<Rect>) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if let Some(w) = app.get_webview_window(POPUP) {
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

fn open_popup(app: &AppHandle, state: &TrayState, anchor: Option<Rect>) -> tauri::Result<()> {
    state.mark_started();
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
    state.mark_opened();
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            let Some(state) = handle.try_state::<TrayState>() else {
                return;
            };
            if state.in_focus_grace() {
                return;
            }
            if let Some(w) = handle.get_webview_window(POPUP) {
                state.mark_closed();
                let _ = w.destroy();
            }
        }
    });
    window.show()?;
    window.set_focus()?;
    if let Some(ms) = state.popup_started_ms() {
        crate::measure::log("popup_shown", &format!("ms={ms}"));
    }
    Ok(())
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

pub fn menu_id(event: &MenuEvent) -> &str {
    event.id().as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

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
