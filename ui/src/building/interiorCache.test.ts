import { describe, expect, it } from 'vitest';
import { planPrune } from './interiorCache';

describe('planPrune', () => {
  it('keeps_ceo_the_viewed_floor_and_two_recent', () => {
    const p = planPrune([5, 0, 1, 2, 3], [1, 0], 2, 2);
    expect(p.recent).toEqual([2, 1]);
    expect(p.dispose.sort()).toEqual([0, 3]);
  });

  it('never_disposes_the_current_view_even_if_an_older_navigation_finishes_late', () => {
    // Điều hướng cũ tới tầng 1 kết thúc muộn khi người dùng đã sang tầng 4.
    const p = planPrune([5, 1, 2, 3, 4], [2, 3], 1, 4);
    expect(p.dispose).not.toContain(4);
    expect(p.dispose).not.toContain(5);
  });

  it('exterior_and_ceo_do_not_change_recent', () => {
    expect(planPrune([5, 1], [1], 'ext', 'ext').recent).toEqual([1]);
    expect(planPrune([5, 1], [1], 5, 5).recent).toEqual([1]);
  });
});
