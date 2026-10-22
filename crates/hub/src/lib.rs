//! Daemon Arcturus: nhận hook Claude Code, giữ yêu cầu duyệt, WebSocket cho UI (`docs/PLAN.md` §3.3, §4).
//!
//! T1: chế độ ghi băng (`--record`). T3: luồng duyệt (`approvals`, `hooks`). T5a: `hub` nối luồng duyệt với trạng
//! thái agent, cấu hình và kênh broadcast; `view` dựng thẻ duyệt. WebSocket ở T5b.

pub mod approvals;
pub mod auth;
pub mod hooks;
pub mod hub;
pub mod record;
pub mod server;
pub mod view;
