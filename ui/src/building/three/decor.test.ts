import { describe, expect, it } from 'vitest';
import { ART_IDS, artRect } from './decor';

describe('art atlas', () => {
  it('every_rect_is_inside_the_unit_square_and_rects_do_not_overlap', () => {
    const rects = ART_IDS.map((id) => ({ id, r: artRect(id) }));
    for (const { id, r } of rects) {
      expect(r[0], id).toBeGreaterThanOrEqual(0);
      expect(r[1], id).toBeGreaterThanOrEqual(0);
      expect(r[2], id).toBeLessThanOrEqual(1);
      expect(r[3], id).toBeLessThanOrEqual(1);
      expect(r[2] - r[0], id).toBeGreaterThan(0.2);
      expect(r[3] - r[1], id).toBeGreaterThan(0.2);
    }
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i]?.r;
        const b = rects[j]?.r;
        if (a === undefined || b === undefined) continue;
        const overlap = a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
        expect(overlap, `${rects[i]?.id} vs ${rects[j]?.id}`).toBe(false);
      }
    }
  });
});
