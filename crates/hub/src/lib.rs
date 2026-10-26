//! Daemon Arcturus: nhận hook Claude Code, giữ yêu cầu duyệt, WebSocket cho UI (`docs/PLAN.md` §3.3, §4).
//!
//! T1: chế độ ghi băng (`--record`). T3: luồng duyệt (`approvals`, `hooks`). T5a: `hub` nối luồng duyệt với trạng
//! thái agent, cấu hình và kênh broadcast; `view` dựng thẻ duyệt. T5b: `ws` (WebSocket, `/approvals/{id}/input`) với
//! token UI riêng (`ui_token`).

pub mod approvals;
pub mod auth;
pub mod hooks;
pub mod hub;
pub mod reconcile;
pub mod record;
pub mod server;
pub mod ui_token;
pub mod view;
pub mod ws;
