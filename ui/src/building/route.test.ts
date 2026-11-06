import { describe, expect, it } from 'vitest';
import type { AgentFloorId } from './floors';
import { briefSpot, deskAt, deskCount, ELEVATOR_DOOR, insideFloor, LOUNGE_SPOTS, obstacles, SECRETARY_HOME, SHAFT_X, type Rect, type Vec3 } from './layout';
import { planRoute, RIDE_MIN_MS, routePosAt, walkPath, WALK_MPS, type Place } from './route';

const crosses = (a: Vec3, b: Vec3, r: Rect): boolean => {
  const e = 0.02;
  const x0 = Math.min(a.x, b.x);
  const x1 = Math.max(a.x, b.x);
  const z0 = Math.min(a.z, b.z);
  const z1 = Math.max(a.z, b.z);
  return x1 > r.x0 + e && x0 < r.x1 - e && z1 > r.z0 + e && z0 < r.z1 - e;
};

const home: Place = { floor: 5, pos: SECRETARY_HOME };
const at = (floor: AgentFloorId, i: number): Place => ({ floor, pos: briefSpot(deskAt(floor, i).seat) });

describe('planRoute', () => {
  it('same_floor_has_no_ride_and_cross_floor_exactly_one', () => {
    const a = planRoute(at(1, 0), at(1, 7), false, true);
    expect(a.legs.some((l) => l.k === 'ride')).toBe(false);
    const b = planRoute(home, at(2, 3), false, true);
    expect(b.legs.filter((l) => l.k === 'ride')).toHaveLength(1);
    expect(b.legs.find((l) => l.k === 'ride')).toMatchObject({ fromFloor: 5, toFloor: 2 });
  });

  it('walking_legs_never_cut_a_desk', () => {
    for (const f of [0, 1, 2, 3, 4] as AgentFloorId[]) {
      const targets: Vec3[] = [
        ...Array.from({ length: deskCount(f) }, (_, i) => briefSpot(deskAt(f, i).seat)),
        ...(LOUNGE_SPOTS[f] ?? []),
        ELEVATOR_DOOR[f] as Vec3,
      ];
      const rects = obstacles(f);
      for (const a of targets) {
        for (const b of targets) {
          for (const leg of planRoute({ floor: f, pos: a }, { floor: f, pos: b }, false, true).legs) {
            if (leg.k !== 'walk') continue;
            expect(insideFloor(f, leg.from) && insideFloor(f, leg.to), `floor ${f} leaves floor`).toBe(true);
            for (const r of rects) expect(crosses(leg.from, leg.to, r), `floor ${f}: ${JSON.stringify(leg.from)} -> ${JSON.stringify(leg.to)}`).toBe(false);
          }
        }
      }
    }
  });

  it('ceo_floor_routes_avoid_both_desks', () => {
    const rects = obstacles(5);
    const door = ELEVATOR_DOOR[5] as Vec3;
    for (const [a, b] of [[SECRETARY_HOME, door], [door, SECRETARY_HOME]] as const) {
      for (const leg of planRoute({ floor: 5, pos: a }, { floor: 5, pos: b }, false, true).legs) {
        if (leg.k !== 'walk') continue;
        for (const r of rects) expect(crosses(leg.from, leg.to, r)).toBe(false);
      }
    }
  });

  it('routePosAt_is_continuous_and_ends_at_the_destination', () => {
    const route = planRoute(home, at(2, 3), false, true);
    let prev = routePosAt(route, 0);
    expect(prev.pos).toEqual(home.pos);
    for (let t = 20; t <= route.totalMs; t += 20) {
      const p = routePosAt(route, t);
      if (p.floor === prev.floor && p.floor !== 'car') {
        expect(Math.hypot(p.pos.x - prev.pos.x, p.pos.z - prev.pos.z)).toBeLessThan((WALK_MPS * 20) / 1000 + 0.05);
      }
      prev = p;
    }
    const end = routePosAt(route, route.totalMs + 1000);
    expect(end).toMatchObject({ floor: 2, pos: at(2, 3).pos, walking: false });
  });

  it('rides_in_the_car_on_the_shaft_and_the_car_moves_down', () => {
    const route = planRoute(home, at(2, 3), false, true);
    const ride = route.legs.find((l) => l.k === 'ride');
    expect(ride?.ms).toBeGreaterThanOrEqual(RIDE_MIN_MS);
    let walked = 0;
    for (const l of route.legs) {
      if (l === ride) break;
      walked += l.ms;
    }
    const mid = routePosAt(route, walked + (ride?.ms ?? 0) / 2);
    expect(mid.floor).toBe('car');
    expect(mid.pos.x).toBe(SHAFT_X);
    const before = routePosAt(route, walked - 1).carY ?? 0;
    const after = routePosAt(route, walked + (ride?.ms ?? 0) + 1).carY ?? 0;
    expect(after).toBeLessThan(before);
  });

  it('hurry_is_faster', () => {
    const slow = planRoute(home, at(2, 3), false, true).totalMs;
    const fast = planRoute(home, at(2, 3), true, true).totalMs;
    expect(fast).toBeLessThan(slow);
  });

  it('motion_off_has_zero_duration_but_still_goes_through_the_ride', () => {
    const route = planRoute(home, at(2, 3), false, false);
    expect(route.totalMs).toBe(0);
    expect(route.legs.some((l) => l.k === 'ride')).toBe(true);
    expect(routePosAt(route, 0)).toMatchObject({ floor: 2 });
  });

  it('walkPath_is_axis_parallel', () => {
    const pts = walkPath(1, briefSpot(deskAt(1, 0).seat), briefSpot(deskAt(1, 18).seat));
    for (let i = 1; i < pts.length; i++) {
      const a = pts[i - 1] as Vec3;
      const b = pts[i] as Vec3;
      expect(Math.abs(a.x - b.x) < 1e-9 || Math.abs(a.z - b.z) < 1e-9).toBe(true);
    }
  });
});
