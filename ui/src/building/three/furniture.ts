// Đồ nội thất và cây cảnh thủ tục cho cảnh Tòa nhà (art pass 2026-11-06). Mọi hàm chỉ thêm hình vào `Batch`/`QuadSet` của tầng nên
// không tốn draw call riêng. Tọa độ cục bộ của tầng; `FY` là mặt sàn gỗ. Hộp bo góc (`rbox`) cho đồ mềm, bóng đổ giả (`blob`)
// thay cho ambient occlusion thật.
import { artRect, type ArtId } from './decor';
import { Batch, seeded, type QuadSet, type UvRect } from './kit';
import { BOOKS, FABRICS, LEAVES, mix, PAL, POTS } from './palette';

/** Mặt sàn gỗ (sàn bê tông cao 0,30 + lớp ván 0,02). */
export const FY = 0.32;
const BLOB: UvRect = [0, 0, 1, 1];

export interface Ctx {
  b: Batch;
  /** Gắn trần (ẩn khi xem mặt cắt). */
  top: Batch;
  /** Bóng đổ giả. */
  sh: QuadSet;
  /** Tranh/bảng trên tường (atlas). */
  art: QuadSet;
  /** Chiều cao trần của tầng. */
  H: number;
}

/** Bóng mềm dưới đồ vật (đen mờ, hình elip `w × d`). */
export function blob(c: Ctx, x: number, z: number, w: number, d: number, ry = 0): void {
  c.sh.flat(x, FY + 0.016, z, w, d, ry, BLOB);
}

const rot = (ry: number, x: number, z: number) => (dx: number, dz: number): [number, number] => [x + dx * Math.cos(ry) + dz * Math.sin(ry), z - dx * Math.sin(ry) + dz * Math.cos(ry)];

// --- cây ---

function pot(c: Ctx, x: number, z: number, s: number, color: string, h = 0.4, rt = 0.25, rb = 0.19): void {
  c.b.cyl(rt * s, rb * s, h * s, x, FY + (h * s) / 2, z, color, { ao: 0.3 }, 14);
  c.b.cyl(rt * s * 0.9, rt * s * 0.9, 0.02, x, FY + h * s - 0.004, z, '#4A3B30', {}, 14);
  c.b.cyl(rt * s * 1.04, rt * s * 1.04, 0.03, x, FY + h * s - 0.012, z, mix(color, '#FFFFFF', 0.12), {}, 14);
  blob(c, x, z, rt * s * 3.2, rt * s * 3.2);
}

/** Một lá dẹt hình elip, trục dài hướng ra góc `a`, ngẩng đầu `tilt` rad. */
function leaf(b: Batch, x: number, y: number, z: number, a: number, len: number, wid: number, tilt: number, color: string): void {
  b.sph(1, x, y, z, color, { ry: Math.PI / 2 - a, rx: -tilt, sx: wid, sy: 0.07, sz: len }, 1, 8);
}

/** Cây lá to kiểu monstera trong chậu. */
export function monstera(c: Ctx, x: number, z: number, s = 1, potColor: string = PAL.white): void {
  pot(c, x, z, s, potColor);
  const n = 7;
  for (let i = 0; i < n; i++) {
    const a = (i / n) * Math.PI * 2 + 0.4;
    const reach = (0.18 + (i % 3) * 0.09) * s;
    const top = FY + (0.62 + (i % 3) * 0.17) * s;
    const lx = x + Math.cos(a) * reach;
    const lz = z + Math.sin(a) * reach;
    c.b.link([x, FY + 0.38 * s, z], [lx, top - 0.06 * s, lz], 0.012 * s, '#3C7A55', {}, 5);
    leaf(c.b, lx + Math.cos(a) * 0.1 * s, top, lz + Math.sin(a) * 0.1 * s, a, 0.2 * s, 0.15 * s, 0.35, LEAVES[(i + 1) % LEAVES.length] ?? '#3F8F63');
  }
}

/** Cây thân cao kiểu bàng Singapore/cao su: thân nâu, lá xoắn ốc. */
export function tallPlant(c: Ctx, x: number, z: number, s = 1, potColor: string = PAL.terracotta): void {
  pot(c, x, z, s, potColor, 0.5, 0.26, 0.2);
  c.b.link([x, FY + 0.45 * s, z], [x + 0.04 * s, FY + 1.7 * s, z - 0.02 * s], 0.025 * s, '#6B4E3A', {}, 6, 0.014 * s);
  for (let k = 0; k < 12; k++) {
    const a = k * 2.4;
    const h = FY + (0.7 + k * 0.1) * s;
    const out = (0.2 - k * 0.006) * s;
    leaf(c.b, x + 0.03 * s + Math.cos(a) * out, h, z + Math.sin(a) * out, a, 0.2 * s, 0.11 * s, 0.5, LEAVES[k % LEAVES.length] ?? '#3F8F63');
  }
}

/** Lưỡi hổ: nhiều lá thẳng nhọn. */
export function snakePlant(c: Ctx, x: number, z: number, s = 1, potColor: string = PAL.charcoal): void {
  pot(c, x, z, s, potColor, 0.36, 0.2, 0.16);
  const rnd = seeded(Math.floor(Math.abs(x * 31 + z * 17) * 100) + 5);
  for (let i = 0; i < 8; i++) {
    const a = (i / 8) * Math.PI * 2;
    const r = 0.04 + rnd() * 0.07;
    const h = (0.45 + rnd() * 0.5) * s;
    c.b.cyl(0.004, 0.035 * s, h, x + Math.cos(a) * r * s, FY + 0.34 * s + h / 2, z + Math.sin(a) * r * s, i % 3 === 0 ? '#7FB58A' : '#2F6B4A', { rz: Math.cos(a) * 0.12, rx: Math.sin(a) * 0.12 }, 5);
  }
}

/** Cây mọng nhỏ đặt trên bàn (y là mặt bàn). */
export function succulent(b: Batch, x: number, y: number, z: number, color: string = PAL.terracotta): void {
  b.cyl(0.042, 0.032, 0.055, x, y + 0.028, z, color, {}, 8);
  for (let i = 0; i < 6; i++) {
    const a = (i / 6) * Math.PI * 2;
    b.cyl(0.002, 0.016, 0.07, x + Math.cos(a) * 0.018, y + 0.085, z + Math.sin(a) * 0.018, i % 2 === 0 ? '#5FA878' : '#7BC08E', { rz: Math.cos(a) * 0.7, rx: Math.sin(a) * 0.7 }, 5);
  }
}

/** Chậu treo trần với dây leo rủ (gắn vào lô `top`). */
export function hangingPlant(c: Ctx, x: number, z: number, drop = 1.2): void {
  const t = c.top;
  const y = c.H - drop;
  for (const [dx, dz] of [[0.12, 0], [-0.06, 0.1], [-0.06, -0.1]] as const) t.link([x, c.H - 0.1, z], [x + dx, y + 0.02, z + dz], 0.006, PAL.espresso, {}, 4);
  t.sph(0.15, x, y - 0.03, z, PAL.terracotta, {}, 0.8, 10);
  t.sph(0.14, x, y + 0.05, z, '#4C9A6A', {}, 0.5, 8);
  const rnd = seeded(Math.floor(Math.abs(x * 13 + z * 7) * 100) + 3);
  for (let v = 0; v < 5; v++) {
    const a = (v / 5) * Math.PI * 2;
    const len = 0.35 + rnd() * 0.45;
    const x0 = x + Math.cos(a) * 0.13;
    const z0 = z + Math.sin(a) * 0.13;
    t.link([x0, y - 0.05, z0], [x0 + Math.cos(a) * 0.08, y - 0.05 - len, z0 + Math.sin(a) * 0.08], 0.006, '#3C7A55', {}, 4);
    for (let k = 1; k <= 3; k++) t.sph(0.045, x0 + Math.cos(a) * 0.08 * (k / 3), y - 0.05 - (len * k) / 3.4, z0 + Math.sin(a) * 0.08 * (k / 3), LEAVES[(v + k) % LEAVES.length] ?? '#3F8F63', {}, 0.6, 6);
  }
}

/** Ba loại cây xoay vòng theo chỉ số để các tầng không lặp lại. */
export function plantAt(c: Ctx, x: number, z: number, variant: number, s = 1): void {
  const pc = POTS[variant % POTS.length] ?? PAL.white;
  if (variant % 3 === 0) monstera(c, x, z, s, pc);
  else if (variant % 3 === 1) tallPlant(c, x, z, s, pc);
  else snakePlant(c, x, z, s, pc);
}

// --- ghế, sofa, bàn ---

/** Ghế xoay văn phòng: đệm bo, tựa lưng ngả nhẹ, trụ và chân sao. Hướng về −z khi ry = 0. */
export function chair(c: Ctx, x: number, z: number, color: string, ry = 0): void {
  const at = rot(ry, x, z);
  const [sx, sz] = at(0, 0);
  c.b.rbox(0.48, 0.08, 0.46, sx, FY + 0.4, sz, color, { ry, ao: 0.15 }, 0.035);
  const [bx, bz] = at(0, 0.23);
  c.b.rbox(0.46, 0.5, 0.07, bx, FY + 0.7, bz, color, { ry, rx: -0.12 }, 0.035);
  const [lx, lz] = at(-0.27, 0.0);
  c.b.rbox(0.05, 0.04, 0.4, lx, FY + 0.56, lz, PAL.charcoal, { ry }, 0.015);
  const [ax, az] = at(0.27, 0.0);
  c.b.rbox(0.05, 0.04, 0.4, ax, FY + 0.56, az, PAL.charcoal, { ry }, 0.015);
  c.b.cyl(0.03, 0.03, 0.3, sx, FY + 0.25, sz, PAL.charcoal, {}, 6);
  for (let i = 0; i < 5; i++) {
    const a = (i / 5) * Math.PI * 2;
    c.b.link([sx, FY + 0.1, sz], [sx + Math.cos(a) * 0.27, FY + 0.045, sz + Math.sin(a) * 0.27], 0.017, PAL.charcoal, {}, 5);
    c.b.cyl(0.026, 0.026, 0.04, sx + Math.cos(a) * 0.27, FY + 0.025, sz + Math.sin(a) * 0.27, PAL.ink, {}, 5);
  }
  blob(c, sx, sz, 0.8, 0.8);
}

/** Sofa vải bo tròn quay mặt +z (ry = 0), mặt đệm cao 0,74 để người ngồi vừa (khớp tư thế `lounge_sit`). */
export function sofa(c: Ctx, x: number, z: number, w: number, color: string, ry = 0, pillow: string = PAL.mustard): void {
  const at = rot(ry, x, z);
  const dark = mix(color, '#000000', 0.12);
  const put = (dx: number, dz: number, bw: number, bh: number, bd: number, y: number, col: string, r: number, extra: { rx?: number } = {}) => {
    const [px, pz] = at(dx, dz);
    c.b.rbox(bw, bh, bd, px, y, pz, col, { ry, ...extra, ao: 0.12 }, r);
  };
  for (const sx of [-1, 1]) for (const sz of [-1, 1]) {
    const [px, pz] = at(sx * (w / 2 - 0.12), sz * 0.34);
    c.b.cyl(0.03, 0.022, 0.08, px, FY + 0.04, pz, PAL.walnut, {}, 6);
  }
  put(0, 0, w, 0.22, 0.9, FY + 0.17, dark, 0.06);
  put(0, -0.38, w, 0.56, 0.18, FY + 0.5, color, 0.08, { rx: -0.06 });
  put(-w / 2 + 0.11, 0.0, 0.22, 0.46, 0.9, FY + 0.37, color, 0.09);
  put(w / 2 - 0.11, 0.0, 0.22, 0.46, 0.9, FY + 0.37, color, 0.09);
  const n = Math.max(1, Math.round((w - 0.44) / 0.75));
  const cw = (w - 0.44) / n;
  for (let i = 0; i < n; i++) {
    const dx = -w / 2 + 0.22 + cw * (i + 0.5);
    put(dx, 0.04, cw - 0.03, 0.14, 0.66, FY + 0.35, mix(color, '#FFFFFF', 0.1), 0.06);
    put(dx, -0.27, cw - 0.06, 0.34, 0.14, FY + 0.6, mix(color, '#FFFFFF', 0.06), 0.06, { rx: -0.22 });
  }
  if (w > 1.8) {
    const [px, pz] = at(w / 2 - 0.34, -0.28);
    c.b.rbox(0.3, 0.3, 0.1, px, FY + 0.72, pz, pillow, { ry: ry - 0.35, rx: -0.35 }, 0.05);
  }
  blob(c, x, z + 0.0, w + 0.5, 1.4, ry);
}

/** Ghế bành một chỗ. */
export function armchair(c: Ctx, x: number, z: number, color: string, ry = 0): void {
  sofa(c, x, z, 0.95, color, ry, mix(color, '#FFFFFF', 0.3));
}

/** Bean bag. */
export function beanBag(c: Ctx, x: number, z: number, color: string): void {
  c.b.sph(0.42, x, FY + 0.27, z, color, { sx: 1.1, sz: 1.05, ao: 0.25 }, 0.72, 12);
  c.b.sph(0.3, x, FY + 0.46, z + 0.1, mix(color, '#FFFFFF', 0.12), { sx: 1.0, sz: 0.9 }, 0.5, 10);
  blob(c, x, z, 1.25, 1.2);
}

/** Bàn tròn một chân (bàn trà). `h` là chiều cao mặt bàn so với sàn. */
export function roundTable(c: Ctx, x: number, z: number, r: number, h: number, top: string = PAL.oak, base: string = PAL.walnut): void {
  c.b.cyl(r, r, 0.045, x, FY + h, z, top, {}, 24);
  c.b.cyl(r * 0.98, r * 0.98, 0.012, x, FY + h - 0.03, z, mix(top, '#000000', 0.2), {}, 24);
  c.b.cyl(0.04, 0.05, h - 0.03, x, FY + (h - 0.03) / 2, z, base, {}, 8);
  c.b.cyl(r * 0.45, r * 0.5, 0.025, x, FY + 0.012, z, base, {}, 16);
  blob(c, x, z, r * 2.8, r * 2.8);
}

/** Bàn chữ nhật chân gỗ (bàn họp, bàn thử…). */
export function table(c: Ctx, x: number, z: number, w: number, d: number, h = 0.72, top: string = PAL.oak, ry = 0): void {
  const at = rot(ry, x, z);
  c.b.rbox(w, 0.05, d, x, FY + h, z, top, { ry }, 0.02);
  for (const sx of [-1, 1]) for (const sz of [-1, 1]) {
    const [px, pz] = at((sx * (w - 0.14)) / 2, (sz * (d - 0.14)) / 2);
    c.b.cyl(0.02, 0.028, h - 0.02, px, FY + (h - 0.02) / 2, pz, PAL.walnut, {}, 6);
  }
  blob(c, x, z, w + 0.5, d + 0.5, ry);
}

/** Ghế đẩu. */
export function stool(c: Ctx, x: number, z: number, color: string, h = 0.65): void {
  c.b.cyl(0.18, 0.17, 0.06, x, FY + h, z, color, {}, 14);
  for (let i = 0; i < 3; i++) {
    const a = (i / 3) * Math.PI * 2 + 0.5;
    c.b.link([x + Math.cos(a) * 0.1, FY + h - 0.03, z + Math.sin(a) * 0.1], [x + Math.cos(a) * 0.2, FY, z + Math.sin(a) * 0.2], 0.014, PAL.walnut, {}, 5);
  }
  blob(c, x, z, 0.6, 0.6);
}

// --- đèn ---

export function floorLamp(c: Ctx, x: number, z: number, h = 1.6, shade: string = PAL.warmLight): void {
  c.b.cyl(0.14, 0.16, 0.03, x, FY + 0.015, z, PAL.charcoal, {}, 12);
  c.b.cyl(0.012, 0.012, h, x, FY + h / 2, z, PAL.brass, {}, 6);
  c.b.cyl(0.14, 0.2, 0.26, x, FY + h + 0.05, z, shade, { kind: 'glow' }, 14);
  c.b.cyl(0.14, 0.2, 0.01, x, FY + h - 0.08, z, mix(shade, '#000000', 0.15), {}, 14);
  blob(c, x, z, 0.5, 0.5);
}

/** Đèn thả (gắn trần). `y` là cao độ đáy chao. */
export function pendant(c: Ctx, x: number, z: number, y: number, color: string = PAL.terracotta, r = 0.28): void {
  c.top.link([x, c.H, z], [x, y + 0.2, z], 0.008, PAL.ink, {}, 4);
  c.top.cyl(0.05, r, 0.22, x, y + 0.1, z, color, {}, 18);
  c.top.cyl(r * 0.78, r * 0.78, 0.01, x, y - 0.005, z, PAL.warmLight, { kind: 'glow' }, 18);
}

/** Đèn bàn có tay: y là mặt bàn. */
export function deskLamp(b: Batch, x: number, y: number, z: number, color: string = PAL.brass): void {
  b.cyl(0.07, 0.08, 0.02, x, y + 0.01, z, PAL.charcoal, {}, 10);
  b.link([x, y + 0.02, z], [x + 0.06, y + 0.3, z - 0.02], 0.008, color, {}, 5);
  b.link([x + 0.06, y + 0.3, z - 0.02], [x + 0.17, y + 0.36, z + 0.03], 0.008, color, {}, 5);
  b.cyl(0.02, 0.07, 0.07, x + 0.19, y + 0.34, z + 0.04, color, { rz: -0.4 }, 10);
  b.sph(0.022, x + 0.2, y + 0.3, z + 0.045, PAL.warmLight, { kind: 'glow' }, 1, 6);
}

// --- tranh, bảng, kệ ---

/** Treo tranh/bảng từ atlas lên tường. `ry` hướng pháp tuyến (0 = +z, π/2 = +x, π = −z, −π/2 = −x). */
export function hang(c: Ctx, id: ArtId, x: number, y: number, z: number, w: number, h: number, ry: number, frame: string = PAL.walnut): void {
  const n: [number, number] = [Math.sin(ry), Math.cos(ry)];
  c.b.rbox(w + 0.07, h + 0.07, 0.035, x, y, z, frame, { ry }, 0.012);
  c.art.wall(x + n[0] * 0.022, y, z + n[1] * 0.022, w, h, ry, artRect(id));
}

/** Bảng trắng có chân đỡ bút. */
export function whiteboard(c: Ctx, id: ArtId, x: number, y: number, z: number, w: number, h: number, ry: number): void {
  const n: [number, number] = [Math.sin(ry), Math.cos(ry)];
  c.b.rbox(w + 0.06, h + 0.06, 0.04, x, y, z, '#E9E6E0', { ry }, 0.015);
  c.b.rbox(w * 0.8, 0.04, 0.07, x + n[0] * 0.04, y - h / 2 - 0.02, z + n[1] * 0.04, '#D7D3CC', { ry }, 0.012);
  c.art.wall(x + n[0] * 0.022, y, z + n[1] * 0.022, w, h, ry, artRect(id));
}

/** Kệ sách gỗ có sách, cây và đồ trang trí. Quay mặt +z khi ry = 0; `h` cao, `w` rộng, sâu 0,32. */
export function bookshelf(c: Ctx, x: number, z: number, w: number, h: number, ry = 0, seed = 1): void {
  const at = rot(ry, x, z);
  const rnd = seeded(seed * 97 + 13);
  const depth = 0.32;
  const levels = Math.max(3, Math.round(h / 0.42));
  const gap = (h - 0.05) / levels;
  for (const sx of [-1, 1]) {
    const [px, pz] = at((sx * (w - 0.04)) / 2, 0);
    c.b.rbox(0.04, h, depth, px, FY + h / 2, pz, PAL.walnut, { ry }, 0.008);
  }
  const [bx, bz] = at(0, -depth / 2 + 0.01);
  c.b.box(w - 0.04, h, 0.02, bx, FY + h / 2, bz, mix(PAL.walnut, '#000000', 0.2), { ry });
  for (let l = 0; l <= levels; l++) {
    const [px, pz] = at(0, 0);
    c.b.box(w, 0.03, depth, px, FY + 0.015 + l * gap, pz, PAL.walnut, { ry });
  }
  for (let l = 0; l < levels; l++) {
    const y0 = FY + 0.03 + l * gap;
    let dx = -w / 2 + 0.06;
    while (dx < w / 2 - 0.1) {
      const r = rnd();
      if (r < 0.14 && l > 0) {
        const [px, pz] = at(dx + 0.07, 0);
        if (r < 0.07) {
          c.b.cyl(0.05, 0.04, 0.1, px, y0 + 0.05, pz, POTS[Math.floor(rnd() * POTS.length)] ?? PAL.white, {}, 8);
          c.b.sph(0.08, px, y0 + 0.15, pz, LEAVES[Math.floor(rnd() * LEAVES.length)] ?? '#3F8F63', {}, 0.9, 8);
        } else {
          c.b.cyl(0.05, 0.06, 0.16, px, y0 + 0.08, pz, FABRICS[Math.floor(rnd() * FABRICS.length)] ?? PAL.sage, {}, 8);
        }
        dx += 0.2;
        continue;
      }
      const run = 3 + Math.floor(rnd() * 6);
      for (let k = 0; k < run && dx < w / 2 - 0.08; k++) {
        const bw = 0.025 + rnd() * 0.035;
        const bh = Math.min(gap - 0.05, 0.18 + rnd() * 0.12);
        const [px, pz] = at(dx + bw / 2, 0.01);
        c.b.box(bw, bh, depth - 0.1, px, y0 + bh / 2, pz, BOOKS[Math.floor(rnd() * BOOKS.length)] ?? PAL.sage, { ry });
        dx += bw + 0.004;
      }
      dx += 0.04 + rnd() * 0.08;
    }
  }
  blob(c, ...at(0, 0.25), w + 0.3, 0.7, ry);
}

// --- thảm ---

export function rug(c: Ctx, x: number, z: number, w: number, d: number, color: string, border: string, ry = 0): void {
  c.b.rbox(w, 0.012, d, x, FY + 0.006, z, border, { ry }, 0.005);
  c.b.rbox(w - 0.22, 0.012, d - 0.22, x, FY + 0.009, z, color, { ry }, 0.005);
  c.b.rbox(w - 0.5, 0.012, d - 0.5, x, FY + 0.011, z, mix(color, '#FFFFFF', 0.18), { ry }, 0.005);
}

export function roundRug(c: Ctx, x: number, z: number, r: number, color: string, border: string): void {
  c.b.cyl(r, r, 0.012, x, FY + 0.006, z, border, {}, 36);
  c.b.cyl(r - 0.12, r - 0.12, 0.012, x, FY + 0.009, z, color, {}, 36);
  c.b.cyl(r * 0.55, r * 0.55, 0.012, x, FY + 0.011, z, mix(color, '#FFFFFF', 0.16), {}, 36);
}

// --- bếp, buồng điện thoại, bàn đứng ---

/** Quầy bếp/cà phê dọc theo trục x cục bộ (quay `ry`), mặt quay +z: tủ dưới, mặt gỗ, tủ trên, máy pha cà phê, bát trái cây. */
export function kitchen(c: Ctx, x: number, z: number, len: number, ry: number, accent: string): void {
  const at = rot(ry, x, z);
  const hh = 0.9;
  c.b.rbox(len, hh - 0.08, 0.6, x, FY + 0.08 + (hh - 0.08) / 2, z, PAL.white, { ry, ao: 0.25 }, 0.03);
  c.b.box(len - 0.1, 0.08, 0.5, x, FY + 0.04, z, PAL.espresso, { ry });
  c.b.rbox(len + 0.06, 0.045, 0.68, x, FY + hh + 0.02, z, PAL.oak, { ry }, 0.018);
  const n = Math.max(2, Math.round(len / 0.6));
  for (let i = 0; i < n; i++) {
    const [px, pz] = at(-len / 2 + (len / n) * (i + 0.5), 0.305);
    c.b.box(0.012, 0.012, 0.012, px, FY + 0.5, pz, PAL.brass, { ry });
    c.b.rbox(0.18, 0.012, 0.012, px, FY + 0.82, pz + 0.0, PAL.brass, { ry }, 0.004);
  }
  for (let i = 1; i < n; i++) {
    const [px, pz] = at(-len / 2 + (len / n) * i, 0.3);
    c.b.box(0.008, hh - 0.14, 0.01, px, FY + 0.5, pz, mix(PAL.white, '#000000', 0.18), { ry });
  }
  // tủ trên (màu nhấn dịu) + kệ mở
  const [ux, uz] = at(0, -0.12);
  c.b.rbox(len, 0.62, 0.36, ux, FY + 1.75, uz, mix(accent, PAL.cream, 0.55), { ry }, 0.03);
  const [sx2, sz2] = at(len * 0.2, -0.1);
  c.b.rbox(len * 0.3, 0.04, 0.3, sx2, FY + 1.3, sz2, PAL.oak, { ry }, 0.012);
  // máy pha cà phê, ấm, cốc, bát trái cây
  const top = FY + hh + 0.045;
  const [mx, mz] = at(-len / 2 + 0.35, -0.08);
  c.b.rbox(0.3, 0.38, 0.32, mx, top + 0.19, mz, PAL.charcoal, { ry }, 0.03);
  const [gx, gz] = at(-len / 2 + 0.35, 0.09);
  c.b.box(0.1, 0.03, 0.02, gx, top + 0.26, gz, PAL.warmLight, { ry, kind: 'glow' });
  const [kx, kz] = at(-len / 2 + 0.8, 0.0);
  c.b.cyl(0.07, 0.09, 0.16, kx, top + 0.08, kz, PAL.steel, {}, 10);
  for (let i = 0; i < 3; i++) {
    const [px, pz] = at(-len / 2 + 1.05 + i * 0.12, 0.12);
    c.b.cyl(0.04, 0.035, 0.09, px, top + 0.045, pz, [PAL.white, PAL.terracotta, PAL.mustard][i] ?? PAL.white, {}, 8);
  }
  const [fx, fz] = at(len / 2 - 0.4, 0.05);
  c.b.cyl(0.17, 0.1, 0.07, fx, top + 0.035, fz, PAL.walnut, {}, 12);
  c.b.sph(0.055, fx - 0.06, top + 0.1, fz, '#E8735C', {}, 1, 8);
  c.b.sph(0.055, fx + 0.05, top + 0.1, fz + 0.03, PAL.mustard, {}, 1, 8);
  c.b.sph(0.055, fx, top + 0.12, fz - 0.05, '#8DBF6A', {}, 1, 8);
  blob(c, x, z, len + 0.6, 1.2, ry);
}

/** Buồng điện thoại/họp nhanh: tường nỉ màu, khung gỗ, cửa kính phía +z, đèn trần, ghế đẩu. */
export function phoneBooth(c: Ctx, x: number, z: number, ry: number, color: string): void {
  const at = rot(ry, x, z);
  const s = 1.35;
  const h = 2.3;
  c.b.rbox(s, 0.03, s, x, FY + 0.015, z, PAL.oat, { ry }, 0.012);
  const [bx, bz] = at(0, -s / 2 + 0.04);
  c.b.rbox(s, h, 0.07, bx, FY + h / 2, bz, color, { ry }, 0.02);
  for (const sx of [-1, 1]) {
    const [px, pz] = at(sx * (s / 2 - 0.04), 0);
    c.b.rbox(0.07, h, s, px, FY + h / 2, pz, color, { ry }, 0.02);
    const [qx, qz] = at(sx * (s / 2), s / 2 - 0.03);
    c.b.rbox(0.08, h + 0.06, 0.08, qx, FY + h / 2, qz, PAL.walnut, { ry }, 0.02);
  }
  const [gx, gz] = at(0, s / 2 - 0.03);
  c.b.box(s - 0.1, h - 0.3, 0.025, gx, FY + h / 2 + 0.05, gz, '#CFE6EA', { ry, kind: 'glass' });
  c.b.rbox(s + 0.1, 0.08, s + 0.1, x, FY + h + 0.04, z, PAL.walnut, { ry }, 0.03);
  c.b.cyl(0.35, 0.35, 0.015, x, FY + h - 0.02, z, PAL.warmLight, { kind: 'glow' }, 16);
  const [tx, tz] = at(0, -0.1);
  c.b.rbox(0.38, 0.04, 0.3, tx, FY + 0.42, tz, PAL.mustard, { ry }, 0.018);
  c.b.link([tx - 0.1, FY + 0.4, tz], [tx - 0.1, FY, tz], 0.014, PAL.walnut, {}, 5);
  c.b.link([tx + 0.1, FY + 0.4, tz], [tx + 0.1, FY, tz], 0.014, PAL.walnut, {}, 5);
  const [lx, lz] = at(s / 2 - 0.12, -0.3);
  c.b.rbox(0.3, 0.03, 0.2, lx, FY + 0.95, lz, PAL.oak, { ry }, 0.01);
  succulent(c.b, lx, FY + 0.965, lz, PAL.white);
  blob(c, x, z, s + 0.7, s + 0.7, ry);
}

/** Bàn đứng (cao 1,12): mặt gỗ, hai cột, laptop và cốc; không ghế. */
export function standingDesk(c: Ctx, x: number, z: number, ry: number, top: string = PAL.oakLight): void {
  const at = rot(ry, x, z);
  const h = 1.1;
  c.b.rbox(1.3, 0.05, 0.65, x, FY + h, z, top, { ry }, 0.02);
  for (const sx of [-1, 1]) {
    const [px, pz] = at(sx * 0.5, 0);
    c.b.rbox(0.07, h - 0.05, 0.07, px, FY + (h - 0.05) / 2, pz, PAL.charcoal, { ry }, 0.02);
    c.b.rbox(0.07, 0.04, 0.6, px, FY + 0.02, pz, PAL.charcoal, { ry }, 0.015);
  }
  const [lx, lz] = at(-0.1, 0.0);
  c.b.rbox(0.34, 0.016, 0.23, lx, FY + h + 0.033, lz, PAL.steel, { ry }, 0.006);
  const [sx2, sz2] = at(-0.1, -0.12);
  c.b.rbox(0.34, 0.22, 0.012, sx2, FY + h + 0.14, sz2, PAL.steel, { ry, rx: 0.2 }, 0.005);
  const [mx, mz] = at(0.45, 0.1);
  c.b.cyl(0.035, 0.03, 0.08, mx, FY + h + 0.065, mz, PAL.terracotta, {}, 8);
  blob(c, x, z, 1.9, 1.2, ry);
}

/** Ghế da điều hành tựa cao có đường may, tay vịn, chân sao crôm. Quay mặt +z khi ry = 0. */
export function leatherChair(c: Ctx, x: number, z: number, ry = 0, color = '#7A4A2E'): void {
  const at = rot(ry, x, z);
  const dark = mix(color, '#000000', 0.25);
  c.b.rbox(0.62, 0.14, 0.6, x, FY + 0.46, z, color, { ry, ao: 0.2 }, 0.05);
  const [bx, bz] = at(0, -0.3);
  c.b.rbox(0.64, 1.0, 0.14, bx, FY + 1.0, bz, color, { ry, rx: 0.1 }, 0.06);
  for (const dy of [0.82, 1.08, 1.34]) {
    const [gx, gz] = at(0, -0.37);
    c.b.box(0.6, 0.015, 0.02, gx, FY + dy, gz, dark, { ry, rx: 0.1 });
  }
  for (const sx of [-1, 1]) {
    const [ax, az] = at(sx * 0.36, -0.02);
    c.b.rbox(0.08, 0.06, 0.5, ax, FY + 0.7, az, dark, { ry }, 0.02);
    c.b.rbox(0.05, 0.22, 0.05, ax, FY + 0.58, az, '#B8B2A8', { ry }, 0.015);
  }
  c.b.cyl(0.04, 0.04, 0.34, x, FY + 0.24, z, '#B8B2A8', {}, 8);
  for (let i = 0; i < 5; i++) {
    const a = (i / 5) * Math.PI * 2;
    c.b.link([x, FY + 0.08, z], [x + Math.cos(a) * 0.32, FY + 0.045, z + Math.sin(a) * 0.32], 0.02, '#B8B2A8', {}, 5);
    c.b.cyl(0.028, 0.028, 0.04, x + Math.cos(a) * 0.32, FY + 0.025, z + Math.sin(a) * 0.32, INK_CASTER, {}, 5);
  }
  blob(c, x, z, 0.95, 0.95);
}
const INK_CASTER = '#2B2B30';
