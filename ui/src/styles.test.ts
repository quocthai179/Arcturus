// Giữ hợp đồng kiểu dáng (docs/UI_EXTENSION.md): đổi giao diện sau này chỉ cần đổi token và bộ vẽ, nên không để màu, bo góc
// hay thời lượng chuyển động viết cứng rải rác trong các tệp CSS.
import { describe, expect, it } from 'vitest';

const FILES = import.meta.glob<string>('./**/*.css', { query: '?raw', import: 'default', eager: true });

/** Bỏ khối `:root { ... }` đầu tiên (nơi duy nhất được định nghĩa token). */
function withoutRootTokens(css: string): string {
  const start = css.indexOf(':root {');
  if (start < 0) return css;
  const end = css.indexOf('\n}', start);
  return css.slice(0, start) + css.slice(end + 2);
}

describe('styles', () => {
  const entries = Object.entries(FILES);

  it('finds_the_stylesheets', () => {
    expect(entries.length).toBeGreaterThanOrEqual(3);
  });

  it('no_hardcoded_colors_outside_the_root_token_block', () => {
    for (const [file, css] of entries) {
      expect(withoutRootTokens(css).match(/#[0-9a-fA-F]{3,8}\b/g), `${file} có màu viết cứng`).toBeNull();
    }
  });

  it('radius_and_motion_durations_come_from_tokens', () => {
    for (const [file, css] of entries) {
      const body = withoutRootTokens(css);
      expect(body.match(/border-radius:\s*\d+px/g), `${file} có border-radius viết cứng`).toBeNull();
      expect(body.match(/(animation|transition)[^;{]*\b\d+m?s\b/g), `${file} có thời lượng viết cứng`).toBeNull();
    }
  });

  it('global_rule_turns_off_all_animation_when_motion_is_off', () => {
    const css = Object.entries(FILES).find(([f]) => f.endsWith('/index.css'))?.[1] ?? '';
    expect(Object.keys(FILES)).toContain('./index.css');
    expect(css).toMatch(/:root\[data-motion='off'\] \*/);
    expect(css).toMatch(/animation:\s*none\s*!important/);
    expect(css).toMatch(/transition:\s*none\s*!important/);
  });
});
