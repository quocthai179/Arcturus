import { describe, expect, it } from 'vitest';
import { revealHidden, splitSpans } from './text';

describe('splitSpans', () => {
  it('uses_code_point_indices_with_non_bmp', () => {
    // Chuỗi mẫu của PROTOCOL.md: span theo ký tự là [20,33]; theo UTF-16 là [21,…], theo byte là [26,…].
    const cmd = 'echo "🚀 thư mục" && rm -rf đầu-ra';
    const pieces = splitSpans(cmd, [[20, 33]]);
    expect(pieces).toEqual([
      { text: 'echo "🚀 thư mục" && ', danger: false },
      { text: 'rm -rf đầu-ra', danger: true },
    ]);
  });

  it('merges_clamps_and_drops_bad_spans', () => {
    expect(splitSpans('abcdef', [[4, 99], [1, 3], [2, 4], [5, 5], [3, 1]])).toEqual([
      { text: 'a', danger: false },
      { text: 'bcdef', danger: true },
    ]);
    expect(splitSpans('', [])).toEqual([{ text: '', danger: false }]);
    expect(splitSpans('ab', [])).toEqual([{ text: 'ab', danger: false }]);
  });

  it('pieces_always_rejoin_to_text', () => {
    const s = '🦀a\u{1F600}b đ';
    for (const spans of [[[0, 1]], [[1, 3], [4, 6]], [[0, 99]]] as [number, number][][]) {
      expect(splitSpans(s, spans).map((p) => p.text).join('')).toBe(s);
    }
  });
});

describe('revealHidden', () => {
  it('shows_codes_for_invisible_characters', () => {
    expect(revealHidden('r​m -rf x')).toBe('r⟨U+200B⟩m -rf x');
    expect(revealHidden('safe‮fdp.exe')).toBe('safe⟨U+202E⟩fdp.exe');
    expect(revealHidden('a​​‌b')).toBe('a⟨U+200B×2⟩⟨U+200C⟩b');
    expect(revealHidden('x\u{E0041}')).toBe('x⟨U+E0041⟩');
    expect(revealHidden('nb sp')).toBe('nb⟨U+00A0⟩sp');
  });

  it('keeps_visible_text_newlines_and_tabs', () => {
    const s = 'tiếng Việt 🦀\n\tok';
    expect(revealHidden(s)).toBe(s);
  });
});
