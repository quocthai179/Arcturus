//! Daemon Arcturus. T0 chỉ là khung: server HTTP/WebSocket được thêm ở T1 (ghi băng) và T3/T5.

fn main() {
    println!("{}", banner());
}

fn banner() -> String {
    format!(
        "arcturus hub {} (schema v{}): chưa có server, xem T1/T3 trong .squad/TASKS.md",
        env!("CARGO_PKG_VERSION"),
        hub_core::SCHEMA_VERSION
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_names_version_and_schema() {
        let b = banner();
        assert!(b.contains(env!("CARGO_PKG_VERSION")));
        assert!(b.contains("schema v1"));
    }
}
