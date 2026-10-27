//! Standup sinh từ sự kiện đã lưu và `git log` (`docs/PLAN.md` T19). Không gọi LLM (quy tắc bất biến 5): số liệu đếm từ
//! sự kiện, văn bản sinh từ template tiếng Việt cố định.
//!
//! Mọi chuỗi đi vào kết quả (tên persona, tóm tắt lệnh, tiêu đề commit) là dữ liệu do agent hoặc repo tạo ra, không tin
//! cậy: [`clean`] hiện ký tự ẩn/điều khiển thành `⟨U+XXXX⟩`, gộp khoảng trắng và cắt độ dài trước khi vào JSON hay văn bản.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use time::OffsetDateTime;

use crate::policy::normalize::display;
use crate::schema::{Event, EventKind};
use crate::store::format_ts;

/// Tối đa commit liệt kê cho mỗi persona (số lượng thật vẫn được đếm trong `commit_count`).
pub const MAX_COMMITS_LISTED: usize = 5;
/// Tối đa thư mục `cwd` khác nhau cho mỗi persona khi hỏi `git log`.
pub const MAX_CWDS_PER_PERSONA: usize = 5;
/// Số lần lặp tối thiểu để coi là "lỗi lặp lại".
pub const REPEATED_ERROR_MIN: u32 = 3;
/// Độ dài tối đa (ký tự) của một chuỗi hiển thị.
pub const MAX_TEXT_CHARS: usize = 120;

/// Nguồn commit. Bản thật chạy `git log` (crate `hub`); test dùng bản giả.
pub trait GitLog {
    /// Tiêu đề commit (`<hash ngắn> <tiêu đề>`) trong `cwd` kể từ `since`, mới nhất trước. Lỗi (không phải repo, không có
    /// git, hết giờ) trả rỗng.
    fn commits(&self, cwd: &str, since: OffsetDateTime) -> Vec<String>;
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct RepeatedError {
    pub tool: String,
    pub what: String,
    pub count: u32,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PersonaStandup {
    pub name: String,
    /// Số lượt trả lời xong (`TurnStopped`).
    pub turns_done: u32,
    /// Tiêu đề commit (tối đa [`MAX_COMMITS_LISTED`]).
    pub commits: Vec<String>,
    pub commit_count: u32,
    pub tests_passed: u32,
    pub tests_failed: u32,
    /// Yêu cầu duyệt đang chờ ngay lúc này.
    pub pending: u32,
    pub repeated_errors: Vec<RepeatedError>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Standup {
    pub since: String,
    pub until: String,
    pub personas: Vec<PersonaStandup>,
    /// Văn bản template, nhiều dòng.
    pub text: String,
}

pub struct Input<'a> {
    /// Sự kiện trong khoảng `[since, until]` (thứ tự bất kỳ).
    pub events: &'a [Event],
    /// Tên persona của từng yêu cầu duyệt đang chờ (lặp lại theo số yêu cầu).
    pub pending: &'a [String],
    pub since: OffsetDateTime,
    pub until: OffsetDateTime,
    /// Persona của một sự kiện (hub tra trạng thái agent, rồi `match_cwd`, rồi tên mặc định).
    pub name_of: &'a dyn Fn(&Event) -> String,
}

/// Chuỗi an toàn để hiển thị: ký tự ẩn/điều khiển hiện thành `⟨U+XXXX⟩`, gộp khoảng trắng, cắt còn [`MAX_TEXT_CHARS`]
/// ký tự (thêm `…`).
pub fn clean(s: &str) -> String {
    let shown = display(s);
    let flat = shown.text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX_TEXT_CHARS {
        flat
    } else {
        let mut cut: String = flat.chars().take(MAX_TEXT_CHARS - 1).collect();
        cut.push('…');
        cut
    }
}

/// Lệnh chạy test của các bộ chạy phổ biến (đầu đoạn lệnh, sau khi bỏ gán biến môi trường).
const TEST_RUNNERS: [&str; 20] = [
    "npm test",
    "npm run test",
    "npm t",
    "pnpm test",
    "pnpm run test",
    "yarn test",
    "cargo test",
    "cargo nextest",
    "pytest",
    "python -m pytest",
    "python3 -m pytest",
    "go test",
    "dotnet test",
    "mvn test",
    "gradle test",
    "./gradlew test",
    "vitest",
    "jest",
    "ctest",
    "make test",
];

/// Lệnh chạy test? So từng đoạn của lệnh ghép (`a && b`, `a; b`, `a | b`); chấp nhận `CI=1 npm test`, `npx vitest`,
/// `pnpm exec jest`, `pnpm -C ui test`.
pub fn is_test_command(cmd: &str) -> bool {
    cmd.split(['&', ';', '|', '\n']).any(|seg| {
        let mut words: Vec<&str> = seg.split_whitespace().collect();
        while words
            .first()
            .is_some_and(|w| w.contains('=') && !w.starts_with('-'))
        {
            words.remove(0);
        }
        if words.first() == Some(&"pnpm") && words.get(1) == Some(&"-C") {
            return words.get(3) == Some(&"test")
                || (words.get(3) == Some(&"run") && words.get(4) == Some(&"test"));
        }
        let line = words.join(" ");
        let line = line
            .strip_prefix("npx ")
            .or_else(|| line.strip_prefix("pnpm exec "))
            .unwrap_or(&line);
        TEST_RUNNERS
            .iter()
            .any(|r| line == *r || line.starts_with(&format!("{r} ")))
    })
}

#[derive(Default)]
struct Acc {
    out: PersonaStandup,
    cwds: Vec<String>,
    errors: BTreeMap<(String, String), u32>,
}

fn acc_for<'a>(map: &'a mut BTreeMap<String, Acc>, name: &str) -> &'a mut Acc {
    map.entry(name.to_owned()).or_insert_with(|| Acc {
        out: PersonaStandup {
            name: name.to_owned(),
            ..PersonaStandup::default()
        },
        ..Acc::default()
    })
}

/// Dựng standup. `git` được hỏi một lần cho mỗi thư mục `cwd` khác nhau của mỗi persona.
pub fn build(input: &Input<'_>, git: &dyn GitLog) -> Standup {
    let mut by_name: BTreeMap<String, Acc> = BTreeMap::new();
    for ev in input.events {
        if ev.ts < input.since || ev.ts > input.until {
            continue;
        }
        let acc = acc_for(&mut by_name, &clean(&(input.name_of)(ev)));
        if let Some(cwd) = ev.cwd.as_deref().filter(|c| !c.is_empty()) {
            if acc.cwds.len() < MAX_CWDS_PER_PERSONA && !acc.cwds.iter().any(|c| c == cwd) {
                acc.cwds.push(cwd.to_owned());
            }
        }
        let tool = ev.tool.as_ref();
        let is_test = tool.is_some_and(|t| t.name == "Bash" && is_test_command(&t.summary));
        match ev.kind {
            EventKind::TurnStopped => acc.out.turns_done += 1,
            EventKind::ToolPost if is_test => acc.out.tests_passed += 1,
            EventKind::ToolFailed => {
                if is_test {
                    acc.out.tests_failed += 1;
                }
                if let Some(t) = tool {
                    *acc.errors
                        .entry((clean(&t.name), clean(&t.summary)))
                        .or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }
    for name in input.pending {
        acc_for(&mut by_name, &clean(name)).out.pending += 1;
    }

    let mut personas = Vec::new();
    for (_, mut acc) in by_name {
        let mut seen = BTreeSet::new();
        let mut commits = Vec::new();
        for cwd in &acc.cwds {
            for c in git.commits(cwd, input.since) {
                let c = clean(&c);
                if !c.is_empty() && seen.insert(c.clone()) {
                    commits.push(c);
                }
            }
        }
        acc.out.commit_count = u32::try_from(commits.len()).unwrap_or(u32::MAX);
        commits.truncate(MAX_COMMITS_LISTED);
        acc.out.commits = commits;
        let mut repeated: Vec<RepeatedError> = acc
            .errors
            .into_iter()
            .filter(|(_, n)| *n >= REPEATED_ERROR_MIN)
            .map(|((tool, what), count)| RepeatedError { tool, what, count })
            .collect();
        repeated.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.what.cmp(&b.what)));
        acc.out.repeated_errors = repeated;
        personas.push(acc.out);
    }

    let (since, until) = (format_ts(input.since), format_ts(input.until));
    let text = render(&since, &until, &personas);
    Standup {
        since,
        until,
        personas,
        text,
    }
}

/// `2026-10-27T09:00:00.000Z` → `2026-10-27 09:00 UTC`.
fn short_time(ts: &str) -> String {
    let t = ts.replace('T', " ");
    format!("{} UTC", t.get(..16).unwrap_or(&t))
}

fn render(since: &str, until: &str, personas: &[PersonaStandup]) -> String {
    let mut out = format!("Standup {} → {}\n", short_time(since), short_time(until));
    if personas.is_empty() {
        out.push_str("Chưa có hoạt động trong khoảng này.\n");
        return out;
    }
    for p in personas {
        out.push_str(&format!(
            "{}: xong {} lượt, {} commit, test {} đạt / {} lỗi, {} yêu cầu đang chờ.\n",
            p.name, p.turns_done, p.commit_count, p.tests_passed, p.tests_failed, p.pending
        ));
        for c in &p.commits {
            out.push_str(&format!("  - commit: {c}\n"));
        }
        for e in &p.repeated_errors {
            out.push_str(&format!(
                "  - lỗi lặp lại: {} \"{}\" ×{}\n",
                e.tool, e.what, e.count
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AgentKind, AgentRef, Source, ToolInfo};
    use time::macros::datetime;

    struct FakeGit(Vec<(&'static str, Vec<&'static str>)>);

    impl GitLog for FakeGit {
        fn commits(&self, cwd: &str, _since: OffsetDateTime) -> Vec<String> {
            self.0
                .iter()
                .filter(|(c, _)| *c == cwd)
                .flat_map(|(_, v)| v.iter().map(|s| (*s).to_owned()))
                .collect()
        }
    }

    const T0: OffsetDateTime = datetime!(2026-10-27 09:00 UTC);
    const T1: OffsetDateTime = datetime!(2026-10-27 17:30 UTC);

    fn ev(
        kind: EventKind,
        session: &str,
        cwd: &str,
        tool: Option<(&str, &str)>,
        mins: i64,
    ) -> Event {
        Event {
            v: 1,
            id: format!("evt_{session}_{mins}"),
            ts: T0 + time::Duration::minutes(mins),
            host: "laptop".to_owned(),
            source: Source::ClaudeCode,
            session_id: session.to_owned(),
            cwd: Some(cwd.to_owned()),
            agent: AgentRef {
                id: format!("agt_{session}"),
                parent_id: None,
                kind: AgentKind::Session,
                agent_type: None,
            },
            kind,
            activity: None,
            tool: tool.map(|(n, s)| ToolInfo {
                name: n.to_owned(),
                summary: s.to_owned(),
                risk: None,
            }),
            approval: None,
            metrics: None,
        }
    }

    fn name_by_session(e: &Event) -> String {
        match e.session_id.as_str() {
            "s1" | "s1b" => "Khoa".to_owned(),
            "s2" => "Linh".to_owned(),
            other => format!("Agent {other}"),
        }
    }

    fn sample_events() -> Vec<Event> {
        let hub_fail = Some(("Bash", "cargo test -p hub"));
        let mut v = vec![
            // Khoa: hai phiên cùng persona, 3 lượt xong, 2 test đạt, test lỗi 4 lần (cùng lệnh).
            ev(EventKind::TurnStopped, "s1", "/w/hub", None, 10),
            ev(EventKind::TurnStopped, "s1", "/w/hub", None, 20),
            ev(EventKind::TurnStopped, "s1b", "/w/hub", None, 30),
            ev(EventKind::ToolPost, "s1", "/w/hub", hub_fail, 11),
            ev(
                EventKind::ToolPost,
                "s1",
                "/w/hub",
                Some(("Bash", "CI=1 npm test && echo ok")),
                12,
            ),
            ev(EventKind::ToolFailed, "s1", "/w/hub", hub_fail, 13),
            ev(
                EventKind::ToolPost,
                "s1",
                "/w/hub",
                Some(("Bash", "ls -la")),
                14,
            ),
            ev(
                EventKind::ToolPost,
                "s1",
                "/w/hub",
                Some(("Read", "a.rs")),
                15,
            ),
            // Linh: một lỗi lặp lại 2 lần (chưa đủ ngưỡng).
            ev(
                EventKind::ToolFailed,
                "s2",
                "/w/ui",
                Some(("Edit", "App.tsx")),
                40,
            ),
            ev(
                EventKind::ToolFailed,
                "s2",
                "/w/ui",
                Some(("Edit", "App.tsx")),
                41,
            ),
            // Ngoài khoảng: không tính.
            ev(EventKind::TurnStopped, "s1", "/w/hub", None, -5),
            ev(EventKind::TurnStopped, "s1", "/w/hub", None, 9 * 60),
        ];
        for m in [50, 51, 52] {
            v.push(ev(EventKind::ToolFailed, "s1", "/w/hub", hub_fail, m));
        }
        v
    }

    fn run(events: &[Event], pending: &[String], git: &dyn GitLog) -> Standup {
        build(
            &Input {
                events,
                pending,
                since: T0,
                until: T1,
                name_of: &name_by_session,
            },
            git,
        )
    }

    fn persona<'a>(s: &'a Standup, name: &str) -> &'a PersonaStandup {
        s.personas.iter().find(|p| p.name == name).unwrap()
    }

    #[test]
    fn counts_turns_tests_and_repeated_errors_per_persona() {
        let events = sample_events();
        let git = FakeGit(vec![
            (
                "/w/hub",
                vec!["abc1234 feat(hub): luồng duyệt", "def5678 fix: sửa lỗi"],
            ),
            ("/w/ui", vec![]),
        ]);
        let pending = vec!["Linh".to_owned(), "Linh".to_owned(), "Khoa".to_owned()];
        let s = run(&events, &pending, &git);
        assert_eq!(s.personas.len(), 2);
        let k = persona(&s, "Khoa");
        // 3 lượt trong khoảng (hai sự kiện ngoài khoảng không tính); 2 test đạt (`cargo test`, `CI=1 npm test && …`),
        // `ls` không phải test; 4 test lỗi (1 + 3).
        assert_eq!((k.turns_done, k.tests_passed, k.tests_failed), (3, 2, 4));
        assert_eq!(k.commit_count, 2);
        assert_eq!(k.pending, 1);
        assert_eq!(
            k.repeated_errors,
            [RepeatedError {
                tool: "Bash".to_owned(),
                what: "cargo test -p hub".to_owned(),
                count: 4
            }]
        );
        let l = persona(&s, "Linh");
        assert_eq!(
            (l.turns_done, l.tests_passed, l.tests_failed, l.pending),
            (0, 0, 0, 2)
        );
        assert!(
            l.repeated_errors.is_empty(),
            "2 lần chưa đủ ngưỡng {REPEATED_ERROR_MIN}"
        );
    }

    #[test]
    fn text_is_a_fixed_vietnamese_template() {
        let events = sample_events();
        let git = FakeGit(vec![("/w/hub", vec!["abc1234 feat(hub): luồng duyệt"])]);
        let s = run(&events, &["Linh".to_owned()], &git);
        let expected = [
            "Standup 2026-10-27 09:00 UTC → 2026-10-27 17:30 UTC",
            "Khoa: xong 3 lượt, 1 commit, test 2 đạt / 4 lỗi, 0 yêu cầu đang chờ.",
            "  - commit: abc1234 feat(hub): luồng duyệt",
            "  - lỗi lặp lại: Bash \"cargo test -p hub\" ×4",
            "Linh: xong 0 lượt, 0 commit, test 0 đạt / 0 lỗi, 1 yêu cầu đang chờ.",
            "",
        ]
        .join("\n");
        assert_eq!(s.text, expected);
    }

    #[test]
    fn empty_input_says_so() {
        let s = run(&[], &[], &FakeGit(vec![]));
        assert!(s.personas.is_empty());
        assert!(s.text.contains("Chưa có hoạt động trong khoảng này."));
    }

    #[test]
    fn pending_only_persona_still_appears() {
        let s = run(&[], &["Seginus".to_owned()], &FakeGit(vec![]));
        assert_eq!(persona(&s, "Seginus").pending, 1);
    }

    #[test]
    fn commits_are_deduplicated_across_cwds_and_capped_but_counted() {
        let events = vec![
            ev(EventKind::TurnStopped, "s1", "/w/a", None, 1),
            ev(EventKind::TurnStopped, "s1b", "/w/b", None, 2),
        ];
        let many = vec![
            "c1 một", "c2 hai", "c3 ba", "c4 bốn", "c5 năm", "c6 sáu", "c7 bảy",
        ];
        let git = FakeGit(vec![("/w/a", many), ("/w/b", vec!["c1 một", "c8 tám"])]);
        let s = run(&events, &[], &git);
        let k = persona(&s, "Khoa");
        assert_eq!(
            k.commit_count, 8,
            "c1 trùng giữa hai thư mục chỉ đếm một lần"
        );
        assert_eq!(k.commits.len(), MAX_COMMITS_LISTED);
        assert_eq!(k.commits[0], "c1 một");
    }

    #[test]
    fn untrusted_strings_are_cleaned_before_output() {
        let evil: &'static str =
            Box::leak(format!("a\u{202E}b\u{200B}c\n\nxx{}", "y".repeat(300)).into_boxed_str());
        let events = vec![ev(EventKind::TurnStopped, "s1", "/w/hub", None, 1)];
        let git = FakeGit(vec![("/w/hub", vec![evil])]);
        let s = run(&events, &[], &git);
        let c = &persona(&s, "Khoa").commits[0];
        assert!(
            !c.contains('\u{202E}') && !c.contains('\u{200B}') && !c.contains('\n'),
            "{c}"
        );
        assert!(c.contains("⟨U+202E⟩") && c.contains("⟨U+200B⟩"), "{c}");
        assert!(c.chars().count() <= MAX_TEXT_CHARS, "{}", c.chars().count());
        assert!(c.ends_with('…'));
    }

    #[test]
    fn at_most_five_cwds_per_persona_are_queried() {
        let events: Vec<Event> = (0..9)
            .map(|i| {
                ev(
                    EventKind::TurnStopped,
                    "s1",
                    &format!("/w/{i}"),
                    None,
                    i64::from(i),
                )
            })
            .collect();
        struct Count(std::cell::Cell<usize>);
        impl GitLog for Count {
            fn commits(&self, _cwd: &str, _since: OffsetDateTime) -> Vec<String> {
                self.0.set(self.0.get() + 1);
                Vec::new()
            }
        }
        let c = Count(std::cell::Cell::new(0));
        run(&events, &[], &c);
        assert_eq!(c.0.get(), MAX_CWDS_PER_PERSONA);
    }

    #[test]
    fn test_command_detection() {
        for yes in [
            "npm test",
            "npm run test -- auth",
            "pnpm test",
            "pnpm -C ui test",
            "pnpm -C ui run test",
            "cargo test --workspace",
            "cargo nextest run",
            "CI=1 npm test",
            "cd ui && pnpm test",
            "npx vitest run",
            "pnpm exec jest",
            "pytest -q",
            "python -m pytest tests/",
            "go test ./...",
            "dotnet test",
            "./gradlew test",
            "make test",
        ] {
            assert!(is_test_command(yes), "{yes}");
        }
        for no in [
            "ls -la",
            "cargo build",
            "cargo testing-tool",
            "npm install",
            "echo npm test",
            "git commit -m \"cargo test\"",
            "pnpm -C ui build",
            "",
        ] {
            assert!(!is_test_command(no), "{no}");
        }
    }
}
