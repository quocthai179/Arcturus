// Nội thất từng tầng (docs/OFFICE_BUILDING.md §4.3–4.4, art pass 2026-11-06): bàn làm việc kiểu dáng xen kẽ (hai màn hình, đèn bàn, cây mọng,
// cốc), ghế xoay nhiều màu, thảm, góc nghỉ có sofa vải/bean bag/đèn cây, cây cảnh nhiều loại, tranh và bảng trên tường, bếp/cà phê,
// buồng điện thoại, bàn đứng, đạo cụ riêng của tầng và Phòng CEO (bàn gỗ điều hành, ghế da, thảm, kệ sách, ghế bành, tường trạng thái,
// bàn chỉ huy của thư ký). Hình tĩnh gộp bằng `Batch` (vài draw call mỗi tầng); tranh + bóng đổ giả là hai `QuadSet` (mỗi cái một
// draw call); màn hình bàn là InstancedMesh đổi màu theo người ngồi. Mỗi tầng có `Registry` riêng để rời tầng thì `dispose()`.
import { Color, Group, InstancedMesh, Mesh, MeshBasicMaterial, MeshLambertMaterial, PlaneGeometry, type CanvasTexture } from 'three';
import type { Mood } from '../../protocol';
import { FLOORS, type AgentFloorId, type FloorId } from '../floors';
import {
  CEO_DESK, CEO_LOUNGE, deskAt, deskCount, GEO, LAY, SECRETARY_DESK, SECRETARY_HOME, STATUS_WALL, type Vec3,
} from '../layout';
import { artAtlas, blobTexture } from './decor';
import {
  armchair, beanBag, blob, bookshelf, chair, deskLamp, floorLamp, FY, hang, hangingPlant, kitchen, leatherChair, monstera, pendant, phoneBooth, plantAt, tallPlant,
  rug, roundRug, roundTable, snakePlant, sofa, standingDesk, stool, succulent, table, whiteboard, type Ctx,
} from './furniture';
import { Batch, QuadSet, Registry, type Materials } from './kit';
import { canvasTexture, drawSign, drawStatusScreen, redraw, roundRect, type StatusScreen } from './labels';
import { FABRICS, mix, PAL, pastel } from './palette';

export { mix } from './palette';

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
const INK = PAL.ink;

function signTexture(reg: Registry, text: string, color: string): CanvasTexture {
  return canvasTexture(reg, 512, 128, (g, w, h) => {
    g.fillStyle = '#2A211A';
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
    g.fillStyle = '#FBF7EF';
    g.fillRect(0, 0, w, h);
    const cols = ['Todo', 'Doing', 'Done'];
    const notes = ['#FFD27A', '#8B7CF6', '#34D1B5'];
    cols.forEach((c, i) => {
      const x = 12 + i * 166;
      g.fillStyle = '#F1E9DB';
      g.fillRect(x, 12, 156, h - 24);
      g.fillStyle = '#4A3F36';
      g.font = '700 22px sans-serif';
      g.fillText(c, x + 10, 36);
      for (let k = 0; k < 4 - i; k++) {
        g.fillStyle = notes[(i + k) % 3] ?? '#FFD27A';
        roundRect(g, x + 10, 52 + k * 48, 136, 38, 5);
        g.fill();
        g.fillStyle = '#4A3F36';
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
    g.fillStyle = '#FBF7EF';
    g.fillRect(0, 0, w, h);
    g.fillStyle = '#4A3F36';
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

function chartBoard(reg: Registry): CanvasTexture {
  return canvasTexture(reg, 384, 224, (g, w, h) => {
    g.fillStyle = '#FBF7EF';
    g.fillRect(0, 0, w, h);
    g.strokeStyle = '#4A3F36';
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

// --- bàn làm việc, góc nghỉ ---

const CHAIRS = [PAL.teal, PAL.mustard, PAL.terracotta, PAL.dustyBlue, PAL.sage, PAL.plum, PAL.blush, '#4E5A6B'];
const MUGS = [PAL.white, PAL.terracotta, PAL.mustard, PAL.sage, PAL.dustyBlue];
const TOP = 0.3 + 0.72;

/** Một bàn làm việc: bốn kiểu xen kẽ (chân ghim, giá chữ A, bàn nâng hạ, bàn hộc) + đồ dùng thay đổi theo chỉ số. Mặt sáng màn hình là instance. */
function desk(c: Ctx, p: Vec3, accent: string, i: number): void {
  const b = c.b;
  const { x, z } = p;
  const v = i % 4;
  const topColor = [PAL.oak, PAL.white, PAL.oakLight, PAL.walnut][v] ?? PAL.oak;
  const legColor = [PAL.charcoal, PAL.walnut, PAL.charcoal, PAL.white][v] ?? PAL.charcoal;
  b.rbox(1.5, 0.05, 0.7, x, TOP, z, topColor, {}, 0.02);
  const legH = TOP - 0.025 - FY;
  if (v === 0) {
    for (const sx of [-1, 1]) for (const sz of [-1, 1]) b.cyl(0.022, 0.018, legH, x + sx * 0.68, FY + legH / 2, z + sz * 0.28, legColor, {}, 6);
  } else if (v === 1) {
    for (const sx of [-1, 1]) for (const sz of [-1, 1]) b.link([x + sx * 0.62, FY, z + sz * 0.3], [x + sx * 0.62, TOP - 0.03, z + sz * 0.06], 0.022, legColor, {}, 5);
  } else if (v === 2) {
    for (const sx of [-1, 1]) {
      b.rbox(0.07, legH, 0.07, x + sx * 0.55, FY + legH / 2, z, legColor, {}, 0.02);
      b.rbox(0.07, 0.04, 0.62, x + sx * 0.55, FY + 0.02, z, legColor, {}, 0.015);
    }
    b.rbox(0.12, 0.03, 0.06, x + 0.6, TOP - 0.04, z + 0.3, PAL.charcoal, {}, 0.01);
  } else {
    for (const sx of [-1, 1]) b.rbox(0.04, legH, 0.62, x + sx * 0.7, FY + legH / 2, z, legColor, {}, 0.012);
    b.rbox(1.36, 0.3, 0.02, x, TOP - 0.2, z - 0.3, mix(legColor, '#000000', 0.08), {}, 0.008);
  }
  // màn hình chính và phụ (thân tối; mặt sáng là instance, giữ nguyên vị trí)
  b.box(0.04, 0.12, 0.04, x - 0.1, TOP + 0.08, z - 0.12, INK);
  b.rbox(0.22, 0.015, 0.15, x - 0.1, TOP + 0.03, z - 0.12, INK, {}, 0.006);
  b.rbox(0.66, 0.42, 0.035, x - 0.1, TOP + 0.34, z - 0.14, INK, {}, 0.014);
  b.cyl(0.015, 0.02, 0.1, x + 0.5, TOP + 0.07, z - 0.1, INK, {}, 6);
  b.rbox(0.44, 0.34, 0.03, x + 0.5, TOP + 0.27, z - 0.12, INK, { ry: -0.35 }, 0.012);
  // bàn phím, chuột, thảm bàn
  b.rbox(0.7, 0.006, 0.32, x - 0.02, TOP + 0.028, z + 0.15, pastel(accent, 0.45), {}, 0.003);
  b.rbox(0.45, 0.02, 0.14, x - 0.05, TOP + 0.04, z + 0.17, '#EFE9DD', {}, 0.008);
  b.rbox(0.06, 0.025, 0.1, x + 0.3, TOP + 0.04, z + 0.18, '#EFE9DD', {}, 0.012);
  // đồ dùng thay đổi
  b.cyl(0.04, 0.035, 0.09, x + 0.66, TOP + 0.07, z + 0.15, MUGS[i % MUGS.length] ?? PAL.white, {}, 10);
  if (i % 2 === 0) deskLamp(b, x - 0.66, TOP + 0.025, z - 0.2);
  if (i % 3 === 0) succulent(b, x - 0.64, TOP + 0.025, z + 0.2, MUGS[(i + 2) % MUGS.length]);
  else {
    b.rbox(0.2, 0.02, 0.28, x - 0.58, TOP + 0.035, z + 0.08, FABRICS[i % FABRICS.length] ?? PAL.sage, { ry: 0.25 }, 0.006);
    b.cyl(0.004, 0.004, 0.14, x - 0.5, TOP + 0.055, z + 0.05, PAL.ink, { rz: Math.PI / 2, ry: 0.3 }, 4);
  }
  if (i % 2 === 1) {
    b.box(0.045, 0.045, 0.004, x - 0.4, TOP + 0.5, z - 0.12, '#FFE28A');
    b.box(0.04, 0.04, 0.004, x - 0.37, TOP + 0.46, z - 0.12, '#FFB3A7', { rz: 0.15 });
  }
  chair(c, x, z + 0.75, CHAIRS[i % CHAIRS.length] ?? PAL.teal);
  blob(c, x, z + 0.05, 1.9, 1.1);
}

/** Góc nghỉ quanh bàn trà: thảm, sofa vải, ghế bành, bean bag, đèn cây, cây, đèn thả. Ba chỗ sofa + ba chỗ đứng giữ nguyên. */
function lounge(c: Ctx, floor: AgentFloorId, accent: string): void {
  const l = LAY[floor];
  if (l === undefined) return;
  const [ax, az] = l.lounge;
  const fab = FABRICS[(floor * 3 + 1) % FABRICS.length] ?? PAL.dustyBlue;
  rug(c, ax + 1, az + 0.1, 4.6, 3.8, mix(accent, PAL.oat, 0.45), mix(accent, PAL.cream, 0.45));
  sofa(c, ax + 1, az - 1.7, 3.0, fab, 0, FABRICS[(floor + 4) % FABRICS.length]);
  roundTable(c, ax + 1, az - 0.25, 0.45, 0.42);
  c.b.cyl(0.18, 0.15, 0.07, ax + 1.05, FY + 0.45, az - 0.2, PAL.white, {}, 12);
  c.b.rbox(0.28, 0.04, 0.2, ax + 0.7, FY + 0.46, az - 0.3, '#C9795A', { ry: 0.3 }, 0.01);
  c.b.rbox(0.26, 0.03, 0.18, ax + 0.72, FY + 0.5, az - 0.29, PAL.mustard, { ry: 0.1 }, 0.01);
  armchair(c, ax - 1.5, az - 0.1, FABRICS[(floor + 2) % FABRICS.length] ?? PAL.sage, Math.PI / 2);
  beanBag(c, ax + 3.4, az - 0.3, FABRICS[(floor + 5) % FABRICS.length] ?? PAL.terracotta);
  floorLamp(c, ax + 3.0, az - 2.3);
  tallPlant(c, ax - 1.4, az - 2.2, 0.85, PAL.white);
  pendant(c, ax + 1, az - 0.25, FY + 1.8, PAL.mustard, 0.3);
}

function common(c: Ctx, floor: AgentFloorId, accent: string): void {
  const l = LAY[floor];
  if (l === undefined) return;
  const n = deskCount(floor);
  const rows = Math.ceil(n / l.cols);
  const cx = l.x0 + ((l.cols - 1) * l.dx) / 2;
  const cz = l.z0 + ((rows - 1) * l.dz) / 2 + 0.4;
  rug(c, cx, cz, l.cols * l.dx + 0.6, rows * l.dz + 0.4, mix(accent, PAL.oat, 0.5), mix(accent, PAL.cream, 0.5));
  for (let i = 0; i < n; i++) desk(c, deskAt(floor, i).desk, accent, i + floor);
  lounge(c, floor, accent);
}

// --- các tầng ---

function lobby(c: Ctx, reg: Registry, root: Group, accent: string): void {
  const g = GEO[0];
  if (g === undefined) return;
  const b = c.b;
  // Quầy lễ tân: thân gỗ nan đứng, mặt đá trắng, dải đèn nhấn, hai màn hình, chuông, cây.
  b.rbox(5.2, 1.1, 1.0, 5.5, FY + 0.55, -2.6, PAL.oak, { ao: 0.2 }, 0.05);
  for (let i = 0; i < 25; i++) b.box(0.03, 1.0, 0.02, 3.2 + i * 0.2, FY + 0.55, -2.09, PAL.oakDark);
  b.rbox(5.4, 0.07, 1.25, 5.5, FY + 1.13, -2.6, '#F3EEE6', {}, 0.025);
  b.box(4.8, 0.05, 0.03, 5.5, FY + 0.2, -2.08, accent, { kind: 'glow' });
  b.rbox(0.5, 0.35, 0.03, 6.4, FY + 1.37, -2.75, INK, {}, 0.012);
  b.rbox(0.5, 0.35, 0.03, 5.6, FY + 1.37, -2.75, INK, {}, 0.012);
  b.cyl(0.05, 0.05, 0.03, 4.6, FY + 1.19, -2.4, PAL.brass, {}, 10);
  b.cyl(0.14, 0.12, 0.2, 7.5, FY + 1.27, -2.4, PAL.terracotta, {}, 10);
  b.sph(0.16, 7.5, FY + 1.5, -2.4, '#4C9A6A', {}, 0.9, 8);
  roundRug(c, 5.5, -0.9, 0.9, pastel(accent, 0.6), PAL.oat);
  blob(c, 5.5, -2.5, 5.9, 1.7);
  // Quầy cà phê dọc tường sau và giá báo
  kitchen(c, -2.0, -7.45, 3.2, 0, accent);
  for (const x of [-3.2, -1.6]) stool(c, x, -6.55, FABRICS[Math.abs(Math.round(x * 3)) % FABRICS.length] ?? PAL.sage, 0.62);
  b.rbox(0.3, 1.4, 1.2, -g.w / 2 + 0.55, FY + 0.7, -3.5, PAL.walnut, {}, 0.03);
  for (let i = 0; i < 4; i++) b.rbox(0.06, 0.28, 0.9, -g.w / 2 + 0.7, FY + 0.3 + i * 0.3, -3.5, FABRICS[i + 1] ?? PAL.sage, {}, 0.01);
  plane(reg, root, ledBoard(reg, '#FFB020'), 8, 2, 5.5, 3.4, -g.d / 2 + 0.22);
  // Bộ sofa chờ (khu khách) quanh bàn tròn, cây lớn
  rug(c, 5.6, 4.9, 8.6, 3.2, mix(accent, PAL.oat, 0.55), mix(accent, PAL.cream, 0.5));
  beanBag(c, 10.4, 3.4, PAL.terracotta);
  beanBag(c, 0.9, 3.6, PAL.mustard);
  sofa(c, 3.2, 4.8, 2.6, '#7C9CBF', 0, PAL.mustard);
  sofa(c, 8.0, 4.8, 2.6, PAL.sage, 0, PAL.blush);
  roundTable(c, 5.6, 4.9, 0.4, 0.42);
  monstera(c, -g.w / 2 + 1.0, -g.d / 2 + 1.1, 1.5, PAL.white);
  tallPlant(c, 10.6, -g.d / 2 + 1.1, 1.3, PAL.terracotta);
  snakePlant(c, 0.5, g.d / 2 - 1.0, 1.4, PAL.charcoal);
  plantAt(c, -g.w / 2 + 1.0, g.d / 2 - 1.0, 1, 1.2);
  // Cửa kính chính (khung gỗ) và cửa thang máy trong sảnh
  for (const x of [-1.7, 0, 1.7]) b.rbox(0.08, 3.0, 0.08, x, FY + 1.5, g.d / 2 - 0.05, PAL.walnut, {}, 0.02);
  b.rbox(0.5, 2.6, 1.6, g.w / 2 - 0.5, FY + 1.3, 0, '#E2D6C2', {}, 0.04);
  b.box(0.04, 0.3, 0.3, g.w / 2 - 0.78, FY + 2.8, 0, '#FFB020', { kind: 'glow' });
  // Tranh
  hang(c, 'artSun', -8.6, 2.0, -g.d / 2 + 0.22, 0.9, 0.9, 0);
  hang(c, 'artHills', -7.2, 2.0, -g.d / 2 + 0.22, 0.9, 0.9, 0);
  hang(c, 'artShapes', -5.8, 2.0, -g.d / 2 + 0.22, 0.9, 0.9, 0);
  hang(c, 'posterRocket', -11.78, 2.3, 0.4, 0.9, 0.9, Math.PI / 2);
  hang(c, 'artLeaf', -11.78, 2.3, 1.7, 0.9, 0.9, Math.PI / 2);
  // Đèn thả: ba dải sáng và ba chao lớn trên quầy, hai chậu treo
  for (const x of [-6, 0, 6]) {
    c.top.link([x, 6, -1], [x, 5.1, -1], 0.01, PAL.ink, {}, 4);
    c.top.rbox(4.2, 0.08, 0.3, x, 5.1, -1, PAL.warmLight, { kind: 'glow' }, 0.03);
  }
  for (const x of [4.2, 5.5, 6.8]) pendant(c, x, -2.6, 3.6, PAL.terracotta, 0.3);
  hangingPlant(c, -3, 6.2, 1.4);
  hangingPlant(c, 1.8, 6.4, 1.1);
  void accent;
}

function dev(c: Ctx, reg: Registry, root: Group, accent: string): void {
  const b = c.b;
  const bw = -GEO[1]!.d / 2 + 0.2;
  plane(reg, root, kanban(reg), 3.6, 1.8, 4, 2.4, bw);
  whiteboard(c, 'whiteboardNotes', -3.4, 1.95, bw + 0.02, 2.2, 1.1, 0);
  hang(c, 'artArcs', -6.6, 2.1, bw + 0.02, 0.8, 0.8, 0);
  hang(c, 'artLeaf', -5.5, 1.9, bw + 0.02, 0.55, 0.55, 0);
  // Bàn họp tròn 6 ghế
  roundTable(c, 5.5, -3.4, 1.15, 0.74, PAL.white, PAL.walnut);
  for (let i = 0; i < 6; i++) {
    const a = (i / 6) * Math.PI * 2;
    chair(c, 5.5 + Math.cos(a) * 1.65, -3.4 + Math.sin(a) * 1.65, CHAIRS[(i + 2) % CHAIRS.length] ?? PAL.teal, Math.PI / 2 - a);
  }
  b.rbox(0.6, 0.02, 0.4, 5.5, FY + 0.77, -3.4, pastel(accent, 0.4), {}, 0.006);
  b.rbox(0.3, 0.02, 0.22, 5.1, FY + 0.78, -3.1, PAL.charcoal, { ry: 0.4 }, 0.006);
  succulent(b, 5.9, FY + 0.765, -3.6, PAL.terracotta);
  pendant(c, 5.5, -3.4, FY + 1.9, PAL.white, 0.38);
  // Kệ sách thật (tường trái), bếp/cà phê, buồng điện thoại, bàn đứng
  bookshelf(c, -10.64, 4.5, 3.0, 2.2, Math.PI / 2, 3);
  kitchen(c, 8.8, -6.9, 3.0, 0, accent);
  phoneBooth(c, -10.05, -6.5, 0, pastel(accent, 0.2));
  standingDesk(c, -6.8, -6.75, 0);
  // Đèn báo build (xanh) và tủ server có đèn LED
  b.box(0.3, 0.3, 0.1, -0.5, 3.0, bw + 0.05, '#34D1B5', { kind: 'glow' });
  b.box(0.3, 0.3, 0.1, 0.0, 3.0, bw + 0.05, '#FF6B5A', { kind: 'glow' });
  b.rbox(0.9, 1.9, 0.8, 9.9, FY + 0.95, -3.0, '#2A2630', {}, 0.04);
  for (let i = 0; i < 16; i++) b.box(0.05, 0.03, 0.02, 9.6 + (i % 4) * 0.2, FY + 0.35 + Math.floor(i / 4) * 0.38, -2.58, i % 5 === 0 ? '#FFB020' : '#34D1B5', { kind: 'glow' });
  blob(c, 9.9, -3.0, 1.5, 1.4);
  tallPlant(c, -10.1, 6.6, 1.0, PAL.terracotta);
  monstera(c, 10.2, 6.7, 1.1, PAL.white);
  snakePlant(c, -1.6, -6.9, 1.2, PAL.charcoal);
  monstera(c, 1.9, -6.8, 0.9, PAL.oat);
  hangingPlant(c, -5, 5.5, 1.3);
  hangingPlant(c, 0.5, 6.4, 1.1);
}

function test(c: Ctx, reg: Registry, root: Group, accent: string): void {
  const b = c.b;
  const g = GEO[2];
  if (g === undefined) return;
  const bw = -g.d / 2 + 0.2;
  plane(reg, root, deviceWall(reg), 4.4, 2.2, 6.2, 2.1, bw);
  plane(reg, root, bugBoard(reg), 3.2, 1.6, -2.0, 2.1, bw);
  whiteboard(c, 'whiteboardFlow', 1.7, 2.0, bw + 0.02, 2.2, 1.1, 0);
  hang(c, 'artShapes', -6.4, 2.1, bw + 0.02, 0.8, 0.8, 0);
  hang(c, 'artHello', -5.2, 2.1, bw + 0.02, 0.8, 0.8, 0);
  // Bàn thử có thảm PASS
  rug(c, 7.5, -4.6, 3.2, 2.0, '#8FD3C3', '#E8F1EC');
  table(c, 7.5, -4.6, 2.2, 1.0, 0.75, PAL.white);
  for (const x of [6.9, 7.5, 8.1]) b.rbox(0.2, 0.02, 0.35, x, FY + 0.78, -4.6, INK, {}, 0.01);
  // Kệ thiết bị (gỗ, có thiết bị màu)
  b.rbox(0.5, 1.9, 2.4, 11.55, FY + 0.95, -3.0, PAL.walnut, { ao: 0.15 }, 0.03);
  for (let i = 0; i < 4; i++) {
    b.box(0.45, 0.035, 2.3, 11.5, FY + 0.35 + i * 0.45, -3.0, PAL.oakLight);
    for (let k = 0; k < 5; k++) b.rbox(0.18, 0.2, 0.28, 11.45, FY + 0.47 + i * 0.45, -3.9 + k * 0.45, ['#1E1E24', '#F0ECE4', '#34D1B5', '#FFB020', '#8B7CF6'][(i + k) % 5] ?? INK, {}, 0.02);
  }
  kitchen(c, -11.43, 4.4, 3.4, Math.PI / 2, accent);
  phoneBooth(c, -10.9, -7.4, 0, pastel(accent, 0.25));
  standingDesk(c, -6.4, -7.65, 0);
  tallPlant(c, 11, 7.5, 1.1, PAL.terracotta);
  monstera(c, -11, 7.6, 1.1, PAL.white);
  snakePlant(c, 3.4, -7.8, 1.2, PAL.oat);
  hangingPlant(c, -4, 6.4, 1.2);
  hangingPlant(c, 3.2, 7.2, 1.2);
}

function rnd(c: Ctx, reg: Registry, root: Group, accent: string): void {
  const b = c.b;
  const g = GEO[3];
  if (g === undefined) return;
  const bw = -g.d / 2 + 0.2;
  bookshelf(c, -4.0, bw + 0.18, 3.2, 2.6, 0, 5);
  plane(reg, root, chartBoard(reg), 3.4, 2.0, 2.5, 2.1, bw + 0.02);
  b.rbox(3.6, 0.1, 0.08, 2.5, 3.2, bw + 0.06, PAL.walnut, {}, 0.02);
  whiteboard(c, 'whiteboardFlow', 6.7, 2.0, bw + 0.02, 2.0, 1.0, 0);
  hang(c, 'artHello', -1.5, 2.1, bw + 0.02, 0.7, 0.7, 0);
  hang(c, 'artHills', -0.4, 2.1, bw + 0.02, 0.7, 0.7, 0);
  // Tủ GPU có đèn
  b.rbox(1.0, 2.0, 0.9, 9.4, FY + 1.0, -4.5, '#2A2630', {}, 0.04);
  for (let i = 0; i < 18; i++) b.box(0.05, 0.03, 0.02, 9.0 + (i % 6) * 0.15, FY + 0.4 + Math.floor(i / 6) * 0.55, -4.04, i % 4 === 0 ? '#FFB020' : '#8B7CF6', { kind: 'glow' });
  blob(c, 9.4, -4.5, 1.6, 1.5);
  // Góc đọc sách: hai ghế bành, thảm tròn, bàn phụ, đèn cây
  roundRug(c, -7.6, 3.9, 1.7, pastel('#FFD27A', 0.45), PAL.oat);
  armchair(c, -8.6, 3.8, PAL.terracotta, 0.35);
  armchair(c, -6.6, 4.0, PAL.mustard, -0.35);
  roundTable(c, -7.6, 3.1, 0.28, 0.5);
  floorLamp(c, -9.6, 4.6, 1.7);
  phoneBooth(c, -10.0, -4.75, 0, pastel(accent, 0.3));
  tallPlant(c, 10.2, 4.6, 1.1, PAL.terracotta);
  monstera(c, -10.3, 1.4, 0.9, PAL.white);
  snakePlant(c, 7.8, -4.8, 1.1, PAL.charcoal);
  hangingPlant(c, -2, 4.6, 1.5);
  hangingPlant(c, 9.2, 3.8, 1.2);
}

function media(c: Ctx, reg: Registry, root: Group, accent: string): void {
  const b = c.b;
  const g = GEO[4];
  if (g === undefined) return;
  const bw = -g.d / 2 + 0.2;
  // Phòng thu kính: khung gỗ, sàn tối, micro, phông xanh, softbox, máy quay, biển ON AIR
  rug(c, 6.4, -3.0, 6.2, 4.4, '#4A4350', '#7B5A43');
  for (const [x, z, w, d] of [[6.4, -5.3, 6.4, 0.06], [3.2, -3.0, 0.06, 4.6], [9.6, -3.0, 0.06, 4.6]] as const) b.box(w, 2.8, d, x, FY + 1.4, z, '#D6E7E8', { kind: 'glass' });
  b.box(6.4, 2.8, 0.06, 6.4, FY + 1.4, -0.7, '#D6E7E8', { kind: 'glass' });
  for (const [x, z] of [[3.2, -5.3], [9.6, -5.3], [3.2, -0.7], [9.6, -0.7]] as const) b.rbox(0.1, 2.9, 0.1, x, FY + 1.45, z, PAL.walnut, {}, 0.025);
  b.rbox(6.5, 0.1, 0.1, 6.4, FY + 2.85, -5.3, PAL.walnut, {}, 0.025);
  b.rbox(6.5, 0.1, 0.1, 6.4, FY + 2.85, -0.7, PAL.walnut, {}, 0.025);
  b.rbox(0.1, 0.1, 4.7, 3.2, FY + 2.85, -3.0, PAL.walnut, {}, 0.025);
  b.rbox(0.1, 0.1, 4.7, 9.6, FY + 2.85, -3.0, PAL.walnut, {}, 0.025);
  b.box(3.6, 2.3, 0.05, 6.4, FY + 1.2, -5.2, '#3DBF78');
  b.cyl(0.02, 0.02, 1.3, 6.2, FY + 0.65, -3.4, PAL.charcoal, {}, 6);
  b.sph(0.08, 6.2, FY + 1.35, -3.4, INK, {}, 1, 8);
  b.cyl(0.14, 0.16, 0.02, 6.2, FY + 0.03, -3.4, PAL.charcoal, {}, 10);
  for (let i = 0; i < 3; i++) b.link([8.0, FY + 1.3, -1.6], [8.0 + Math.cos(i * 2.1) * 0.35, FY, -1.6 + Math.sin(i * 2.1) * 0.35], 0.014, PAL.charcoal, {}, 5);
  b.rbox(0.35, 0.22, 0.4, 8.0, FY + 1.35, -1.6, INK, {}, 0.03);
  b.cyl(0.07, 0.1, 0.15, 8.0, FY + 1.35, -1.38, PAL.steel, { rx: Math.PI / 2 }, 10);
  for (const x of [4.0, 8.8]) {
    b.link([x, FY, -4.4], [x, FY + 1.6, -4.4], 0.02, PAL.charcoal, {}, 5);
    b.rbox(0.7, 0.7, 0.18, x, FY + 1.7, -4.4, PAL.warmLight, { kind: 'glow' }, 0.04);
  }
  plane(reg, root, signTexture(reg, 'ON AIR', '#FF4D4D'), 1.6, 0.4, 6.4, 3.3, -0.62);
  // Bàn dựng 3 màn hình
  table(c, -6, 4.6, 2.6, 0.8, 0.75, PAL.white);
  for (const dx of [-0.8, 0, 0.8]) b.rbox(0.7, 0.42, 0.03, -6 + dx, FY + 1.1, 4.45, INK, { ry: -dx * 0.25 }, 0.012);
  for (const dx of [-0.8, 0, 0.8]) b.box(0.62, 0.34, 0.01, -6 + dx, FY + 1.1, 4.47 - Math.abs(dx) * 0.05, accent, { kind: 'glow', ry: -dx * 0.25 });
  chair(c, -6, 5.5, PAL.terracotta, 0);
  // Tranh/bảng tường sau
  hang(c, 'posterRocket', -5.6, 2.0, bw + 0.02, 0.9, 0.9, 0);
  hang(c, 'posterStripes', -4.4, 2.0, bw + 0.02, 0.9, 0.9, 0);
  hang(c, 'artShapes', -3.2, 2.0, bw + 0.02, 0.9, 0.9, 0);
  whiteboard(c, 'cork', 0.1, 1.9, bw + 0.02, 2.2, 1.1, 0);
  phoneBooth(c, -8.9, -4.75, 0, pastel(accent, 0.25));
  standingDesk(c, -1.8, -5.0, 0);
  tallPlant(c, 9.0, 4.8, 1.1, PAL.terracotta);
  monstera(c, -9.2, 1.0, 1.0, PAL.white);
  snakePlant(c, 9.2, 0.6, 1.0, PAL.charcoal);
  hangingPlant(c, 1.0, 4.4, 1.3);
  hangingPlant(c, -3.6, 3.4, 1.0);
}

function ceo(c: Ctx, reg: Registry, root: Group): { meshes: Mesh[]; textures: CanvasTexture[] } {
  const g = GEO[5];
  if (g === undefined) throw new Error('GEO[5]');
  const b = c.b;
  const accent = FLOORS[5]?.accent ?? '#FFB020';
  // Thảm lớn dưới bàn điều hành (đỏ gạch + viền kem)
  rug(c, -1.5, -0.9, 5.6, 3.8, '#8E4F3F', '#E8D9BC');
  // Bàn điều hành gỗ óc chó: mặt dày, hộc kéo bên trái có tay nắm đồng, tấm chắn gối; ghế da quay mặt ra phía +z (luôn trống)
  const d = CEO_DESK.at;
  b.rbox(CEO_DESK.w, 0.08, CEO_DESK.d, d.x, FY + 0.75, d.z, PAL.walnut, {}, 0.03);
  b.rbox(CEO_DESK.w + 0.06, 0.02, CEO_DESK.d + 0.06, d.x, FY + 0.71, d.z, '#5E3F2E', {}, 0.01);
  b.rbox(0.62, 0.68, CEO_DESK.d - 0.08, d.x - 0.95, FY + 0.34, d.z, '#5E3F2E', { ao: 0.2 }, 0.025);
  for (let i = 0; i < 3; i++) {
    b.box(0.5, 0.005, 0.01, d.x - 0.95, FY + 0.2 + i * 0.2, d.z + CEO_DESK.d / 2 - 0.035, '#3A281D');
    b.rbox(0.16, 0.02, 0.02, d.x - 0.95, FY + 0.28 + i * 0.2, d.z + CEO_DESK.d / 2 - 0.03, PAL.brass, {}, 0.008);
  }
  b.rbox(0.07, 0.68, CEO_DESK.d - 0.08, d.x + CEO_DESK.w / 2 - 0.1, FY + 0.34, d.z, '#5E3F2E', {}, 0.02);
  b.rbox(1.5, 0.4, 0.03, d.x + 0.3, FY + 0.5, d.z - CEO_DESK.d / 2 + 0.1, '#4E362A', {}, 0.01);
  blob(c, d.x, d.z, CEO_DESK.w + 0.9, CEO_DESK.d + 1.0);
  deskLamp(b, d.x - 1.05, FY + 0.79, d.z - 0.25, PAL.brass);
  b.rbox(0.42, 0.015, 0.3, d.x + 0.45, FY + 0.8, d.z + 0.02, PAL.steel, {}, 0.006);
  b.rbox(0.3, 0.02, 0.22, d.x - 0.2, FY + 0.8, d.z + 0.1, '#F1EADB', { ry: 0.15 }, 0.004);
  b.rbox(0.3, 0.07, 0.03, d.x + 0.2, FY + 0.83, d.z + 0.35, PAL.brass, {}, 0.01); // biển tên "CEO"
  b.cyl(0.04, 0.035, 0.1, d.x + 0.9, FY + 0.84, d.z - 0.15, PAL.white, {}, 10);
  succulent(b, d.x + 1.05, FY + 0.79, d.z + 0.2, PAL.charcoal);
  leatherChair(c, d.x, d.z - 0.95, 0);
  for (const [x, ry] of [[-2.6, Math.PI - 0.3], [-0.5, Math.PI + 0.3]] as const) armchair(c, x, d.z + 1.35, FABRICS[1] ?? PAL.sage, ry);
  // Tủ thấp phía sau (credenza) có sách, bình hoa, quả địa cầu
  b.rbox(3.2, 0.8, 0.45, d.x, FY + 0.4, -4.45, PAL.walnut, { ao: 0.15 }, 0.03);
  for (let i = 0; i < 4; i++) b.rbox(0.14, 0.02, 0.02, d.x - 1.2 + i * 0.8, FY + 0.55, -4.21, PAL.brass, {}, 0.008);
  b.sph(0.16, d.x - 1.2, FY + 1.05, -4.45, '#4C7BA8', {}, 1, 12);
  b.cyl(0.03, 0.05, 0.1, d.x - 1.2, FY + 0.86, -4.45, PAL.brass, {}, 8);
  b.cyl(0.07, 0.05, 0.25, d.x + 1.1, FY + 0.93, -4.45, PAL.white, {}, 10);
  for (let k = 0; k < 4; k++) b.link([d.x + 1.1, FY + 1.05, -4.45], [d.x + 1.1 + (k - 1.5) * 0.08, FY + 1.5 + (k % 2) * 0.1, -4.45 + (k % 2 ? 0.05 : -0.04)], 0.008, '#3C7A55', {}, 4);
  for (let i = 0; i < 5; i++) b.rbox(0.05, 0.22, 0.16, d.x + 0.3 + i * 0.06, FY + 0.92, -4.45, ['#8E4F3F', '#E3B55F', '#5F6B8A', '#F5EEE3', '#8FB59A'][i] ?? PAL.sage, { rz: i === 4 ? 0.2 : 0 }, 0.006);
  blob(c, d.x, -4.3, 3.8, 1.0);
  // Bàn chỉ huy của thư ký: mặt trắng, chân mảnh, 3 màn hình cong, giá tai nghe, máy tính bảng
  const s = SECRETARY_DESK.at;
  b.rbox(SECRETARY_DESK.w, 0.06, SECRETARY_DESK.d, s.x, FY + 0.74, s.z, '#F3EEE6', {}, 0.025);
  for (const sx of [-1, 1]) b.rbox(0.05, 0.72, SECRETARY_DESK.d - 0.12, s.x + sx * (SECRETARY_DESK.w / 2 - 0.1), FY + 0.36, s.z, PAL.oakLight, {}, 0.015);
  blob(c, s.x, s.z, SECRETARY_DESK.w + 0.8, SECRETARY_DESK.d + 0.9);
  for (const [dx, ry] of [[-0.62, 0.45], [0, 0], [0.62, -0.45]] as const) {
    b.rbox(0.64, 0.4, 0.03, s.x + dx, FY + 1.2, s.z - 0.2 + Math.abs(dx) * 0.18, INK, { ry }, 0.012);
    b.box(0.58, 0.34, 0.01, s.x + dx, FY + 1.2, s.z - 0.18 + Math.abs(dx) * 0.18, '#8B7CF6', { kind: 'glow', ry });
    b.box(0.04, 0.16, 0.04, s.x + dx, FY + 0.88, s.z - 0.2 + Math.abs(dx) * 0.18, INK);
  }
  b.rbox(0.45, 0.02, 0.15, s.x, FY + 0.78, s.z + 0.18, '#EFE9DD', {}, 0.008);
  b.link([s.x + 0.8, FY + 0.77, s.z], [s.x + 0.8, FY + 1.05, s.z], 0.015, PAL.charcoal, {}, 5);
  b.rbox(0.2, 0.025, 0.14, s.x + 0.8, FY + 1.06, s.z, '#2B2F3C', {}, 0.01);
  succulent(b, s.x - 0.8, FY + 0.77, s.z + 0.15, PAL.white);
  chair(c, SECRETARY_HOME.x, SECRETARY_HOME.z + 0.05, '#F3ECE0', 0);
  // Kệ sách (tường trái) với cúp vàng trên đỉnh
  bookshelf(c, -g.w / 2 + 0.36, -1.2, 3.0, 2.2, Math.PI / 2, 9);
  for (let k = 0; k < 4; k++) {
    b.cyl(0.05, 0.08, 0.2, -g.w / 2 + 0.36, FY + 2.32, -2.3 + k * 0.7, PAL.mustard, {}, 8);
    b.sph(0.065, -g.w / 2 + 0.36, FY + 2.5, -2.3 + k * 0.7, PAL.mustard, {}, 1, 8);
  }
  // Khu tiếp khách: sofa da vải 3 chỗ, hai ghế bành, bàn trà tròn, thảm tròn, đèn cây, cây
  roundRug(c, CEO_LOUNGE.x, CEO_LOUNGE.z + 0.2, 2.3, '#D7C3A2', '#E8D9BC');
  sofa(c, CEO_LOUNGE.x, CEO_LOUNGE.z + 1.4, 3.0, '#6C86B5', Math.PI, PAL.mustard);
  armchair(c, CEO_LOUNGE.x - 2.1, CEO_LOUNGE.z - 0.1, PAL.terracotta, Math.PI / 2);
  armchair(c, CEO_LOUNGE.x + 2.1, CEO_LOUNGE.z - 0.1, PAL.terracotta, -Math.PI / 2);
  roundTable(c, CEO_LOUNGE.x, CEO_LOUNGE.z - 0.1, 0.55, 0.42, PAL.walnut, PAL.brass);
  c.b.cyl(0.18, 0.15, 0.2, CEO_LOUNGE.x, FY + 0.55, CEO_LOUNGE.z - 0.1, PAL.white, {}, 12);
  floorLamp(c, -g.w / 2 + 0.9, 4.2, 1.7);
  floorLamp(c, 6.4, 4.0, 1.7);
  tallPlant(c, g.w / 2 - 1.0, -g.d / 2 + 1.0, 1.4, PAL.terracotta);
  monstera(c, g.w / 2 - 1.0, 3.2, 1.3, PAL.white);
  snakePlant(c, -3.0, 4.3, 1.2, PAL.charcoal);
  hangingPlant(c, 3.0, 2.6, 1.5);
  hang(c, 'artHills', 6.2, 2.3, -g.d / 2 + 0.22, 1.1, 0.8, 0, PAL.brass);
  hang(c, 'artSun', 7.2, 1.9, -g.d / 2 + 0.22, 0.55, 0.55, 0, PAL.brass);
  hang(c, 'artArcs', -7.78, 2.4, 3.4, 0.9, 0.9, Math.PI / 2, PAL.brass);
  // Đèn thả đồng trên bàn CEO (gắn trần)
  for (const dx of [-0.8, 0, 0.8]) pendant(c, d.x + dx, d.z, FY + 2.4, PAL.brass, 0.24);

  // Tường trạng thái: 5 màn hình (tầng 0…4)
  const meshes: Mesh[] = [];
  const textures: CanvasTexture[] = [];
  b.rbox(STATUS_WALL.xs[4]! - STATUS_WALL.xs[0]! + STATUS_WALL.w + 0.4, STATUS_WALL.h + 0.4, 0.06, (STATUS_WALL.xs[0]! + STATUS_WALL.xs[4]!) / 2, STATUS_WALL.y, STATUS_WALL.z - 0.07, '#3A2A20', {}, 0.03);
  STATUS_WALL.xs.forEach((x, i) => {
    const t = canvasTexture(reg, 256, 144, (gc, w, h) =>
      drawStatusScreen(gc, w, h, { floor: i, name: FLOORS[i]?.name ?? '', accent: FLOORS[i]?.accent ?? '#9AA3B5', busy: 0, raised: 0, errors: 0, present: 0, alert: 'none', topLine: '' }),
    );
    const m = plane(reg, root, t, STATUS_WALL.w, STATUS_WALL.h, x, STATUS_WALL.y, STATUS_WALL.z + 0.0);
    m.userData['statusFloor'] = i;
    meshes.push(m);
    textures.push(t);
  });
  void accent;
  return { meshes, textures };
}

export function buildInterior(floor: FloorId, shared: Materials): Interior {
  const reg = new Registry();
  const group = new Group();
  const topGroup = new Group();
  const accent = FLOORS[floor]?.accent ?? '#9AA3B5';
  const g = GEO[floor];
  const c: Ctx = { b: new Batch(), top: new Batch(), sh: new QuadSet(), art: new QuadSet(), H: g?.H ?? 3.8 };
  let screens: InstancedMesh | null = null;
  let screens2: InstancedMesh | null = null;
  let statusMeshes: Mesh[] = [];
  let statusTextures: CanvasTexture[] = [];
  const sigs: string[] = [];

  if (floor !== 5) {
    const f = floor as AgentFloorId;
    common(c, f, accent);
    switch (f) {
      case 0: lobby(c, reg, group, accent); break;
      case 1: dev(c, reg, group, accent); break;
      case 2: test(c, reg, group, accent); break;
      case 3: rnd(c, reg, group, accent); break;
      case 4: media(c, reg, group, accent); break;
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
      m.position.set(d.x + 0.5, 0.3 + 0.72 + 0.27, d.z - 0.12 + 0.02);
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
    const r = ceo(c, reg, group);
    statusMeshes = r.meshes;
    statusTextures = r.textures;
    if (g !== undefined) plane(reg, group, signTexture(reg, 'PHÒNG CEO', accent), 2.8, 0.7, -g.w / 2 + 2.2, 3.6, -g.d / 2 + 0.22);
  }
  group.add(c.b.build(reg, shared));
  topGroup.add(c.top.build(reg, shared, false));
  // Bóng đổ giả và tranh/bảng tường (mỗi loại một draw call).
  const shadowMesh = c.sh.build(reg, reg.add(new MeshBasicMaterial({ map: blobTexture(reg), transparent: true, depthWrite: false, toneMapped: false, polygonOffset: true, polygonOffsetFactor: -2 })));
  if (shadowMesh !== null) {
    shadowMesh.renderOrder = 1;
    group.add(shadowMesh);
  }
  const artMesh = c.art.build(reg, reg.add(new MeshLambertMaterial({ map: artAtlas(reg) })));
  if (artMesh !== null) {
    artMesh.receiveShadow = true;
    group.add(artMesh);
  }

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
