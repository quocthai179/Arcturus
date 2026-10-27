//! `scripts/metrics.sql` (T20 · ARC-41) chạy trên cơ sở dữ liệu dựng bằng migration thật của `Store`, với dữ liệu giả lập có
//! đáp số tính tay.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use hub_core::store::Store;
use rusqlite::types::Value;
use rusqlite::{params, Connection};

const METRICS_SQL: &str = include_str!("../../../scripts/metrics.sql");

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("arcturus-metrics-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Một dòng kết quả: tên cột → giá trị.
struct Row(Vec<(String, Value)>);

impl Row {
    fn get(&self, name: &str) -> &Value {
        &self.0.iter().find(|(n, _)| n == name).unwrap().1
    }

    fn num(&self, name: &str) -> f64 {
        match self.get(name) {
            Value::Integer(i) => *i as f64,
            Value::Real(r) => *r,
            other => panic!("{name} không phải số: {other:?}"),
        }
    }

    fn text(&self, name: &str) -> String {
        match self.get(name) {
            Value::Text(s) => s.clone(),
            other => panic!("{name} không phải chuỗi: {other:?}"),
        }
    }
}

/// Tách `scripts/metrics.sql` thành các truy vấn theo dòng `-- @query <tên>`.
fn queries() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in METRICS_SQL.lines() {
        if let Some(name) = line.strip_prefix("-- @query ") {
            out.push((name.trim().to_owned(), String::new()));
        } else if let Some((_, sql)) = out.last_mut() {
            sql.push_str(line);
            sql.push('\n');
        }
    }
    out
}

fn run(conn: &Connection, name: &str) -> Vec<Row> {
    let (_, sql) = queries()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("không có truy vấn {name}"));
    let mut stmt = conn.prepare(&sql).unwrap();
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    stmt.query_map([], |r| {
        let mut cols = Vec::new();
        for (i, n) in names.iter().enumerate() {
            cols.push((n.clone(), r.get::<_, Value>(i)?));
        }
        Ok(Row(cols))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn approval(
    c: &Connection,
    id: &str,
    session: &str,
    risk: &str,
    status: &str,
    by: Option<&str>,
    created: &str,
    latency: Option<i64>,
) {
    c.execute(
        "INSERT INTO approvals (request_id, session_id, agent_id, tool, command_original, command_normalized, risk,
           group_key, status, decided_by, created_at, decided_at, latency_ms, channel)
         VALUES (?1, ?2, 'main', 'Bash', 'npm test', 'npm test', ?3, 'g', ?4, ?5, ?6, ?6, ?7, NULL)",
        params![id, session, risk, status, by, created, latency],
    )
    .unwrap();
}

fn event(c: &Connection, id: &str, session: &str, kind: &str, ts: &str) {
    c.execute(
        "INSERT INTO events (id, ts, session_id, kind, event_json) VALUES (?1, ?2, ?3, ?4, '{}')",
        params![id, ts, session, kind],
    )
    .unwrap();
}

fn seeded() -> (TempDir, Connection) {
    let dir = TempDir::new();
    let db = dir.0.join("arcturus.db");
    Store::open(&db, 30).unwrap().close(); // chạy migration thật
    let c = Connection::open(&db).unwrap();

    // Ngày 2026-10-26: 5 yêu cầu.
    approval(
        &c,
        "a1",
        "s1",
        "low",
        "allowed",
        Some("user"),
        "2026-10-26T10:00:10.000Z",
        Some(2000),
    );
    approval(
        &c,
        "a2",
        "s1",
        "medium",
        "allowed",
        Some("user"),
        "2026-10-26T10:00:30.000Z",
        Some(4000),
    );
    approval(
        &c,
        "a3",
        "s3",
        "medium",
        "denied",
        Some("user"),
        "2026-10-26T11:00:00.000Z",
        Some(6000),
    );
    approval(
        &c,
        "a4",
        "s3",
        "low",
        "allowed",
        Some("policy"),
        "2026-10-26T11:05:00.000Z",
        Some(5),
    );
    approval(
        &c,
        "a5",
        "s3",
        "medium",
        "timed_out",
        Some("timeout"),
        "2026-10-26T12:00:00.000Z",
        Some(110_000),
    );
    // Ngày 2026-10-27: 2 yêu cầu.
    approval(
        &c,
        "a6",
        "s4",
        "low",
        "allowed",
        Some("policy"),
        "2026-10-27T09:00:00.000Z",
        Some(3),
    );
    approval(
        &c,
        "a7",
        "s4",
        "high",
        "handed_off",
        None,
        "2026-10-27T09:01:00.000Z",
        None,
    );

    // Lượt của s1: 100 giây, trong đó a1 (2 s) và a2 (4 s) chờ duyệt. Lượt của s2: 20 giây, không chờ.
    event(
        &c,
        "e1",
        "s1",
        "prompt.submitted",
        "2026-10-26T10:00:00.000Z",
    );
    event(&c, "e2", "s1", "tool.pre", "2026-10-26T10:00:05.000Z");
    event(&c, "e3", "s1", "turn.stopped", "2026-10-26T10:01:40.000Z");
    event(
        &c,
        "e4",
        "s2",
        "prompt.submitted",
        "2026-10-26T10:10:00.000Z",
    );
    event(&c, "e5", "s2", "turn.stopped", "2026-10-26T10:10:20.000Z");
    // Không có prompt.submitted ngay trước: không phải một lượt.
    event(&c, "e6", "s5", "turn.stopped", "2026-10-26T13:00:00.000Z");
    (dir, c)
}

#[test]
fn every_query_in_metrics_sql_runs_on_the_real_schema() {
    let (_dir, c) = seeded();
    let names: Vec<String> = queries().into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names,
        [
            "per_day",
            "total",
            "auto_approved_risk",
            "auto_approved_dangerous",
            "agent_self_run"
        ]
    );
    for n in &names {
        run(&c, n); // không panic = cú pháp và tên cột đúng
    }
}

#[test]
fn per_day_counts_waits_policy_share_denies_and_timeouts() {
    let (_dir, c) = seeded();
    let rows = run(&c, "per_day");
    assert_eq!(rows.len(), 2);

    let d26 = &rows[0];
    assert_eq!(d26.text("day"), "2026-10-26");
    assert_eq!(d26.num("requests"), 5.0);
    assert_eq!(d26.num("decided"), 4.0);
    // (2000 + 4000 + 6000 + 5) / 4 ms = 3,00125 s: hết giờ (110 s) không đưa vào trung bình.
    assert_eq!(d26.num("avg_wait_all_s"), 3.0);
    assert_eq!(d26.num("avg_wait_user_s"), 4.0);
    assert_eq!(d26.num("max_wait_user_s"), 6.0);
    assert_eq!(d26.num("policy_pct"), 25.0);
    assert_eq!(d26.num("denied"), 1.0);
    assert_eq!(d26.num("timed_out"), 1.0);

    let d27 = &rows[1];
    assert_eq!(d27.text("day"), "2026-10-27");
    assert_eq!(d27.num("requests"), 2.0);
    assert_eq!(d27.num("decided"), 1.0);
    assert_eq!(d27.num("policy_pct"), 100.0);
    assert_eq!(d27.num("handed_off"), 1.0);
    // Ngày chỉ có quyết định của policy: không có người duyệt thì trung bình người là NULL, không phải 0.
    assert!(matches!(d27.get("avg_wait_user_s"), Value::Null));
}

#[test]
fn total_row_summarises_everything() {
    let (_dir, c) = seeded();
    let rows = run(&c, "total");
    assert_eq!(rows.len(), 1);
    let t = &rows[0];
    assert_eq!(t.num("requests"), 7.0);
    assert_eq!(t.num("decided"), 5.0);
    assert_eq!(t.num("denied"), 1.0);
    assert_eq!(t.num("timed_out"), 1.0);
    assert_eq!(t.num("days_with_data"), 2.0);
    assert_eq!(t.num("policy_pct"), 40.0);
}

#[test]
fn empty_database_gives_no_rows_and_no_division_by_zero() {
    let dir = TempDir::new();
    let db = dir.0.join("arcturus.db");
    Store::open(&db, 30).unwrap().close();
    let c = Connection::open(&db).unwrap();
    for n in [
        "per_day",
        "auto_approved_risk",
        "auto_approved_dangerous",
        "agent_self_run",
    ] {
        assert!(run(&c, n).is_empty(), "{n}");
    }
    let t = &run(&c, "total")[0];
    assert_eq!(t.num("requests"), 0.0);
    assert!(matches!(t.get("policy_pct"), Value::Null));
}

#[test]
fn auto_approved_dangerous_list_catches_a_high_risk_policy_allow() {
    let (_dir, c) = seeded();
    // Dữ liệu hợp lệ: policy chỉ tự duyệt mức thấp, danh sách nguy hiểm rỗng.
    assert!(run(&c, "auto_approved_dangerous").is_empty());
    let by_risk = run(&c, "auto_approved_risk");
    assert_eq!(by_risk.len(), 1);
    assert_eq!(by_risk[0].text("risk"), "low");
    assert_eq!(by_risk[0].num("auto_allowed"), 2.0);

    // Một lần tự duyệt sai (vi phạm quy tắc bất biến 2) phải hiện ra.
    approval(
        &c,
        "bad",
        "s6",
        "critical",
        "allowed",
        Some("policy"),
        "2026-10-27T10:00:00.000Z",
        Some(1),
    );
    let bad = run(&c, "auto_approved_dangerous");
    assert_eq!(bad.len(), 1);
    assert_eq!(bad[0].text("request_id"), "bad");
    assert_eq!(
        run(&c, "auto_approved_risk")[0].text("risk"),
        "critical",
        "mức nặng nhất xếp đầu"
    );
}

#[test]
fn agent_self_run_subtracts_approval_waits_and_ignores_orphan_stops() {
    let (_dir, c) = seeded();
    let rows = run(&c, "agent_self_run");
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.text("day"), "2026-10-26");
    assert_eq!(r.num("turns"), 2.0, "turn.stopped mồ côi không tính");
    assert_eq!(r.num("avg_turn_s"), 60.0); // (100 + 20) / 2
    assert_eq!(r.num("avg_wait_s"), 3.0); // (2 + 4 + 0) / 2
    assert_eq!(r.num("avg_self_run_s"), 57.0); // (94 + 20) / 2
}

#[test]
fn self_run_never_goes_negative_when_parallel_waits_exceed_the_turn() {
    let dir = TempDir::new();
    let db = dir.0.join("arcturus.db");
    Store::open(&db, 30).unwrap().close();
    let c = Connection::open(&db).unwrap();
    event(
        &c,
        "p",
        "s1",
        "prompt.submitted",
        "2026-10-26T10:00:00.000Z",
    );
    event(&c, "t", "s1", "turn.stopped", "2026-10-26T10:00:10.000Z");
    // Ba yêu cầu song song, mỗi cái chờ 8 s: tổng chờ 24 s > lượt 10 s.
    for i in 0..3 {
        approval(
            &c,
            &format!("x{i}"),
            "s1",
            "low",
            "allowed",
            Some("user"),
            "2026-10-26T10:00:02.000Z",
            Some(8000),
        );
    }
    let r = &run(&c, "agent_self_run")[0];
    assert_eq!(r.num("avg_wait_s"), 24.0);
    assert_eq!(r.num("avg_self_run_s"), 0.0);
}
