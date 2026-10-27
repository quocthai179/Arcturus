//! Arcturus hub-core: schema sự kiện, state machine agent, policy, lưu trữ SQLite.
//!
//! Các module được thêm theo task trong `docs/PLAN.md` (T2 schema + hook, T4 state, T6 store, T8 policy…).

pub mod config;
pub mod hook;
pub mod persona;
pub mod policy;
pub mod protocol;
pub mod schema;
pub mod standup;
pub mod state;
pub mod store;

pub use schema::SCHEMA_VERSION;
