//! Daemon Arcturus: nhận hook Claude Code, giữ yêu cầu duyệt, WebSocket cho UI (`docs/PLAN.md` §3.3, §4).
//!
//! T1 mới có chế độ ghi băng (`--record`). Luồng duyệt thật được thêm ở T3, WebSocket ở T5.

pub mod auth;
pub mod record;
pub mod server;
