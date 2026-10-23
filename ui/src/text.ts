// Xử lý chữ cho thẻ duyệt. Mọi vị trí tính theo ký tự Unicode (code point, `Array.from`), không theo UTF-16
// (`String.prototype.slice`): lệnh có tiếng Việt, emoji, ký tự ngoài BMP (docs/PROTOCOL.md, `danger_spans`).

export interface Piece {
  text: string;
  danger: boolean;
}

/**
 * Cắt `text` thành các đoạn thường/nguy hiểm theo `spans` (`[start, end)` theo code point). Khoảng chồng nhau được gộp,
 * khoảng ra ngoài chuỗi bị cắt bớt, khoảng rỗng hoặc ngược bị bỏ. Nối các đoạn lại luôn ra đúng `text`.
 */
export function splitSpans(text: string, spans: readonly (readonly [number, number])[]): Piece[] {
  const chars = Array.from(text);
  const n = chars.length;
  const ranges = spans
    .map(([s, e]) => [Math.max(0, Math.min(s, n)), Math.max(0, Math.min(e, n))] as const)
    .filter(([s, e]) => e > s)
    .sort((a, b) => a[0] - b[0]);
  const merged: [number, number][] = [];
  for (const [s, e] of ranges) {
    const last = merged[merged.length - 1];
    if (last !== undefined && s <= last[1]) {
      last[1] = Math.max(last[1], e);
    } else {
      merged.push([s, e]);
    }
  }
  const out: Piece[] = [];
  let pos = 0;
  for (const [s, e] of merged) {
    if (s > pos) out.push({ text: chars.slice(pos, s).join(''), danger: false });
    out.push({ text: chars.slice(s, e).join(''), danger: true });
    pos = e;
  }
  if (pos < n || out.length === 0) out.push({ text: chars.slice(pos).join(''), danger: false });
  return out;
}

/**
 * Ký tự không hiện ra hoặc giả khoảng trắng (cùng nhóm với hub, `hub_core::policy::normalize::is_hidden`): Cc trừ `\n`
 * `\t`, Cf, Co, Cn, Zl, Zp, khoảng trắng không phải ASCII, U+2800, Hangul filler, các Default_Ignorable ngoài Cf.
 */
const HIDDEN =
  /[\p{Cf}\p{Co}\p{Cn}\p{Zl}\p{Zp}\u0000-\u0008\u000B-\u001F\u007F-\u009F   -   　⠀ᅟᅠㅤﾠ͏឴឵᠋-᠏︀-️]/u;

export function isHidden(ch: string): boolean {
  return HIDDEN.test(ch);
}

/** Thay ký tự ẩn bằng `⟨U+XXXX⟩` (chuỗi lặp `⟨U+XXXX×N⟩`), như `command_normalized` của hub. */
export function revealHidden(s: string): string {
  const chars = Array.from(s);
  let out = '';
  for (let i = 0; i < chars.length; ) {
    const c = chars[i] ?? '';
    if (!isHidden(c)) {
      out += c;
      i += 1;
      continue;
    }
    let run = 1;
    while (chars[i + run] === c) run += 1;
    const code = (c.codePointAt(0) ?? 0).toString(16).toUpperCase().padStart(4, '0');
    out += run === 1 ? `⟨U+${code}⟩` : `⟨U+${code}×${run}⟩`;
    i += run;
  }
  return out;
}
