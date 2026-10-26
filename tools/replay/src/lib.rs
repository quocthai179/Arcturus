//! Phát lại fixture hook vào hub Arcturus (`docs/PLAN.md` T15): thư viện cho binary `replay` và cho test tích hợp.
//!
//! - [`files`]: liệt kê fixture `.json`.
//! - [`send`]: gửi fixture tới `/hooks/event` và `/hooks/permission` với số phiên, tốc độ, thời lượng tùy chọn, đo độ trễ.
//! - `scrub` (chỉ trong binary): làm sạch bản ghi băng thô trước khi đưa vào `fixtures/hooks/`.

pub mod files;
pub mod send;
