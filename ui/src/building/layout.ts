// Bố cục tòa nhà (docs/OFFICE_BUILDING.md §4). Thuần (không three). Đơn vị mét, trục y lên, mặt trước hướng +z.
// Tọa độ tầng là CỤC BỘ so với tâm tầng `(cx, BASE[n], cz)`.
import type { AgentFloorId, FloorId } from './floors';
import { FLOORS } from './floors';

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export interface FloorGeo {
  w: number;
  d: number;
  cx: number;
  cz: number;
  H: number;
}

export const GEO: readonly FloorGeo[] = [
  { w: 24, d: 16, cx: 0, cz: 0, H: 6.0 },
  { w: 22, d: 15, cx: 0, cz: 0, H: 3.8 },
  { w: 24, d: 17, cx: 0, cz: 0.75, H: 3.8 },
  { w: 22, d: 11, cx: 0, cz: -2.5, H: 3.8 },
  { w: 20, d: 11.5, cx: -1, cz: -2.25, H: 3.8 },
  { w: 16, d: 10, cx: -1, cz: -3, H: 4.4 },
];

export const BASE: readonly number[] = GEO.reduce<number[]>((acc, _g, i) => {
  acc.push(i === 0 ? 0 : (acc[i - 1] ?? 0) + (GEO[i - 1]?.H ?? 0));
  return acc;
}, []);
export const ROOF_Y: number = (BASE[5] ?? 0) + (GEO[5]?.H ?? 0);
/** Các tầng trên tầng đang xem nâng lên chừng này. */
export const LIFT = 16;
/** Trục thang máy kính (ngoài tòa, bên phải). */
export const SHAFT_X = 13.8;
export const CAR = { w: 2.5, h: 2.7 };
export const FLOOR_SLAB = 0.3;

export function geo(floor: FloorId): FloorGeo {
  return GEO[floor] ?? GEO[0] as FloorGeo;
}

/** Điểm đứng của thang máy trên tầng `n` (cục bộ). */
export function elevatorDoor(floor: FloorId): Vec3 {
  const g = geo(floor);
  return { x: g.w / 2 - 0.6, y: FLOOR_SLAB, z: -g.cz };
}
export const ELEVATOR_DOOR: readonly Vec3[] = GEO.map((_, i) => elevatorDoor(i as FloorId));

// --- bàn tầng 0–4 ---

export interface DeskGrid {
  cols: number;
  x0: number;
  z0: number;
  dx: number;
  dz: number;
  lounge: [number, number];
}

// Ghi chú: góc nghỉ tầng Media dời từ (-7.4, 3.6) của tài liệu sang (5.6, 2.8) để không chồng hàng bàn cuối và để chỗ đứng
// (az + 2.2) còn nằm trong sàn sâu 11,5 m.
export const LAY: readonly DeskGrid[] = [
  { cols: 5, x0: -10.5, z0: -5.2, dx: 2.1, dz: 2.8, lounge: [-8.5, 3.2] },
  { cols: 5, x0: -9.2, z0: -5.0, dx: 2.1, dz: 2.7, lounge: [6.0, 4.6] },
  { cols: 5, x0: -10.2, z0: -6.0, dx: 2.1, dz: 2.7, lounge: [6.2, 5.0] },
  { cols: 4, x0: -9.0, z0: -3.4, dx: 2.1, dz: 2.7, lounge: [5.2, 2.6] },
  { cols: 5, x0: -8.2, z0: -3.2, dx: 2.1, dz: 2.7, lounge: [5.6, 2.8] },
];
export const HOT_DESKS = 4;
export const LOBBY_SESSION_DESKS = 6;
export const LOBBY_GENERAL_DESKS = 4;
export const GUEST_SOFA_SPOTS = 4;

export const DESK_SIZE = { w: 1.5, d: 0.7 };

/** Số bàn của tầng (vai trò + linh hoạt; Sảnh: 6 session + 4 general). */
export function deskCount(floor: AgentFloorId): number {
  return floor === 0 ? LOBBY_SESSION_DESKS + LOBBY_GENERAL_DESKS : (FLOORS[floor]?.roles.length ?? 0) + HOT_DESKS;
}

function grid(floor: AgentFloorId): DeskGrid {
  return LAY[floor] ?? (LAY[0] as DeskGrid);
}

export function deskRows(floor: AgentFloorId): number {
  return Math.ceil(deskCount(floor) / grid(floor).cols);
}

export function deskAt(floor: AgentFloorId, index: number): { desk: Vec3; seat: Vec3 } {
  const g = grid(floor);
  const desk = { x: g.x0 + (index % g.cols) * g.dx, y: FLOOR_SLAB, z: g.z0 + Math.floor(index / g.cols) * g.dz };
  return { desk, seat: { x: desk.x, y: FLOOR_SLAB, z: desk.z + 0.95 } };
}

/** Chỗ nghỉ: 3 sofa + 3 đứng quanh bàn trà. */
export const LOUNGE_SPOT_COUNT = 6;
export function loungeSpots(floor: AgentFloorId): Vec3[] {
  const [ax, az] = grid(floor).lounge;
  const y = FLOOR_SLAB;
  return [
    { x: ax, y, z: az - 1.7 },
    { x: ax + 1, y, z: az - 1.7 },
    { x: ax + 2, y, z: az - 1.7 },
    { x: ax + 1 - 1.1, y, z: az + 0.9 },
    { x: ax + 1 + 1.1, y, z: az + 0.9 },
    { x: ax + 1, y, z: az + 1.3 },
  ];
}
export const LOUNGE_SPOTS: readonly (readonly Vec3[])[] = [0, 1, 2, 3, 4].map((n) => loungeSpots(n as AgentFloorId));
/** Ba chỗ đầu của góc nghỉ là sofa (ngồi), còn lại đứng. */
export const LOUNGE_SOFA_SPOTS = 3;

export const STANDING_SPOT_COUNT = 4;
export function standingSpots(floor: AgentFloorId): Vec3[] {
  const [ax, az] = grid(floor).lounge;
  return [-1, 0, 1, 2].map((k) => ({ x: ax + k, y: FLOOR_SLAB, z: az + 2.2 }));
}
export const STANDING_SPOTS: readonly (readonly Vec3[])[] = [0, 1, 2, 3, 4].map((n) => standingSpots(n as AgentFloorId));

/** Chỗ thư ký đứng cạnh agent ngồi ở `seat`. */
export function briefSpot(seat: Vec3): Vec3 {
  return { x: seat.x + 0.75, y: seat.y, z: seat.z + 0.15 };
}

/** Cột lối đi (khe 0,6 m giữa hai cột bàn) gần `x` nhất về bên trái: bên phải bàn có tâm `x` thì là `seat.x + 1.05`. */
export function gapColumn(floor: AgentFloorId, x: number): number {
  const g = grid(floor);
  const k = Math.min(g.cols - 1, Math.max(-1, Math.floor((x - g.x0) / g.dx + 1e-6)));
  return g.x0 + k * g.dx + g.dx / 2;
}

/** Hàng ngang lối đi nằm sau hàng bàn cuối. */
export function aisleZ(floor: AgentFloorId): number {
  const g = grid(floor);
  return g.z0 + deskRows(floor) * g.dz - 0.4;
}

// --- hình chữ nhật chặn lối đi (cục bộ, tâm + nửa kích thước) ---

export interface Rect {
  x0: number;
  x1: number;
  z0: number;
  z1: number;
}

/** Bàn (và ghế) của tầng, dùng kiểm tra lối đi không cắt bàn. */
export function deskRects(floor: AgentFloorId): Rect[] {
  const rects: Rect[] = [];
  for (let i = 0; i < deskCount(floor); i++) {
    const { desk } = deskAt(floor, i);
    rects.push({ x0: desk.x - DESK_SIZE.w / 2, x1: desk.x + DESK_SIZE.w / 2, z0: desk.z - DESK_SIZE.d / 2, z1: desk.z + DESK_SIZE.d / 2 });
  }
  return rects;
}

// --- Phòng CEO (tầng 5) ---

export const CEO_DESK = { at: { x: -1.5, y: 0.3, z: -1.8 }, w: 2.6, d: 0.9 };
export const SECRETARY_DESK = { at: { x: 3.4, y: 0.3, z: -1.2 }, w: 1.8, d: 0.8 };
export const SECRETARY_HOME: Vec3 = { x: 3.4, y: 0.3, z: -0.3 };
export const STATUS_WALL = { z: -4.74, y: 2.4, xs: [-6, -3.6, -1.2, 1.2, 3.6], w: 2.1, h: 1.2 };
export const CEO_LOUNGE: Vec3 = { x: -5.2, y: 0.3, z: 2.4 };
/** Làn đi ngang trong Phòng CEO (trống: không bàn nào). */
export const CEO_LANE_Z = 1.4;

export function ceoRects(): Rect[] {
  return [
    { x0: CEO_DESK.at.x - CEO_DESK.w / 2, x1: CEO_DESK.at.x + CEO_DESK.w / 2, z0: CEO_DESK.at.z - CEO_DESK.d / 2, z1: CEO_DESK.at.z + CEO_DESK.d / 2 },
    { x0: SECRETARY_DESK.at.x - SECRETARY_DESK.w / 2, x1: SECRETARY_DESK.at.x + SECRETARY_DESK.w / 2, z0: SECRETARY_DESK.at.z - SECRETARY_DESK.d / 2, z1: SECRETARY_DESK.at.z + SECRETARY_DESK.d / 2 },
  ];
}

/** Hình chữ nhật chặn của tầng bất kỳ (cục bộ). */
export function obstacles(floor: FloorId): Rect[] {
  return floor === 5 ? ceoRects() : deskRects(floor as AgentFloorId);
}

/** Điểm `p` (cục bộ) có nằm trong mặt bằng tầng `floor` (lề `margin`) không. */
export function insideFloor(floor: FloorId, p: { x: number; z: number }, margin = 0): boolean {
  const g = geo(floor);
  return Math.abs(p.x) <= g.w / 2 - margin && Math.abs(p.z) <= g.d / 2 - margin;
}

/** Vị trí thế giới của điểm cục bộ trên tầng `floor`. */
export function toWorld(floor: FloorId, p: Vec3): Vec3 {
  const g = geo(floor);
  return { x: g.cx + p.x, y: (BASE[floor] ?? 0) + p.y, z: g.cz + p.z };
}
