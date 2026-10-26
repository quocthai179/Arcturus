import { describe, expect, it } from 'vitest';
import { countdownEndsAt } from './countdown';
import { approval } from './test/fixtures';

const T0 = Date.parse('2026-10-23T09:00:00Z');
const timed = (over = {}) => approval({ countdown_ms: 5000, reversible: true, risk: 'low', ...over });

describe('countdownEndsAt', () => {
  it('only_exists_when_countdown_ms_is_set', () => {
    expect(countdownEndsAt(timed({ countdown_ms: null }), T0, false)).toBeNull();
    expect(countdownEndsAt(timed(), T0, false)).toBe(T0 + 5000);
  });

  it('never_for_high_critical_or_irreversible', () => {
    for (const risk of ['high', 'critical'] as const) {
      expect(countdownEndsAt(timed({ risk }), T0, false)).toBeNull();
    }
    expect(countdownEndsAt(timed({ reversible: false }), T0, false)).toBeNull();
    expect(countdownEndsAt(timed({ risk: 'medium' }), T0, false)).toBe(T0 + 5000);
  });

  it('never_while_paused_or_before_card_is_shown', () => {
    expect(countdownEndsAt(timed(), T0, true)).toBeNull();
    expect(countdownEndsAt(timed(), undefined, false)).toBeNull();
  });

  it('counts_from_when_card_was_shown_if_it_arrived_late', () => {
    // Thẻ đã tạo từ lâu nhưng popup chỉ vừa mở: người dùng vẫn có đủ thời gian phủ quyết.
    expect(countdownEndsAt(timed(), T0 + 60_000, false)).toBe(T0 + 65_000);
    // Hub đếm từ created_at; không bao giờ sớm hơn mốc đó.
    expect(countdownEndsAt(timed(), T0 - 1000, false)).toBe(T0 + 5000);
  });
});
