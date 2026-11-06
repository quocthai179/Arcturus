import { describe, expect, it } from 'vitest';
import { DEFAULT_VIEW, ease, FOV, frameFor, viewTargets, type View } from './camera';
import { BASE, GEO, LIFT, ROOF_Y } from './layout';

describe('viewTargets', () => {
  it('ceo_view_lifts_only_the_roof', () => {
    expect(viewTargets(5).lift).toEqual([0, 0, 0, 0, 0, 0, LIFT]);
  });
  it('floor_2_lifts_3_4_5_and_roof', () => {
    const t = viewTargets(2);
    expect(t.lift).toEqual([0, 0, 0, LIFT, LIFT, LIFT, LIFT]);
    expect(t.cut).toEqual([0, 0, 1, 0, 0, 0]);
    expect(t.visible).toEqual([true, true, true, false, false, false]);
  });
  it('exterior_lifts_nothing', () => {
    expect(viewTargets('ext').lift.every((x) => x === 0)).toBe(true);
    expect(viewTargets('ext').cut.every((x) => x === 0)).toBe(true);
    expect(viewTargets('ext').visible.every(Boolean)).toBe(true);
  });
  it('default_view_is_the_ceo_room', () => {
    expect(DEFAULT_VIEW).toBe(5);
  });
});

describe('ease', () => {
  it('endpoints_and_monotonic', () => {
    expect(ease(0)).toBe(0);
    expect(ease(1)).toBe(1);
    expect(ease(0.5)).toBeCloseTo(0.5);
    expect(ease(0.3)).toBeLessThan(ease(0.6));
  });
});

/** Chiếu điểm thế giới qua camera nhìn-vào (up = +y) với FOV dọc và aspect. */
function project(pos: { x: number; y: number; z: number }, target: { x: number; y: number; z: number }, p: { x: number; y: number; z: number }, aspect: number) {
  const f = { x: target.x - pos.x, y: target.y - pos.y, z: target.z - pos.z };
  const fl = Math.hypot(f.x, f.y, f.z);
  const fw = { x: f.x / fl, y: f.y / fl, z: f.z / fl };
  // right = fw × up(0,1,0)
  const r0 = { x: -fw.z, y: 0, z: fw.x };
  const rl = Math.hypot(r0.x, r0.z);
  const right = { x: r0.x / rl, y: 0, z: r0.z / rl };
  const up = { x: right.y * fw.z - right.z * fw.y, y: right.z * fw.x - right.x * fw.z, z: right.x * fw.y - right.y * fw.x };
  const d = { x: p.x - pos.x, y: p.y - pos.y, z: p.z - pos.z };
  const depth = d.x * fw.x + d.y * fw.y + d.z * fw.z;
  const t = Math.tan((FOV * Math.PI) / 360);
  return {
    x: (d.x * right.x + d.y * right.y + d.z * right.z) / (depth * t * aspect),
    y: -(d.x * up.x + d.y * up.y + d.z * up.z) / (depth * t),
  };
}

describe('frameFor', () => {
  for (const aspect of [0.6, 1, 1.6, 2.2]) {
    for (const view of [0, 1, 2, 3, 4, 5] as View[]) {
      it(`floor_corners_are_in_frame view=${view} aspect=${aspect}`, () => {
        const g = GEO[view as number];
        if (g === undefined) throw new Error('GEO');
        const { pos, target } = frameFor(view, aspect);
        for (const sx of [-1, 1]) {
          for (const sz of [-1, 1]) {
            const p = project(pos, target, { x: g.cx + (sx * g.w) / 2, y: (BASE[view as number] ?? 0) + 0.3, z: g.cz + (sz * g.d) / 2 }, aspect);
            expect(Math.abs(p.x), `x ${view}`).toBeLessThanOrEqual(1.0001);
            expect(Math.abs(p.y), `y ${view}`).toBeLessThanOrEqual(1.0001);
          }
        }
      });
    }
  }

  it('exterior_frames_the_whole_building_height', () => {
    const { pos, target } = frameFor('ext', 1.6);
    const top = project(pos, target, { x: 0, y: ROOF_Y + 1.5, z: 0 }, 1.6);
    const ground = project(pos, target, { x: 0, y: 0, z: 8 }, 1.6);
    expect(Math.abs(top.y)).toBeLessThan(1);
    expect(Math.abs(ground.y)).toBeLessThan(1);
  });
});
