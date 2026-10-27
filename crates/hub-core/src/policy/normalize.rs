//! Chuẩn hóa lệnh trước khi so luật (`docs/PLAN.md` T8).
//!
//! Cùng một lệnh có hai dạng:
//! - **Hiển thị** ([`display`], [`escape_hidden`]): giữ nguyên mọi ký tự, ký tự ẩn hiện thành `⟨U+XXXX⟩`. Đây là
//!   `command_normalized` trên thẻ duyệt (quy tắc bất biến 3): người duyệt thấy cả ký tự ẩn, không ai "dọn" giùm.
//! - **So khớp** ([`normalize`]): bỏ ký tự vô hình, đổi khoảng trắng Unicode và tab thành dấu cách, gộp dấu cách, rồi
//!   tách đoạn lệnh và từ như shell (bỏ dấu nháy và `\`). Luật danger chạy trên dạng này; vị trí khớp đổi ngược về chỉ
//!   số ký tự của lệnh gốc ([`Normalized::original_chars`]) rồi sang dạng hiển thị ([`Display::span`]).
//!
//! Hướng an toàn: mọi phép biến đổi chỉ được làm lệnh trông NGUY HIỂM HƠN, không bao giờ che một mẫu nguy hiểm (ghép
//! `r⟨U+200B⟩m` thành `rm`; tách `rm⟨U+00A0⟩-rf` thành hai từ; phân tích lại cả nội dung trong dấu nháy).

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use unicode_general_category::{get_general_category, GeneralCategory};

use crate::protocol::Suspicion;

/// Độ sâu tối đa khi phân tích lại nội dung trong dấu nháy (`bash -c "…"`, `ssh host '…'`).
const MAX_NESTING: u8 = 4;

/// Lệnh dài hơn chừng này (byte UTF-8) không được tách từ (bảng vị trí tốn khoảng 12 byte mỗi ký tự): [`normalize`]
/// trả `too_long = true` và policy coi như Critical. Lệnh Bash dài hơn `handoff_max_bytes` (mặc định 4.096) đã được
/// trả về terminal trước khi tới policy, nên chỉ chạm ngưỡng này khi cấu hình nới `handoff_max_bytes`.
pub const MAX_ANALYZED_BYTES: usize = 64 * 1024;

// ---------------------------------------------------------------------------------------------------------------
// Ký tự ẩn
// ---------------------------------------------------------------------------------------------------------------

/// Ký tự không hiện ra (hoặc trông như khoảng trắng nhưng shell không tách từ) trên thẻ duyệt.
///
/// Gồm: Cc (trừ `\n`, `\t`); toàn bộ Default_Ignorable_Code_Point; Cf; Zl/Zp; Co (vùng riêng); Cn (chưa gán, theo
/// Unicode 16 của `unicode-general-category`: ký tự được gán ở bản Unicode mới hơn cũng bị gắn cờ, an toàn); khoảng
/// trắng không phải ASCII (Zs trừ U+0020) và U+2800.
pub fn is_hidden(c: char) -> bool {
    is_blank_like(c) || is_invisible(c)
}

/// Trông như khoảng trắng (hoặc trống) nhưng shell không tách từ. Khi so khớp: đổi thành dấu cách.
fn is_blank_like(c: char) -> bool {
    matches!(
        c,
        '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{2800}'
            | '\u{2028}'..='\u{2029}'
            // Hangul filler (Default_Ignorable nhưng hiện như ô trống)
            | '\u{115F}'..='\u{1160}'
            | '\u{3164}'
            | '\u{FFA0}'
    )
}

/// Không hiện ra. Khi so khớp: bỏ hẳn.
fn is_invisible(c: char) -> bool {
    if c == '\n' || c == '\t' {
        return false;
    }
    matches!(
        c,
        '\u{0000}'..='\u{001F}'
            | '\u{007F}'..='\u{009F}'
            // Default_Ignorable_Code_Point (DerivedCoreProperties) ngoài Cf
            | '\u{034F}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{2065}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FFF0}'..='\u{FFF8}'
            | '\u{E0000}'..='\u{E0FFF}'
    ) || (!c.is_ascii()
        && matches!(
            get_general_category(c),
            GeneralCategory::Control
                | GeneralCategory::Format
                | GeneralCategory::PrivateUse
                | GeneralCategory::Unassigned
                | GeneralCategory::Surrogate
        ))
}

/// Có ký tự ẩn trong bất kỳ khóa hay chuỗi nào của `v` (trước khi serde đổi ký tự điều khiển thành `\uXXXX`).
pub fn value_has_hidden(v: &Value) -> bool {
    match v {
        Value::String(s) => s.chars().any(is_hidden),
        Value::Array(a) => a.iter().any(value_has_hidden),
        Value::Object(o) => o
            .iter()
            .any(|(k, v)| k.chars().any(is_hidden) || value_has_hidden(v)),
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Dạng hiển thị
// ---------------------------------------------------------------------------------------------------------------

/// Dạng hiển thị của một chuỗi kèm bảng đổi vị trí (để tô sáng `danger_spans`).
pub struct Display {
    pub text: String,
    /// Có ký tự ẩn được thay.
    pub found: bool,
    /// Phần tử `i`: khoảng chỉ số ký tự trong `text` thể hiện ký tự thứ `i` của chuỗi gốc. Chuỗi lặp cùng ký tự ẩn
    /// (`⟨U+XXXX×N⟩`) dùng chung một khoảng.
    map: Vec<(u32, u32)>,
}

impl Display {
    /// Đổi khoảng `[start, end)` theo chỉ số ký tự của chuỗi gốc sang chỉ số ký tự trong [`Display::text`].
    pub fn span(&self, orig: Range<usize>) -> Option<(usize, usize)> {
        if orig.start >= orig.end || orig.end > self.map.len() {
            return None;
        }
        let start = self.map[orig.start].0 as usize;
        let end = self.map[orig.end - 1].1 as usize;
        Some((start, end))
    }
}

/// Dạng hiển thị kèm bảng vị trí. Chuỗi gốc phải ngắn hơn 4 GiB (body hook tối đa 1 MB).
pub fn display(s: &str) -> Display {
    let mut map = Vec::with_capacity(s.len());
    let (text, found) = escape_impl(s, Some(&mut map));
    Display { text, found, map }
}

/// Thay ký tự ẩn bằng `⟨U+XXXX⟩`, chuỗi lặp cùng ký tự thành `⟨U+XXXX×N⟩` (không phình gấp 12 lần). Trả `true`
/// nếu có thay.
pub fn escape_hidden(s: &str) -> (String, bool) {
    escape_impl(s, None)
}

fn escape_impl(s: &str, mut map: Option<&mut Vec<(u32, u32)>>) -> (String, bool) {
    let mut out = String::with_capacity(s.len());
    let mut out_chars: u32 = 0;
    let mut found = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if !is_hidden(c) {
            out.push(c);
            if let Some(m) = map.as_deref_mut() {
                m.push((out_chars, out_chars + 1));
            }
            out_chars += 1;
            continue;
        }
        found = true;
        let mut run = 1usize;
        while chars.next_if_eq(&c).is_some() {
            run += 1;
        }
        let marker = if run == 1 {
            format!("⟨U+{:04X}⟩", u32::from(c))
        } else {
            format!("⟨U+{:04X}×{run}⟩", u32::from(c))
        };
        let len = u32::try_from(marker.chars().count()).unwrap_or(u32::MAX);
        out.push_str(&marker);
        if let Some(m) = map.as_deref_mut() {
            m.extend(std::iter::repeat_n((out_chars, out_chars + len), run));
        }
        out_chars = out_chars.saturating_add(len);
    }
    (out, found)
}

// ---------------------------------------------------------------------------------------------------------------
// Dạng so khớp
// ---------------------------------------------------------------------------------------------------------------

/// Chiều của một chuyển hướng (`>`, `>>`, `&>` là `Out`; `<` là `In`). `<<` (heredoc) và `<<<` không có đích là file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Redirect {
    In,
    Out,
}

/// Một từ của shell sau khi bỏ dấu nháy và `\`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    /// Giá trị shell thấy (đã bỏ dấu nháy, `\`). Biến (`$X`) giữ nguyên.
    pub value: String,
    /// Vị trí byte trong [`Normalized::text`], gồm cả dấu nháy.
    pub range: Range<usize>,
    /// Từ là đích của chuyển hướng.
    pub redirect: Option<Redirect>,
}

/// Một đoạn lệnh: phần giữa các dấu `;`, `&&`, `||`, `|`, `&`, xuống dòng, `$(`, `` ` ``, `(`, `)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub words: Vec<Word>,
    /// Vị trí byte trong [`Normalized::text`].
    pub range: Range<usize>,
    /// 0: lệnh chính; >0: phân tích lại từ nội dung trong dấu nháy.
    pub depth: u8,
}

/// Một lần gọi chương trình trong một đoạn, sau khi bỏ gán biến (`A=1`), từ khóa (`if`, `then`, `{`…) và lệnh bọc
/// (`sudo`, `env`, `xargs`, `timeout`…). `find … -exec rm …` cho thêm một lần gọi.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    /// Tên chương trình: phần sau `/` hoặc `\` cuối, chữ thường, bỏ `.exe`.
    pub name: String,
    pub segment: usize,
    /// Chỉ số từ của tên chương trình trong đoạn.
    pub cmd: usize,
    /// Chỉ số các từ đối số trong đoạn.
    pub args: Range<usize>,
    /// Lệnh bọc đứng trước (`sudo`, `env`…), chữ thường.
    pub wrappers: Vec<String>,
    /// Tên chương trình không xác định được lúc phân tích (có `$`, `` ` ``, hoặc rỗng).
    pub dynamic: bool,
}

/// Lệnh sau chuẩn hóa (`docs/PLAN.md` T8). Khác bản phác ở PLAN: `segments` là [`Segment`] (từ đã tách, vị trí) thay
/// vì chuỗi, thêm [`Invocation`] và cờ `unparsed`/`too_long`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Normalized {
    pub text: String,
    pub segments: Vec<Segment>,
    pub invocations: Vec<Invocation>,
    /// Thứ tự cố định: HiddenChars, Subshell, Eval, Encoded; không trùng.
    pub suspicious: Vec<Suspicion>,
    /// Không phân tích trọn được: dấu nháy chưa đóng, `\` cuối lệnh.
    pub unparsed: bool,
    /// Dài hơn [`MAX_ANALYZED_BYTES`]: không tách từ (`segments`, `invocations` rỗng).
    pub too_long: bool,
    /// Với mỗi ký tự của `text` theo thứ tự: (byte trong `text`, khoảng chỉ số ký tự trong lệnh gốc).
    origin: Vec<(u32, u32, u32)>,
}

impl Normalized {
    /// Đổi khoảng byte trong [`Normalized::text`] sang khoảng chỉ số ký tự trong lệnh gốc. Ký tự đã bị bỏ nằm giữa
    /// khoảng được tính vào.
    pub fn original_chars(&self, bytes: Range<usize>) -> Option<Range<usize>> {
        let first = self
            .origin
            .partition_point(|o| (o.0 as usize) < bytes.start);
        let end = self.origin.partition_point(|o| (o.0 as usize) < bytes.end);
        if first >= end {
            return None;
        }
        Some(self.origin[first].1 as usize..self.origin[end - 1].2 as usize)
    }

    /// Các từ đối số của một lần gọi.
    pub fn args(&self, inv: &Invocation) -> &[Word] {
        self.segments
            .get(inv.segment)
            .and_then(|s| s.words.get(inv.args.clone()))
            .unwrap_or(&[])
    }

    /// Vị trí byte của một lần gọi trong `text`: từ tên chương trình tới hết đối số cuối.
    pub fn span(&self, inv: &Invocation) -> Range<usize> {
        let Some(seg) = self.segments.get(inv.segment) else {
            return 0..0;
        };
        let start = seg
            .words
            .get(inv.cmd)
            .map_or(seg.range.start, |w| w.range.start);
        let last = if inv.args.is_empty() {
            inv.cmd
        } else {
            inv.args.end - 1
        };
        let end = seg.words.get(last).map_or(seg.range.end, |w| w.range.end);
        start..end
    }
}

/// Chuẩn hóa một lệnh shell (Bash, PowerShell) để so luật.
pub fn normalize(cmd: &str) -> Normalized {
    let mut suspicious = quick_suspicions(cmd);
    if cmd.len() > MAX_ANALYZED_BYTES {
        return Normalized {
            text: String::new(),
            segments: Vec::new(),
            invocations: Vec::new(),
            suspicious,
            unparsed: true,
            too_long: true,
            origin: Vec::new(),
        };
    }
    let (text, origin) = fold(cmd);
    let mut lexer = Lexer {
        text: &text,
        segments: Vec::new(),
        unparsed: false,
    };
    lexer.lex(0..text.len(), 0);
    let Lexer {
        segments, unparsed, ..
    } = lexer;
    let invocations = invocations(&segments);
    if invocations.iter().any(|i| {
        matches!(
            i.name.as_str(),
            "eval" | "source" | "." | "iex" | "invoke-expression"
        )
    }) {
        suspicious.push(Suspicion::Eval);
    }
    sort_suspicions(&mut suspicious);
    Normalized {
        text,
        segments,
        invocations,
        suspicious,
        unparsed,
        too_long: false,
        origin,
    }
}

/// Cờ tính thẳng trên chữ, không tách từ, không cấp phát theo độ dài lệnh: dùng cho thẻ "duyệt trong terminal" (lệnh
/// tới 1 MB) và làm phần nền của [`normalize`].
pub fn quick_suspicions(cmd: &str) -> Vec<Suspicion> {
    static SUBSHELL: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"\$\(|`|[<>]\(").ok());
    static EVAL: LazyLock<Option<Regex>> = LazyLock::new(|| {
        Regex::new(r"(?mi)(?:^|[;&|(){}`]|\$\(|\b(?:then|do|else|sudo|exec|xargs|command|builtin)\s)\s*(?:eval|source|\.|iex|invoke-expression)(?:\s|$)")
            .ok()
    });
    static ENCODED: LazyLock<Option<Regex>> = LazyLock::new(|| {
        Regex::new(concat!(
            r"(?i)\bbase(?:32|64)\b[^|;&\n]*\s(?:-[a-z]*d|--decode)\b",
            r"|\bxxd\b[^|;&\n]*\s-[a-z]*r",
            r"|\bprintf\b[^|;&\n]*\\(?:x[0-9a-f]|[0-7]{3}|u[0-9a-f]{4})",
            r"|\becho\b[^|;&\n]*\s-[a-z]*e[a-z]*\s[^|;&\n]*\\(?:x[0-9a-f]|[0-7]{3})",
            r"|\$'[^']*\\(?:x[0-9a-f]|[0-7]{3}|u[0-9a-f]{4})",
            r"|\bopenssl\b[^|;&\n]*\s(?:enc|base64)\b[^|;&\n]*\s-d\b",
            r"|frombase64string",
            r"|\b(?:powershell|pwsh)(?:\.exe)?\b[^|;&\n]*\s-e(?:c|nc|ncodedcommand)?\s",
        ))
        .ok()
    });
    let mut out = Vec::new();
    if cmd.chars().any(is_hidden) {
        out.push(Suspicion::HiddenChars);
    }
    // Regex không biên dịch được (không xảy ra với mẫu cố định): coi như khớp, an toàn.
    let hit = |re: &Option<Regex>| re.as_ref().is_none_or(|re| re.is_match(cmd));
    if hit(&SUBSHELL) {
        out.push(Suspicion::Subshell);
    }
    if hit(&EVAL) {
        out.push(Suspicion::Eval);
    }
    if hit(&ENCODED) {
        out.push(Suspicion::Encoded);
    }
    out
}

fn sort_suspicions(v: &mut Vec<Suspicion>) {
    let rank = |s: &Suspicion| match s {
        Suspicion::HiddenChars => 0,
        Suspicion::Subshell => 1,
        Suspicion::Eval => 2,
        Suspicion::Encoded => 3,
    };
    v.sort_by_key(rank);
    v.dedup();
}

/// Bước 1–2 của PLAN T8: bỏ ký tự vô hình, khoảng trắng Unicode và tab thành dấu cách, gộp dấu cách.
fn fold(cmd: &str) -> (String, Vec<(u32, u32, u32)>) {
    let mut text = String::with_capacity(cmd.len());
    let mut origin: Vec<(u32, u32, u32)> = Vec::with_capacity(cmd.len());
    for (i, c) in cmd.chars().enumerate() {
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        let c = if c == '\t' || is_blank_like(c) {
            ' '
        } else if is_invisible(c) {
            continue;
        } else {
            c
        };
        if c == ' ' && text.ends_with(' ') {
            if let Some(last) = origin.last_mut() {
                last.2 = i + 1;
            }
            continue;
        }
        let at = u32::try_from(text.len()).unwrap_or(u32::MAX);
        origin.push((at, i, i + 1));
        text.push(c);
    }
    (text, origin)
}

// ---------------------------------------------------------------------------------------------------------------
// Tách từ
// ---------------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct Builder {
    words: Vec<Word>,
    value: String,
    start: Option<usize>,
    end: usize,
    redirect: Option<Redirect>,
    /// Chuyển hướng vừa gặp, áp cho từ kế tiếp.
    pending: Option<Redirect>,
}

impl Builder {
    fn touch(&mut self, at: usize, end: usize) {
        if self.start.is_none() {
            self.start = Some(at);
            self.redirect = self.pending.take();
        }
        self.end = end;
    }

    fn push(&mut self, c: char, at: usize, end: usize) {
        self.touch(at, end);
        self.value.push(c);
    }

    fn end_word(&mut self) {
        if let Some(start) = self.start.take() {
            self.words.push(Word {
                value: std::mem::take(&mut self.value),
                range: start..self.end,
                redirect: self.redirect.take(),
            });
        }
    }

    fn end_segment(&mut self, out: &mut Vec<Segment>, depth: u8) {
        self.end_word();
        self.pending = None;
        if let (Some(first), Some(last)) = (self.words.first(), self.words.last()) {
            let range = first.range.start..last.range.end;
            out.push(Segment {
                words: std::mem::take(&mut self.words),
                range,
                depth,
            });
        }
    }

    /// Từ đang dở chỉ gồm chữ số và nằm sát toán tử (`2>`): đó là số fd, không phải từ.
    fn drop_fd_number(&mut self, at: usize) -> bool {
        if self.start.is_some()
            && self.end == at
            && !self.value.is_empty()
            && self.value.bytes().all(|b| b.is_ascii_digit())
        {
            self.start = None;
            self.value.clear();
            self.redirect = None;
            return true;
        }
        false
    }
}

struct Lexer<'a> {
    text: &'a str,
    segments: Vec<Segment>,
    unparsed: bool,
}

impl Lexer<'_> {
    fn lex(&mut self, range: Range<usize>, depth: u8) {
        let text = self.text;
        let Some(s) = text.get(range.clone()) else {
            return;
        };
        let base = range.start;
        let mut nested: Vec<Range<usize>> = Vec::new();
        let mut b = Builder::default();
        let mut it = s.char_indices().peekable();
        while let Some((i, c)) = it.next() {
            let at = base + i;
            let end = at + c.len_utf8();
            let peek = it.peek().map(|&(_, n)| n);
            match c {
                ' ' => b.end_word(),
                '\n' | ';' | '(' | ')' | '`' => b.end_segment(&mut self.segments, depth),
                '&' => {
                    b.end_word();
                    match peek {
                        Some('&') => {
                            it.next();
                            b.end_segment(&mut self.segments, depth);
                        }
                        Some('>') => {
                            it.next();
                            if it.peek().map(|&(_, n)| n) == Some('>') {
                                it.next();
                            }
                            b.pending = Some(Redirect::Out);
                        }
                        _ => b.end_segment(&mut self.segments, depth),
                    }
                }
                '|' => {
                    if matches!(peek, Some('|' | '&')) {
                        it.next();
                    }
                    b.end_segment(&mut self.segments, depth);
                }
                '$' if peek == Some('(') => {
                    it.next();
                    b.end_segment(&mut self.segments, depth);
                }
                '$' if peek == Some('\'') => {
                    // ANSI-C `$'…'`: nội dung có mã thoát (cờ Encoded do `quick_suspicions`); giữ nguyên chữ.
                    it.next();
                    b.touch(at, end + 1);
                    let start = end + 1 - base;
                    let close = find_single_close(&s[start..]);
                    let (content_end, next) = match close {
                        Some(j) => (start + j, start + j + 1),
                        None => {
                            self.unparsed = true;
                            (s.len(), s.len())
                        }
                    };
                    b.value.push_str(&s[start..content_end]);
                    b.touch(at, base + next);
                    skip_to(&mut it, next);
                }
                '<' | '>' => {
                    if peek == Some('(') {
                        // Thay thế tiến trình `<(…)`, `>(…)`.
                        it.next();
                        b.end_segment(&mut self.segments, depth);
                        continue;
                    }
                    if !b.drop_fd_number(at) {
                        b.end_word();
                    }
                    let mut kind = if c == '<' {
                        Redirect::In
                    } else {
                        Redirect::Out
                    };
                    let mut target = true;
                    match (c, peek) {
                        ('<', Some('<')) => {
                            // Heredoc `<<`, `<<-`, herestring `<<<`: từ kế tiếp không phải file.
                            it.next();
                            if matches!(it.peek().map(|&(_, n)| n), Some('<' | '-')) {
                                it.next();
                            }
                            target = false;
                        }
                        ('<', Some('>')) => {
                            it.next();
                            kind = Redirect::Out;
                        }
                        ('>', Some('>' | '|')) => {
                            it.next();
                        }
                        _ => {}
                    }
                    if it.peek().map(|&(_, n)| n) == Some('&') {
                        it.next();
                        // `>&2`, `<&0`, `>&-`: nhân bản fd, không có file.
                        if matches!(it.peek().map(|&(_, n)| n), Some('0'..='9' | '-')) {
                            while matches!(it.peek().map(|&(_, n)| n), Some('0'..='9' | '-')) {
                                it.next();
                            }
                            target = false;
                        }
                    }
                    b.pending = target.then_some(kind);
                }
                '\'' => {
                    b.touch(at, end);
                    let start = end - base;
                    let (content_end, next) = match find_single_close(&s[start..]) {
                        Some(j) => (start + j, start + j + 1),
                        None => {
                            self.unparsed = true;
                            (s.len(), s.len())
                        }
                    };
                    b.value.push_str(&s[start..content_end]);
                    b.touch(at, base + next);
                    nested.push(base + start..base + content_end);
                    skip_to(&mut it, next);
                }
                '"' => {
                    b.touch(at, end);
                    let start = end - base;
                    let mut j = start;
                    let mut closed = false;
                    let bytes = s.as_bytes();
                    while j < s.len() {
                        match bytes[j] {
                            b'"' => {
                                closed = true;
                                break;
                            }
                            b'\\' if j + 1 < s.len() => {
                                let next = bytes[j + 1];
                                if matches!(next, b'"' | b'\\' | b'$' | b'`') {
                                    b.value.push(char::from(next));
                                    j += 2;
                                } else if next == b'\n' {
                                    j += 2;
                                } else {
                                    b.value.push('\\');
                                    j += 1;
                                }
                            }
                            _ => {
                                // Không cắt giữa ký tự nhiều byte.
                                let ch_len = s[j..].chars().next().map_or(1, char::len_utf8);
                                b.value.push_str(&s[j..j + ch_len]);
                                j += ch_len;
                            }
                        }
                    }
                    if !closed {
                        self.unparsed = true;
                    }
                    let next = if closed { j + 1 } else { s.len() };
                    b.touch(at, base + next);
                    nested.push(base + start..base + j.min(s.len()));
                    skip_to(&mut it, next);
                }
                '\\' => match peek {
                    None => self.unparsed = true,
                    Some('\n') => {
                        it.next();
                    }
                    Some(n) => {
                        it.next();
                        b.push(n, at, end + n.len_utf8());
                    }
                },
                // Brace expansion không trong dấu nháy (`.e{nv,}`, `{rm,-rf,~}`, `{1..3}`): shell đổi chữ trước khi chạy nên không phân
                // tích được theo chữ; coi là không phân tích được (risk High, nhóm ghép) thay vì để lọt xuống Low/Medium.
                // Chỉ ở cấp ngoài cùng: chuỗi trong dấu nháy cũng được lex lại như mã lồng nhau (để thấy `bash -c '…'`) nên `{a,b}`
                // trong dữ liệu (JSON, jq, thông điệp commit) không phải brace expansion.
                '{' if depth == 0 && is_brace_expansion(&s[i..]) => {
                    self.unparsed = true;
                    b.push(c, at, end);
                }
                '#' if b.start.is_none() => {
                    while let Some(&(_, n)) = it.peek() {
                        if n == '\n' {
                            break;
                        }
                        it.next();
                    }
                }
                _ => b.push(c, at, end),
            }
        }
        b.end_segment(&mut self.segments, depth);
        if depth < MAX_NESTING {
            for r in nested {
                if !r.is_empty() {
                    self.lex(r, depth + 1);
                }
            }
        }
    }
}

/// `s` bắt đầu bằng `{`: có phải brace expansion (`{a,b}` hoặc `{1..3}`, không có khoảng trắng hay toán tử shell giữa hai ngoặc)?
fn is_brace_expansion(s: &str) -> bool {
    let mut depth = 0usize;
    let mut separated = false;
    let mut prev = '\0';
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return separated;
                }
            }
            ',' if depth >= 1 => separated = true,
            '.' if depth >= 1 && prev == '.' => separated = true,
            c if c.is_whitespace()
                || matches!(c, ';' | '|' | '&' | '<' | '>' | '(' | ')' | '`') =>
            {
                return false;
            }
            _ => {}
        }
        prev = c;
    }
    false
}

/// Dấu nháy đơn đóng đầu tiên trong `s` (trong `'…'` không có thoát; `$'…'` cho phép `\'`).
fn find_single_close(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut j = 0;
    while j < bytes.len() {
        match bytes[j] {
            b'\'' => return Some(j),
            _ => j += 1,
        }
    }
    None
}

/// Bỏ qua các ký tự có vị trí byte (tương đối) nhỏ hơn `to`.
fn skip_to(it: &mut std::iter::Peekable<std::str::CharIndices<'_>>, to: usize) {
    while let Some(&(k, _)) = it.peek() {
        if k >= to {
            break;
        }
        it.next();
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Lần gọi chương trình
// ---------------------------------------------------------------------------------------------------------------

/// Từ khóa shell đứng trước lệnh.
const KEYWORDS: [&str; 12] = [
    "if", "then", "do", "else", "elif", "while", "until", "!", "{", "}", "time", "coproc",
];

/// Lệnh bọc: chạy chương trình ở đối số đầu tiên không phải tùy chọn.
const WRAPPERS: [&str; 18] = [
    "sudo", "doas", "env", "nohup", "exec", "command", "builtin", "nice", "ionice", "stdbuf",
    "timeout", "xargs", "watch", "setsid", "busybox", "chrt", "taskset", "unbuffer",
];

/// Tùy chọn của lệnh bọc có giá trị ở từ kế tiếp.
fn wrapper_option_takes_value(wrapper: &str, opt: &str) -> bool {
    match wrapper {
        "sudo" => matches!(
            opt,
            "-u" | "-g" | "-C" | "-D" | "-h" | "-p" | "-r" | "-t" | "-U" | "-T"
        ),
        "doas" => matches!(opt, "-u" | "-C"),
        "env" => matches!(
            opt,
            "-u" | "-C" | "--unset" | "--chdir" | "-S" | "--split-string"
        ),
        "nice" => matches!(opt, "-n" | "--adjustment"),
        "ionice" => matches!(opt, "-c" | "-n" | "-p" | "-P" | "-u"),
        "timeout" => matches!(opt, "-s" | "-k" | "--signal" | "--kill-after"),
        "xargs" => matches!(
            opt,
            "-I" | "-n"
                | "-P"
                | "-L"
                | "-d"
                | "-s"
                | "-a"
                | "-E"
                | "--max-args"
                | "--max-procs"
                | "--delimiter"
                | "--arg-file"
        ),
        "watch" => matches!(opt, "-n" | "--interval" | "-d"),
        "chrt" | "taskset" => true,
        _ => false,
    }
}

fn is_assignment(w: &str) -> bool {
    let Some((name, _)) = w.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Tên chương trình: phần sau `/` hoặc `\` cuối, chữ thường, bỏ `.exe`.
pub fn program_name(w: &str) -> String {
    let trimmed = w.trim_end_matches(['/', '\\']);
    let base = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
    let lower = base.to_lowercase();
    lower
        .strip_suffix(".exe")
        .map(str::to_owned)
        .unwrap_or(lower)
}

fn invocations(segments: &[Segment]) -> Vec<Invocation> {
    let mut out = Vec::new();
    for (si, seg) in segments.iter().enumerate() {
        invocations_in(si, seg, 0, seg.words.len(), &mut out);
    }
    out
}

/// Lần gọi trong các từ `[from, to)` của một đoạn.
fn invocations_in(si: usize, seg: &Segment, from: usize, to: usize, out: &mut Vec<Invocation>) {
    let words = &seg.words;
    let mut k = from;
    let mut wrappers: Vec<String> = Vec::new();
    // Lệnh bọc vừa gặp còn đang đọc tùy chọn; `timeout` còn một đối số thời lượng.
    let mut in_wrapper: Option<String> = None;
    let mut timeout_duration = false;
    while k < to {
        let w = &words[k];
        if w.redirect.is_some() {
            k += 1;
            continue;
        }
        let v = w.value.as_str();
        if let Some(wr) = in_wrapper.as_deref() {
            if v.starts_with('-') && v.len() > 1 {
                if wrapper_option_takes_value(wr, v) {
                    k += 1;
                }
                k += 1;
                continue;
            }
            if wr == "env" && is_assignment(v) {
                k += 1;
                continue;
            }
            if wr == "timeout" && timeout_duration {
                timeout_duration = false;
                k += 1;
                continue;
            }
            if (wr == "chrt" || wr == "taskset")
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || b == b',' || b == b'-')
            {
                k += 1;
                continue;
            }
            in_wrapper = None;
        }
        if is_assignment(v) || KEYWORDS.contains(&v) {
            k += 1;
            continue;
        }
        let name = program_name(v);
        if WRAPPERS.contains(&name.as_str()) {
            wrappers.push(name.clone());
            timeout_duration = name == "timeout";
            in_wrapper = Some(name);
            k += 1;
            continue;
        }
        let dynamic = v.is_empty() || v.contains('$') || v.contains('`');
        let mut args_end = to;
        // `find … -exec CMD … ;` (hoặc `+`): CMD là một lần gọi riêng.
        if name == "find" {
            let mut j = k + 1;
            while j < to {
                let a = words[j].value.as_str();
                if matches!(a, "-exec" | "-execdir" | "-ok" | "-okdir") {
                    let start = j + 1;
                    let mut stop = start;
                    while stop < to && !matches!(words[stop].value.as_str(), ";" | "+") {
                        stop += 1;
                    }
                    invocations_in(si, seg, start, stop, out);
                    j = stop;
                }
                j += 1;
            }
        }
        if args_end < k + 1 {
            args_end = k + 1;
        }
        out.push(Invocation {
            name,
            segment: si,
            cmd: k,
            args: k + 1..args_end,
            wrappers,
            dynamic,
        });
        return;
    }
    // Chỉ có lệnh bọc (`sudo` trơn): vẫn ghi lại để luật `privilege` thấy.
    if !wrappers.is_empty() {
        let last = to.saturating_sub(1).max(from);
        out.push(Invocation {
            name: String::new(),
            segment: si,
            cmd: last,
            args: last + 1..last + 1,
            wrappers,
            dynamic: true,
        });
    }
}

/// Mọi chuỗi trong `v` (khóa và giá trị), theo thứ tự duyệt.
pub fn strings_of(v: &Value) -> Vec<&str> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
        match v {
            Value::String(s) => out.push(s),
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => {
                for (k, x) in o {
                    out.push(k);
                    walk(x, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(v, &mut out);
    out
}

/// Tìm `needle` (chữ thường ASCII, dùng `/`) trong `s` như thể `s` đã được gập: bỏ ký tự vô hình, `\` thành `/`, chữ
/// hoa ASCII thành chữ thường. Không cấp phát theo độ dài `s` (chạy được trên nội dung Write tới 1 MB). Trả các khoảng
/// byte trong `s` (tối đa `limit`).
pub fn find_folded(s: &str, needle: &str, limit: usize) -> Vec<Range<usize>> {
    let fold_char = |c: char| -> Option<char> {
        if is_invisible(c) {
            None
        } else if c == '\\' {
            Some('/')
        } else {
            Some(c.to_ascii_lowercase())
        }
    };
    let pattern: Vec<char> = needle.chars().collect();
    let Some(&first) = pattern.first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut search_from = 0;
    for (i, c) in s.char_indices() {
        if out.len() >= limit {
            break;
        }
        if i < search_from || fold_char(c) != Some(first) {
            continue;
        }
        let mut matched = 1;
        let mut end = i + c.len_utf8();
        for (j, d) in s[end..].char_indices() {
            if matched == pattern.len() {
                break;
            }
            match fold_char(d) {
                None => continue,
                Some(f) if f == pattern[matched] => {
                    matched += 1;
                    end = i + c.len_utf8() + j + d.len_utf8();
                }
                Some(_) => break,
            }
        }
        if matched == pattern.len() {
            out.push(i..end);
            search_from = end;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(n: &Normalized) -> Vec<Vec<&str>> {
        n.segments
            .iter()
            .map(|s| s.words.iter().map(|w| w.value.as_str()).collect())
            .collect()
    }

    #[test]
    fn hidden_chars_removed_and_spaces_collapsed() {
        let n = normalize("r\u{200B}m\u{00A0}\u{00A0}-rf\t\tx");
        assert_eq!(n.text, "rm -rf x");
        assert_eq!(n.suspicious, vec![Suspicion::HiddenChars]);
        assert_eq!(words(&n), vec![vec!["rm", "-rf", "x"]]);
    }

    #[test]
    fn quoted_semicolon_not_split() {
        let n = normalize(r#"echo "a; b" 'c && d' e\;f; ls"#);
        assert_eq!(
            words(&n)[..2],
            [vec!["echo", "a; b", "c && d", "e;f"], vec!["ls"]]
        );
        // Nội dung trong nháy được phân tích lại (độ sâu 1), không làm mất đoạn ở độ sâu 0.
        assert!(n.segments.iter().any(|s| s.depth == 1));
    }

    #[test]
    fn separators_split_segments() {
        let n = normalize("a && b || c | d; e & f\ng");
        assert_eq!(
            words(&n),
            vec![
                vec!["a"],
                vec!["b"],
                vec!["c"],
                vec!["d"],
                vec!["e"],
                vec!["f"],
                vec!["g"]
            ]
        );
    }

    #[test]
    fn quotes_and_escapes_are_removed_from_words() {
        let n = normalize(r#""r"m -r\f 'x y'"#);
        assert_eq!(words(&n)[0], vec!["rm", "-rf", "x y"]);
        assert_eq!(n.invocations[0].name, "rm");
    }

    #[test]
    fn redirects_and_fd_numbers() {
        let n = normalize("cmd 2>/dev/null >out.txt < in.txt 2>&1 <<EOF");
        let seg = &n.segments[0];
        let got: Vec<(&str, Option<Redirect>)> = seg
            .words
            .iter()
            .map(|w| (w.value.as_str(), w.redirect))
            .collect();
        assert_eq!(
            got,
            vec![
                ("cmd", None),
                ("/dev/null", Some(Redirect::Out)),
                ("out.txt", Some(Redirect::Out)),
                ("in.txt", Some(Redirect::In)),
                ("EOF", None),
            ]
        );
    }

    #[test]
    fn wrappers_and_assignments_are_skipped() {
        let n = normalize("FOO=1 sudo -u root env A=b nice -n 5 timeout 10 /bin/rm -rf x");
        let inv = &n.invocations[0];
        assert_eq!(inv.name, "rm");
        assert_eq!(inv.wrappers, vec!["sudo", "env", "nice", "timeout"]);
        let args: Vec<&str> = n.args(inv).iter().map(|w| w.value.as_str()).collect();
        assert_eq!(args, vec!["-rf", "x"]);
    }

    #[test]
    fn find_exec_is_an_invocation() {
        let n = normalize(r"find . -name '*.tmp' -exec rm -rf {} \;");
        // Nội dung nháy `'*.tmp'` được phân tích lại ở độ sâu 1 (chỉ cho luật danger): bỏ ra khi so.
        let names: Vec<&str> = n
            .invocations
            .iter()
            .filter(|i| n.segments[i.segment].depth == 0)
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(names, vec!["rm", "find"]);
        let rm = &n.invocations[0];
        let args: Vec<&str> = n.args(rm).iter().map(|w| w.value.as_str()).collect();
        assert_eq!(args, vec!["-rf", "{}"]);
    }

    #[test]
    fn nested_quotes_are_analyzed() {
        let n = normalize(r#"bash -c "sh -c 'rm -rf /'""#);
        let names: Vec<&str> = n.invocations.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"rm"), "{names:?}");
    }

    #[test]
    fn unquoted_brace_expansion_is_unparsed() {
        // Brace expansion đổi chữ trước khi shell chạy (`.e{nv,}` → `.e .env`), nên không phân tích được theo chữ.
        for c in [
            "cat .e{nv,}",
            "{rm,-rf,~}",
            "echo {1..3}",
            "cp file{,.bak}",
            "cat ~/.ss{h,}/id_rsa",
            "ls {a,b}{c,d}",
        ] {
            assert!(normalize(c).unparsed, "{c:?} phải là unparsed");
        }
        for c in [
            "echo '{a,b}'",
            r#"echo "{a,b}""#,
            r#"curl -d '{"a":1,"b":2}' http://x"#,
            "{ ls; }",
            "echo ${HOME}",
            "ls {a}",
            "echo {}",
            "find . -exec ls {} ;",
        ] {
            assert!(!normalize(c).unparsed, "{c:?} không được là unparsed");
        }
    }

    #[test]
    fn unterminated_quote_is_unparsed() {
        assert!(normalize("echo 'abc").unparsed);
        assert!(normalize("echo \"abc").unparsed);
        assert!(normalize("echo abc\\").unparsed);
        assert!(!normalize("echo 'abc' \"d\"").unparsed);
    }

    #[test]
    fn suspicions_detected() {
        assert_eq!(
            normalize("echo $(whoami)").suspicious,
            vec![Suspicion::Subshell]
        );
        assert_eq!(normalize("echo `id`").suspicious, vec![Suspicion::Subshell]);
        assert_eq!(normalize("eval \"$X\"").suspicious, vec![Suspicion::Eval]);
        assert_eq!(
            normalize("ls; . ./env.sh").suspicious,
            vec![Suspicion::Eval]
        );
        assert_eq!(normalize("source x").suspicious, vec![Suspicion::Eval]);
        assert_eq!(
            normalize("echo cm0gLXJmIC8= | base64 -d | sh").suspicious,
            vec![Suspicion::Encoded]
        );
        assert_eq!(
            normalize("xxd -r -p f").suspicious,
            vec![Suspicion::Encoded]
        );
        assert_eq!(
            normalize(r"printf '\x72\x6d'").suspicious,
            vec![Suspicion::Encoded]
        );
        assert!(normalize("ls -la && cargo test").suspicious.is_empty());
        assert!(normalize("echo source of truth").suspicious.is_empty());
    }

    #[test]
    fn bidi_override_flagged() {
        let n = normalize("echo safe\u{202E}fdp.exe");
        assert_eq!(n.suspicious, vec![Suspicion::HiddenChars]);
        assert_eq!(n.text, "echo safefdp.exe");
    }

    #[test]
    fn unassigned_code_points_are_hidden() {
        // U+0378 chưa gán (Cn) trong Unicode 16.
        assert!(is_hidden('\u{0378}'));
        assert!(is_hidden('\u{E0001}'));
        assert!(!is_hidden('ế'));
        assert!(!is_hidden('🚀'));
    }

    #[test]
    fn original_chars_maps_back_through_removed_chars() {
        let cmd = "ls; r\u{200B}m -rf x";
        let n = normalize(cmd);
        let start = n.text.find("rm").unwrap();
        let r = n.original_chars(start..start + "rm -rf".len()).unwrap();
        let orig: String = cmd.chars().skip(r.start).take(r.len()).collect();
        assert_eq!(orig, "r\u{200B}m -rf");
    }

    #[test]
    fn display_span_covers_escape_markers() {
        let cmd = "a\u{200B}\u{200B}b c";
        let d = display(cmd);
        assert_eq!(d.text, "a⟨U+200B×2⟩b c");
        // Ký tự gốc 0..4 ("a", 2 ký tự ẩn, "b") → toàn bộ "a⟨U+200B×2⟩b".
        let (s, e) = d.span(0..4).unwrap();
        let shown: String = d.text.chars().skip(s).take(e - s).collect();
        assert_eq!(shown, "a⟨U+200B×2⟩b");
        assert_eq!(d.span(5..6), Some((13, 14)));
        assert_eq!(d.span(3..3), None);
    }

    #[test]
    fn find_folded_skips_hidden_and_case() {
        let s = "cat C:\\Users\\U\\AppData\\Roaming\\Arcturus\\UI\u{200B}-Token";
        let hits = find_folded(s, "arcturus/ui-token", 4);
        assert_eq!(hits.len(), 1);
        assert_eq!(&s[hits[0].clone()], "Arcturus\\UI\u{200B}-Token");
        assert!(find_folded("arcturus/ui", "arcturus/ui-token", 4).is_empty());
    }

    #[test]
    fn too_long_is_not_tokenized() {
        let n = normalize(&"a".repeat(MAX_ANALYZED_BYTES + 1));
        assert!(n.too_long && n.unparsed);
        assert!(n.segments.is_empty());
    }
}
