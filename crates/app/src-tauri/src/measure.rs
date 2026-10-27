//! Chế độ đo popup (`docs/PLAN.md` T16 · ARC-37). Chỉ bật khi có biến môi trường `ARCTURUS_MEASURE_POPUP=<số lần>`;
//! `scripts/measure.ps1 -Target app` đặt biến này để đo thời gian mở popup và bộ nhớ mà không cần người bấm tray.
//!
//! Chuỗi hành động (mọi mốc ghi một dòng `[measure] t=<unix ms> event=<tên> ...` ra stderr):
//! 1. chờ `ARCTURUS_MEASURE_DELAY_SEC` giây (mặc định 30) ở trạng thái tray-only;
//! 2. `cycle_start`, rồi `N` lần: (nếu `ARCTURUS_MEASURE_HOVER_MS` > 0: tạo sẵn popup ẩn như khi con trỏ dừng trên icon và
//!    chờ chừng đó) mở popup, chờ 2 s, đóng, chờ 1,5 s;
//! 3. `hold_start`: mở popup và giữ `ARCTURUS_MEASURE_HOLD_SEC` giây (mặc định 20); `hold_end`: đóng;
//! 4. `done`. App vẫn chạy tiếp để script đo bộ nhớ sau khi đóng.
//!
//! Mỗi lần mở, UI gọi `popup_ready` khi React đã dựng xong khung đầu tiên (một `requestAnimationFrame` sau khi mount) và
//! app ghi `event=popup_ready ms=<từ lúc bắt đầu mở cửa sổ>`: đây là số đo của tiêu chí "popup mở ≤ 300 ms". Ngoài chế độ
//! đo, `popup_ready` không làm gì.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, PhysicalPosition, PhysicalSize, Position, Rect, Size};

use crate::tray;

static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub cycles: u32,
    pub delay: Duration,
    pub hold: Duration,
    /// Mô phỏng "con trỏ dừng trên icon" trước khi bấm: tạo sẵn popup rồi chờ chừng này. 0 = không (đường nguội).
    pub hover: Duration,
}

/// Đọc cấu hình từ biến môi trường (`get` tách ra để test). `None` nếu không bật hoặc số lần không hợp lệ.
pub fn config_from(get: impl Fn(&str) -> Option<String>) -> Option<Config> {
    let cycles: u32 = get("ARCTURUS_MEASURE_POPUP")?.trim().parse().ok()?;
    if cycles == 0 || cycles > 100 {
        return None;
    }
    let secs = |name: &str, default: u64, max: u64| -> Duration {
        let v = get(name)
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(default);
        Duration::from_secs(v.min(max))
    };
    Some(Config {
        cycles,
        delay: secs("ARCTURUS_MEASURE_DELAY_SEC", 30, 600),
        hold: secs("ARCTURUS_MEASURE_HOLD_SEC", 20, 600),
        hover: Duration::from_millis(
            get("ARCTURUS_MEASURE_HOVER_MS")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(0)
                .min(5000),
        ),
    })
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Ghi một mốc đo (không làm gì ngoài chế độ đo).
pub fn log(event: &str, extra: &str) {
    if enabled() {
        eprintln!("[measure] t={} event={event} {extra}", unix_ms());
    }
}

/// Bật chế độ đo và chạy chuỗi hành động ở một thread riêng. Việc mở/đóng cửa sổ chạy trên thread chính của Tauri.
pub fn start(app: AppHandle, cfg: Config) {
    ENABLED.store(true, Ordering::Relaxed);
    let spawned = std::thread::Builder::new()
        .name("measure".to_owned())
        .spawn(move || run(&app, cfg));
    if let Err(e) = spawned {
        eprintln!("arcturus app: không chạy được chế độ đo: {e}");
    }
}

/// Một icon tray giả ở góc màn hình, đủ để chạy đúng đường xử lý rê chuột của tray.
fn fake_icon_rect() -> Rect {
    Rect {
        position: Position::Physical(PhysicalPosition::new(0, 0)),
        size: Size::Physical(PhysicalSize::new(16, 16)),
    }
}

fn toggle(app: &AppHandle) {
    let a = app.clone();
    if app
        .run_on_main_thread(move || tray::toggle_popup(&a, None))
        .is_err()
    {
        eprintln!("arcturus app: không đưa được thao tác popup lên thread chính");
    }
}

/// Con trỏ dừng trên icon (đi qua `hover_enter` thật: dwell, hủy khi bấm, trần thời gian) rồi chờ `cfg.hover` trước khi bấm.
fn hover(app: &AppHandle, cfg: Config) {
    if !cfg.hover.is_zero() {
        tray::hover_enter(app, fake_icon_rect(), true);
        std::thread::sleep(cfg.hover);
    }
}

/// Rê vào icon rồi rời đi mà không bấm: popup tạo sẵn phải bị destroy (sự kiện `popup_prewarm_dropped`).
fn abandon(app: &AppHandle) {
    log("abandon_start", "");
    tray::hover_enter(app, fake_icon_rect(), true);
    std::thread::sleep(Duration::from_secs(1));
    tray::hover_leave(app);
    std::thread::sleep(Duration::from_secs(5));
    log("abandon_end", "");
}

fn run(app: &AppHandle, cfg: Config) {
    log("armed", &format!("cycles={}", cfg.cycles));
    std::thread::sleep(cfg.delay);
    log("cycle_start", "");
    for i in 0..cfg.cycles {
        log("popup_open", &format!("n={}", i + 1));
        hover(app, cfg);
        toggle(app);
        std::thread::sleep(Duration::from_secs(2));
        toggle(app);
        log("popup_closed", &format!("n={}", i + 1));
        // Cú đóng vừa rồi chặn mở lại trong REOPEN_GUARD (400 ms); chờ lâu hơn hẳn.
        std::thread::sleep(Duration::from_millis(1500));
    }
    if !cfg.hover.is_zero() {
        abandon(app);
    }
    log("popup_open", "n=hold");
    hover(app, cfg);
    toggle(app);
    std::thread::sleep(Duration::from_secs(2));
    log("hold_start", "");
    std::thread::sleep(cfg.hold);
    toggle(app);
    log("hold_end", "");
    log("done", "");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn off_unless_cycles_is_a_sane_number() {
        assert_eq!(config_from(env(&[])), None);
        for bad in ["", "0", "abc", "-1", "101", "1.5"] {
            assert_eq!(
                config_from(env(&[("ARCTURUS_MEASURE_POPUP", bad)])),
                None,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn defaults_and_overrides_with_caps() {
        let c = config_from(env(&[("ARCTURUS_MEASURE_POPUP", " 5 ")])).unwrap();
        assert_eq!(
            c,
            Config {
                cycles: 5,
                delay: Duration::from_secs(30),
                hold: Duration::from_secs(20),
                hover: Duration::ZERO
            }
        );
        let c = config_from(env(&[
            ("ARCTURUS_MEASURE_POPUP", "3"),
            ("ARCTURUS_MEASURE_DELAY_SEC", "5"),
            ("ARCTURUS_MEASURE_HOLD_SEC", "99999"),
        ]))
        .unwrap();
        assert_eq!(c.delay, Duration::from_secs(5));
        assert_eq!(c.hold, Duration::from_secs(600), "trần 600 giây");
        let c = config_from(env(&[
            ("ARCTURUS_MEASURE_POPUP", "2"),
            ("ARCTURUS_MEASURE_HOVER_MS", "500"),
        ]))
        .unwrap();
        assert_eq!(c.hover, Duration::from_millis(500));
        // Giá trị hỏng rơi về mặc định.
        let c = config_from(env(&[
            ("ARCTURUS_MEASURE_POPUP", "1"),
            ("ARCTURUS_MEASURE_DELAY_SEC", "x"),
        ]))
        .unwrap();
        assert_eq!(c.delay, Duration::from_secs(30));
    }

    #[test]
    fn logging_is_silent_when_disabled() {
        // Không bật chế độ đo thì `log` không ghi gì (không panic, không cần stderr).
        assert!(!enabled());
        log("x", "y");
    }
}
