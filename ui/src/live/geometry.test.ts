import { describe, expect, it } from 'vitest';
import { findPath, Grid, lineClear, type Point } from './geometry';
import { buildGrid, CHAT_POIS, DOOR, NAP_SPOTS, POIS, SEATS, STANDING_SPOTS } from './layout';

function walkable(grid: Grid, from: Point, path: Point[]): boolean {
  let prev = from;
  for (const p of path) {
    if (!lineClear(grid, prev, p, (c, r) => {
      const [sc, sr] = grid.cellOf(from);
      const last = path[path.length - 1] ?? from;
      const [gc, gr] = grid.cellOf(last);
      return (c === sc && r === sr) || (c === gc && r === gr) || grid.free(c, r);
    })) return false;
    prev = p;
  }
  return true;
}

describe('findPath', () => {
  it('goes_around_a_wall_and_ends_exactly_at_the_target', () => {
    const grid = new Grid(10, 10, 10);
    grid.block({ x: 40, y: 0, w: 10, h: 80 });
    const from = { x: 15, y: 15 };
    const to = { x: 85, y: 17 };
    const path = findPath(grid, from, to);
    expect(path).not.toBeNull();
    expect(path?.[path.length - 1]).toEqual(to);
    expect(path?.some((p) => p.y > 80)).toBe(true);
    expect(walkable(grid, from, path ?? [])).toBe(true);
  });

  it('never_cuts_a_blocked_corner_diagonally', () => {
    const grid = new Grid(3, 3, 10);
    grid.block({ x: 10, y: 0, w: 10, h: 10 });
    grid.block({ x: 0, y: 10, w: 10, h: 10 });
    expect(findPath(grid, { x: 5, y: 5 }, { x: 15, y: 15 })).toBeNull();
  });

  it('returns_null_when_the_target_is_walled_off', () => {
    const grid = new Grid(10, 10, 10);
    grid.block({ x: 50, y: 0, w: 10, h: 100 });
    expect(findPath(grid, { x: 5, y: 5 }, { x: 95, y: 95 })).toBeNull();
  });

  it('smoothed_path_has_few_corners_in_open_space', () => {
    const grid = new Grid(20, 20, 10);
    const path = findPath(grid, { x: 5, y: 5 }, { x: 195, y: 105 });
    expect(path).toEqual([{ x: 195, y: 105 }]);
  });
});

describe('office layout', () => {
  const grid = buildGrid();

  it('every_seat_poi_nap_spot_and_standing_spot_is_reachable_from_the_door', () => {
    const targets: Point[] = [
      ...SEATS.map((s) => s.at),
      ...POIS.flatMap((p) => p.spots),
      ...NAP_SPOTS,
      ...STANDING_SPOTS,
    ];
    for (const t of targets) {
      const path = findPath(grid, DOOR, t);
      expect(path, `không tới được (${t.x}, ${t.y})`).not.toBeNull();
      expect(walkable(grid, DOOR, path ?? [])).toBe(true);
    }
  });

  it('targets_stand_on_free_cells_so_nobody_stands_inside_furniture', () => {
    for (const t of [...SEATS.map((s) => s.at), ...POIS.flatMap((p) => p.spots), ...STANDING_SPOTS, DOOR]) {
      const [c, r] = grid.cellOf(t);
      expect(grid.free(c, r), `(${t.x}, ${t.y}) nằm trong đồ đạc`).toBe(true);
    }
  });

  it('has_twelve_seats_and_several_chat_spots', () => {
    expect(SEATS).toHaveLength(12);
    expect(CHAT_POIS.length).toBeGreaterThanOrEqual(6);
  });
});
