//! Trình cài đặt trong app: cài/gỡ hook của Arcturus vào `settings.json` của Claude Code sau khi người dùng xem trước
//! thay đổi và bấm đồng ý, với token hook do app tự sinh. Không cần `bash`, `jq` hay đặt `ARCTURUS_TOKEN` bằng tay.
//!
//! Giữ đúng hành vi của `scripts/dogfood-lib.sh` (T18):
//! - Chỉ đụng khóa `hooks` (quy tắc bất biến 7); mọi khóa khác và hook của người dùng giữ nguyên, hook của Arcturus
//!   nằm cuối mảng của từng sự kiện.
//! - Nhận diện hook của Arcturus theo đích `http://127.0.0.1:<cổng>/hooks/event|permission`, không thêm khóa lạ.
//! - Sao lưu `settings.json.bak-<giây unix>` trước khi ghi; JSON hỏng hay hình dạng lạ thì dừng, không ghi gì.
//! - Ghi đè tại chỗ (giữ symlink và quyền file); ghi lỗi thì khôi phục từ bản sao lưu.
//!
//! Khác script: token hook ghi thẳng vào `settings.json` (header và lệnh `curl` của `SessionStart`) thay cho
//! `$ARCTURUS_TOKEN`. Token không còn nằm trong biến môi trường của `claude` nên Bash tool không thừa hưởng nó (F6
//! nhẹ đi); agent vẫn đọc được nếu đọc `settings.json`, và token hook không duyệt được gì (duyệt cần token UI).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::http;

/// Mẫu hook (nguồn sự thật chung với script cài): cổng 7878, token `$ARCTURUS_TOKEN`.
const TEMPLATE: &str = include_str!("../../../../hooks/arcturus-dogfood.json");
const TEMPLATE_HOST: &str = "127.0.0.1:7878";
const TOKEN_VAR: &str = "$ARCTURUS_TOKEN";
/// Biến môi trường hub đọc token hook (`crates/hub/src/auth.rs`).
pub const TOKEN_ENV: &str = "ARCTURUS_TOKEN";
const ARC_PREFIX: &str = "http://127.0.0.1:";
const ARC_PATHS: [&str; 2] = ["/hooks/event", "/hooks/permission"];

/// Giống `crates/hub/src/auth.rs`: hub từ chối token ngắn hoặc ít ký tự khác nhau.
const MIN_TOKEN_LEN: usize = 32;
const MIN_DISTINCT_CHARS: usize = 10;

/// File cấu hình lớn hơn chừng này thì không diff từng dòng (chỉ báo số dòng thêm/bớt).
const DIFF_MAX_LINES: usize = 1500;
const DIFF_CONTEXT: usize = 3;

/// Tiền tố `session_id` của sự kiện tự kiểm tra (agent `agt_arcturus-selftest-<nonce>`).
pub const SELFTEST_PREFIX: &str = "arcturus-selftest-";

// ---------------------------------------------------------------------------------------------------------------------
// Token hook

/// Token hook dùng được: đủ mạnh như hub đòi, và chỉ gồm ký tự an toàn khi ghép vào JSON và vào lệnh shell của
/// `SessionStart` (không nháy, không `$`, không khoảng trắng).
pub fn valid_hook_token(t: &str) -> bool {
    let distinct = t.chars().collect::<std::collections::BTreeSet<_>>().len();
    t.len() >= MIN_TOKEN_LEN
        && t.len() <= 256
        && distinct >= MIN_DISTINCT_CHARS
        && t.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.~+/=".contains(&b))
}

/// Token hook của hub do app khởi động: `ARCTURUS_TOKEN` (người dùng đã đặt từ trước, giữ tương thích), rồi file
/// `hook-token` trong `dir`, rồi sinh mới (24 byte ngẫu nhiên, hex) và ghi file chỉ người dùng đọc được.
pub fn hook_token(dir: &Path, env: Option<String>) -> Result<String, String> {
    if let Some(t) = env.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()) {
        if valid_hook_token(&t) {
            return Ok(t);
        }
        return Err(
            "ARCTURUS_TOKEN đang đặt không dùng được (cần ≥ 32 ký tự, ≥ 10 ký tự khác nhau, chỉ chữ, số và -_.~+/=); \
             bỏ biến này để app tự sinh token"
                .to_owned(),
        );
    }
    let path = dir.join("hook-token");
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let t = text.trim().to_owned();
            if valid_hook_token(&t) {
                return Ok(t);
            }
            return Err(format!(
                "token hook ở {} không hợp lệ; xóa file để app sinh token mới (rồi cài lại hook)",
                path.display()
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("không đọc được {}: {e}", path.display())),
    }
    let mut buf = [0u8; 24];
    getrandom::fill(&mut buf).map_err(|e| format!("không sinh được token hook: {e}"))?;
    let token: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::create_dir_all(dir).map_err(|e| format!("không tạo được {}: {e}", dir.display()))?;
    write_private_new(&path, token.as_bytes())
        .map_err(|e| format!("không ghi được {}: {e}", path.display()))?;
    Ok(token)
}

/// Tạo file mới (không ghi đè), Unix: quyền 0600.
fn write_private_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)?.write_all(bytes)
}

/// Che token khi hiện cho người xem (ảnh chụp màn hình diff không làm lộ token).
fn mask(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.to_owned();
    }
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    text.replace(token, &format!("<token …{tail}>"))
}

// ---------------------------------------------------------------------------------------------------------------------
// Hook mẫu và nhận diện

/// Hook của Arcturus cho `port` và `token`, theo thứ tự sự kiện của mẫu.
pub fn arcturus_hooks(port: u16, token: &str) -> Result<Map<String, Value>, String> {
    let mut tpl: Value =
        serde_json::from_str(TEMPLATE).map_err(|e| format!("mẫu hook hỏng: {e}"))?;
    let host = format!("127.0.0.1:{port}");
    let Some(Value::Object(mut hooks)) = tpl.get_mut("hooks").map(Value::take) else {
        return Err("mẫu hook thiếu khóa hooks".to_owned());
    };
    for entry in hooks.values_mut().filter_map(Value::as_array_mut).flatten() {
        let Some(list) = entry.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        for h in list.iter_mut().filter_map(Value::as_object_mut) {
            if let Some(Value::String(url)) = h.get_mut("url") {
                *url = url.replace(TEMPLATE_HOST, &host);
            }
            if let Some(Value::String(cmd)) = h.get_mut("command") {
                *cmd = cmd.replace(TEMPLATE_HOST, &host).replace(TOKEN_VAR, token);
            }
            if let Some(Value::Object(headers)) = h.get_mut("headers") {
                for v in headers.values_mut() {
                    if let Value::String(s) = v {
                        *s = s.replace(TOKEN_VAR, token);
                    }
                }
            }
            h.remove("allowedEnvVars");
        }
    }
    Ok(hooks)
}

/// Độ dài của `http://127.0.0.1:<cổng>/hooks/event|permission` ở đầu `s`, kèm cổng.
fn arc_target_at(s: &str) -> Option<(usize, u16)> {
    let rest = s.strip_prefix(ARC_PREFIX)?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let port = rest.get(..digits)?.parse::<u16>().ok()?;
    let after = &rest[digits..];
    let path = ARC_PATHS.iter().find(|p| after.starts_with(*p))?;
    Some((ARC_PREFIX.len() + digits + path.len(), port))
}

/// Cổng mà một hook của Arcturus trỏ tới; `None` nếu không phải hook của Arcturus. Hook http: URL phải đúng bằng đích;
/// hook command: lệnh có chứa đích (như `is_arc` của script).
pub fn arc_port(hook: &Value) -> Option<u16> {
    match hook.get("type").and_then(Value::as_str)? {
        "http" => {
            let url = hook.get("url").and_then(Value::as_str)?;
            arc_target_at(url)
                .filter(|(n, _)| *n == url.len())
                .map(|(_, p)| p)
        }
        "command" => {
            let cmd = hook.get("command").and_then(Value::as_str)?;
            cmd.match_indices(ARC_PREFIX)
                .find_map(|(i, _)| arc_target_at(&cmd[i..]))
                .map(|(_, p)| p)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Đọc, gộp, gỡ

/// Đọc nội dung settings: thiếu file hoặc chỉ có khoảng trắng là `{}`. Không phải object hoặc `hooks` có hình dạng lạ
/// là lỗi (không sửa gì).
pub fn parse_settings(text: Option<&str>) -> Result<Value, String> {
    let text = text.unwrap_or("");
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let v: Value = serde_json::from_str(text).map_err(|e| {
        format!("settings.json không phải JSON hợp lệ ({e}); Claude Code dùng JSON thuần, không có chú thích. Không sửa gì.")
    })?;
    if !v.is_object() {
        return Err("settings.json không phải object JSON. Không sửa gì.".to_owned());
    }
    match v.get("hooks") {
        None => {}
        Some(Value::Object(h)) if h.values().all(Value::is_array) => {}
        Some(_) => {
            return Err(
                "khóa \"hooks\" trong settings.json có hình dạng lạ (cần object, mỗi sự kiện là mảng). Không sửa gì."
                    .to_owned(),
            )
        }
    }
    Ok(v)
}

/// Gỡ mọi hook của Arcturus. Chỉ bỏ mục/khóa trở nên rỗng vì việc gỡ; mục rỗng sẵn của người dùng giữ nguyên.
pub fn strip(settings: &mut Value) {
    let Some(obj) = settings.as_object_mut() else {
        return;
    };
    let Some(Value::Object(hooks)) = obj.get_mut("hooks") else {
        return;
    };
    let had = hooks.len();
    hooks.retain(|_, arr| {
        let Value::Array(entries) = arr else {
            return true;
        };
        let before = entries.len();
        entries.retain_mut(|entry| {
            let Some(Value::Array(list)) = entry.get_mut("hooks") else {
                return true;
            };
            let n = list.len();
            list.retain(|h| arc_port(h).is_none());
            !(n > 0 && list.is_empty())
        });
        !(before > 0 && entries.is_empty())
    });
    if had > 0 && hooks.is_empty() {
        obj.remove("hooks");
    }
}

/// Thêm hook nguồn vào cuối mảng của từng sự kiện (tạo `hooks` nếu chưa có).
pub fn add(settings: &mut Value, src: &Map<String, Value>) {
    let Some(obj) = settings.as_object_mut() else {
        return;
    };
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(hooks) = hooks.as_object_mut() else {
        return;
    };
    for (ev, entries) in src {
        let slot = hooks
            .entry(ev.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let (Some(dst), Some(more)) = (slot.as_array_mut(), entries.as_array()) {
            dst.extend(more.iter().cloned());
        }
    }
}

/// Mọi hook của Arcturus trong settings, theo sự kiện.
fn arc_hooks_by_event(settings: &Value) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return out;
    };
    for (ev, entries) in hooks {
        for entry in entries.as_array().into_iter().flatten() {
            for h in entry
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if arc_port(h).is_some() {
                    out.push((ev.clone(), h.clone()));
                }
            }
        }
    }
    out
}

fn pretty(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".to_owned());
    s.push('\n');
    s
}

/// FNV-1a 64 của nội dung file (hoặc của "không có file"): phát hiện file bị đổi giữa lúc xem trước và lúc ghi.
pub fn digest(raw: Option<&[u8]>) -> String {
    let Some(raw) = raw else {
        return "absent".to_owned();
    };
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in raw {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

// ---------------------------------------------------------------------------------------------------------------------
// Kế hoạch thay đổi và diff

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Install,
    Uninstall,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    /// `ctx` (giữ nguyên), `add`, `del`, hoặc `skip` (lược `text` dòng không đổi).
    pub op: &'static str,
    pub text: String,
}

/// Kết quả tính trước một thay đổi (chưa ghi gì).
pub struct Change {
    pub before: String,
    pub after: String,
    pub changed: bool,
}

/// Nội dung mới của settings sau `action`. `text`: nội dung hiện tại (`None` là chưa có file).
pub fn compute(
    text: Option<&str>,
    action: Action,
    port: u16,
    token: &str,
) -> Result<Change, String> {
    let cur = parse_settings(text)?;
    let mut next = cur.clone();
    strip(&mut next);
    if action == Action::Install {
        add(&mut next, &arcturus_hooks(port, token)?);
    }
    let changed = cur != next;
    let before = if text.is_some_and(|t| !t.trim().is_empty()) {
        pretty(&cur)
    } else {
        String::new()
    };
    Ok(Change {
        before,
        after: pretty(&next),
        changed,
    })
}

/// Diff theo dòng (LCS), giữ `DIFF_CONTEXT` dòng ngữ cảnh quanh mỗi chỗ đổi.
pub fn line_diff(a: &str, b: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = a.lines().collect();
    let b: Vec<&str> = b.lines().collect();
    if a.len() > DIFF_MAX_LINES || b.len() > DIFF_MAX_LINES {
        return vec![DiffLine {
            op: "skip",
            text: format!(
                "file quá dài để xem từng dòng ({} → {} dòng)",
                a.len(),
                b.len()
            ),
        }];
    }
    // lcs[i][j] = độ dài LCS của a[i..] và b[j..].
    let w = b.len() + 1;
    let mut lcs = vec![0u16; (a.len() + 1) * w];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i * w + j] = if a[i] == b[j] {
                lcs[(i + 1) * w + j + 1] + 1
            } else {
                lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
            };
        }
    }
    let mut full: Vec<(&'static str, &str)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            full.push(("ctx", a[i]));
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i * w + j + 1] >= lcs[(i + 1) * w + j]) {
            full.push(("add", b[j]));
            j += 1;
        } else {
            full.push(("del", a[i]));
            i += 1;
        }
    }
    // Chỉ giữ dòng ngữ cảnh gần chỗ đổi; đoạn dài không đổi thành một dòng `skip`.
    let near: Vec<bool> = (0..full.len())
        .map(|k| {
            let lo = k.saturating_sub(DIFF_CONTEXT);
            let hi = (k + DIFF_CONTEXT + 1).min(full.len());
            full[lo..hi].iter().any(|(op, _)| *op != "ctx")
        })
        .collect();
    let mut out = Vec::new();
    let mut skipped = 0usize;
    for (k, (op, text)) in full.iter().enumerate() {
        if *op == "ctx" && !near[k] {
            skipped += 1;
            continue;
        }
        if skipped > 0 {
            out.push(DiffLine {
                op: "skip",
                text: format!("… {skipped} dòng không đổi"),
            });
            skipped = 0;
        }
        out.push(DiffLine {
            op,
            text: (*text).to_owned(),
        });
    }
    if skipped > 0 {
        out.push(DiffLine {
            op: "skip",
            text: format!("… {skipped} dòng không đổi"),
        });
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------------
// Trạng thái

/// Đường dẫn `settings.json` của Claude Code: `$CLAUDE_CONFIG_DIR/settings.json` hoặc `~/.claude/settings.json`.
pub fn settings_path() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d).join("settings.json"));
    }
    dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

fn read_raw(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("không đọc được {}: {e}", path.display())),
    }
}

fn utf8(raw: &Option<Vec<u8>>, path: &Path) -> Result<Option<String>, String> {
    raw.as_ref()
        .map(|b| {
            String::from_utf8(b.clone())
                .map_err(|_| format!("{} không phải UTF-8. Không sửa gì.", path.display()))
        })
        .transpose()
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub path: String,
    pub exists: bool,
    /// Cài rồi và đúng bản hiện tại (cài thêm lần nữa không đổi gì).
    pub up_to_date: bool,
    /// Số hook của Arcturus đang có trong file.
    pub arc_hooks: usize,
    /// Vì sao chưa đúng (thiếu sự kiện, sai cổng, sai token…).
    pub problems: Vec<String>,
    /// File hỏng hoặc không đọc được: không cài/gỡ được.
    pub error: Option<String>,
    pub hub_port: u16,
}

/// So hook đang có với hook mong muốn, giải thích từng chỗ lệch bằng lời.
pub fn problems(settings: &Value, port: u16, token: &str) -> Vec<String> {
    let mut out = Vec::new();
    let want = match arcturus_hooks(port, token) {
        Ok(w) => w,
        Err(e) => return vec![e],
    };
    let have = arc_hooks_by_event(settings);
    if have.is_empty() {
        return vec!["Chưa cài hook của Arcturus".to_owned()];
    }
    let mut wrong_port = Vec::new();
    let mut env_token = false;
    let mut wrong_token = false;
    for (ev, h) in &have {
        if let Some(p) = arc_port(h).filter(|p| *p != port) {
            wrong_port.push(format!("{ev} → cổng {p}"));
        }
        let text = h.to_string();
        if text.contains(TOKEN_VAR) {
            env_token = true;
        } else if !text.contains(token) {
            wrong_token = true;
        }
    }
    let missing: Vec<&str> = want
        .keys()
        .filter(|ev| !have.iter().any(|(e, _)| e == *ev))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        out.push(format!("Thiếu hook cho: {}", missing.join(", ")));
    }
    if !wrong_port.is_empty() {
        out.push(format!(
            "Hook trỏ tới cổng khác hub (hub ở cổng {port}): {}",
            wrong_port.join(", ")
        ));
    }
    if env_token {
        out.push(
            "Hook dùng biến $ARCTURUS_TOKEN (cách cài bằng script): chỉ chạy khi terminal mở claude có đúng biến này. \
             Cài lại để ghi token của app vào hook."
                .to_owned(),
        );
    }
    if wrong_token {
        out.push("Token trong hook khác token hub đang dùng: cài lại để cập nhật.".to_owned());
    }
    let extra = have.len().saturating_sub(want.len());
    if extra > 0 && missing.is_empty() {
        out.push(format!("Có {extra} hook của Arcturus bị lặp"));
    }
    if out.is_empty() {
        out.push(
            "Hook khác bản hiện tại của app (timeout hoặc lệnh): cài lại để cập nhật.".to_owned(),
        );
    }
    out
}

pub fn status(path: &Path, port: u16, token: &str) -> Status {
    let mut st = Status {
        path: path.display().to_string(),
        exists: path.exists(),
        up_to_date: false,
        arc_hooks: 0,
        problems: Vec::new(),
        error: None,
        hub_port: port,
    };
    let text = match read_raw(path).and_then(|raw| utf8(&raw, path)) {
        Ok(t) => t,
        Err(e) => {
            st.error = Some(e);
            return st;
        }
    };
    let settings = match parse_settings(text.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            st.error = Some(e);
            return st;
        }
    };
    st.arc_hooks = arc_hooks_by_event(&settings).len();
    match compute(text.as_deref(), Action::Install, port, token) {
        Ok(c) => st.up_to_date = !c.changed,
        Err(e) => st.error = Some(e),
    }
    if !st.up_to_date && st.error.is_none() {
        st.problems = problems(&settings, port, token);
    }
    st
}

// ---------------------------------------------------------------------------------------------------------------------
// Xem trước và ghi

#[derive(Clone, Debug, Serialize)]
pub struct Preview {
    pub action: Action,
    pub path: String,
    pub changed: bool,
    /// Token đã được che.
    pub diff: Vec<DiffLine>,
    /// Gửi lại khi ghi: file đổi sau lúc xem trước thì không ghi.
    pub digest: String,
}

pub fn preview(path: &Path, action: Action, port: u16, token: &str) -> Result<Preview, String> {
    let raw = read_raw(path)?;
    let text = utf8(&raw, path)?;
    let c = compute(text.as_deref(), action, port, token)?;
    let diff = if c.changed {
        line_diff(&mask(&c.before, token), &mask(&c.after, token))
    } else {
        Vec::new()
    };
    Ok(Preview {
        action,
        path: path.display().to_string(),
        changed: c.changed,
        diff,
        digest: digest(raw.as_deref()),
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct Applied {
    pub changed: bool,
    /// Bản sao lưu đã tạo (không có khi file chưa tồn tại hoặc không có gì đổi).
    pub backup: Option<String>,
}

/// Ghi thay đổi đã xem trước. `expected_digest` khác nội dung hiện tại: không ghi (file vừa bị sửa ở nơi khác).
pub fn apply(
    path: &Path,
    action: Action,
    port: u16,
    token: &str,
    expected_digest: &str,
) -> Result<Applied, String> {
    let raw = read_raw(path)?;
    if digest(raw.as_deref()) != expected_digest {
        return Err(
            "settings.json vừa thay đổi sau lúc xem trước. Xem trước lại rồi hãy cài.".to_owned(),
        );
    }
    let text = utf8(&raw, path)?;
    let c = compute(text.as_deref(), action, port, token)?;
    if !c.changed {
        return Ok(Applied {
            changed: false,
            backup: None,
        });
    }
    let backup = match &raw {
        Some(bytes) => Some(write_backup(path, bytes)?),
        None => {
            if action == Action::Uninstall {
                return Ok(Applied {
                    changed: false,
                    backup: None,
                });
            }
            match path.parent() {
                Some(dir) if dir.is_dir() => None,
                _ => {
                    return Err(format!(
                        "thư mục của {} chưa có (đã chạy claude lần nào chưa?)",
                        path.display()
                    ))
                }
            }
        }
    };
    let restore = |why: String| -> String {
        if let (Some(b), Some(orig)) = (&backup, &raw) {
            if std::fs::write(path, orig).is_ok() {
                return format!("{why}; đã khôi phục từ {}", b.display());
            }
        }
        why
    };
    // Ghi đè tại chỗ: `fs::write` đi theo symlink và giữ quyền của file đang có.
    std::fs::write(path, c.after.as_bytes())
        .map_err(|e| restore(format!("ghi {} thất bại: {e}", path.display())))?;
    let back = std::fs::read_to_string(path)
        .map_err(|e| restore(format!("đọc lại {} thất bại: {e}", path.display())))?;
    if serde_json::from_str::<Value>(&back).is_err() || back != c.after {
        return Err(restore("nội dung ghi ra không khớp".to_owned()));
    }
    Ok(Applied {
        changed: true,
        backup: backup.map(|b| b.display().to_string()),
    })
}

fn write_backup(path: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "settings.json".to_owned());
    for n in 0..100u32 {
        let suffix = if n == 0 {
            format!("{name}.bak-{secs}")
        } else {
            format!("{name}.bak-{secs}-{n}")
        };
        let b = path.with_file_name(suffix);
        if b.exists() {
            continue;
        }
        return write_private_new(&b, bytes)
            .map(|()| b.clone())
            .map_err(|e| format!("không sao lưu được {}: {e}; không sửa gì", path.display()));
    }
    Err("quá nhiều bản sao lưu trong cùng một giây; không sửa gì".to_owned())
}

// ---------------------------------------------------------------------------------------------------------------------
// Tự kiểm tra: settings.json → hub → hook tới được hub. Bước "UI thấy sự kiện" do webview kiểm (nó là người nhận).

#[derive(Clone, Debug, Serialize)]
pub struct Step {
    pub id: &'static str,
    /// `ok`, `warn` (chạy được nhưng nên sửa), `fail`, `skip` (bước trước hỏng).
    pub state: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SelfTest {
    pub steps: Vec<Step>,
    /// Agent UI cần thấy xuất hiện (chỉ có khi đã gửi được sự kiện giả).
    pub agent_id: Option<String>,
    pub nonce: Option<String>,
}

/// Mục đích của một hook http: địa chỉ loopback và đường dẫn.
fn http_target(url: &str) -> Option<(std::net::SocketAddr, String)> {
    let (n, port) = arc_target_at(url)?;
    let path = url
        .get(ARC_PREFIX.len() + port.to_string().len()..n)?
        .to_owned();
    Some((std::net::SocketAddr::from(([127, 0, 0, 1], port)), path))
}

/// Header của hook như Claude Code gửi: `$VAR`/`${VAR}` chỉ được thay khi `VAR` nằm trong `allowedEnvVars`.
fn hook_headers(hook: &Value, env: &dyn Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let allowed: Vec<&str> = hook
        .get("allowedEnvVars")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for (k, v) in hook
        .get("headers")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let Some(v) = v.as_str() else { continue };
        let mut s = v.to_owned();
        for var in &allowed {
            let val = env(var).unwrap_or_default();
            s = s
                .replace(&format!("${{{var}}}"), &val)
                .replace(&format!("${var}"), &val);
        }
        if k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') && !s.contains(['\r', '\n']) {
            out.push((k.clone(), s));
        }
    }
    out
}

fn hook_payload(event: &str, session_id: &str) -> String {
    serde_json::json!({
        "hook_event_name": event,
        "session_id": session_id,
        "prompt": "Arcturus tự kiểm tra",
        "reason": "other",
    })
    .to_string()
}

fn find_on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.exe"), name.to_owned()]
    } else {
        vec![name.to_owned()]
    };
    std::env::split_paths(&paths).any(|d| names.iter().any(|n| d.join(n).is_file()))
}

pub fn valid_nonce(n: &str) -> bool {
    n.len() == 16 && n.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Chạy ba bước đầu của tự kiểm tra và gửi một sự kiện `UserPromptSubmit` giả qua đúng hook đã cài.
pub fn selftest(path: &Path, hub: std::net::SocketAddr, token: &str) -> SelfTest {
    let mut steps = Vec::new();
    let mut result = SelfTest {
        steps: Vec::new(),
        agent_id: None,
        nonce: None,
    };

    // 1. settings.json có hook đúng.
    let st = status(path, hub.port(), token);
    let settings = read_raw(path)
        .and_then(|r| utf8(&r, path))
        .and_then(|t| parse_settings(t.as_deref()))
        .ok();
    let prompt_hook = settings.as_ref().and_then(|s| {
        arc_hooks_by_event(s)
            .into_iter()
            .find(|(ev, h)| {
                ev == "UserPromptSubmit" && h.get("type").and_then(Value::as_str) == Some("http")
            })
            .map(|(_, h)| h)
    });
    let has_session_start = settings.as_ref().is_some_and(|s| {
        arc_hooks_by_event(s).iter().any(|(ev, h)| {
            ev == "SessionStart" && h.get("type").and_then(Value::as_str) == Some("command")
        })
    });
    let settings_step = if let Some(e) = &st.error {
        Step {
            id: "settings",
            state: "fail",
            detail: e.clone(),
        }
    } else if st.up_to_date {
        if has_session_start && !find_on_path("curl") {
            Step {
                id: "settings",
                state: "warn",
                detail: "Hook đã cài, nhưng không thấy curl trong PATH: hook SessionStart (gọi curl) sẽ không gửi được."
                    .to_owned(),
            }
        } else {
            Step {
                id: "settings",
                state: "ok",
                detail: format!("Đã cài {} hook trong {}", st.arc_hooks, st.path),
            }
        }
    } else {
        let state = if prompt_hook.is_some() {
            "warn"
        } else {
            "fail"
        };
        Step {
            id: "settings",
            state,
            detail: st.problems.join(" · "),
        }
    };
    steps.push(settings_step);

    // 2. Hub trả lời.
    let hub_ok = crate::hub_proc::healthy(hub);
    steps.push(if hub_ok {
        Step { id: "hub", state: "ok", detail: format!("Hub đang chạy ở {hub}") }
    } else {
        Step {
            id: "hub",
            state: "fail",
            detail: format!("Hub không trả lời ở {hub}. Thử Thoát app rồi mở lại; xem tooltip của icon để biết lỗi."),
        }
    });

    // 3. Gửi sự kiện giả qua đúng URL và header của hook đã cài.
    let Some(hook) = prompt_hook else {
        steps.push(Step {
            id: "hook",
            state: "skip",
            detail: "Chưa có hook để thử: cài hook trước.".to_owned(),
        });
        result.steps = steps;
        return result;
    };
    if !hub_ok {
        steps.push(Step {
            id: "hook",
            state: "skip",
            detail: "Hub chưa chạy.".to_owned(),
        });
        result.steps = steps;
        return result;
    }
    let Some((addr, hpath)) = hook
        .get("url")
        .and_then(Value::as_str)
        .and_then(http_target)
    else {
        steps.push(Step {
            id: "hook",
            state: "fail",
            detail: "URL của hook không đọc được".to_owned(),
        });
        result.steps = steps;
        return result;
    };
    let mut nonce_bytes = [0u8; 8];
    if getrandom::fill(&mut nonce_bytes).is_err() {
        steps.push(Step {
            id: "hook",
            state: "fail",
            detail: "không sinh được mã kiểm tra".to_owned(),
        });
        result.steps = steps;
        return result;
    }
    let nonce: String = nonce_bytes.iter().map(|b| format!("{b:02x}")).collect();
    let session = format!("{SELFTEST_PREFIX}{nonce}");
    let headers = hook_headers(&hook, &|k| std::env::var(k).ok());
    let body = hook_payload("UserPromptSubmit", &session);
    let resp = http::post_json(
        addr,
        &hpath,
        &headers,
        body.as_bytes(),
        Duration::from_secs(3),
    );
    steps.push(match resp {
        Ok(r) if (200..300).contains(&r.status) => {
            result.agent_id = Some(format!("agt_{session}"));
            result.nonce = Some(nonce);
            Step { id: "hook", state: "ok", detail: format!("Hub nhận sự kiện thử qua {}", hook_url(&hook)) }
        }
        Ok(r) if r.status == 401 => Step {
            id: "hook",
            state: "fail",
            detail: "Hub từ chối token của hook (401). Token trong settings.json khác token hub đang dùng: cài lại hook; \
                     nếu hub do bạn tự chạy thì nó dùng ARCTURUS_TOKEN của terminal đó."
                .to_owned(),
        },
        Ok(r) => Step { id: "hook", state: "fail", detail: format!("Hub trả lỗi {} cho hook", r.status) },
        Err(e) => Step {
            id: "hook",
            state: "fail",
            detail: format!("Hook không tới được hub ({e}). Hook trỏ tới {addr}, hub ở {hub}."),
        },
    });
    result.steps = steps;
    result
}

fn hook_url(hook: &Value) -> String {
    hook.get("url")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_owned()
}

/// Kết thúc phiên giả của tự kiểm tra (agent biến khỏi UI).
pub fn selftest_cleanup(hub: std::net::SocketAddr, token: &str, nonce: &str) -> Result<(), String> {
    if !valid_nonce(nonce) {
        return Err("mã kiểm tra không hợp lệ".to_owned());
    }
    let session = format!("{SELFTEST_PREFIX}{nonce}");
    let headers = vec![("Authorization".to_owned(), format!("Bearer {token}"))];
    let body = hook_payload("SessionEnd", &session);
    let r = http::post_json(
        hub,
        "/hooks/event",
        &headers,
        body.as_bytes(),
        Duration::from_secs(3),
    )?;
    if (200..300).contains(&r.status) {
        Ok(())
    } else {
        Err(format!("hub trả lỗi {}", r.status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";

    fn tmpdir(tag: &str) -> PathBuf {
        let mut b = [0u8; 6];
        getrandom::fill(&mut b).unwrap();
        let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let d = std::env::temp_dir().join(format!("arc-setup-{tag}-{hex}"));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn user_settings() -> Value {
        json!({
            "model": "opus",
            "permissions": { "allow": ["Bash(cargo test:*)"] },
            "env": { "FOO": "1" },
            "hooks": {
                "Stop": [ { "hooks": [ { "type": "command", "command": "echo mine-stop" } ] } ],
                "PreToolUse": [ { "matcher": "Bash", "hooks": [ { "type": "command", "command": "echo mine-pre" } ] } ],
                "Custom": []
            }
        })
    }

    fn arc_count(v: &Value) -> usize {
        arc_hooks_by_event(v).len()
    }

    fn installed(text: Option<&str>) -> Value {
        let c = compute(text, Action::Install, 7878, TOKEN).unwrap();
        serde_json::from_str(&c.after).unwrap()
    }

    #[test]
    fn template_has_12_events_and_safe_targets() {
        let h = arcturus_hooks(7878, TOKEN).unwrap();
        assert_eq!(h.len(), 12);
        let text = Value::Object(h.clone()).to_string();
        assert!(!text.contains("$ARCTURUS_TOKEN"), "token phải được thay");
        assert!(!text.contains("allowedEnvVars"));
        assert_eq!(text.matches(TOKEN).count(), 12, "mọi hook mang token");
        let ss = &h["SessionStart"][0]["hooks"][0];
        assert_eq!(ss["type"], "command");
        assert!(ss["command"]
            .as_str()
            .unwrap()
            .contains(&format!("Bearer {TOKEN}")));
        assert_eq!(
            h["PermissionRequest"][0]["hooks"][0]["url"],
            "http://127.0.0.1:7878/hooks/permission"
        );
        assert_eq!(h["PermissionRequest"][0]["hooks"][0]["timeout"], 120);
        for (_, hook) in arc_hooks_by_event(&json!({ "hooks": h })) {
            assert_eq!(arc_port(&hook), Some(7878));
        }
    }

    #[test]
    fn port_is_substituted_everywhere() {
        let h = arcturus_hooks(9123, TOKEN).unwrap();
        let text = Value::Object(h).to_string();
        assert!(!text.contains(":7878"));
        assert_eq!(text.matches("127.0.0.1:9123/hooks/").count(), 12);
    }

    #[test]
    fn arc_hook_detection_matches_script() {
        let http = |u: &str| json!({ "type": "http", "url": u });
        assert_eq!(
            arc_port(&http("http://127.0.0.1:7878/hooks/event")),
            Some(7878)
        );
        assert_eq!(
            arc_port(&http("http://127.0.0.1:1/hooks/permission")),
            Some(1)
        );
        assert_eq!(
            arc_port(&http("http://127.0.0.1:7878/hooks/eventx")),
            None,
            "http phải khớp trọn"
        );
        assert_eq!(arc_port(&http("http://localhost:7878/hooks/event")), None);
        assert_eq!(arc_port(&http("https://127.0.0.1:7878/hooks/event")), None);
        assert_eq!(arc_port(&http("http://127.0.0.1:/hooks/event")), None);
        assert_eq!(arc_port(&http("http://127.0.0.1:99999/hooks/event")), None);
        assert_eq!(arc_port(&http("http://127.0.0.1:7878/other")), None);
        let cmd = |c: &str| json!({ "type": "command", "command": c });
        assert_eq!(
            arc_port(&cmd("curl -s http://127.0.0.1:7878/hooks/event || true")),
            Some(7878)
        );
        assert_eq!(
            arc_port(&cmd(
                "x http://127.0.0.1:a http://127.0.0.1:5/hooks/permission"
            )),
            Some(5)
        );
        assert_eq!(arc_port(&cmd("echo mine")), None);
        assert_eq!(
            arc_port(&json!({ "type": "prompt", "url": "http://127.0.0.1:7878/hooks/event" })),
            None
        );
    }

    #[test]
    fn install_keeps_user_keys_and_hooks_first() {
        let text = user_settings().to_string();
        let v = installed(Some(&text));
        assert_eq!(arc_count(&v), 12);
        let orig = user_settings();
        for k in ["model", "permissions", "env"] {
            assert_eq!(v[k], orig[k]);
        }
        assert_eq!(
            v["hooks"]["Stop"][0]["hooks"][0]["command"],
            "echo mine-stop"
        );
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "Bash");
        assert_eq!(v["hooks"]["Custom"], json!([]));
        // Thứ tự khóa của người dùng giữ nguyên (preserve_order).
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "permissions", "env", "hooks"]);
    }

    #[test]
    fn install_is_idempotent_and_uninstall_restores() {
        let text = user_settings().to_string();
        let once = compute(Some(&text), Action::Install, 7878, TOKEN).unwrap();
        assert!(once.changed);
        let twice = compute(Some(&once.after), Action::Install, 7878, TOKEN).unwrap();
        assert!(!twice.changed);
        assert_eq!(arc_count(&serde_json::from_str(&twice.after).unwrap()), 12);
        let un = compute(Some(&once.after), Action::Uninstall, 7878, TOKEN).unwrap();
        assert!(un.changed);
        assert_eq!(
            serde_json::from_str::<Value>(&un.after).unwrap(),
            user_settings()
        );
        let un2 = compute(Some(&un.after), Action::Uninstall, 7878, TOKEN).unwrap();
        assert!(!un2.changed);
    }

    #[test]
    fn reinstall_replaces_script_style_and_other_port() {
        // Bản cài bằng script: `$ARCTURUS_TOKEN`, cổng khác. Cài lại thay hẳn, không nhân đôi.
        let mut old = user_settings();
        let tpl: Value = serde_json::from_str(TEMPLATE).unwrap();
        add(&mut old, tpl["hooks"].as_object().unwrap());
        let old_text = old.to_string().replace("7878", "7000");
        let probs = problems(&serde_json::from_str(&old_text).unwrap(), 7878, TOKEN);
        assert!(probs.iter().any(|p| p.contains("cổng khác")), "{probs:?}");
        assert!(
            probs.iter().any(|p| p.contains("$ARCTURUS_TOKEN")),
            "{probs:?}"
        );
        let v = installed(Some(&old_text));
        assert_eq!(arc_count(&v), 12);
        assert!(!v.to_string().contains("7000"));
        assert!(!v.to_string().contains("$ARCTURUS_TOKEN"));
    }

    #[test]
    fn strip_only_drops_entries_emptied_by_strip() {
        let v = json!({
            "hooks": {
                "PermissionRequest": [ { "hooks": [ { "type": "http", "url": "http://127.0.0.1:7878/hooks/permission" } ] } ],
                "Stop": [ { "hooks": [
                    { "type": "command", "command": "echo mine" },
                    { "type": "http", "url": "http://127.0.0.1:7878/hooks/event" }
                ] } ],
                "PostToolUse": [ { "hooks": [ { "type": "http", "url": "http://example.test/hooks/event" } ] } ],
                "Empty": [ { "hooks": [] } ]
            }
        });
        let mut s = v.clone();
        strip(&mut s);
        assert!(s["hooks"].get("PermissionRequest").is_none());
        assert_eq!(
            s["hooks"]["Stop"][0]["hooks"],
            json!([{ "type": "command", "command": "echo mine" }])
        );
        assert_eq!(s["hooks"]["PostToolUse"], v["hooks"]["PostToolUse"]);
        assert_eq!(
            s["hooks"]["Empty"],
            json!([{ "hooks": [] }]),
            "mục rỗng của người dùng giữ nguyên"
        );
        let mut only =
            json!({ "a": 1, "hooks": { "Stop": v["hooks"]["PermissionRequest"].clone() } });
        strip(&mut only);
        assert_eq!(only, json!({ "a": 1 }), "gỡ hết thì bỏ khóa hooks");
        let mut empty_hooks = json!({ "hooks": {} });
        strip(&mut empty_hooks);
        assert_eq!(
            empty_hooks,
            json!({ "hooks": {} }),
            "hooks rỗng sẵn giữ nguyên"
        );
    }

    #[test]
    fn missing_or_blank_file_is_empty_object() {
        for t in [None, Some(""), Some("  \n")] {
            let v = installed(t);
            assert_eq!(arc_count(&v), 12);
            let c = compute(t, Action::Uninstall, 7878, TOKEN).unwrap();
            assert!(!c.changed);
        }
        let c = compute(None, Action::Install, 7878, TOKEN).unwrap();
        let back = compute(Some(&c.after), Action::Uninstall, 7878, TOKEN).unwrap();
        assert_eq!(back.after, "{}\n");
    }

    #[test]
    fn broken_settings_are_never_rewritten() {
        for bad in [
            "{ // chú thích\n}",
            "[1]",
            "{\"hooks\": []}",
            "{\"hooks\": {\"Stop\": {}}}",
            "nope",
        ] {
            assert!(
                compute(Some(bad), Action::Install, 7878, TOKEN).is_err(),
                "{bad}"
            );
            assert!(
                compute(Some(bad), Action::Uninstall, 7878, TOKEN).is_err(),
                "{bad}"
            );
        }
        let d = tmpdir("broken");
        let p = d.join("settings.json");
        std::fs::write(&p, "{ broken").unwrap();
        let st = status(&p, 7878, TOKEN);
        assert!(st.error.is_some());
        let dg = digest(Some(b"{ broken"));
        assert!(apply(&p, Action::Install, 7878, TOKEN, &dg).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ broken");
        assert_eq!(
            std::fs::read_dir(&d).unwrap().count(),
            1,
            "không sao lưu khi không ghi"
        );
    }

    #[test]
    fn apply_backs_up_writes_and_detects_concurrent_edit() {
        let d = tmpdir("apply");
        let p = d.join("settings.json");
        let orig = user_settings().to_string();
        std::fs::write(&p, &orig).unwrap();

        let pv = preview(&p, Action::Install, 7878, TOKEN).unwrap();
        assert!(pv.changed);
        let shown: String = pv.diff.iter().map(|l| l.text.as_str()).collect();
        assert!(!shown.contains(TOKEN), "diff không lộ token");
        assert!(shown.contains("<token …cdef>"));
        assert!(pv.diff.iter().any(|l| l.op == "add"));

        // File bị sửa sau lúc xem trước: không ghi.
        std::fs::write(&p, format!("{orig} ")).unwrap();
        let err = apply(&p, Action::Install, 7878, TOKEN, &pv.digest).unwrap_err();
        assert!(err.contains("vừa thay đổi"));
        std::fs::write(&p, &orig).unwrap();

        let done = apply(&p, Action::Install, 7878, TOKEN, &pv.digest).unwrap();
        assert!(done.changed);
        let backup = PathBuf::from(done.backup.unwrap());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), orig);
        assert!(status(&p, 7878, TOKEN).up_to_date);

        // Cài lại: không đổi, không thêm bản sao lưu.
        let pv2 = preview(&p, Action::Install, 7878, TOKEN).unwrap();
        assert!(!pv2.changed && pv2.diff.is_empty());
        let again = apply(&p, Action::Install, 7878, TOKEN, &pv2.digest).unwrap();
        assert!(!again.changed && again.backup.is_none());
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 2);

        // Gỡ: về đúng nội dung gốc (sau chuẩn hóa).
        let pu = preview(&p, Action::Uninstall, 7878, TOKEN).unwrap();
        apply(&p, Action::Uninstall, 7878, TOKEN, &pu.digest).unwrap();
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(back, user_settings());
        assert!(!status(&p, 7878, TOKEN).up_to_date);
    }

    #[test]
    fn apply_creates_file_only_when_dir_exists() {
        let d = tmpdir("new");
        let p = d.join("settings.json");
        let pv = preview(&p, Action::Install, 7878, TOKEN).unwrap();
        assert_eq!(pv.digest, "absent");
        let r = apply(&p, Action::Install, 7878, TOKEN, &pv.digest).unwrap();
        assert!(r.changed && r.backup.is_none());
        assert!(status(&p, 7878, TOKEN).up_to_date);

        let gone = d.join("khong-co").join("settings.json");
        assert!(apply(&gone, Action::Install, 7878, TOKEN, "absent").is_err());
        let un = apply(&d.join("x.json"), Action::Uninstall, 7878, TOKEN, "absent").unwrap();
        assert!(!un.changed);
        assert!(
            !d.join("x.json").exists(),
            "gỡ khi chưa có file không tạo file"
        );
    }

    #[cfg(unix)]
    #[test]
    fn apply_follows_symlink_and_keeps_mode() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmpdir("link");
        let real = d.join("real.json");
        std::fs::write(&real, "{}").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = d.join("settings.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let pv = preview(&link, Action::Install, 7878, TOKEN).unwrap();
        apply(&link, Action::Install, 7878, TOKEN, &pv.digest).unwrap();
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            arc_count(&serde_json::from_str(&std::fs::read_to_string(&real).unwrap()).unwrap()),
            12
        );
        assert_eq!(
            std::fs::metadata(&real).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn status_explains_problems() {
        let d = tmpdir("status");
        let p = d.join("settings.json");
        let st = status(&p, 7878, TOKEN);
        assert!(!st.exists && !st.up_to_date && st.error.is_none());
        assert_eq!(st.problems, ["Chưa cài hook của Arcturus"]);

        // Thiếu một sự kiện, token khác.
        let mut v = installed(None);
        v["hooks"].as_object_mut().unwrap().remove("Stop");
        let text = v
            .to_string()
            .replace(TOKEN, "ffffffffffffffffffffffffffffffffeeeeeeee0123456789");
        std::fs::write(&p, text).unwrap();
        let st = status(&p, 7878, TOKEN);
        assert!(!st.up_to_date);
        assert_eq!(st.arc_hooks, 11);
        assert!(
            st.problems
                .iter()
                .any(|x| x.contains("Thiếu hook cho: Stop")),
            "{:?}",
            st.problems
        );
        assert!(
            st.problems
                .iter()
                .any(|x| x.contains("Token trong hook khác")),
            "{:?}",
            st.problems
        );
    }

    #[test]
    fn hook_token_prefers_env_then_file_then_generates() {
        let d = tmpdir("token");
        assert_eq!(hook_token(&d, Some(format!(" {TOKEN}\n"))).unwrap(), TOKEN);
        assert!(hook_token(&d, Some("ngắn".to_owned())).is_err());
        assert!(
            hook_token(&d, Some(format!("{TOKEN}\"; rm -rf ~"))).is_err(),
            "ký tự shell bị từ chối"
        );
        assert!(!d.join("hook-token").exists(), "env không ghi file");

        let t = hook_token(&d, None).unwrap();
        assert!(valid_hook_token(&t) && t.len() == 48);
        assert_eq!(
            hook_token(&d, Some(String::new())).unwrap(),
            t,
            "đọc lại từ file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(d.join("hook-token"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(d.join("hook-token"), "aaaa").unwrap();
        assert!(
            hook_token(&d, None).is_err(),
            "file hỏng không bị ghi đè lặng lẽ"
        );
    }

    #[test]
    fn token_validation() {
        assert!(valid_hook_token(TOKEN));
        assert!(!valid_hook_token(&"ab".repeat(20)), "ít ký tự khác nhau");
        assert!(!valid_hook_token("0123456789abcdef"), "ngắn");
        for bad in ["$", "\"", "'", " ", "`", "\\", ";"] {
            assert!(!valid_hook_token(&format!("{TOKEN}{bad}")), "{bad}");
        }
    }

    #[test]
    fn diff_shows_changes_with_context() {
        let a = (1..=20).map(|i| format!("l{i}\n")).collect::<String>();
        let b = a.replace("l10\n", "l10\nmoi\n").replace("l1\n", "");
        let d = line_diff(&a, &b);
        assert_eq!(
            d[0],
            DiffLine {
                op: "del",
                text: "l1".to_owned()
            }
        );
        assert!(d.iter().any(|l| l.op == "add" && l.text == "moi"));
        assert!(d.iter().any(|l| l.op == "skip"));
        assert!(d.iter().filter(|l| l.op == "ctx").count() <= 3 * 3);
        assert!(line_diff("x\n", "x\n")
            .iter()
            .all(|l| l.op != "add" && l.op != "del"));
        let big = "x\n".repeat(DIFF_MAX_LINES + 1);
        assert_eq!(line_diff(&big, "")[0].op, "skip");
    }

    #[test]
    fn headers_expand_only_allowed_env() {
        let env = |k: &str| (k == "ARCTURUS_TOKEN" || k == "HOME").then(|| format!("v-{k}"));
        let h = json!({ "headers": { "Authorization": "Bearer $ARCTURUS_TOKEN", "X-Home": "${HOME}", "Bad\nKey": "x" },
                        "allowedEnvVars": ["ARCTURUS_TOKEN"] });
        let out = hook_headers(&h, &env);
        assert_eq!(
            out,
            [
                (
                    "Authorization".to_owned(),
                    "Bearer v-ARCTURUS_TOKEN".to_owned()
                ),
                ("X-Home".to_owned(), "${HOME}".to_owned())
            ]
        );
    }

    #[test]
    fn http_target_parses_hook_url() {
        let (a, p) = http_target("http://127.0.0.1:7878/hooks/event").unwrap();
        assert_eq!(
            (a.to_string().as_str(), p.as_str()),
            ("127.0.0.1:7878", "/hooks/event")
        );
        assert!(http_target("http://localhost:7878/hooks/event").is_none());
        assert!(valid_nonce("0123456789abcdef") && !valid_nonce("../../x") && !valid_nonce("0123"));
    }

    /// Hub giả: nhận một kết nối, trả `status`, ghi lại request.
    fn fake_hub(status: u16) -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            let mut reqs = String::new();
            // /healthz rồi /hooks/event.
            for _ in 0..2 {
                let (mut s, _) = l.accept().unwrap();
                let mut buf = vec![0u8; 8192];
                let mut got = Vec::new();
                loop {
                    let n = s.read(&mut buf).unwrap();
                    got.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&got).into_owned();
                    if let Some(i) = text.find("\r\n\r\n") {
                        let len = text[..i]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if got.len() >= i + 4 + len {
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&got).into_owned();
                let code = if text.starts_with("GET /healthz") {
                    200
                } else {
                    status
                };
                write!(
                    s,
                    "HTTP/1.1 {code} X\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                )
                .unwrap();
                reqs.push_str(&text);
            }
            reqs
        });
        (addr, h)
    }

    #[test]
    fn selftest_sends_through_installed_hook() {
        let (addr, hub) = fake_hub(204);
        let d = tmpdir("selftest");
        let p = d.join("settings.json");
        let c = compute(None, Action::Install, addr.port(), TOKEN).unwrap();
        std::fs::write(&p, c.after).unwrap();
        let r = selftest(&p, addr, TOKEN);
        let reqs = hub.join().unwrap();
        assert_eq!(
            r.steps.iter().map(|s| s.id).collect::<Vec<_>>(),
            ["settings", "hub", "hook"]
        );
        assert!(
            r.steps.iter().all(|s| s.state == "ok" || s.state == "warn"),
            "{:?}",
            r.steps
        );
        assert_eq!(r.steps[2].state, "ok");
        let nonce = r.nonce.unwrap();
        assert!(valid_nonce(&nonce));
        assert_eq!(r.agent_id.unwrap(), format!("agt_{SELFTEST_PREFIX}{nonce}"));
        assert!(reqs.contains("POST /hooks/event HTTP/1.1"));
        assert!(reqs.contains(&format!("Authorization: Bearer {TOKEN}")));
        assert!(reqs.contains("\"hook_event_name\":\"UserPromptSubmit\""));
    }

    #[test]
    fn selftest_reports_wrong_token_and_missing_hooks() {
        let (addr, hub) = fake_hub(401);
        let d = tmpdir("selftest401");
        let p = d.join("settings.json");
        std::fs::write(
            &p,
            compute(None, Action::Install, addr.port(), TOKEN)
                .unwrap()
                .after,
        )
        .unwrap();
        let r = selftest(&p, addr, TOKEN);
        hub.join().unwrap();
        assert_eq!(r.steps[2].state, "fail");
        assert!(r.steps[2].detail.contains("401"));
        assert!(r.agent_id.is_none());

        // Chưa cài: bước settings hỏng, bước hook bỏ qua, không gửi gì.
        let none = d.join("none.json");
        let unused = std::net::SocketAddr::from(([127, 0, 0, 1], 9));
        let r = selftest(&none, unused, TOKEN);
        assert_eq!(r.steps[0].state, "fail");
        assert_eq!(r.steps[1].state, "fail");
        assert_eq!(r.steps[2].state, "skip");
    }

    /// Đầu-cuối với hub thật (cần `cargo build -p hub` ở workspace gốc): cài hook vào settings tạm, tự kiểm tra, rồi
    /// nghe WebSocket như UI: agent thử phải xuất hiện, và biến mất sau khi dọn. Token khác thì bước hook báo 401.
    /// Chạy: `cargo test --manifest-path crates/app/src-tauri/Cargo.toml -- --ignored real_hub`.
    #[test]
    #[ignore]
    fn selftest_against_real_hub() {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::Message;

        let d = tmpdir("realhub");
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        // Hub con chỉ ghi vào thư mục tạm (cơ sở dữ liệu, token UI).
        std::env::set_var("ARCTURUS_HUB_ADDR", format!("127.0.0.1:{port}"));
        std::env::set_var("XDG_DATA_HOME", d.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", d.join("config"));
        let (info, child) = crate::hub_proc::connect(Some(TOKEN)).unwrap();
        assert!(child.is_some(), "phải là hub con do test khởi động");
        let p = d.join("settings.json");
        std::fs::write(
            &p,
            compute(None, Action::Install, port, TOKEN).unwrap().after,
        )
        .unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut req = info.ws_url().into_client_request().unwrap();
            req.headers_mut().insert(
                "authorization",
                format!("Bearer {}", info.token).parse().unwrap(),
            );
            let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
            let first = tokio::time::timeout(Duration::from_secs(5), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let first: Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
            assert_eq!(first["t"], "snapshot");

            let r = tokio::task::spawn_blocking({
                let p = p.clone();
                move || selftest(&p, info.addr, TOKEN)
            })
            .await
            .unwrap();
            assert!(
                r.steps.iter().all(|s| s.state == "ok" || s.state == "warn"),
                "{:?}",
                r.steps
            );
            let agent = r.agent_id.clone().unwrap();
            let mut seen = false;
            for _ in 0..20 {
                let m = tokio::time::timeout(Duration::from_secs(5), ws.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                let Message::Text(t) = m else { continue };
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["t"] == "agent_updated" && v["agent"]["id"] == agent.as_str() {
                    seen = true;
                    break;
                }
            }
            assert!(seen, "UI phải thấy agent thử {agent}");

            let nonce = r.nonce.clone().unwrap();
            let addr = info.addr;
            tokio::task::spawn_blocking(move || selftest_cleanup(addr, TOKEN, &nonce))
                .await
                .unwrap()
                .unwrap();
            let mut removed = false;
            for _ in 0..20 {
                let m = tokio::time::timeout(Duration::from_secs(5), ws.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                let Message::Text(t) = m else { continue };
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["t"] == "agent_removed" && v["id"] == agent.as_str() {
                    removed = true;
                    break;
                }
            }
            assert!(removed, "dọn xong thì agent thử biến mất");
        });

        // Token trong settings khác token của hub: hub từ chối, tự kiểm tra chỉ ra đúng chỗ.
        let other = "f0e1d2c3b4a5968778695a4b3c2d1e0ff0e1d2c3b4a59687";
        std::fs::write(
            &p,
            compute(None, Action::Install, port, other).unwrap().after,
        )
        .unwrap();
        let r = selftest(&p, info.addr, TOKEN);
        assert_eq!(r.steps[0].state, "warn", "{:?}", r.steps);
        assert_eq!(r.steps[2].state, "fail");
        assert!(r.steps[2].detail.contains("401"), "{}", r.steps[2].detail);
        drop(child);
    }
}
