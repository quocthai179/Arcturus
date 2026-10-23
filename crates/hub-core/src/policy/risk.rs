//! Phân loại rủi ro (`docs/PLAN.md` T8). Luật danger (Critical) nằm ở [`super::danger`]; file này chia phần còn lại:
//!
//! - **High**: có cờ nghi ngờ (ký tự ẩn, subshell, eval, mã hóa), không phân tích trọn được (dấu nháy chưa đóng, tên
//!   lệnh là biến), cài gói (`npm i`, `pip install`, `cargo add`…), chạy mã viết thẳng trên dòng lệnh (`python -c`,
//!   `node -e`, `bash -c`…).
//! - **Medium**: còn lại (git push không force, migration, ghi file trong workspace, lệnh không nhận diện, tool MCP).
//! - **Low**: chỉ đọc trong workspace, chạy test/lint/build (`cargo test`, `pnpm test`, `pytest`…).
//!
//! Chỉ lệnh ở độ sâu 0 được phân loại; nội dung trong dấu nháy chỉ dùng cho luật danger (`git commit -m "fix bug"`
//! không phải lệnh `fix`). Batch rule và trust grant chỉ tự duyệt tới Medium (kiểm ở [`super::enforce`]), nên mọi chỗ
//! không chắc phải rơi vào High hoặc Critical, không bao giờ Low/Medium.

use serde_json::Value;

use super::danger::{is_harmless_sink, path_inside};
use super::normalize::{Invocation, Normalized, Redirect, Word};
use crate::schema::Risk;

/// Rủi ro của một lệnh shell (chưa tính luật danger).
pub fn command_risk(n: &Normalized, cwd: &str, home: Option<&str>) -> Risk {
    if n.too_long || n.unparsed || !n.suspicious.is_empty() {
        return Risk::High;
    }
    let top: Vec<&Invocation> = n
        .invocations
        .iter()
        .filter(|i| n.segments.get(i.segment).is_some_and(|s| s.depth == 0))
        .collect();
    if top.is_empty() {
        return Risk::High;
    }
    let mut risk = Risk::Low;
    for inv in top {
        risk = risk.max(invocation_risk(n, inv, cwd, home));
    }
    // Ghi ra file bằng chuyển hướng: không còn là chỉ đọc.
    let writes = n
        .segments
        .iter()
        .filter(|s| s.depth == 0)
        .flat_map(|s| &s.words)
        .any(|w| w.redirect == Some(Redirect::Out) && !is_harmless_sink(&w.value));
    if writes {
        risk = risk.max(Risk::Medium);
    }
    risk
}

fn invocation_risk(n: &Normalized, inv: &Invocation, cwd: &str, home: Option<&str>) -> Risk {
    let args: Vec<&str> = n
        .args(inv)
        .iter()
        .filter(|w| w.redirect.is_none())
        .map(|w| w.value.as_str())
        .collect();
    let name = inv.name.as_str();
    if inv.dynamic || installs(name, &args) || inline_code(name, &args) {
        return Risk::High;
    }
    if test_or_build(name, &args) {
        return Risk::Low;
    }
    if read_only(name, &args) && reads_inside(name, n.args(inv), cwd, home) {
        return Risk::Low;
    }
    Risk::Medium
}

/// Đối số vị trí đầu tiên (bỏ tùy chọn; `skip_value` cho biết tùy chọn nào ăn thêm một từ).
fn subcommand<'a>(args: &[&'a str], skip_value: impl Fn(&str) -> bool) -> Option<(usize, &'a str)> {
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        if a.starts_with('-') || a.starts_with('+') {
            i += if skip_value(a) { 2 } else { 1 };
            continue;
        }
        return Some((i, a));
    }
    None
}

fn js_pm_value_opt(a: &str) -> bool {
    matches!(
        a,
        "-C" | "--dir" | "--filter" | "-F" | "--prefix" | "--cwd" | "-w" | "--workspace"
    )
}

/// Cài hoặc tải gói về chạy.
fn installs(name: &str, args: &[&str]) -> bool {
    if matches!(name, "npx" | "bunx" | "uvx" | "pnpx") {
        return true;
    }
    let sub = subcommand(args, js_pm_value_opt);
    let sub_is = |names: &[&str]| sub.is_some_and(|(_, s)| names.contains(&s));
    match name {
        "npm" => sub_is(&[
            "i", "install", "add", "ci", "update", "up", "upgrade", "exec", "x", "create", "init",
        ]),
        "pnpm" => sub_is(&[
            "i", "install", "add", "update", "up", "upgrade", "dlx", "create",
        ]),
        "yarn" => sub.is_none() || sub_is(&["install", "add", "upgrade", "up", "dlx", "create"]),
        "bun" => sub_is(&["i", "install", "add", "update", "x", "create"]),
        "pip" | "pip3" | "pipx" => sub_is(&["install", "run"]),
        "uv" => {
            sub_is(&["add", "sync", "run"])
                || (sub_is(&["pip", "tool"])
                    && args.iter().any(|a| matches!(*a, "install" | "run")))
        }
        "cargo" => sub_is(&["add", "install", "binstall"]),
        "go" => sub_is(&["install", "get", "run"]),
        "gem" | "choco" | "winget" | "scoop" | "conda" | "mamba" => {
            sub_is(&["install", "update", "create"])
        }
        "brew" => sub_is(&["install", "upgrade", "tap", "reinstall"]),
        "apt" | "apt-get" | "dnf" | "yum" | "zypper" | "apk" => {
            sub_is(&["install", "add", "upgrade"])
        }
        "pacman" => args.iter().any(|a| a.starts_with("-S")),
        "composer" => sub_is(&["require", "install", "update"]),
        "dotnet" => sub_is(&["add", "tool"]),
        _ if name.starts_with("python") => args
            .windows(3)
            .any(|w| w[0] == "-m" && w[1].starts_with("pip") && w[2] == "install"),
        _ => false,
    }
}

/// Mã viết thẳng trên dòng lệnh: không đọc được trước khi chạy.
fn inline_code(name: &str, args: &[&str]) -> bool {
    let has = |opts: &[&str]| {
        args.iter()
            .any(|a| opts.iter().any(|o| a.eq_ignore_ascii_case(o)))
    };
    match name {
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish" | "ash" => has(&["-c"]),
        "node" | "nodejs" | "bun" => has(&["-e", "--eval", "-p", "--print"]),
        "deno" => has(&["eval"]),
        "perl" => has(&["-e", "-E"]),
        "ruby" => has(&["-e"]),
        "php" => has(&["-r"]),
        "pwsh" | "powershell" => has(&["-c", "-command", "-e", "-ec", "-encodedcommand"]),
        "cmd" => has(&["/c", "/k"]),
        "osascript" => has(&["-e"]),
        _ if name.starts_with("python") => has(&["-c"]),
        _ => false,
    }
}

const SCRIPTS: [&str; 10] = [
    "test",
    "t",
    "lint",
    "build",
    "typecheck",
    "check",
    "tsc",
    "vitest",
    "jest",
    "eslint",
];

fn test_or_build(name: &str, args: &[&str]) -> bool {
    let fixes = args
        .iter()
        .any(|a| a.starts_with("--fix") || *a == "--write");
    let sub = subcommand(args, js_pm_value_opt);
    let run_target = |i: usize| args.get(i + 1).is_some_and(|s| SCRIPTS.contains(s));
    match name {
        "npm" => match sub {
            Some((_, "test" | "t" | "tst")) => true,
            Some((i, "run" | "run-script")) => run_target(i),
            _ => false,
        },
        "pnpm" | "yarn" | "bun" => match sub {
            Some((i, "run" | "exec")) => run_target(i) && !fixes,
            Some((_, s)) => SCRIPTS.contains(&s) && !fixes,
            None => false,
        },
        "cargo" => match sub {
            Some((
                _,
                "test" | "t" | "clippy" | "build" | "b" | "check" | "c" | "doc" | "nextest"
                | "bench",
            )) => !args.contains(&"--fix"),
            Some((_, "fmt")) => args.contains(&"--check"),
            _ => false,
        },
        "go" => matches!(sub, Some((_, "test" | "build" | "vet"))),
        "pytest" | "vitest" | "jest" | "tsc" | "mypy" => true,
        "eslint" => !fixes,
        "ruff" => matches!(sub, Some((_, "check"))) && !fixes,
        _ if name.starts_with("python") => args
            .windows(2)
            .any(|w| w[0] == "-m" && matches!(w[1], "pytest" | "unittest" | "mypy")),
        _ => false,
    }
}

/// Chỉ đọc (không ghi file, không mạng, không chạy lệnh khác).
fn read_only(name: &str, args: &[&str]) -> bool {
    match name {
        "ls" | "dir" | "pwd" | "cat" | "head" | "tail" | "less" | "more" | "wc" | "grep"
        | "egrep" | "fgrep" | "rg" | "ag" | "which" | "where" | "type" | "file" | "stat"
        | "tree" | "du" | "df" | "echo" | "printf" | "true" | "false" | "test" | "[" | "date"
        | "whoami" | "id" | "uname" | "hostname" | "basename" | "dirname" | "realpath"
        | "readlink" | "uniq" | "cut" | "tr" | "diff" | "cmp" | "comm" | "nl" | "column" | "jq"
        | "cd" | "get-childitem" | "gci" | "get-content" | "gc" | "select-string" | "sls"
        | "get-location" => true,
        "sort" => !args.iter().any(|a| *a == "-o" || a.starts_with("--output")),
        "find" => !args.iter().any(|a| {
            matches!(
                *a,
                "-delete"
                    | "-exec"
                    | "-execdir"
                    | "-ok"
                    | "-okdir"
                    | "-fprint"
                    | "-fprint0"
                    | "-fprintf"
                    | "-fls"
            )
        }),
        "sed" => !args.iter().any(|a| {
            a.starts_with("-i")
                || a.starts_with("--in-place")
                || (a.starts_with('-') && !a.starts_with("--") && a.contains('i'))
        }),
        "git" => git_read_only(args),
        _ => false,
    }
}

fn git_read_only(args: &[&str]) -> bool {
    let Some((i, sub)) = subcommand(args, |a| {
        matches!(a, "-C" | "-c" | "--git-dir" | "--work-tree")
    }) else {
        return false;
    };
    let rest = &args[i + 1..];
    match sub {
        "status" | "log" | "diff" | "show" | "rev-parse" | "ls-files" | "blame" | "describe"
        | "shortlog" | "grep" | "ls-tree" | "cat-file" => true,
        "branch" => rest.iter().all(|a| {
            matches!(
                *a,
                "-a" | "-r" | "-v" | "-vv" | "--list" | "--show-current" | "--all" | "--remotes"
            )
        }),
        "remote" => rest
            .iter()
            .all(|a| matches!(*a, "-v" | "--verbose" | "show")),
        "tag" => rest.iter().all(|a| matches!(*a, "-l" | "--list")),
        "stash" => rest.first().is_some_and(|a| matches!(*a, "list" | "show")),
        _ => false,
    }
}

/// Lệnh không đọc file (`echo`, `date`…): đối số không phải đường dẫn.
const NO_PATH_ARGS: [&str; 10] = [
    "echo", "printf", "true", "false", "date", "whoami", "id", "uname", "hostname", "test",
];

/// Mọi đối số trông như đường dẫn tuyệt đối, đi lên (`..`), hay dùng biến đều nằm trong `cwd`.
fn reads_inside(name: &str, args: &[Word], cwd: &str, home: Option<&str>) -> bool {
    if NO_PATH_ARGS.contains(&name) {
        return true;
    }
    args.iter().all(|w| {
        let a = w.value.as_str();
        let a = a
            .split_once('=')
            .map_or(a, |(k, v)| if k.starts_with('-') { v } else { a });
        let pathlike = a.starts_with(['/', '~', '$', '\\'])
            || a.contains("..")
            || a.as_bytes().get(1) == Some(&b':');
        !pathlike || path_inside(a, cwd, home)
    })
}

/// Rủi ro của tool khác Bash/PowerShell (chưa tính luật danger và ký tự ẩn).
pub fn tool_risk(tool: &str, input: &Value, cwd: &str, home: Option<&str>) -> Risk {
    if !input.is_object() {
        return Risk::High;
    }
    let field = |k: &str| input.get(k);
    let path_risk = |v: Option<&Value>, required: bool| match v {
        None if !required => Risk::Low,
        Some(Value::String(p)) if path_inside(p, cwd, home) => Risk::Low,
        Some(Value::String(_)) => Risk::Medium,
        _ => Risk::High,
    };
    match tool {
        "Read" => path_risk(field("file_path"), true),
        "NotebookRead" => path_risk(field("notebook_path"), true),
        "Grep" | "LS" => path_risk(field("path"), false),
        "Glob" => {
            let pattern_outside = field("pattern")
                .and_then(Value::as_str)
                .is_some_and(|p| p.starts_with(['/', '~', '\\']) || p.contains(".."));
            path_risk(field("path"), false).max(if pattern_outside {
                Risk::Medium
            } else {
                Risk::Low
            })
        }
        "Edit" | "Write" | "MultiEdit" => path_risk(field("file_path"), true).max(Risk::Medium),
        "NotebookEdit" => path_risk(field("notebook_path"), true).max(Risk::Medium),
        _ => Risk::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::super::normalize::normalize;
    use super::*;
    use serde_json::json;

    const CWD: &str = "/home/u/proj";

    fn r(cmd: &str) -> Risk {
        command_risk(&normalize(cmd), CWD, Some("/home/u"))
    }

    #[test]
    fn low_examples() {
        for c in [
            "cargo test",
            "cargo test -p hub -- --nocapture",
            "cargo clippy --workspace --all-targets -- -D warnings",
            "pnpm -C ui test",
            "npm test",
            "npm run lint",
            "pytest -q",
            "python -m pytest tests",
            "go test ./...",
            "ls -la",
            "cat src/main.rs | grep foo",
            "git status && git diff",
            "git log --oneline -5",
            "cargo test 2>/dev/null",
            "cd crates/hub && cargo build",
            "find . -name '*.rs'",
            "grep -rn \"fix bug\" src",
            "timeout 60 cargo test",
        ] {
            assert_eq!(r(c), Risk::Low, "{c}");
        }
    }

    #[test]
    fn medium_examples() {
        for c in [
            "git push origin main",
            "git commit -m 'fix bug'",
            "rm -r dir",
            "rm file.txt",
            "cat /etc/hosts",
            "echo x > out.txt",
            "make deploy",
            "./scripts/migrate.sh",
            "cargo fmt",
            "sed -i s/a/b/ f",
            "find . -name x -delete",
            "eslint --fix src",
            "git branch -D old",
            "cat ../other/file",
        ] {
            assert_eq!(r(c), Risk::Medium, "{c}");
        }
    }

    #[test]
    fn high_examples() {
        for c in [
            "npm install left-pad",
            "pnpm add react",
            "pip install requests",
            "python -m pip install x",
            "cargo add serde",
            "npx create-react-app x",
            "yarn",
            "brew install jq",
            "python -c 'import os'",
            "node -e 'process.exit(1)'",
            "bash -c 'ls'",
            "ls $(pwd)",
            "eval \"$CMD\"",
            "$TOOL --version",
            "echo 'unterminated",
            "echo cm0= | base64 -d",
            "l\u{200B}s",
            "",
            "# chỉ có chú thích",
        ] {
            assert_eq!(r(c), Risk::High, "{c:?}");
        }
    }

    #[test]
    fn tool_risk_examples() {
        let t = |tool: &str, v: Value| tool_risk(tool, &v, CWD, Some("/home/u"));
        assert_eq!(
            t("Read", json!({"file_path": "/home/u/proj/a.rs"})),
            Risk::Low
        );
        assert_eq!(t("Read", json!({"file_path": "src/a.rs"})), Risk::Low);
        assert_eq!(t("Read", json!({"file_path": "/etc/hosts"})), Risk::Medium);
        assert_eq!(t("Read", json!({})), Risk::High);
        assert_eq!(t("Grep", json!({"pattern": "x"})), Risk::Low);
        assert_eq!(t("Glob", json!({"pattern": "/etc/**"})), Risk::Medium);
        assert_eq!(t("Edit", json!({"file_path": "src/a.rs"})), Risk::Medium);
        assert_eq!(t("Write", json!({"content": "x"})), Risk::High);
        assert_eq!(t("WebFetch", json!({"url": "https://x"})), Risk::Medium);
        assert_eq!(t("mcp__x__y", json!({"a": 1})), Risk::Medium);
        assert_eq!(t("Bash", json!("ls")), Risk::High);
    }
}
