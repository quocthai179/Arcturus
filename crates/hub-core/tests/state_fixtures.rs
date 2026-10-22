//! Phát lại toàn bộ fixture hook thật (T1) theo thứ tự thời gian qua `TeamState` (T4).

// Code test được dùng unwrap (CLAUDE.md); clippy chỉ tự miễn cho hàm #[test], không miễn helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use hub_core::config::Config;
use hub_core::hook::{self, HookInput};
use hub_core::protocol::ServerMsg;
use hub_core::schema::{AgentKind, EventKind, Mood};
use hub_core::state::TeamState;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

const T0: OffsetDateTime = datetime!(2026-10-21 09:00:00 UTC);

/// Mọi fixture, sắp theo tên file (ULID = thứ tự nhận).
fn fixtures_in_order() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/hooks");
    let mut out = Vec::new();
    for dir in std::fs::read_dir(root).unwrap() {
        let dir = dir.unwrap().path();
        if dir.is_dir() {
            for f in std::fs::read_dir(&dir).unwrap() {
                let f = f.unwrap().path();
                if f.extension().is_some_and(|e| e == "json") {
                    out.push(f);
                }
            }
        }
    }
    out.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    out
}

#[test]
fn replay_all_fixtures_through_state() {
    let cfg =
        Config::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../arcturus.example.toml"))
            .unwrap();
    let book = cfg.persona_book().unwrap();
    let mut st = TeamState::default();
    let mut ended = BTreeSet::new();
    let mut subagents_seen = 0;

    for (i, path) in fixtures_in_order().iter().enumerate() {
        let now = T0 + Duration::seconds(i as i64);
        let input = HookInput::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let ev = hook::to_event(&input, "laptop", now);
        let msgs = st.apply(&ev, &book, now);
        st.tick(now);

        for m in &msgs {
            if let ServerMsg::AgentUpdated { agent } = m {
                assert!(!agent.name.is_empty(), "{path:?}");
                assert!(!agent.act_text.contains('\n'), "{path:?}");
                if agent.kind == AgentKind::Subagent {
                    subagents_seen += 1;
                    assert_eq!(agent.role, "general-purpose", "{path:?}");
                }
            }
        }
        match ev.kind {
            EventKind::SessionEnded => {
                ended.insert(ev.session_id.clone());
                assert!(
                    !st.agents
                        .keys()
                        .any(|k| k.starts_with(&format!("agt_{}", ev.session_id))),
                    "SessionEnd phải xóa agent và con: {path:?}"
                );
            }
            EventKind::ApprovalRequested => {
                assert_eq!(
                    st.agents[&ev.agent.id].view.mood,
                    Mood::Approval,
                    "{path:?}"
                );
            }
            _ => {}
        }
    }

    assert!(subagents_seen > 0, "fixture Q4 có subagent");
    assert!(!ended.is_empty());
    // Agent còn lại đều thuộc phiên chưa có SessionEnd.
    for a in st.agents.values() {
        let session = a.view.parent_id.as_deref().unwrap_or(&a.view.id);
        assert!(
            !ended.iter().any(|s| session == format!("agt_{s}")),
            "{}",
            a.view.id
        );
    }
    // Sau khi mọi `done` hết hạn, không còn gì để hẹn giờ.
    let late = T0 + Duration::hours(1);
    st.tick(late);
    assert_eq!(st.next_wakeup(), None);
}
