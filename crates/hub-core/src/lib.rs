//! Arcturus hub-core: schema sự kiện, state machine agent, policy, lưu trữ SQLite.
//!
//! Các module được thêm theo task trong `docs/PLAN.md` (T2 schema, T4 state, T6 store, T8 policy…).

/// Phiên bản schema sự kiện (`docs/PLAN.md` §3.1). Tăng khi thay đổi phá tương thích.
pub const SCHEMA_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_is_one() {
        assert_eq!(SCHEMA_VERSION, 1);
    }
}
