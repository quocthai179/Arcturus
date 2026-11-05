// Nội thất từng tầng (docs/OFFICE_BUILDING.md §4.3–4.4): bàn làm việc chi tiết (hai màn hình, bàn phím, chuột, cốc, ghế), thảm,
// góc nghỉ, đạo cụ riêng của tầng, và Phòng CEO (bàn CEO trống, bàn chỉ huy của thư ký, tường trạng thái). Hình tĩnh gộp bằng
// `Batch` (vài draw call mỗi tầng); màn hình bàn là InstancedMesh đổi màu theo người ngồi. Mỗi tầng có `Registry` riêng để rời
// tầng thì `dispose()`.
import { Color, Group, InstancedMesh, Mesh, MeshBasicMaterial, PlaneGeometry, type CanvasTexture } from 'three';
import type { Mood } from '../../protocol';
import { FLOORS, type AgentFloorId, type FloorId } from '../floors';
import {
  CEO_DESK, CEO_LOUNGE, deskAt, deskCount, GEO, LAY, SECRETARY_DESK, SECRETARY_HOME, STATUS_WALL, type Vec3,
} from '../layout';
import { Batch, Registry, type Materials } from './kit';
import { canvasTexture, drawSign, drawStatusScreen, redraw, roundRect, type StatusScreen } from './labels';

export interface Interior {
  floor: FloorId;
  group: Group;
  /** Phần gắn trần: ẩn khi xem mặt cắt (đèn thả, dây đèn sân hiên…). */
  top: Group;
  /** Màn hình bàn (chỉ tầng 0–4). */
  setDeskStates(moods: readonly (Mood | null)[]): void;
  /** Tường trạng thái (chỉ Phòng CEO). */
  setStatus(screens: readonly StatusScreen[]): void;
  statusMeshes: readonly Mesh[];
  dispose(): void;
}

export const SCREEN_COLORS: Record<Mood, string> = { working: '#8B7CF6', approval: '#FFB020', error: '#FF6B5A', done: '#34D1B5', idle: '#8B7CF6' };
const OFF = '#12141A';
const WOOD = '#D8C3A5';
const DARKWOOD = '#4A3426';
const METAL = '#59607A';
const FABRIC = '#6B7BA8';
const CHAIR = '#2F3547';
const GREEN = '#3F8F63';

export function mix(a: string, b: string, t: number): string {
  const c = new Color(a).lerp(new Color(b), t);
  return `#${c.getHexString()}`;
}

function plant(b: Batch, x: number, z: number, s = 1): void {
  b.cyl(0.28 * s, 0.22 * s, 0.4 * s, x, 0.3 + 0.2 * s, z, '#8A92A6');
  b.sph(0.42 * s, x, 0.3 + 0.8 * s, z, GREEN, {}, 1.1);
  b.sph(0.3 * s, x + 0.2 * s, 0.3 + 1.2 * s, z - 0.1 * s, '#4FA874', {}, 1.1);
  b.sph(0.28 * s, x - 0.2 * s, 0.3 + 1.05 * s, z + 0.12 * s, '#2F7A52', {}, 1.1);
}

function sofa(b: Batch, x: number, z: number, w: number, color: string, ry = 0): void {
  // Sofa quay mặt +z (ry = 0); `w` là bề rộng.
  const cosy = Math.cos(ry);
  const siny = Math.sin(ry);
  const at = (dx: number, dz: number): [number, number] => [x + dx * cosy + dz * siny, z - dx * siny + dz * cosy];
  const put = (dx: number, dz: number, bw: number, bh: number, bd: number, y: number, c: string) => {
    const [px, pz] = at(dx, dz);
    b.box(bw, bh, bd, px, y, pz, c, { ry });
  };
  put(0, 0, w, 0.32, 0.9, 0.3 + 0.16, color);
  put(0, -0.38, w, 0.55, 0.16, 0.3 + 0.5, mix(color, '#000000', 0.15));
  put(-w / 2 + 0.1, 0, 0.2, 0.5, 0.9, 0.3 + 0.3, mix(color, '#000000', 0.15));
  put(w / 2 - 0.1, 0, 0.2, 0.5, 0.9, 0.3 + 0.3, mix(color, '#000000', 0.15));
  for (let i = 0; i < Math.max(1, Math.round(w / 0.75)); i++) {
    const cw = w / Math.max(1, Math.round(w / 0.75));
    put(-w / 2 + cw * (i + 0.5), 0.05, cw - 0.06, 0.12, 0.7, 0.3 + 0.38, mix(color, '#FFFFFF', 0.12));
  }
}

function table(b: Batch, x: number, z: number, w: number, d: number, color = WOOD): void {
  b.box(w, 0.05, d, x, 0.3 + 0.42, z, color);
  for (const sx of [-1, 1]) for (const sz of [-1, 1]) b.box(0.05, 0.4, 0.05, x + (sx * (w - 0.1)) / 2, 0.3 + 0.2, z + (sz * (d - 0.1)) / 2, METAL);
}

function lounge(b: Batch, floor: AgentFloorId, accent: string): void {
  const l = LAY[floor];
  if (l === undefined) return;
  const [ax, az] = l.lounge;
  b.box(4.2, 0.02, 3.8, ax + 1, 0.325, az, mix(accent, '#E4E7F0', 0.7));
  sofa(b, ax + 1, az - 1.7, 3.0, FABRIC);
  table(b, ax + 1, az - 0.3, 1.3, 0.7);
  b.cyl(0.07, 0.05, 0.1, ax + 0.7, 0.3 + 0.5, az - 0.3, '#FFFFFF');
  b.box(0.2, 0.02, 0.15, ax + 1.3, 0.3 + 0.46, az - 0.3, '#FF7A59');
  plant(b, ax - 1.2, az - 1.9, 0.9);
  b.box(1.6, 0.02, 0.3, ax + 1, 0.3 + 1.5, az - 2.55, accent, { kind: 'glow' });
}

/** Một bàn làm việc chi tiết (không gồm màn hình sáng, vẽ bằng InstancedMesh). */
function desk(b: Batch, p: Vec3, accent: string): void {
  const { x, z } = p;
  const top = 0.3 + 0.72;
  b.box(1.5, 0.05, 0.7, x, top, z, WOOD);
  for (const s of [-1, 1]) b.box(0.05, 0.72, 0.6, x + s * 0.7, 0.3 + 0.36, z, METAL);
  b.box(1.4, 0.3, 0.02, x, 0.3 + 0.55, z + 0.3, mix(METAL, '#000000', 0.2));
  // màn hình chính và phụ (thân tối; mặt sáng là instance)
  b.box(0.04, 0.12, 0.04, x - 0.1, top + 0.08, z - 0.12, '#1A1D25');
  b.box(0.2, 0.015, 0.14, x - 0.1, top + 0.03, z - 0.12, '#1A1D25');
  b.box(0.64, 0.4, 0.03, x - 0.1, top + 0.34, z - 0.14, '#1A1D25');
  b.box(0.03, 0.1, 0.03, x + 0.5, top + 0.07, z - 0.1, '#1A1D25');
  b.box(0.42, 0.32, 0.03, x + 0.5, top + 0.27, z - 0.12, '#1A1D25', { ry: -0.35 });
  b.box(0.45, 0.02, 0.15, x - 0.05, top + 0.035, z + 0.17, '#C9CEDB');
  b.box(0.06, 0.03, 0.1, x + 0.3, top + 0.04, z + 0.18, '#C9CEDB');
  b.cyl(0.04, 0.035, 0.09, x + 0.66, top + 0.07, z + 0.15, '#FFFFFF');
  b.box(0.2, 0.01, 0.12, x - 0.55, top + 0.03, z + 0.05, accent);
  // ghế: đệm, tựa, trụ, chân sao
  const cz = z + 0.95 - 0.2;
  b.box(0.44, 0.06, 0.44, x, 0.3 + 0.42, cz, CHAIR);
  b.box(0.44, 0.5, 0.06, x, 0.3 + 0.7, cz + 0.22, CHAIR);
  b.cyl(0.03, 0.03, 0.3, x, 0.3 + 0.25, cz, METAL, {}, 6);
  for (let i = 0; i < 5; i++) {
    const a = (i / 5) * Math.PI * 2;
    b.box(0.3, 0.025, 0.04, x + Math.cos(a) * 0.14, 0.3 + 0.07, cz + Math.sin(a) * 0.14, METAL, { ry: -a });
  }
}

function signTexture(reg: Registry, text: string, color: string): CanvasTexture {
  return canvasTexture(reg, 512, 128, (g, w, h) => {
    g.fillStyle = '#12151C';
    g.fillRect(0, 0, w, h);
    g.fillStyle = color;
    g.fillRect(0, 0, 14, h);
    drawSign(g, w, h, text, color);
  });
}

function plane(reg: Registry, parent: Group, t: CanvasTexture, w: number, h: number, x: number, y: number, z: number, ry = 0): Mesh {
  const m = new Mesh(reg.add(new PlaneGeometry(w, h)), reg.add(new MeshBasicMaterial({ map: t, toneMapped: false })));
  m.position.set(x, y, z);
  m.rotation.y = ry;
  parent.add(m);
  return m;
}

// --- hình trên tường dựng bằng canvas ---

function kanban(reg: Registry): CanvasTexture {
  return canvasTexture(reg, 512, 256, (g, w, h) => {
    g.fillStyle = '#F4F5FA';
    g.fillRect(0, 0, w, h);
    const cols = ['Todo', 'Doing', 'Done'];
    const notes = ['#FFD27A', '#8B7CF6', '#34D1B5'];
    cols.forEach((c, i) => {
      const x = 12 + i * 166;
      g.fillStyle = '#E4E7F0';
      g.fillRect(x, 12, 156, h - 24);
      g.fillStyle = '#3A4054';
      g.font = '700 22px sans-serif';
      g.fillText(c, x + 10, 36);
      for (let k = 0; k < 4 - i; k++) {
        g.fillStyle = notes[(i + k) % 3] ?? '#FFD27A';
        roundRect(g, x + 10, 52 + k * 48, 136, 38, 5);
        g.fill();
        g.fillStyle = '#3A4054';
        g.fillRect(x + 20, 64 + k * 48, 80 - k * 8, 5);
        g.fillRect(x + 20, 76 + k * 48, 100 - k * 10, 5);
      }
    });
  });
}

function deviceWall(reg: Registry): CanvasTexture {
  return canvasTexture(reg, 512, 256, (g, w, h) => {
    g.fillStyle = '#1B1F2B';
    g.fillRect(0, 0, w, h);
    for (let r = 0; r < 2; r++) {
      for (let c = 0; c < 6; c++) {
        const tablet = (r + c) % 3 === 0;
        const dw = tablet ? 62 : 42;
        const dh = tablet ? 88 : 80;
        const x = 18 + c * 82 + (tablet ? 0 : 10);
        const y = 24 + r * 118;
        g.fillStyle = '#0B0D12';
        roundRect(g, x, y, dw, dh, 6);
        g.fill();
        g.fillStyle = ['#34D1B5', '#8B7CF6', '#FFB020', '#4C8DF6'][(r * 6 + c) % 4] ?? '#34D1B5';
        g.fillRect(x + 4, y + 8, dw - 8, dh - 16);
      }
    }
  });
}

function bugBoard(reg: Registry): CanvasTexture {
  return canvasTexture(reg, 384, 192, (g, w, h) => {
    g.fillStyle = '#F4F5FA';
    g.fillRect(0, 0, w, h);
    g.fillStyle = '#3A4054';
    g.font = '700 22px sans-serif';
    g.fillText('BUG', 14, 30);
    const colors = ['#FF6B5A', '#FFB020', '#FFD27A', '#34D1B5'];
    for (let i = 0; i < 10; i++) {
      g.fillStyle = colors[i % 4] ?? '#FF6B5A';
      roundRect(g, 14 + (i % 5) * 72, 46 + Math.floor(i / 5) * 70, 62, 56, 6);
      g.fill();
    }
  });
}

function bookshelfTexture(reg: Registry): CanvasTexture {
  return canvasTexture(reg, 256, 512, (g, w, h) => {
    g.fillStyle = '#3A2C22';
    g.fillRect(0, 0, w, h);
    const colors = ['#8B7CF6', '#FFB020', '#34D1B5', '#FF7A59', '#4C8DF6', '#E8EAF0', '#FFD27A'];
    for (let row = 0; row < 6; row++) {
      let x = 8;
      let i = row * 3;
      while (x < w - 14) {
        const bw = 10 + ((i * 7) % 14);
        const bh = 60 + ((i * 13) % 18);
        g.fillStyle = colors[i % colors.length] ?? '#8B7CF6';
        g.fillRect(x, row * 84 + 84 - bh - 4, bw, bh);
        x += bw + 2;
        i++;
      }
      g.fillStyle = '#5A4332';
      g.fillRect(0, row * 84 + 80, w, 6);
    }
  });
}

function chartBoard(reg: Registry): CanvasTexture {
  return canvasTexture(reg, 384, 224, (g, w, h) => {
    g.fillStyle = '#F8F9FC';
    g.fillRect(0, 0, w, h);
    g.strokeStyle = '#3A4054';
    g.lineWidth = 3;
    g.beginPath();
    g.moveTo(30, 20);
    g.lineTo(30, h - 30);
    g.lineTo(w - 20, h - 30);
    g.stroke();
    g.strokeStyle = '#8B7CF6';
    g.lineWidth = 5;
    g.beginPath();
    [[40, 170], [100, 140], [160, 150], [220, 90], [280, 100], [350, 40]].forEach(([x, y], i) => (i === 0 ? g.moveTo(x ?? 0, y ?? 0) : g.lineTo(x ?? 0, y ?? 0)));
    g.stroke();
    g.fillStyle = '#FF7A59';
    for (const [x, y] of [[100, 140], [220, 90], [350, 40]] as const) {
      g.beginPath();
      g.arc(x, y, 7, 0, Math.PI * 2);
      g.fill();
    }
  });
}

function ledBoard(reg: Registry, color: string): CanvasTexture {
  return canvasTexture(reg, 1024, 256, (g, w, h) => {
    g.fillStyle = '#0D1018';
    g.fillRect(0, 0, w, h);
    g.fillStyle = color;
    g.globalAlpha = 0.25;
    for (let x = 0; x < w; x += 8) g.fillRect(x, 0, 1, h);
    g.globalAlpha = 1;
    drawSign(g, w, h, 'ARCTURUS', color);
  });
}

// --- các tầng ---

function common(b: Batch, floor: AgentFloorId, accent: string): void {
  const g = GEO[floor];
  const l = LAY[floor];
  if (g === undefined || l === undefined) return;
  const n = deskCount(floor);
  const rows = Math.ceil(n / l.cols);
  // Thảm dưới khối bàn (màu nhấn 22 %).
  const cx = l.x0 + ((l.cols - 1) * l.dx) / 2;
  const cz = l.z0 + ((rows - 1) * l.dz) / 2 + 0.4;
  b.box(l.cols * l.dx + 0.6, 0.02, rows * l.dz + 0.4, cx, 0.325, cz, mix(accent, '#E4E7F0', 0.78));
  for (let i = 0; i < n; i++) desk(b, deskAt(floor, i).desk, accent);
  lounge(b, floor, accent);
  plant(b, g.w / 2 - 1.0, -g.d / 2 + 1.1, 1.1);
  plant(b, -g.w / 2 + 1.0, g.d / 2 - 1.1, 1.0);
}

function lobby(b: Batch, reg: Registry, root: Group, top: Batch, accent: string): void {
  const g = GEO[0];
  if (g === undefined) return;
  // Quầy lễ tân bình thường (không có Pet).
  b.box(5.2, 1.3, 1.1, 5.5, 0.3 + 0.65, -2.6, '#E6E9F2');
  b.box(5.4, 0.08, 1.3, 5.5, 0.3 + 1.34, -2.6, '#2A2F3C');
  b.box(4.8, 0.06, 0.04, 5.5, 0.3 + 0.2, -2.04, accent, { kind: 'glow' });
  b.box(0.5, 0.35, 0.04, 6.4, 0.3 + 1.55, -2.7, '#1A1D25');
  b.box(0.2, 0.02, 0.15, 4.6, 0.3 + 1.39, -2.45, '#FFFFFF');
  b.cyl(0.3, 0.3, 0.05, 5.5, 0.3 + 0.03, -1.2, '#B7BDCC'); // thảm tròn trước quầy
  // Giá báo, tường LED
  b.box(0.3, 1.4, 1.2, -g.w / 2 + 0.5, 0.3 + 0.7, -3.5, '#8A92A6');
  for (let i = 0; i < 4; i++) b.box(0.06, 0.28, 0.9, -g.w / 2 + 0.68, 0.3 + 0.3 + i * 0.3, -3.5, ['#FFB020', '#34D1B5', '#8B7CF6', '#FF7A59'][i] ?? '#FFB020');
  plane(reg, root, ledBoard(reg, '#FFB020'), 8, 2, 5.5, 3.4, -g.d / 2 + 0.22);
  // Bộ sofa chờ thứ hai (khu khách) và cây lớn
  sofa(b, 3.2, 4.8, 2.6, '#7A86AE');
  table(b, 3.2, 3.7, 1.1, 0.6);
  sofa(b, 8.0, 4.8, 2.6, '#7A86AE');
  plant(b, -g.w / 2 + 1.0, -g.d / 2 + 1.1, 1.6);
  plant(b, 10.6, -g.d / 2 + 1.1, 1.6);
  plant(b, 0.5, g.d / 2 - 1.0, 1.5);
  // Cửa kính chính và cửa thang máy trong sảnh
  b.box(0.08, 3.0, 0.08, -1.7, 0.3 + 1.5, g.d / 2 - 0.05, METAL);
  b.box(0.08, 3.0, 0.08, 1.7, 0.3 + 1.5, g.d / 2 - 0.05, METAL);
  b.box(0.08, 3.0, 0.08, 0, 0.3 + 1.5, g.d / 2 - 0.05, METAL);
  b.box(0.5, 2.6, 1.6, g.w / 2 - 0.5, 0.3 + 1.3, 0, '#C9CEDB');
  b.box(0.04, 0.3, 0.3, g.w / 2 - 0.78, 0.3 + 2.8, 0, '#FFB020', { kind: 'glow' });
  // Đèn thả 3 dải (gắn trần)
  for (const x of [-6, 0, 6]) {
    top.box(0.06, 0.8, 0.06, x, 5.55, -1, METAL);
    top.box(4.2, 0.08, 0.3, x, 5.1, -1, '#FFF8E0', { kind: 'glow' });
  }
}

function dev(b: Batch, reg: Registry, root: Group, accent: string): void {
  plane(reg, root, kanban(reg), 3.6, 1.8, 4, 2.4, -7.3);
  // Bàn họp tròn 6 ghế
  b.cyl(1.2, 1.2, 0.06, 5.5, 0.3 + 0.74, -3.4, '#E6E9F2', {}, 24);
  b.cyl(0.12, 0.12, 0.7, 5.5, 0.3 + 0.37, -3.4, METAL);
  for (let i = 0; i < 6; i++) {
    const a = (i / 6) * Math.PI * 2;
    b.box(0.42, 0.06, 0.42, 5.5 + Math.cos(a) * 1.7, 0.3 + 0.44, -3.4 + Math.sin(a) * 1.7, CHAIR, { ry: -a });
    b.box(0.42, 0.45, 0.05, 5.5 + Math.cos(a) * 1.93, 0.3 + 0.7, -3.4 + Math.sin(a) * 1.93, CHAIR, { ry: -a + Math.PI / 2 });
  }
  b.box(0.6, 0.02, 0.4, 5.5, 0.3 + 0.78, -3.4, accent);
  // Kệ sách (tường trái)
  plane(reg, root, bookshelfTexture(reg), 2.6, 1.9, -10.78, 1.5, 4.5, Math.PI / 2);
  b.box(0.4, 2.2, 3.2, -10.82, 0.3 + 1.1, 4.5, '#3A2C22');
  // Quầy cà phê
  b.box(2.6, 1.0, 0.7, 9.0, 0.3 + 0.5, -6.8, '#E6E9F2');
  b.box(2.7, 0.06, 0.8, 9.0, 0.3 + 1.03, -6.8, DARKWOOD);
  b.box(0.45, 0.5, 0.4, 9.7, 0.3 + 1.3, -6.9, '#1A1D25');
  b.box(0.1, 0.1, 0.1, 9.7, 0.3 + 1.2, -6.7, '#FF6B5A', { kind: 'glow' });
  for (let i = 0; i < 3; i++) b.cyl(0.05, 0.04, 0.1, 8.4 + i * 0.18, 0.3 + 1.11, -6.7, '#FFFFFF');
  // Đèn báo build (xanh) và tủ server có đèn LED
  b.box(0.3, 0.3, 0.1, -0.5, 3.0, -7.35, '#34D1B5', { kind: 'glow' });
  b.box(0.3, 0.3, 0.1, 0.0, 3.0, -7.35, '#FF6B5A', { kind: 'glow' });
  b.box(0.9, 1.9, 0.8, 9.9, 0.3 + 0.95, -3.0, '#1A1D25');
  for (let i = 0; i < 16; i++) b.box(0.05, 0.03, 0.02, 9.6 + (i % 4) * 0.2, 0.3 + 0.35 + Math.floor(i / 4) * 0.38, -2.58, i % 5 === 0 ? '#FFB020' : '#34D1B5', { kind: 'glow' });
}

function test(b: Batch, reg: Registry, root: Group, top: Batch, accent: string): void {
  plane(reg, root, deviceWall(reg), 4.4, 2.2, 6.2, 2.1, -8.3);
  plane(reg, root, bugBoard(reg), 3.2, 1.6, -2.0, 2.1, -8.3);
  // Bàn thử có thảm PASS
  table(b, 7.5, -4.6, 2.2, 1.0, '#E6E9F2');
  b.box(2.6, 0.02, 1.6, 7.5, 0.325, -4.6, '#34D1B5');
  for (const x of [6.9, 7.5, 8.1]) b.box(0.2, 0.02, 0.35, x, 0.3 + 0.75, -4.6, '#1A1D25');
  // Kệ thiết bị
  b.box(0.5, 1.9, 2.4, 11.6, 0.3 + 0.95, -3.0, '#59607A');
  for (let i = 0; i < 4; i++) b.box(0.45, 0.04, 2.3, 11.6, 0.3 + 0.35 + i * 0.45, -3.0, '#C9CEDB');
  // Sân hiên trên mái (trên trần tầng này; phần R&D lùi vào để lộ sân): ván gỗ, chậu, ghế dài, dây đèn 15 bóng
  const g = GEO[2];
  if (g !== undefined) {
    const z0 = 2.4;
    const z1 = g.d / 2 - 0.3;
    top.box(g.w - 0.6, 0.12, z1 - z0, 0, g.H + 0.06, (z0 + z1) / 2, '#B98A5E');
    for (let i = 0; i < 9; i++) top.box(g.w - 0.8, 0.02, 0.05, 0, g.H + 0.13, z0 + 0.4 + i * 0.6, '#9C7048');
    for (const x of [-9, -3, 4, 10]) {
      top.cyl(0.3, 0.24, 0.5, x, g.H + 0.4, z1 - 0.5, '#8A92A6');
      top.sph(0.45, x, g.H + 1.0, z1 - 0.5, GREEN, {}, 1.1);
    }
    top.box(2.2, 0.08, 0.5, -6, g.H + 0.5, z1 - 1.8, DARKWOOD);
    top.box(2.2, 0.5, 0.08, -6, g.H + 0.75, z1 - 2.05, DARKWOOD);
    top.box(2.2, 0.08, 0.5, 6.5, g.H + 0.5, z1 - 1.8, DARKWOOD);
    for (const x of [-10.5, 10.5]) top.box(0.08, 2.4, 0.08, x, g.H + 1.2, z0 + 0.6, METAL);
    for (let i = 0; i < 15; i++) {
      const x = -10.5 + (i / 14) * 21;
      top.sph(0.07, x, g.H + 2.4 - Math.sin((i / 14) * Math.PI) * 0.35, z0 + 0.6, '#FFE29A', { kind: 'glow' }, 1, 6);
    }
  }
  void accent;
}

function rnd(b: Batch, reg: Registry, root: Group, accent: string): void {
  plane(reg, root, bookshelfTexture(reg), 3.2, 3.0, -4.0, 2.0, -7.7);
  plane(reg, root, chartBoard(reg), 3.4, 2.0, 2.5, 2.1, -5.6 - 0.3);
  b.box(3.6, 0.12, 0.1, 2.5, 3.2, -5.9, METAL);
  // Tủ GPU có đèn
  b.box(1.0, 2.0, 0.9, 9.4, 0.3 + 1.0, -4.8, '#1A1D25');
  for (let i = 0; i < 18; i++) b.box(0.05, 0.03, 0.02, 9.0 + (i % 6) * 0.15, 0.3 + 0.4 + Math.floor(i / 6) * 0.55, -4.34, i % 4 === 0 ? '#FFB020' : '#8B7CF6', { kind: 'glow' });
  // Ghế bành đọc sách và đèn cây
  for (const [x, z, ry] of [[-8.6, 3.8, 0.4], [-6.6, 4.0, -0.4]] as const) {
    b.box(0.9, 0.35, 0.8, x, 0.3 + 0.18, z, '#FF7A59', { ry });
    b.box(0.9, 0.55, 0.15, x - Math.sin(ry) * 0.4, 0.3 + 0.55, z - Math.cos(ry) * 0.4, mix('#FF7A59', '#000000', 0.15), { ry });
  }
  b.cyl(0.03, 0.03, 1.6, -7.6, 0.3 + 0.8, 4.4, METAL);
  b.sph(0.25, -7.6, 0.3 + 1.7, 4.4, '#FFE29A', { kind: 'glow' });
  void accent;
}

function media(b: Batch, reg: Registry, root: Group, accent: string): void {
  // Phòng thu kính, micro và biển ON AIR
  b.box(6.4, 0.04, 4.6, 6.4, 0.32, -3.0, '#2A2F3C');
  for (const [x, z, w, d] of [[6.4, -5.3, 6.4, 0.06], [3.2, -3.0, 0.06, 4.6], [9.6, -3.0, 0.06, 4.6]] as const) b.box(w, 2.8, d, x, 0.3 + 1.4, z, '#BFD8FF', { kind: 'glass' });
  b.box(6.4, 2.8, 0.06, 6.4, 0.3 + 1.4, -0.7, '#BFD8FF', { kind: 'glass' });
  for (const [x, z] of [[3.2, -5.3], [9.6, -5.3], [3.2, -0.7], [9.6, -0.7]] as const) b.box(0.1, 2.9, 0.1, x, 0.3 + 1.45, z, METAL);
  b.box(6.5, 0.1, 0.1, 6.4, 0.3 + 2.85, -5.3, METAL);
  b.box(6.5, 0.1, 0.1, 6.4, 0.3 + 2.85, -0.7, METAL);
  b.box(0.1, 0.1, 4.7, 3.2, 0.3 + 2.85, -3.0, METAL);
  b.box(0.1, 0.1, 4.7, 9.6, 0.3 + 2.85, -3.0, METAL);
  // Phông xanh, micro, máy quay, softbox
  b.box(3.6, 2.3, 0.05, 6.4, 0.3 + 1.2, -5.2, '#2FBF6A');
  b.cyl(0.02, 0.02, 1.3, 6.2, 0.3 + 0.65, -3.4, METAL);
  b.sph(0.08, 6.2, 0.3 + 1.35, -3.4, '#1A1D25');
  b.box(0.18, 0.02, 0.18, 6.2, 0.3 + 0.03, -3.4, METAL);
  for (let i = 0; i < 3; i++) b.box(0.03, 1.2, 0.03, 8.0 + Math.cos(i * 2.1) * 0.25, 0.3 + 0.6, -1.6 + Math.sin(i * 2.1) * 0.25, METAL, { rz: Math.cos(i * 2.1) * 0.15 });
  b.box(0.35, 0.22, 0.4, 8.0, 0.3 + 1.35, -1.6, '#1A1D25');
  b.cyl(0.07, 0.1, 0.15, 8.0, 0.3 + 1.35, -1.38, '#59607A', { rx: Math.PI / 2 });
  for (const x of [4.0, 8.8]) {
    b.box(0.04, 1.6, 0.04, x, 0.3 + 0.8, -4.4, METAL);
    b.box(0.7, 0.7, 0.18, x, 0.3 + 1.7, -4.4, '#FFF8E0', { kind: 'glow' });
  }
  plane(reg, root, signTexture(reg, 'ON AIR', '#FF4D4D'), 1.6, 0.4, 6.4, 3.3, -0.62);
  // Bàn dựng 3 màn hình
  table(b, -6, 4.6, 2.6, 0.8, '#E6E9F2');
  for (const dx of [-0.8, 0, 0.8]) b.box(0.7, 0.42, 0.03, -6 + dx, 0.3 + 1.08, 4.45, '#1A1D25', { ry: -dx * 0.25 });
  for (const dx of [-0.8, 0, 0.8]) b.box(0.62, 0.34, 0.01, -6 + dx, 0.3 + 1.08, 4.47, accent, { kind: 'glow', ry: -dx * 0.25 });
  b.box(0.45, 0.4, 0.45, -6, 0.3 + 0.28, 5.4, CHAIR);
}

function ceo(b: Batch, reg: Registry, root: Group, top: Batch): { meshes: Mesh[]; textures: CanvasTexture[] } {
  const g = GEO[5];
  if (g === undefined) throw new Error('GEO[5]');
  const accent = FLOORS[5]?.accent ?? '#FFB020';
  // Thảm lớn
  b.box(11, 0.02, 7, -1.5, 0.325, 0.6, mix(accent, '#E4E7F0', 0.8));
  // Bàn CEO chữ L (luôn trống), ghế cao lưng quay về phía kính, đèn bàn, biển tên
  const c = CEO_DESK.at;
  b.box(CEO_DESK.w, 0.08, CEO_DESK.d, c.x, 0.3 + 0.75, c.z, DARKWOOD);
  b.box(0.9, 0.08, 0.9, c.x - CEO_DESK.w / 2 + 0.45, 0.3 + 0.75, c.z + CEO_DESK.d / 2 + 0.45, DARKWOOD);
  for (const sx of [-1, 1]) b.box(0.08, 0.72, CEO_DESK.d - 0.1, c.x + sx * (CEO_DESK.w / 2 - 0.1), 0.3 + 0.36, c.z, '#2E2018');
  b.box(0.45, 0.02, 0.3, c.x + 0.5, 0.3 + 0.8, c.z + 0.05, '#1A1D25');
  b.cyl(0.02, 0.02, 0.4, c.x - 0.9, 0.3 + 0.97, c.z - 0.2, METAL);
  b.cyl(0.12, 0.06, 0.12, c.x - 0.9, 0.3 + 1.2, c.z - 0.2, '#FFE29A', { kind: 'glow' });
  b.box(0.3, 0.07, 0.03, c.x + 0.2, 0.3 + 0.82, c.z + 0.3, '#FFD166'); // biển tên "CEO"
  const chairZ = c.z - 0.9;
  b.box(0.6, 0.1, 0.6, c.x, 0.3 + 0.5, chairZ, '#1F2330');
  b.box(0.6, 0.95, 0.1, c.x, 0.3 + 1.0, chairZ - 0.3, '#1F2330');
  b.cyl(0.04, 0.04, 0.4, c.x, 0.3 + 0.28, chairZ, METAL);
  for (let i = 0; i < 5; i++) {
    const a = (i / 5) * Math.PI * 2;
    b.box(0.34, 0.03, 0.05, c.x + Math.cos(a) * 0.17, 0.3 + 0.07, chairZ + Math.sin(a) * 0.17, METAL, { ry: -a });
  }
  // Bàn chỉ huy của thư ký: 3 màn hình cong, tai nghe trên giá
  const s = SECRETARY_DESK.at;
  b.box(SECRETARY_DESK.w, 0.06, SECRETARY_DESK.d, s.x, 0.3 + 0.74, s.z, '#E6E9F2');
  for (const sx of [-1, 1]) b.box(0.06, 0.72, SECRETARY_DESK.d - 0.1, s.x + sx * (SECRETARY_DESK.w / 2 - 0.1), 0.3 + 0.36, s.z, METAL);
  for (const [dx, ry] of [[-0.62, 0.45], [0, 0], [0.62, -0.45]] as const) {
    b.box(0.64, 0.4, 0.03, s.x + dx, 0.3 + 1.2, s.z - 0.2 + Math.abs(dx) * 0.18, '#1A1D25', { ry });
    b.box(0.58, 0.34, 0.01, s.x + dx, 0.3 + 1.2, s.z - 0.18 + Math.abs(dx) * 0.18, accent === '' ? '#8B7CF6' : '#8B7CF6', { kind: 'glow', ry });
    b.box(0.04, 0.16, 0.04, s.x + dx, 0.3 + 0.88, s.z - 0.2 + Math.abs(dx) * 0.18, '#1A1D25');
  }
  b.box(0.45, 0.02, 0.15, s.x, 0.3 + 0.78, s.z + 0.18, '#C9CEDB');
  b.box(0.04, 0.3, 0.04, s.x + 0.8, 0.3 + 0.9, s.z, METAL);
  b.box(0.2, 0.025, 0.14, s.x + 0.8, 0.3 + 1.06, s.z, '#2B2F3C', { rz: 0.0 });
  // ghế thư ký
  const sz = SECRETARY_HOME.z + 0.05;
  b.box(0.44, 0.06, 0.44, SECRETARY_HOME.x, 0.3 + 0.42, sz, CHAIR);
  b.box(0.44, 0.5, 0.06, SECRETARY_HOME.x, 0.3 + 0.7, sz + 0.22, CHAIR);
  b.cyl(0.03, 0.03, 0.3, SECRETARY_HOME.x, 0.3 + 0.25, sz, METAL, {}, 6);
  // Kệ giải thưởng
  b.box(0.4, 2.0, 3.0, -g.w / 2 + 0.5, 0.3 + 1.0, -1.2, '#3A2C22');
  for (let i = 0; i < 4; i++) {
    b.box(0.36, 0.03, 2.9, -g.w / 2 + 0.5, 0.3 + 0.35 + i * 0.5, -1.2, '#5A4332');
    for (let k = 0; k < 4; k++) {
      b.cyl(0.05, 0.08, 0.2, -g.w / 2 + 0.5, 0.3 + 0.47 + i * 0.5, -2.3 + k * 0.7, '#FFD166');
      b.sph(0.06, -g.w / 2 + 0.5, 0.3 + 0.62 + i * 0.5, -2.3 + k * 0.7, '#FFD166');
    }
  }
  // Sofa tiếp khách + bàn trà, cây, đèn cây
  sofa(b, CEO_LOUNGE.x, CEO_LOUNGE.z + 1.2, 3.0, '#5B6FA6', Math.PI);
  sofa(b, CEO_LOUNGE.x - 2.0, CEO_LOUNGE.z, 1.6, '#5B6FA6', -Math.PI / 2);
  table(b, CEO_LOUNGE.x, CEO_LOUNGE.z, 1.4, 0.8, '#3A2C22');
  b.cyl(0.18, 0.15, 0.2, CEO_LOUNGE.x, 0.3 + 0.55, CEO_LOUNGE.z, '#FFFFFF');
  plant(b, g.w / 2 - 1.0, -g.d / 2 + 1.0, 1.4);
  plant(b, -g.w / 2 + 1.0, g.d / 2 - 1.0, 1.4);
  b.cyl(0.03, 0.03, 1.7, 6.4, 0.3 + 0.85, 4.0, METAL);
  b.sph(0.25, 6.4, 0.3 + 1.8, 4.0, '#FFE29A', { kind: 'glow' });
  // Đèn thả trên bàn CEO (gắn trần)
  top.box(2.4, 0.08, 0.3, c.x, 4.0, c.z + 0.2, '#FFF8E0', { kind: 'glow' });
  top.box(0.03, 0.4, 0.03, c.x - 1.0, 4.2, c.z + 0.2, METAL);
  top.box(0.03, 0.4, 0.03, c.x + 1.0, 4.2, c.z + 0.2, METAL);

  // Tường trạng thái: 5 màn hình (tầng 0…4)
  const meshes: Mesh[] = [];
  const textures: CanvasTexture[] = [];
  b.box(STATUS_WALL.xs[4]! - STATUS_WALL.xs[0]! + STATUS_WALL.w + 0.4, STATUS_WALL.h + 0.4, 0.06, (STATUS_WALL.xs[0]! + STATUS_WALL.xs[4]!) / 2, STATUS_WALL.y, STATUS_WALL.z - 0.07, '#12151C');
  STATUS_WALL.xs.forEach((x, i) => {
    const t = canvasTexture(reg, 256, 144, (gc, w, h) =>
      drawStatusScreen(gc, w, h, { floor: i, name: FLOORS[i]?.name ?? '', accent: FLOORS[i]?.accent ?? '#9AA3B5', busy: 0, raised: 0, errors: 0, present: 0, alert: 'none', topLine: '' }),
    );
    const m = plane(reg, root, t, STATUS_WALL.w, STATUS_WALL.h, x, STATUS_WALL.y, STATUS_WALL.z + 0.0);
    m.userData['statusFloor'] = i;
    meshes.push(m);
    textures.push(t);
  });
  return { meshes, textures };
}

export function buildInterior(floor: FloorId, shared: Materials): Interior {
  const reg = new Registry();
  const group = new Group();
  const topGroup = new Group();
  const accent = FLOORS[floor]?.accent ?? '#9AA3B5';
  const b = new Batch();
  const top = new Batch();
  let screens: InstancedMesh | null = null;
  let screens2: InstancedMesh | null = null;
  let statusMeshes: Mesh[] = [];
  let statusTextures: CanvasTexture[] = [];
  const sigs: string[] = [];
  const g = GEO[floor];

  if (floor !== 5) {
    const f = floor as AgentFloorId;
    common(b, f, accent);
    switch (f) {
      case 0: lobby(b, reg, group, top, accent); break;
      case 1: dev(b, reg, group, accent); break;
      case 2: test(b, reg, group, top, accent); break;
      case 3: rnd(b, reg, group, accent); break;
      case 4: media(b, reg, group, accent); break;
    }
    // Biển số tầng trên tường sau.
    if (g !== undefined) plane(reg, group, signTexture(reg, `${floor} · ${(FLOORS[floor]?.name ?? '').toUpperCase()}`, accent), 2.4, 0.6, -g.w / 2 + 2.0, 2.9, -g.d / 2 + 0.22);
    // Màn hình bàn: hai InstancedMesh mặt sáng (chính và phụ).
    const n = deskCount(f);
    const mat = reg.add(new MeshBasicMaterial({ color: '#FFFFFF', toneMapped: false }));
    screens = new InstancedMesh(reg.add(new PlaneGeometry(0.58, 0.34)), mat, n);
    screens2 = new InstancedMesh(reg.add(new PlaneGeometry(0.38, 0.28)), mat, n);
    const m = new Mesh();
    for (let i = 0; i < n; i++) {
      const d = deskAt(f, i).desk;
      m.position.set(d.x - 0.1, 0.3 + 0.72 + 0.34, d.z - 0.14 + 0.02);
      m.rotation.set(0, 0, 0);
      m.updateMatrix();
      screens.setMatrixAt(i, m.matrix);
      m.position.set(d.x + 0.5 - Math.sin(-0.35) * -0.0, 0.3 + 0.72 + 0.27, d.z - 0.12 + 0.02);
      m.rotation.set(0, -0.35, 0);
      m.updateMatrix();
      screens2.setMatrixAt(i, m.matrix);
      screens.setColorAt(i, new Color(OFF));
      screens2.setColorAt(i, new Color(OFF));
    }
    screens.frustumCulled = false;
    screens2.frustumCulled = false;
    group.add(screens, screens2);
  } else {
    const r = ceo(b, reg, group, top);
    statusMeshes = r.meshes;
    statusTextures = r.textures;
    if (g !== undefined) plane(reg, group, signTexture(reg, 'PHÒNG CEO', accent), 2.8, 0.7, -g.w / 2 + 2.2, 3.6, -g.d / 2 + 0.22);
  }
  group.add(b.build(reg, shared));
  topGroup.add(top.build(reg, shared, false));

  const tmp = new Color();
  return {
    floor,
    group,
    top: topGroup,
    statusMeshes,
    setDeskStates(moods) {
      if (screens === null || screens2 === null) return;
      moods.forEach((mood, i) => {
        tmp.set(mood === null ? OFF : SCREEN_COLORS[mood]);
        if (mood === 'idle') tmp.multiplyScalar(0.55);
        screens?.setColorAt(i, tmp);
        screens2?.setColorAt(i, mood === null ? tmp : tmp.clone().multiplyScalar(0.8));
      });
      if (screens.instanceColor !== null) screens.instanceColor.needsUpdate = true;
      if (screens2.instanceColor !== null) screens2.instanceColor.needsUpdate = true;
    },
    setStatus(list) {
      list.forEach((s, i) => {
        const sig = JSON.stringify(s);
        const t = statusTextures[i];
        if (t === undefined || sigs[i] === sig) return;
        sigs[i] = sig;
        redraw(t, (gc, w, h) => drawStatusScreen(gc, w, h, s));
      });
    },
    dispose() {
      reg.disposeAll();
    },
  };
}
