//! Chạy `hook::to_event` trên toàn bộ fixture hook thật trong `fixtures/hooks/` (T1) — `docs/PLAN.md` T2.

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hub_core::hook::{self, HookInput};
use hub_core::schema::{AgentKind, Event, EventKind, SCHEMA_VERSION};
use time::macros::datetime;
use time::OffsetDateTime;

const NOW: OffsetDateTime = datetime!(2026-10-21 09:15:02 UTC);

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/hooks")
}

/// (tên thư mục sự kiện, đường dẫn file) cho mọi `.json`, sắp xếp ổn định.
fn all_fixtures() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for dir in std::fs::read_dir(fixtures_root()).unwrap() {
        let dir = dir.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        for f in std::fs::read_dir(&dir).unwrap() {
            let f = f.unwrap().path();
            if f.extension().is_some_and(|e| e == "json") {
                out.push((name.clone(), f));
            }
        }
    }
    out.sort();
    out
}

fn load(path: &Path) -> HookInput {
    HookInput::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn every_fixture_parses_to_event() {
    let fixtures = all_fixtures();
    assert!(fixtures.len() >= 30, "có {} fixture", fixtures.len());

    for (dir, path) in &fixtures {
        let input = load(path);
        let ev = hook::to_event(&input, "laptop", NOW);

        assert_eq!(ev.v, SCHEMA_VERSION);
        assert!(ev.id.starts_with("evt_"), "{path:?}");
        assert_eq!(ev.ts, NOW);
        assert_ne!(
            ev.kind,
            EventKind::Unknown,
            "{dir} phải được nhận diện: {path:?}"
        );
        assert_ne!(ev.session_id, hook::UNKNOWN_SESSION, "{path:?}");
        assert!(ev.agent.id.starts_with(&format!("agt_{}", ev.session_id)));
        assert!(ev.approval.is_none(), "to_event không tự tạo yêu cầu duyệt");
        if let Some(tool) = &ev.tool {
            assert!(tool.summary.chars().count() <= hook::SUMMARY_MAX_CHARS);
            assert!(tool.risk.is_none(), "chỉ policy được điền risk");
        }

        // Serde hai chiều không mất gì.
        let json = serde_json::to_string(&ev).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ev, "{path:?}");
    }
}

#[test]
fn tool_events_carry_tool_info() {
    for (dir, path) in all_fixtures() {
        if matches!(
            dir.as_str(),
            "PreToolUse" | "PostToolUse" | "PostToolUseFailure" | "PermissionRequest"
        ) {
            let ev = hook::to_event(&load(&path), "laptop", NOW);
            let tool = ev.tool.unwrap_or_else(|| panic!("{path:?} thiếu tool"));
            assert!(!tool.name.is_empty());
            assert!(!tool.summary.is_empty());
        }
    }
}

#[test]
fn subagent_fixtures_map_to_child_agent() {
    let mut seen = 0;
    for (_, path) in all_fixtures() {
        let input = load(&path);
        let Some(agent_id) = input.agent_id() else {
            continue;
        };
        seen += 1;
        let ev = hook::to_event(&input, "laptop", NOW);
        assert_eq!(ev.agent.kind, AgentKind::Subagent, "{path:?}");
        assert_eq!(ev.agent.id, format!("agt_{}_{agent_id}", ev.session_id));
        assert_eq!(
            ev.agent.parent_id.as_deref(),
            Some(format!("agt_{}", ev.session_id).as_str())
        );
    }
    assert!(seen >= 3, "fixture T1 có subagent (Q4), thấy {seen}");
}

/// Một fixture đại diện (file đầu tiên theo tên) cho mỗi loại sự kiện. `id` là ULID ngẫu nhiên nên bị che.
#[test]
fn snapshot_one_event_per_kind() {
    let mut first: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (dir, path) in all_fixtures() {
        first.entry(dir).or_insert(path);
    }
    assert!(first.len() >= 10, "loại sự kiện: {:?}", first.keys());
    for (dir, path) in first {
        let ev = hook::to_event(&load(&path), "laptop", NOW);
        insta::assert_json_snapshot!(dir, ev, { ".id" => "[evt_id]" });
    }
}
