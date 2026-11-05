import { describe, expect, it } from 'vitest';
import type { AgentFloorId } from './floors';
import {
  aisleZ, BASE, briefSpot, CEO_DESK, deskAt, deskCount, deskRects, DESK_SIZE, ELEVATOR_DOOR, GEO, gapColumn, insideFloor, LIFT, LOUNGE_SPOTS,
  ROOF_Y, SECRETARY_DESK, SECRETARY_HOME, STANDING_SPOTS, STATUS_WALL, toWorld,
} from './layout';

const AGENT_FLOORS: AgentFloorId[] = [0, 1, 2, 3, 4];

describe('layout', () => {
  it('base_and_roof', () => {
    expect(BASE[0]).toBe(0);
    expect(BASE[1]).toBeCloseTo(6.0);
    expect(BASE[5]).toBeCloseTo(6 + 3.8 * 4);
    expect(ROOF_Y).toBeCloseTo((BASE[5] ?? 0) + 4.4);
    expect(LIFT).toBe(16);
  });

  it('desk_counts_match_the_doc', () => {
    expect(AGENT_FLOORS.map(deskCount)).toEqual([10, 19, 15, 11, 14]);
  });

  it('desks_seats_lounge_and_standing_are_inside_the_floor', () => {
    for (const f of AGENT_FLOORS) {
      for (let i = 0; i < deskCount(f); i++) {
        const { desk, seat } = deskAt(f, i);
        expect(insideFloor(f, desk, 0.6), `desk ${f}/${i}`).toBe(true);
        expect(insideFloor(f, seat, 0.3), `seat ${f}/${i}`).toBe(true);
      }
      for (const p of [...(LOUNGE_SPOTS[f] ?? []), ...(STANDING_SPOTS[f] ?? [])]) expect(insideFloor(f, p, 0.3), `spot ${f}`).toBe(true);
      expect(insideFloor(f, ELEVATOR_DOOR[f] ?? { x: 99, z: 99 })).toBe(true);
    }
  });

  it('desks_do_not_overlap_each_other_or_the_lounge', () => {
    for (const f of AGENT_FLOORS) {
      const r = deskRects(f);
      for (let i = 0; i < r.length; i++) {
        for (let j = i + 1; j < r.length; j++) {
          const a = r[i];
          const b = r[j];
          if (a === undefined || b === undefined) continue;
          expect(a.x1 <= b.x0 || b.x1 <= a.x0 || a.z1 <= b.z0 || b.z1 <= a.z0, `${f}: ${i}/${j}`).toBe(true);
        }
      }
      // Ghế (seat) không đè lên bàn kế tiếp, góc nghỉ không đè bàn.
      for (let i = 0; i < r.length; i++) {
        for (const p of LOUNGE_SPOTS[f] ?? []) {
          const a = r[i];
          if (a === undefined) continue;
          const inside = p.x > a.x0 - 0.3 && p.x < a.x1 + 0.3 && p.z > a.z0 - 0.3 && p.z < a.z1 + 0.9;
          expect(inside, `lounge ${f} vs desk ${i}`).toBe(false);
        }
      }
    }
  });

  it('ceo_floor_sits_inside_floor_4_footprint', () => {
    const ceo = GEO[5];
    const f4 = GEO[4];
    if (ceo === undefined || f4 === undefined) throw new Error('GEO');
    expect(ceo.cx - ceo.w / 2).toBeGreaterThanOrEqual(f4.cx - f4.w / 2);
    expect(ceo.cx + ceo.w / 2).toBeLessThanOrEqual(f4.cx + f4.w / 2);
    expect(ceo.cz - ceo.d / 2).toBeGreaterThanOrEqual(f4.cz - f4.d / 2);
    expect(ceo.cz + ceo.d / 2).toBeLessThanOrEqual(f4.cz + f4.d / 2);
  });

  it('ceo_room_items_are_inside_floor_5', () => {
    for (const p of [SECRETARY_HOME, CEO_DESK.at, SECRETARY_DESK.at]) expect(insideFloor(5, p, 0.3)).toBe(true);
    expect(STATUS_WALL.xs).toHaveLength(5);
    for (const x of STATUS_WALL.xs) {
      expect(insideFloor(5, { x: x - STATUS_WALL.w / 2, z: STATUS_WALL.z })).toBe(true);
      expect(insideFloor(5, { x: x + STATUS_WALL.w / 2, z: STATUS_WALL.z })).toBe(true);
    }
  });

  it('brief_spot_and_gap_columns', () => {
    const { seat } = deskAt(1, 6);
    expect(briefSpot(seat)).toEqual({ x: seat.x + 0.75, y: seat.y, z: seat.z + 0.15 });
    expect(gapColumn(1, seat.x)).toBeCloseTo(seat.x + 1.05);
    expect(gapColumn(1, briefSpot(seat).x)).toBeCloseTo(seat.x + 1.05);
    // Cột khe không nằm trong bàn nào.
    const gap = gapColumn(1, seat.x);
    for (const r of deskRects(1)) expect(gap > r.x0 && gap < r.x1).toBe(false);
    expect(aisleZ(1)).toBeGreaterThan(deskAt(1, 18).seat.z);
    expect(DESK_SIZE.w).toBe(1.5);
  });

  it('to_world', () => {
    expect(toWorld(2, { x: 1, y: 0.3, z: 1 })).toEqual({ x: 1, y: (BASE[2] ?? 0) + 0.3, z: 1 + 0.75 });
  });
});
