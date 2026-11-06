// Hình học nhân vật: mọi phần phải có position/normal/color (thiếu `color` thì vật liệu vertexColors vẽ đen), không rỗng, trong ngân sách.
import { describe, expect, it } from 'vitest';
import { PART_IDS } from '../figure';
import { CASTS_SHADOW, partGeometry } from './figureGeo';

describe('partGeometry', () => {
  it('every_part_has_position_normal_color_and_a_sane_triangle_count', () => {
    let total = 0;
    for (const id of PART_IDS) {
      const g = partGeometry(id);
      const pos = g.getAttribute('position');
      expect(pos, id).toBeDefined();
      expect(pos.count, id).toBeGreaterThan(0);
      expect(g.getAttribute('normal').count, id).toBe(pos.count);
      expect(g.getAttribute('color').count, id).toBe(pos.count);
      const tris = (g.index === null ? pos.count : g.index.count) / 3;
      expect(tris, id).toBeLessThanOrEqual(1500);
      total += tris;
      for (let i = 0; i < pos.count; i++) expect(Number.isFinite(pos.getX(i) + pos.getY(i) + pos.getZ(i))).toBe(true);
      g.dispose();
    }
    // một người dùng đủ mọi phần chỉ là cận trên: ngân sách theo phần, không phải theo người
    expect(total).toBeLessThan(25_000);
  });

  it('only_big_parts_cast_shadows', () => {
    for (const id of CASTS_SHADOW) expect(PART_IDS).toContain(id);
    expect(CASTS_SHADOW.has('glasses')).toBe(false);
    expect(CASTS_SHADOW.has('torso')).toBe(true);
  });
});
