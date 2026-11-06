// Góc nhìn và khung camera (docs/OFFICE_BUILDING.md §8.5). Thuần: không three. Vec3 riêng.
import { CEO_FLOOR, FLOOR_COUNT, type FloorId } from './floors';
import { BASE, GEO, LIFT, type Vec3 } from './layout';

export type View = 'ext' | FloorId;
/** Vào cảnh → Phòng CEO (thấy ngay thư ký và tường trạng thái). */
export const DEFAULT_VIEW: View = CEO_FLOOR;
export const FOV = 36;
export const NAV_MS = 1000;

export interface Frame {
  pos: Vec3;
  target: Vec3;
}

const norm = (v: Vec3): Vec3 => {
  const l = Math.hypot(v.x, v.y, v.z);
  return { x: v.x / l, y: v.y / l, z: v.z / l };
};

function frame(target: Vec3, dir: Vec3, hw: number, hh: number, aspect: number): Frame {
  const t = Math.tan((FOV * Math.PI) / 360);
  const dist = Math.max(hh / t, hw / (t * aspect)) * 1.08;
  const d = norm(dir);
  return { target, pos: { x: target.x + d.x * dist, y: target.y + d.y * dist, z: target.z + d.z * dist } };
}

export function frameFor(view: View, aspect: number): Frame {
  if (view === 'ext') return frame({ x: 2.5, y: 13.5, z: 0 }, { x: 0.55, y: 0.5, z: 0.72 }, 24, 19.5, aspect);
  const g = GEO[view] ?? (GEO[0] as (typeof GEO)[number]);
  const target = { x: g.cx + 1.4, y: (BASE[view] ?? 0) + (view === 0 ? 1.6 : 0.9), z: g.cz + 0.9 };
  return frame(target, { x: 0.05, y: 0.9, z: 0.5 }, g.w / 2 + 3.4, view === 0 ? 10.5 : g.d / 2 + 1.6, aspect);
}

export interface ViewTargets {
  /** 7 phần: tầng 0–5 + mái. */
  lift: readonly number[];
  /** 6 phần: 1 = kính tầng đó trong suốt, trần/nan nhôm ẩn, nhãn hiện. */
  cut: readonly number[];
  visible: readonly boolean[];
}

export function viewTargets(view: View): ViewTargets {
  const lift = Array.from({ length: FLOOR_COUNT + 1 }, (_, m) => (view === 'ext' ? 0 : m > view ? LIFT : 0));
  const cut = Array.from({ length: FLOOR_COUNT }, (_, m) => (m === view ? 1 : 0));
  return { lift, cut, visible: lift.slice(0, FLOOR_COUNT).map((l) => l < 0.45 * LIFT) };
}

export function ease(p: number): number {
  const x = Math.min(1, Math.max(0, p));
  return x < 0.5 ? 4 * x * x * x : 1 - (-2 * x + 2) ** 3 / 2;
}

export function viewName(view: View): string {
  return view === 'ext' ? 'ngoại cảnh' : `tầng ${view}`;
}
