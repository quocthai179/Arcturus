// Bố cục sàn văn phòng sống, toạ độ logic 1000 × 620 (docs/OFFICE_LIVE.md §2). Toạ độ nhân vật là điểm chân.
import { Grid, type Point, type Rect } from './geometry';

export const STAGE_W = 1000;
export const STAGE_H = 620;
export const CELL = 20;

/** Tường trên cao 100: không đi được. */
export const WALL_H = 100;

export const DESK_XS = [110, 260, 410, 560] as const;
export const DESK_YS = [200, 340] as const;

export type SeatKind = 'desk' | 'hot';

export interface Seat {
  kind: SeatKind;
  /** Chỗ ngồi (điểm chân). */
  at: Point;
  /** Tâm bàn (chỉ có ở `desk`). */
  desk?: Point;
}

/** 8 bàn làm việc rồi 4 chỗ "hot desk" quanh bàn họp. Thứ tự này là thứ tự cấp chỗ. */
export const SEATS: readonly Seat[] = [
  ...DESK_YS.flatMap((y) => DESK_XS.map((x): Seat => ({ kind: 'desk', at: { x, y: y - 20 }, desk: { x, y } }))),
  ...[330, 383, 437, 490].map((x): Seat => ({ kind: 'hot', at: { x, y: 482 } })),
];

export const MEETING_TABLE: Rect = { x: 300, y: 500, w: 220, h: 48 };

/** Agent không có chỗ ngồi thì làm việc đứng cạnh bảng trắng. */
export const STANDING_SPOTS: readonly Point[] = [
  { x: 60, y: 540 }, { x: 105, y: 540 }, { x: 150, y: 540 }, { x: 195, y: 540 },
];

export const DOOR: Point = { x: 650, y: 598 };

export type PoiId =
  | 'coffee' | 'cooler' | 'pantry-table' | 'lounge' | 'window' | 'bookshelf' | 'whiteboard'
  | 'window-2' | 'plant' | 'fridge' | 'printer' | 'sofa-view';

export interface Poi {
  id: PoiId;
  label: string;
  /** Điểm hội thoại có hai chỗ đứng đối diện; điểm một người có một chỗ. */
  spots: readonly Point[];
}

/** Điểm tới khi đi dạo (một chỗ) hoặc tán gẫu (hai chỗ). */
export const POIS: readonly Poi[] = [
  { id: 'coffee', label: 'máy cà phê', spots: [{ x: 905, y: 458 }, { x: 958, y: 462 }] },
  { id: 'cooler', label: 'bình nước', spots: [{ x: 730, y: 448 }, { x: 778, y: 450 }] },
  { id: 'pantry-table', label: 'bàn pantry', spots: [{ x: 785, y: 540 }, { x: 895, y: 540 }] },
  { id: 'lounge', label: 'bàn trà', spots: [{ x: 788, y: 218 }, { x: 912, y: 218 }] },
  { id: 'window', label: 'cửa sổ', spots: [{ x: 188, y: 116 }, { x: 236, y: 118 }] },
  { id: 'bookshelf', label: 'kệ sách', spots: [{ x: 585, y: 498 }, { x: 636, y: 500 }] },
  { id: 'whiteboard', label: 'bảng trắng', spots: [{ x: 68, y: 500 }, { x: 122, y: 502 }] },
  { id: 'window-2', label: 'cửa sổ', spots: [{ x: 452, y: 116 }] },
  { id: 'plant', label: 'chậu cây', spots: [{ x: 630, y: 152 }] },
  { id: 'fridge', label: 'tủ lạnh', spots: [{ x: 928, y: 584 }] },
  { id: 'printer', label: 'máy in', spots: [{ x: 84, y: 302 }] },
  { id: 'sofa-view', label: 'phòng nghỉ', spots: [{ x: 850, y: 290 }] },
];

export const CHAT_POIS: readonly Poi[] = POIS.filter((p) => p.spots.length === 2);

/** Chỗ ngủ gật khi hết giờ giải lao (sofa 3 chỗ, 2 ghế lười). */
export const NAP_SPOTS: readonly Point[] = [
  { x: 800, y: 152 }, { x: 850, y: 152 }, { x: 900, y: 152 }, { x: 742, y: 318 }, { x: 952, y: 318 },
];

export const SOFA: Rect = { x: 760, y: 100, w: 180, h: 46 };
export const COFFEE_TABLE: Rect = { x: 810, y: 196, w: 80, h: 40 };
export const COUNTER: Rect = { x: 880, y: 396, w: 100, h: 40 };
export const COOLER: Rect = { x: 714, y: 380, w: 32, h: 40 };
export const PANTRY_TABLE: Rect = { x: 810, y: 515, w: 60, h: 45 };
export const FRIDGE: Rect = { x: 946, y: 520, w: 34, h: 84 };
export const WHITEBOARD: Rect = { x: 30, y: 452, w: 120, h: 18 };
export const BOOKSHELF: Rect = { x: 560, y: 440, w: 100, h: 30 };
export const PRINTER: Rect = { x: 22, y: 280, w: 38, h: 40 };
/** Vách kính giữa khu làm việc và phòng nghỉ/pantry, hai cửa thông. */
export const PARTITION_X = 686;
export const PARTITION_DOORS: readonly [number, number][] = [[220, 300], [440, 520]];
/** Tường thấp giữa phòng nghỉ và pantry (chừa lối ở bên phải). */
export const HALF_WALL: Rect = { x: 704, y: 356, w: 156, h: 16 };
export const PLANTS: readonly Point[] = [{ x: 660, y: 132 }, { x: 970, y: 134 }, { x: 30, y: 594 }, { x: 596, y: 598 }];

export function deskRect(desk: Point): Rect {
  return { x: desk.x - 58, y: desk.y - 8, w: 116, h: 30 };
}

export function buildGrid(): Grid {
  const grid = new Grid(STAGE_W / CELL, Math.ceil(STAGE_H / CELL), CELL);
  grid.block({ x: 0, y: 0, w: STAGE_W, h: WALL_H });
  grid.block({ x: 0, y: 0, w: 20, h: STAGE_H });
  grid.block({ x: STAGE_W - 20, y: 0, w: 20, h: STAGE_H });
  grid.block({ x: 0, y: 604, w: 620, h: 20 });
  grid.block({ x: 680, y: 604, w: 320, h: 20 });
  let top = WALL_H;
  for (const [a, b] of PARTITION_DOORS) {
    grid.block({ x: PARTITION_X, y: top, w: 18, h: a - top });
    top = b;
  }
  grid.block({ x: PARTITION_X, y: top, w: 18, h: STAGE_H - top });
  for (const seat of SEATS) if (seat.desk) grid.block(deskRect(seat.desk));
  for (const r of [MEETING_TABLE, SOFA, COFFEE_TABLE, COUNTER, COOLER, PANTRY_TABLE, FRIDGE, WHITEBOARD, BOOKSHELF, PRINTER, HALF_WALL]) {
    grid.block(r);
  }
  for (const p of PLANTS) grid.block({ x: p.x - 12, y: p.y - 26, w: 24, h: 26 });
  // Khe giữa kệ sách và vách kính hẹp hơn một người: chặn để không ai trông như đi xuyên kệ.
  grid.block({ x: BOOKSHELF.x + BOOKSHELF.w, y: BOOKSHELF.y, w: PARTITION_X - BOOKSHELF.x - BOOKSHELF.w, h: BOOKSHELF.h });
  return grid;
}
