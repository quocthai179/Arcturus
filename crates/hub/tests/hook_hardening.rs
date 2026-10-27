//! Cổng v0 (T21 · ARC-42), phát hiện 7: route hook kiểm `Host` loopback; hàng đợi `/hooks/event` có ngân sách byte.
//!
//! Token hook nằm trong môi trường của agent (không tin cậy), nên hai chỗ này là lớp phòng thủ thêm: `Host` lạ → 403
//! (DNS rebinding), và gửi dồn sự kiện lớn không được đẩy RSS của hub qua ngân sách (CLAUDE.md quy tắc 6).

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use hub::approvals::{AlwaysAsk, ApprovalsConfig, NoStore};
use hub::hub::{Hub, EVENT_QUEUE_MAX_BYTES};
use hub::{hooks, server};
use hub_core::config::Config;
use hub_core::hook::HookInput;
use serde_json::json;
use tokio::sync::oneshot;

const TOKEN: &str = "test-token-0123456789abcdef-arcturus-t21";

fn start_hub() -> (std::sync::Arc<Hub>, hub::hub::HubTasks) {
    let cfg = Config::default();
    let personas = cfg.persona_book().unwrap();
    Hub::start_with(
        cfg,
        personas,
        "laptop".to_owned(),
        ApprovalsConfig {
            min_review: Duration::ZERO,
            ..ApprovalsConfig::default()
        },
        Box::new(AlwaysAsk),
        Box::new(NoStore),
    )
}

fn prompt_event(i: usize) -> HookInput {
    HookInput::new(json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": format!("sess-{i}"),
        "cwd": "/w/proj",
        "prompt": "xin chao",
    }))
}

#[tokio::test]
async fn hook_routes_reject_a_foreign_host_header() {
    let (core, _tasks) = start_hub();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = hooks::router_on_port(core.clone(), TOKEN.to_owned(), addr.port());
    let (stop, stop_rx) = oneshot::channel::<()>();
    let c = core.clone();
    let server = tokio::spawn(async move {
        server::serve_until(listener, app, async move {
            let _ = stop_rx.await;
            c.shutdown();
        })
        .await
        .unwrap();
    });

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/hooks/event");
    let body = json!({ "hook_event_name": "UserPromptSubmit", "session_id": "s", "cwd": "/w" });
    let post = |host: Option<&str>| {
        let mut req = client.post(&url).bearer_auth(TOKEN).json(&body);
        if let Some(h) = host {
            req = req.header("Host", h);
        }
        req.send()
    };

    assert_eq!(post(None).await.unwrap().status(), 204, "Host thật của hub");
    assert_eq!(
        post(Some(&format!("localhost:{}", addr.port())))
            .await
            .unwrap()
            .status(),
        204
    );
    for bad in ["evil.example", "evil.example:80", "127.0.0.1:1"] {
        assert_eq!(
            post(Some(bad)).await.unwrap().status(),
            403,
            "Host {bad} phải bị từ chối"
        );
    }

    let _ = stop.send(());
    let _ = server.await;
}

#[tokio::test]
async fn event_queue_drops_events_over_the_byte_budget_and_recovers() {
    let (core, _tasks) = start_hub();
    let big = EVENT_QUEUE_MAX_BYTES * 3 / 8; // 3 mục = 9/8 trần: mục thứ ba phải bị bỏ.

    // Đơn luồng và không `await` giữa các lần gọi: vòng sự kiện chưa chạy, nên cả ba cùng nằm trong hàng đợi.
    for i in 0..3 {
        core.enqueue_event(prompt_event(i), big);
    }
    for _ in 0..200 {
        if core.recent_events(10).len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        core.recent_events(10).len(),
        2,
        "mục thứ ba vượt ngân sách byte phải bị bỏ"
    );

    // Hàng đợi đã rỗng: bộ đếm byte phải về 0, nhận lại bình thường (kể cả mục lớn hơn trần).
    core.enqueue_event(prompt_event(3), EVENT_QUEUE_MAX_BYTES * 10);
    for _ in 0..200 {
        if core.recent_events(10).len() >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        core.recent_events(10).len(),
        3,
        "sau khi nhả byte phải nhận lại"
    );
}
