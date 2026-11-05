// Lộ trình của thư ký (docs/OFFICE_BUILDING.md §7.4). Thuần. Cùng tầng: đi theo lối đi (cột khe bàn → hàng lối đi → cột khe bàn
// đích), các đoạn song song trục nên không cắt bàn. Khác tầng: tới cửa thang máy → `ride` → ra cửa thang máy tầng đích → đích.
import type { AgentFloorId, FloorId } from './floors';
import { aisleZ, BASE, CEO_LANE_Z, ELEVATOR_DOOR, FLOOR_SLAB, gapColumn, SHAFT_X, type Vec3 } from './layout';

export type Leg =
  | { k: 'walk'; floor: FloorId; from: Vec3; to: Vec3; ms: number }
  | { k: 'ride'; fromFloor: FloorId; toFloor: FloorId; ms: number };
export interface Place {
  floor: FloorId;
  pos: Vec3;
}
export interface Route {
  legs: readonly Leg[];
  totalMs: number;
  start: Place;
  end: Place;
}

export const WALK_MPS = 1.4;
export const HURRY_MPS = 2.2;
export const RIDE_MS_PER_M = 160;
export const RIDE_MIN_MS = 500;
/** Quãng đường một chu kỳ bước (2 bước). */
export const STRIDE_M = 1.4;

const same = (a: Vec3, b: Vec3) => Math.abs(a.x - b.x) < 1e-6 && Math.abs(a.z - b.z) < 1e-6;
const dist = (a: Vec3, b: Vec3) => Math.hypot(a.x - b.x, a.z - b.z);

/** Các điểm gấp khúc trên một tầng (cục bộ). */
export function walkPath(floor: FloorId, from: Vec3, to: Vec3): Vec3[] {
  const y = to.y;
  const pts: Vec3[] = [from];
  if (floor === 5) {
    pts.push({ x: from.x, y, z: CEO_LANE_Z }, { x: to.x, y, z: CEO_LANE_Z });
  } else {
    const f = floor as AgentFloorId;
    const a = gapColumn(f, from.x);
    const b = gapColumn(f, to.x);
    const aisle = aisleZ(f);
    pts.push({ x: a, y, z: from.z }, { x: a, y, z: aisle }, { x: b, y, z: aisle }, { x: b, y, z: to.z });
  }
  pts.push(to);
  return pts.filter((p, i) => i === 0 || !same(p, pts[i - 1] as Vec3));
}

function walkLegs(floor: FloorId, from: Vec3, to: Vec3, mps: number, motion: boolean): Leg[] {
  const pts = walkPath(floor, from, to);
  const legs: Leg[] = [];
  for (let i = 1; i < pts.length; i++) {
    const a = pts[i - 1] as Vec3;
    const b = pts[i] as Vec3;
    legs.push({ k: 'walk', floor, from: a, to: b, ms: motion ? (dist(a, b) / mps) * 1000 : 0 });
  }
  return legs;
}

export function planRoute(from: Place, to: Place, hurry: boolean, motion: boolean): Route {
  const mps = hurry ? HURRY_MPS : WALK_MPS;
  const legs: Leg[] = [];
  if (from.floor === to.floor) {
    legs.push(...walkLegs(from.floor, from.pos, to.pos, mps, motion));
  } else {
    const doorA = ELEVATOR_DOOR[from.floor] ?? from.pos;
    const doorB = ELEVATOR_DOOR[to.floor] ?? to.pos;
    legs.push(...walkLegs(from.floor, from.pos, doorA, mps, motion));
    const dy = Math.abs((BASE[to.floor] ?? 0) - (BASE[from.floor] ?? 0));
    legs.push({ k: 'ride', fromFloor: from.floor, toFloor: to.floor, ms: motion ? Math.max(RIDE_MIN_MS, dy * RIDE_MS_PER_M) : 0 });
    legs.push(...walkLegs(to.floor, doorB, to.pos, mps, motion));
  }
  return { legs, totalMs: legs.reduce((s, l) => s + l.ms, 0), start: from, end: to };
}

export interface RoutePos {
  /** `'car'`: đang trong cabin; `pos` là tọa độ THẾ GIỚI (trục thang máy). Ngược lại `pos` cục bộ của tầng. */
  floor: FloorId | 'car';
  pos: Vec3;
  walking: boolean;
  /** Pha bước 0…1 (chỉ ý nghĩa khi `walking`). */
  phase: number;
  /** Độ cao thế giới của sàn cabin; `null` nếu lộ trình không dùng thang máy. */
  carY: number | null;
}

const carFloorY = (f: FloorId) => (BASE[f] ?? 0) + FLOOR_SLAB;
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

export function routePosAt(route: Route, t: number): RoutePos {
  const ride = route.legs.find((l) => l.k === 'ride');
  const endCar = ride?.k === 'ride' ? carFloorY(ride.toFloor) : null;
  if (t >= route.totalMs) return { floor: route.end.floor, pos: route.end.pos, walking: false, phase: 0, carY: endCar };
  let at = Math.max(0, t);
  let rodeFrom: number | null = null;
  for (const leg of route.legs) {
    if (at <= leg.ms && leg.ms > 0) {
      const p = at / leg.ms;
      if (leg.k === 'walk') {
        const pos = { x: lerp(leg.from.x, leg.to.x, p), y: lerp(leg.from.y, leg.to.y, p), z: lerp(leg.from.z, leg.to.z, p) };
        const walked = dist(leg.from, leg.to) * p;
        const carY = ride?.k === 'ride' ? (rodeFrom ?? carFloorY(ride.fromFloor)) : null;
        return { floor: leg.floor, pos, walking: true, phase: (walked / STRIDE_M) % 1, carY };
      }
      const y = lerp(carFloorY(leg.fromFloor), carFloorY(leg.toFloor), p);
      return { floor: 'car', pos: { x: SHAFT_X, y, z: 0 }, walking: false, phase: 0, carY: y };
    }
    at -= leg.ms;
    if (leg.k === 'ride') rodeFrom = carFloorY(leg.toFloor);
  }
  return { floor: route.end.floor, pos: route.end.pos, walking: false, phase: 0, carY: endCar };
}
