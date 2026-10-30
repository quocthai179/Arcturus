// Lưới ô của sàn văn phòng sống và tìm đường (docs/OFFICE_LIVE.md §2). Hàm thuần, không phụ thuộc React hay DOM.

export interface Point {
  x: number;
  y: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export class Grid {
  readonly blocked: Uint8Array;

  constructor(
    readonly cols: number,
    readonly rows: number,
    readonly cell: number,
  ) {
    this.blocked = new Uint8Array(cols * rows);
  }

  /** Chặn mọi ô có tâm nằm trong `rect` (kể cả mép). */
  block(rect: Rect): void {
    for (let r = 0; r < this.rows; r++) {
      const cy = (r + 0.5) * this.cell;
      if (cy < rect.y || cy > rect.y + rect.h) continue;
      for (let c = 0; c < this.cols; c++) {
        const cx = (c + 0.5) * this.cell;
        if (cx >= rect.x && cx <= rect.x + rect.w) this.blocked[r * this.cols + c] = 1;
      }
    }
  }

  inside(c: number, r: number): boolean {
    return c >= 0 && r >= 0 && c < this.cols && r < this.rows;
  }

  free(c: number, r: number): boolean {
    return this.inside(c, r) && this.blocked[r * this.cols + c] === 0;
  }

  cellOf(p: Point): [number, number] {
    const c = Math.min(this.cols - 1, Math.max(0, Math.floor(p.x / this.cell)));
    const r = Math.min(this.rows - 1, Math.max(0, Math.floor(p.y / this.cell)));
    return [c, r];
  }

  center(c: number, r: number): Point {
    return { x: (c + 0.5) * this.cell, y: (r + 0.5) * this.cell };
  }
}

const DIRS: ReadonlyArray<readonly [number, number, number]> = [
  [1, 0, 1], [-1, 0, 1], [0, 1, 1], [0, -1, 1],
  [1, 1, Math.SQRT2], [1, -1, Math.SQRT2], [-1, 1, Math.SQRT2], [-1, -1, Math.SQRT2],
];

/**
 * A* 8 hướng, không cắt góc vật cản. Ô xuất phát và ô đích luôn được coi là đi được (chỗ ngồi có thể nằm sát đồ đạc).
 * Trả các điểm cần đi qua, không gồm điểm xuất phát, điểm cuối đúng bằng `to`; `null` khi không có đường.
 */
export function findPath(grid: Grid, from: Point, to: Point): Point[] | null {
  const [sc, sr] = grid.cellOf(from);
  const [gc, gr] = grid.cellOf(to);
  const start = sr * grid.cols + sc;
  const goal = gr * grid.cols + gc;
  if (start === goal) return [to];
  const passable = (c: number, r: number) => {
    if (!grid.inside(c, r)) return false;
    const i = r * grid.cols + c;
    return i === start || i === goal || grid.blocked[i] === 0;
  };

  const n = grid.cols * grid.rows;
  const g = new Float64Array(n).fill(Infinity);
  const came = new Int32Array(n).fill(-1);
  const closed = new Uint8Array(n);
  const heap = new MinHeap();
  const h = (i: number) => {
    const dx = Math.abs((i % grid.cols) - gc);
    const dy = Math.abs(Math.floor(i / grid.cols) - gr);
    return Math.max(dx, dy) + (Math.SQRT2 - 1) * Math.min(dx, dy);
  };
  g[start] = 0;
  heap.push(start, h(start));
  while (heap.size > 0) {
    const cur = heap.pop();
    if (cur === goal) break;
    if (closed[cur] === 1) continue;
    closed[cur] = 1;
    const c = cur % grid.cols;
    const r = Math.floor(cur / grid.cols);
    for (const [dc, dr, cost] of DIRS) {
      const nc = c + dc;
      const nr = r + dr;
      if (!passable(nc, nr)) continue;
      if (dc !== 0 && dr !== 0 && (!passable(c + dc, r) || !passable(c, r + dr))) continue;
      const ni = nr * grid.cols + nc;
      const ng = (g[cur] ?? Infinity) + cost;
      if (ng < (g[ni] ?? Infinity)) {
        g[ni] = ng;
        came[ni] = cur;
        heap.push(ni, ng + h(ni));
      }
    }
  }
  if (came[goal] === -1) return null;

  const cells: number[] = [];
  for (let i = goal; i !== start; i = came[i] ?? start) cells.push(i);
  cells.reverse();
  const points = cells.map((i) => grid.center(i % grid.cols, Math.floor(i / grid.cols)));
  points[points.length - 1] = to;
  return smooth(grid, from, points, passable);
}

/** Kéo thẳng: bỏ điểm giữa khi đoạn thẳng nối hai đầu không cắt ô bị chặn. */
function smooth(
  grid: Grid,
  from: Point,
  points: Point[],
  passable: (c: number, r: number) => boolean,
): Point[] {
  const out: Point[] = [];
  let anchor = from;
  let i = 0;
  while (i < points.length) {
    let j = points.length - 1;
    while (j > i && !lineClear(grid, anchor, points[j] as Point, passable)) j--;
    const p = points[j] as Point;
    out.push(p);
    anchor = p;
    i = j + 1;
  }
  return out;
}

/** Lấy mẫu dày (¼ ô) dọc đoạn thẳng và kiểm cả hai ô kề khi đi qua góc. */
export function lineClear(
  grid: Grid,
  a: Point,
  b: Point,
  passable: (c: number, r: number) => boolean = (c, r) => grid.free(c, r),
): boolean {
  const dist = Math.hypot(b.x - a.x, b.y - a.y);
  const steps = Math.max(1, Math.ceil(dist / (grid.cell / 4)));
  let prev = grid.cellOf(a);
  for (let s = 1; s <= steps; s++) {
    const t = s / steps;
    const cur = grid.cellOf({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t });
    if (!passable(cur[0], cur[1])) return false;
    if (cur[0] !== prev[0] && cur[1] !== prev[1] && (!passable(cur[0], prev[1]) || !passable(prev[0], cur[1]))) {
      return false;
    }
    prev = cur;
  }
  return true;
}

export function distance(a: Point, b: Point): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

class MinHeap {
  private items: number[] = [];
  private keys: number[] = [];

  get size(): number {
    return this.items.length;
  }

  push(item: number, key: number): void {
    this.items.push(item);
    this.keys.push(key);
    let i = this.items.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if ((this.keys[p] ?? 0) <= key) break;
      this.swap(i, p);
      i = p;
    }
  }

  pop(): number {
    const top = this.items[0] ?? -1;
    const lastItem = this.items.pop();
    const lastKey = this.keys.pop();
    if (this.items.length > 0 && lastItem !== undefined && lastKey !== undefined) {
      this.items[0] = lastItem;
      this.keys[0] = lastKey;
      let i = 0;
      for (;;) {
        const l = 2 * i + 1;
        const r = l + 1;
        let m = i;
        if (l < this.keys.length && (this.keys[l] ?? 0) < (this.keys[m] ?? 0)) m = l;
        if (r < this.keys.length && (this.keys[r] ?? 0) < (this.keys[m] ?? 0)) m = r;
        if (m === i) break;
        this.swap(i, m);
        i = m;
      }
    }
    return top;
  }

  private swap(a: number, b: number): void {
    const ti = this.items[a] ?? 0;
    this.items[a] = this.items[b] ?? 0;
    this.items[b] = ti;
    const tk = this.keys[a] ?? 0;
    this.keys[a] = this.keys[b] ?? 0;
    this.keys[b] = tk;
  }
}
