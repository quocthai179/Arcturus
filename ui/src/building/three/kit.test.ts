import { describe, expect, it, vi } from 'vitest';
import { BufferGeometry } from 'three';
import { Batch, makeMaterials, plankFloor, QuadSet, Registry, seeded } from './kit';

describe('Registry', () => {
  it('disposeAll_disposes_every_item_once_and_empties_the_set', () => {
    const reg = new Registry();
    const a = { dispose: vi.fn() };
    const b = { dispose: vi.fn() };
    reg.add(a);
    reg.add(b);
    reg.add(a);
    expect(reg.size).toBe(2);
    reg.disposeAll();
    expect(a.dispose).toHaveBeenCalledTimes(1);
    expect(b.dispose).toHaveBeenCalledTimes(1);
    expect(reg.size).toBe(0);
    reg.disposeAll();
    expect(a.dispose).toHaveBeenCalledTimes(1);
  });

  it('batch_merges_into_one_mesh_per_kind_and_registers_geometry', () => {
    const reg = new Registry();
    const mats = makeMaterials(reg);
    const before = reg.size;
    const group = new Batch().box(1, 1, 1, 0, 0, 0, '#ffffff').box(1, 1, 1, 2, 0, 0, '#000000').box(1, 1, 1, 0, 2, 0, '#ff0000', { kind: 'glow' }).build(reg, mats);
    expect(group.children).toHaveLength(2);
    expect(reg.size).toBe(before + 2);
    reg.disposeAll();
  });
});

describe('Batch (art pass)', () => {
  it('rounded_boxes_links_and_ao_merge_into_one_lit_mesh_with_colours_and_normals', () => {
    const reg = new Registry();
    const mats = makeMaterials(reg);
    const group = new Batch()
      .rbox(1, 1, 1, 0, 0, 0, '#ffffff', { ao: 0.4 })
      .rbox(0.001, 0.5, 0.5, 2, 0, 0, '#ff0000')
      .link([0, 0, 0], [0, 2, 0], 0.05, '#00ff00')
      .link([1, 1, 1], [1, 1, 1], 0.05, '#00ff00')
      .sph(1, 3, 0, 0, '#0000ff', { sx: 2, sy: 0.1, sz: 1 })
      .build(reg, mats);
    expect(group.children).toHaveLength(1);
    const g = (group.children[0] as unknown as { geometry: BufferGeometry }).geometry;
    expect(g.getAttribute('position').count).toBeGreaterThan(100);
    for (const name of ['position', 'normal', 'color']) expect(g.getAttribute(name).count).toBe(g.getAttribute('position').count);
    expect(g.getAttribute('uv')).toBeUndefined();
    // AO làm tối đáy hộp: màu đỉnh thấp nhất < màu đỉnh cao nhất
    const pos = g.getAttribute('position');
    const col = g.getAttribute('color');
    let lo = 1;
    let hi = 0;
    for (let i = 0; i < 108 * 3; i++) {
      if (Math.abs(pos.getX(i)) > 0.6) continue;
      if (pos.getY(i) < -0.49) lo = Math.min(lo, col.getX(i));
      if (pos.getY(i) > 0.49) hi = Math.max(hi, col.getX(i));
    }
    expect(lo).toBeLessThan(hi);
    reg.disposeAll();
  });

  it('geometry_returns_the_merged_lit_parts_for_figure_parts', () => {
    const g = new Batch().sph(0.1, 0, 0, 0, '#fff').box(0.1, 0.1, 0.1, 0.2, 0, 0, '#fff').geometry();
    expect(g.getAttribute('color').count).toBe(g.getAttribute('position').count);
    expect(new Batch().geometry().getAttribute('position')).toBeUndefined();
  });
});

describe('plankFloor', () => {
  it('covers_the_area_with_upward_facing_planks_inside_the_bounds', () => {
    const g = plankFloor(1, -2, 10, 6, 0.3, ['#d9b98c', '#e8d2ad'], 5);
    const pos = g.getAttribute('position');
    expect(pos.count % 6).toBe(0);
    expect(pos.count / 6).toBeGreaterThan(20);
    for (let i = 0; i < pos.count; i++) {
      expect(pos.getY(i)).toBeCloseTo(0.3, 6);
      expect(pos.getX(i)).toBeGreaterThanOrEqual(1 - 5 - 1e-6);
      expect(pos.getX(i)).toBeLessThanOrEqual(1 + 5 + 1e-6);
      expect(pos.getZ(i)).toBeGreaterThanOrEqual(-2 - 3 - 1e-6);
      expect(pos.getZ(i)).toBeLessThanOrEqual(-2 + 3 + 1e-6);
    }
    // thứ tự đỉnh quay mặt lên: pháp tuyến hình học của từng tam giác có y > 0
    for (let t = 0; t < pos.count; t += 3) {
      const ux = pos.getX(t + 1) - pos.getX(t);
      const uz = pos.getZ(t + 1) - pos.getZ(t);
      const vx = pos.getX(t + 2) - pos.getX(t);
      const vz = pos.getZ(t + 2) - pos.getZ(t);
      expect(uz * vx - ux * vz).toBeGreaterThan(0);
    }
    const again = plankFloor(1, -2, 10, 6, 0.3, ['#d9b98c', '#e8d2ad'], 5);
    expect(Array.from(again.getAttribute('position').array)).toEqual(Array.from(pos.array));
  });
});

describe('QuadSet', () => {
  it('builds_one_mesh_with_uvs_inside_the_rect_and_facing_normals', () => {
    const reg = new Registry();
    const q = new QuadSet().flat(0, 0, 0, 2, 1, 0, [0, 0, 1, 1]).wall(0, 1, 0, 2, 1, 0, [0.25, 0.5, 0.5, 0.75]).wall(0, 1, 0, 2, 1, Math.PI / 2, [0.25, 0.5, 0.5, 0.75]);
    expect(q.count).toBe(3);
    const mesh = q.build(reg, makeMaterials(reg).lit);
    expect(mesh).not.toBeNull();
    const g = mesh?.geometry;
    const n = g?.getAttribute('normal');
    const uv = g?.getAttribute('uv');
    expect(g?.getAttribute('position').count).toBe(18);
    const near = (v: number | undefined, want: number) => expect(v).toBeCloseTo(want, 6);
    near(n?.getX(0), 0);
    near(n?.getY(0), 1);
    near(n?.getZ(0), 0);
    near(n?.getY(6), 0);
    near(n?.getZ(6), 1);
    expect(n?.getX(12)).toBeCloseTo(1, 6);
    for (let i = 6; i < 12; i++) {
      expect(uv?.getX(i)).toBeGreaterThanOrEqual(0.25);
      expect(uv?.getX(i)).toBeLessThanOrEqual(0.5);
      expect(uv?.getY(i)).toBeGreaterThanOrEqual(0.5);
      expect(uv?.getY(i)).toBeLessThanOrEqual(0.75);
    }
    expect(new QuadSet().build(reg, makeMaterials(reg).lit)).toBeNull();
    reg.disposeAll();
  });
});

describe('seeded', () => {
  it('is_deterministic_and_in_unit_range', () => {
    const a = seeded(42);
    const b = seeded(42);
    for (let i = 0; i < 50; i++) {
      const v = a();
      expect(v).toBe(b());
      expect(v).toBeGreaterThan(0);
      expect(v).toBeLessThan(1);
    }
    expect(seeded(0)()).toBeGreaterThan(0);
  });
});

