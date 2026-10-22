//! Daemon Arcturus: nhận hook Claude Code, giữ yêu cầu duyệt, WebSocket cho UI (`docs/PLAN.md` §3.3, §4).
//!
//! T1: chế độ ghi băng (`--record`). T3: luồng duyệt (`approvals`, `hooks`). WebSocket ở T5.

pub mod approvals;
pub mod auth;
pub mod hooks;
pub mod record;
pub mod server;
