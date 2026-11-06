// Thế giới tĩnh của cảnh Tòa nhà (docs/OFFICE_BUILDING.md §4.1, §9.2): khu đất, trời, vỏ 6 tầng (sàn, tường, kính, nan nhôm, trần),
// mái + biển ARCTURUS, trục thang máy kính, cầu nối, cabin. Nội thất từng tầng dựng riêng (`interiors.ts`).
import {
  AmbientLight, BoxGeometry, type CanvasTexture, DirectionalLight, Fog, Group, HemisphereLight, InstancedMesh,
  Mesh, MeshBasicMaterial, MeshLambertMaterial, Object3D, PlaneGeometry, PointLight, Points, PointsMaterial, Float32BufferAttribute, BufferGeometry, Scene,
} from 'three';
import { FLOORS, FLOOR_COUNT } from '../floors';
import { BASE, CAR, GEO, ROOF_Y, SHAFT_X } from '../layout';
import type { TimeMode } from '../model';
import { Batch, makeMaterials, plankFloor, Registry, seeded, type Materials } from './kit';
import { canvasTexture, drawSign } from './labels';
import { LEAVES, mix, PAL, pastel } from './palette';

export interface FloorNode {
  /** Gốc tầng: vị trí (cx, BASE + nâng, cz). */
  group: Group;
  shell: Group;
  /** Trần + nan nhôm: ẩn khi xem mặt cắt. */
  top: Group;
  /** Nội thất (gắn vào khi dựng). */
  interior: Group | null;
  glass: MeshLambertMaterial;
  edge: MeshBasicMaterial;
  pick: Mesh;
  accent: string;
}

export interface World {
  scene: Scene;
  mats: Materials;
  floors: FloorNode[];
  roof: Group;
  car: Group;
  hemi: HemisphereLight;
  sun: DirectionalLight;
  /** Đèn ấm trong nhà (hoàng hôn/đêm); `BuildingCanvas` đặt vị trí theo tầng đang xem. */
  warm: PointLight;
  windows: MeshBasicMaterial;
  stars: Points;
  skyTextures: Record<TimeMode, CanvasTexture>;
  fog: Fog;
  /** Độ phơi sáng tone mapping theo giờ (`BuildingCanvas` gán vào renderer mỗi khung). */
  exposure: number;
  setTime(mode: TimeMode): void;
}

const WALL = PAL.plaster;
const SLAB = '#EDE5D8';
const FRAME = '#EFE8DC';
const WOODC = '#CBA474';
const DARK = '#2B2118';
const GLASS = '#D6E7E8';

function gradient(reg: Registry, top: string, bottom: string): CanvasTexture {
  return canvasTexture(reg, 2, 256, (g, w, h) => {
    const grad = g.createLinearGradient(0, 0, 0, h);
    grad.addColorStop(0, top);
    grad.addColorStop(1, bottom);
    g.fillStyle = grad;
    g.fillRect(0, 0, w, h);
  });
}

function buildGround(reg: Registry, mats: Materials, scene: Scene, windows: MeshBasicMaterial): void {
  const ground = new Mesh(reg.add(new PlaneGeometry(260, 260)), reg.add(new MeshLambertMaterial({ color: '#7DA874' })));
  ground.rotation.x = -Math.PI / 2;
  ground.position.y = -0.06;
  ground.receiveShadow = true;
  scene.add(ground);

  const b = new Batch();
  const rnd = seeded(11);
  // Sân trước sảnh lát đá ấm, có vân ô lát và lối đi ra đường.
  b.box(46, 0.1, 16, 0, -0.02, 15, '#D9CCB8');
  for (let i = -11; i <= 11; i++) b.box(0.05, 0.004, 16, i * 2, 0.034, 15, '#CBBCA5');
  for (let j = 0; j < 7; j++) b.box(46, 0.004, 0.05, 0, 0.034, 7.5 + j * 2.2, '#CBBCA5');
  b.box(7, 0.11, 8, 0, -0.02, 27, '#D9CCB8');
  b.box(260, 0.06, 7, 0, -0.03, 31, '#4E525E'); // đường
  b.box(260, 0.1, 2.2, 0, -0.02, 26.6, '#CFC3B0'); // vỉa hè
  for (let i = -6; i <= 6; i++) b.box(2.4, 0.07, 0.3, i * 9, 0.01, 31, '#F2EBDD');
  b.box(7, 0.12, 22, 38, -0.02, -2, '#CFC3B0'); // lối bên
  // Mảng cỏ đậm nhạt khác nhau và luống hoa trước sảnh.
  for (const [x, z, w, d, c] of [[-34, 10, 16, 10, '#7BAA6E'], [36, 12, 12, 8, '#5F9360'], [-12, -14, 20, 12, '#78A96B'], [18, -18, 22, 10, '#62975F']] as const) b.rbox(w, 0.05, d, x, -0.03, z, c, {}, 0.02);
  const palettes = ['#CDBFB0', '#BFB2A7', '#B2BEC7', '#D3BEAA', '#A9B8C4', '#CCB4A6', '#BCC4B7', '#D8CBBB'];
  // Khối nhà hàng xóm (thấy qua kính Phòng CEO) và cửa sổ sáng.
  const blocks: [number, number, number, number, number][] = [
    [-46, -26, 16, 26, 14], [-30, -42, 14, 34, 12], [-8, -52, 22, 20, 12], [22, -50, 18, 30, 14],
    [46, -34, 16, 22, 14], [62, -8, 14, 32, 16], [-62, 4, 14, 18, 18], [-52, 24, 12, 14, 10],
  ];
  const wins: { x: number; y: number; z: number; sx: number; sy: number; sz: number }[] = [];
  let seed = 7;
  const rand = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  blocks.forEach(([x, z, w, h, d], i) => {
    b.box(w, h, d, x, h / 2, z, palettes[i % palettes.length] ?? '#C9B8A6');
    b.box(w + 0.4, 0.4, d + 0.4, x, h + 0.2, z, '#8E7C6E');
    for (let row = 0; row < Math.floor(h / 3); row++) {
      for (let col = 0; col < Math.floor(w / 3); col++) {
        if (rand() < 0.55) wins.push({ x: x - w / 2 + 1.5 + col * 3, y: 2 + row * 3, z: z + d / 2 + 0.03, sx: 1.6, sy: 1.3, sz: 1 });
      }
    }
  });
  scene.add(b.build(reg, mats, false));
  const win = new InstancedMesh(reg.add(new PlaneGeometry(1, 1)), windows, Math.max(1, wins.length));
  const o = new Object3D();
  wins.forEach((w, i) => {
    o.position.set(w.x, w.y, w.z);
    o.scale.set(w.sx, w.sy, 1);
    o.updateMatrix();
    win.setMatrixAt(i, o.matrix);
  });
  win.count = wins.length;
  win.frustumCulled = false;
  scene.add(win);

  // Cây (tròn, tán đôi, thông), bụi, luống hoa, ghế gỗ, đèn đường ấm: một lô gộp có đổ bóng.
  const d = new Batch();
  const broadleaf = (x: number, z: number, s: number, i: number) => {
    d.cyl(0.14 * s, 0.22 * s, 1.7 * s, x, 0.85 * s, z, '#6B4E3A', {}, 7);
    d.sph(1.35 * s, x, 2.7 * s, z, LEAVES[i % LEAVES.length] ?? '#3F8F63', { ao: 0.25 }, 0.95, 9);
    d.sph(0.95 * s, x + 0.7 * s, 3.1 * s, z + 0.3 * s, LEAVES[(i + 2) % LEAVES.length] ?? '#58A878', {}, 0.95, 8);
    d.sph(0.8 * s, x - 0.6 * s, 2.5 * s, z - 0.4 * s, LEAVES[(i + 3) % LEAVES.length] ?? '#2F7A52', {}, 0.95, 8);
  };
  const pine = (x: number, z: number, s: number) => {
    d.cyl(0.12 * s, 0.18 * s, 1.2 * s, x, 0.6 * s, z, '#6B4E3A', {}, 6);
    for (let k = 0; k < 3; k++) d.cyl(0.05, (1.25 - k * 0.3) * s, 1.4 * s, x, (1.6 + k * 0.9) * s, z, k % 2 === 0 ? '#2F7A52' : '#3F8F63', { ao: 0.15 }, 9);
  };
  [[-20, 17], [-14, 20], [14, 20], [20, 17], [30, 20], [-30, 17], [40, 20], [-36, 9], [34, 6], [-24, 10], [26, 10]].forEach(([x, z], i) => broadleaf(x ?? 0, z ?? 0, 0.85 + (i % 3) * 0.18, i));
  [[-40, 14], [46, 14], [-44, -6], [50, 2], [-18, -10], [16, -12]].forEach(([x, z]) => pine(x ?? 0, z ?? 0, 1.0 + rnd() * 0.5));
  const bush = (x: number, z: number, i: number) => d.sph(0.7, x, 0.45, z, LEAVES[i % LEAVES.length] ?? '#3F8F63', { ao: 0.3 }, 0.7, 8);
  [-9, -7, 7, 9].forEach((x, i) => bush(x, 22.5, i));
  for (const sx of [-1, 1]) {
    // Luống hoa trước kính: khung gỗ, đất, cây cỏ và hoa chấm màu.
    d.rbox(4.2, 0.4, 0.9, sx * 9.5, 0.2, 9.6, '#B88F63', {}, 0.05);
    d.box(3.9, 0.06, 0.6, sx * 9.5, 0.4, 9.6, '#4A3B30');
    for (let k = 0; k < 9; k++) {
      const x = sx * 9.5 - 1.8 + k * 0.45;
      d.sph(0.22, x, 0.58, 9.6 + ((k % 2) - 0.5) * 0.25, LEAVES[k % LEAVES.length] ?? '#3F8F63', {}, 0.8, 6);
      d.sph(0.07, x + 0.05, 0.78, 9.6 + ((k % 2) - 0.5) * 0.25, ['#F4A6B5', '#FFD166', '#FFFFFF', '#F28B6B'][k % 4] ?? '#FFFFFF', {}, 1, 5);
    }
  }
  for (const x of [-6, 6, -16, 16]) {
    d.rbox(1.8, 0.08, 0.5, x, 0.5, 19, PAL.oak, {}, 0.025);
    d.rbox(1.8, 0.4, 0.07, x, 0.75, 19.28, PAL.oak, { rx: -0.15 }, 0.025);
    for (const dx of [-0.75, 0.75]) d.box(0.07, 0.5, 0.45, x + dx, 0.25, 19, '#4B4540');
  }
  for (const x of [-24, -12, 12, 24, 36]) {
    d.cyl(0.05, 0.08, 4, x, 2, 22, '#4B4540', {}, 6);
    d.sph(0.26, x, 4.15, 22, '#FFE2A8', { kind: 'glow' }, 1, 8);
  }
  scene.add(d.build(reg, mats, true));
}

function buildStars(reg: Registry): Points {
  const n = 220;
  const pos = new Float32Array(n * 3);
  let seed = 3;
  const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  for (let i = 0; i < n; i++) {
    const a = rnd() * Math.PI * 2;
    const el = 0.2 + rnd() * 1.1;
    pos[i * 3] = Math.cos(a) * Math.cos(el) * 180;
    pos[i * 3 + 1] = Math.sin(el) * 180 + 10;
    pos[i * 3 + 2] = Math.sin(a) * Math.cos(el) * 180;
  }
  const g = reg.add(new BufferGeometry());
  g.setAttribute('position', new Float32BufferAttribute(pos, 3));
  const stars = new Points(g, reg.add(new PointsMaterial({ color: '#DDE6FF', size: 1.4, sizeAttenuation: false })));
  stars.frustumCulled = false;
  return stars;
}

function buildFloorShell(reg: Registry, mats: Materials, n: number): FloorNode {
  const def = FLOORS[n];
  const g = GEO[n];
  if (def === undefined || g === undefined) throw new Error(`floor ${n}`);
  const { w, d, H } = g;
  const accent = def.accent;
  const wall = pastel(accent, 0.62);
  const group = new Group();
  const glass = reg.add(new MeshLambertMaterial({ vertexColors: true, transparent: true, opacity: n === 5 ? 0.14 : 0.3, depthWrite: false, emissive: '#000000' }));
  const edge = reg.add(new MeshBasicMaterial({ vertexColors: true }));
  const own: Materials = { lit: mats.lit, glow: mats.glow, glass };

  const shell = new Batch();
  shell.box(w, 0.3, d, 0, 0.15, 0, SLAB);
  shell.box(w - 0.4, 0.04, d - 0.4, 0, 0.3, 0, '#8A6A4C');
  shell.raw(plankFloor(0, 0, w - 0.4, d - 0.4, 0.323, [PAL.oak, PAL.oakLight, '#CFAE80', '#DCC095', '#C7A577'], 100 + n));
  // ốp gỗ mép sàn (mặt trước và bên phải)
  shell.rbox(w + 0.08, 0.3, 0.1, 0, 0.15, d / 2 + 0.04, WOODC, {}, 0.03);
  shell.rbox(0.1, 0.3, d + 0.08, w / 2 + 0.04, 0.15, 0, WOODC, {}, 0.03);
  // tường sau (màu nhấn dịu của tầng) và tường trái (kem), chân tường trắng, dải đèn ấm
  shell.box(w, H - 0.3, 0.2, 0, 0.3 + (H - 0.3) / 2, -d / 2 + 0.1, wall, { ao: 0.12 });
  shell.box(0.2, H - 0.3, d, -w / 2 + 0.1, 0.3 + (H - 0.3) / 2, 0, WALL, { ao: 0.12 });
  shell.box(w - 0.4, 0.12, 0.03, 0, 0.4, -d / 2 + 0.215, FRAME);
  shell.box(0.03, 0.12, d - 0.4, -w / 2 + 0.215, 0.4, 0, FRAME);
  shell.box(w - 0.4, 0.05, 0.04, 0, H - 0.5, -d / 2 + 0.22, mix(accent, '#FFFFFF', 0.35), { kind: 'glow' });
  shell.box(0.04, 0.05, d - 0.4, -w / 2 + 0.22, H - 0.5, 0, mix(accent, '#FFFFFF', 0.35), { kind: 'glow' });
  // chân tường kính (mặt trước và bên phải)
  shell.box(w, 0.35, 0.12, 0, 0.3 + 0.175, d / 2 - 0.06, FRAME);
  shell.box(0.12, 0.35, d, w / 2 - 0.06, 0.3 + 0.175, 0, FRAME);
  // kính mặt dựng
  shell.box(w, H - 0.9, 0.04, 0, 0.65 + (H - 0.9) / 2, d / 2, GLASS, { kind: 'glass' });
  shell.box(0.04, H - 0.9, d, w / 2, 0.65 + (H - 0.9) / 2, 0, GLASS, { kind: 'glass' });
  // cột góc (gỗ) và thanh ngang mảnh
  shell.rbox(0.16, H, 0.16, w / 2, H / 2, d / 2, WOODC, {}, 0.04);
  shell.box(w, 0.08, 0.1, 0, H - 0.05, d / 2, FRAME);
  shell.box(0.1, 0.08, d, w / 2, H - 0.05, 0, FRAME);
  if (n === 0) {
    // Cửa kính chính và mái đón trước sảnh: ván gỗ, cột gỗ.
    shell.rbox(7, 0.22, 3.6, 0, H - 0.9, d / 2 + 1.8, WOODC, {}, 0.05);
    shell.box(7, 0.04, 3.6, 0, H - 0.77, d / 2 + 1.8, FRAME);
    shell.box(7, 0.04, 0.04, 0, H - 1.05, d / 2 + 3.6, mix(accent, '#FFFFFF', 0.35), { kind: 'glow' });
    for (const x of [-3.3, 3.3]) shell.rbox(0.18, H - 1.0, 0.18, x, (H - 1.0) / 2 + 0.3, d / 2 + 3.4, WOODC, {}, 0.05);
    shell.box(4, 0.04, 0.05, 0, 2.6, d / 2 + 0.02, '#FFFFFF', { kind: 'glow' });
  }
  // Cầu nối sang trục thang máy (z thế giới 0 = cục bộ −cz).
  const bridgeLen = SHAFT_X - 1.4 - (g.cx + w / 2);
  if (bridgeLen > 0) {
    shell.box(bridgeLen + 0.4, 0.3, 1.5, w / 2 + bridgeLen / 2, 0.15, -g.cz, WOODC);
    shell.box(bridgeLen + 0.4, 0.04, 1.2, w / 2 + bridgeLen / 2, 0.32, -g.cz, PAL.oakLight);
    for (const s of [-1, 1]) shell.box(bridgeLen + 0.4, 0.9, 0.04, w / 2 + bridgeLen / 2, 0.8, -g.cz + s * 0.75, GLASS, { kind: 'glass' });
  }
  const shellGroup = shell.build(reg, own);
  group.add(shellGroup);

  const top = new Batch();
  top.box(w, 0.2, d, 0, H - 0.1, 0, '#F3ECE0');
  // vòm gỗ dưới trần ở sát kính (soffit) + dải nan gỗ nhìn từ ngoài
  top.box(w - 0.4, 0.04, 1.3, 0, H - 0.22, d / 2 - 0.75, WOODC);
  for (let i = 0; i < Math.floor(w / 0.8); i++) top.box(0.07, 0.02, 1.3, -w / 2 + 0.4 + i * 0.8, H - 0.25, d / 2 - 0.75, '#A98355');
  if (n === 2) {
    // Sân hiên trên mái tầng 2 (tầng R&D lùi vào): ván gỗ, chậu cây, ghế dài, dây đèn 15 bóng.
    const z0 = 2.4;
    const z1 = d / 2 - 0.3;
    top.rbox(w - 0.6, 0.12, z1 - z0, 0, H + 0.06, (z0 + z1) / 2, '#B98A5E', {}, 0.03);
    for (let i = 0; i < 9; i++) top.box(w - 0.8, 0.02, 0.05, 0, H + 0.13, z0 + 0.4 + i * 0.6, '#9C7048');
    for (const [i, x] of [-9, -3, 4, 10].entries()) {
      top.rbox(0.9, 0.5, 0.6, x, H + 0.4, z1 - 0.5, '#9C7048', {}, 0.05);
      top.sph(0.45, x, H + 0.95, z1 - 0.5, i % 2 === 0 ? '#3F8F63' : '#58A878', {}, 0.95, 8);
      top.sph(0.07, x + 0.2, H + 1.3, z1 - 0.4, ['#F4A6B5', '#FFD166', '#FFFFFF'][i % 3] ?? '#FFFFFF', {}, 1, 5);
    }
    for (const x of [-6, 6.5]) {
      top.rbox(2.2, 0.08, 0.5, x, H + 0.5, z1 - 1.8, PAL.oak, {}, 0.025);
      top.rbox(2.2, 0.5, 0.08, x, H + 0.75, z1 - 2.05, PAL.oak, {}, 0.025);
      for (const dx of [-0.9, 0.9]) top.box(0.07, 0.4, 0.4, x + dx, H + 0.28, z1 - 1.8, '#4B4540');
    }
    for (const x of [-10.5, 10.5]) top.cyl(0.03, 0.04, 2.4, x, H + 1.2, z0 + 0.6, '#4B4540', {}, 6);
    for (let i = 0; i < 15; i++) {
      const x = -10.5 + (i / 14) * 21;
      top.sph(0.07, x, H + 2.4 - Math.sin((i / 14) * Math.PI) * 0.35, z0 + 0.6, '#FFE29A', { kind: 'glow' }, 1, 6);
    }
  }
  const cols = Math.floor(w / 4);
  const rows = Math.floor(d / 4);
  for (let i = 0; i < cols; i++) for (let j = 0; j < rows; j++) top.box(1.6, 0.03, 0.5, -w / 2 + 2 + i * 4, H - 0.22, -d / 2 + 2 + j * 4, PAL.warmLight, { kind: 'glow' });
  for (let x = -w / 2 + 2; x < w / 2; x += 3) top.box(0.07, H - 0.9, 0.14, x, 0.65 + (H - 0.9) / 2, d / 2 + 0.07, '#D9C7A8');
  for (let z = -d / 2 + 2; z < d / 2; z += 3) top.box(0.14, H - 0.9, 0.07, w / 2 + 0.07, 0.65 + (H - 0.9) / 2, z, '#D9C7A8');
  const topGroup = top.build(reg, own);
  group.add(topGroup);

  const edgeBatch = new Batch();
  edgeBatch.box(w + 0.1, 0.05, 0.1, 0, 0.27, d / 2 + 0.09, accent, { kind: 'glow' });
  edgeBatch.box(0.1, 0.05, d + 0.1, w / 2 + 0.09, 0.27, 0, accent, { kind: 'glow' });
  group.add(edgeBatch.build(reg, { lit: mats.lit, glow: edge, glass }, false));

  const pick = new Mesh(reg.add(new BoxGeometry(w, H, d)), reg.add(new MeshBasicMaterial({ visible: false })));
  pick.position.set(0, H / 2, 0);
  pick.userData['floor'] = n;
  group.add(pick);

  group.position.set(g.cx, BASE[n] ?? 0, g.cz);
  return { group, shell: shellGroup, top: topGroup, interior: null, glass, edge, pick, accent };
}

function buildShaftAndCar(reg: Registry, mats: Materials, scene: Scene): Group {
  const b = new Batch();
  const sx = SHAFT_X;
  const hw = 1.4;
  // Trục kính mảnh: bốn cột gỗ mảnh, kính xanh nhạt, mũ gỗ.
  for (const [x, z] of [[-hw, -hw], [hw, -hw], [-hw, hw], [hw, hw]] as const) b.rbox(0.09, ROOF_Y + 0.3, 0.09, sx + x, (ROOF_Y + 0.3) / 2, z, WOODC, {}, 0.025);
  b.rbox(2 * hw + 0.3, 0.3, 2 * hw + 0.3, sx, 0.15, 0, '#E2D6C2', {}, 0.05);
  b.rbox(2 * hw + 0.5, 0.26, 2 * hw + 0.5, sx, ROOF_Y + 0.2, 0, WOODC, {}, 0.06);
  for (const s of [-1, 1]) {
    b.box(2 * hw, ROOF_Y, 0.03, sx, ROOF_Y / 2, s * hw, GLASS, { kind: 'glass' });
    b.box(0.03, ROOF_Y, 2 * hw, sx + s * hw, ROOF_Y / 2, 0, GLASS, { kind: 'glass' });
  }
  for (let i = 0; i < FLOOR_COUNT; i++) {
    b.box(2 * hw, 0.06, 0.06, sx, BASE[i] ?? 0, hw + 0.02, '#E4D6C0');
    b.box(2 * hw, 0.05, 0.07, sx, (BASE[i] ?? 0) + 0.18, hw + 0.03, mix(FLOORS[i]?.accent ?? '#9AA3B5', '#FFFFFF', 0.25), { kind: 'glow' });
  }
  scene.add(b.build(reg, mats, false));

  const car = new Group();
  const c = new Batch();
  c.rbox(CAR.w, 0.12, CAR.w, 0, 0.06, 0, PAL.oakLight, {}, 0.04);
  c.rbox(CAR.w, 0.12, CAR.w, 0, CAR.h, 0, '#F3ECE0', {}, 0.04);
  c.box(CAR.w - 0.4, 0.04, CAR.w - 0.4, 0, CAR.h - 0.08, 0, PAL.warmLight, { kind: 'glow' });
  for (const [x, z] of [[-1, -1], [1, -1], [-1, 1], [1, 1]] as const) c.rbox(0.09, CAR.h, 0.09, (x * CAR.w) / 2, CAR.h / 2, (z * CAR.w) / 2, '#7B5A43', {}, 0.025);
  for (const s of [-1, 1]) {
    c.box(CAR.w, CAR.h - 0.3, 0.03, 0, CAR.h / 2, (s * CAR.w) / 2, GLASS, { kind: 'glass' });
    c.box(0.03, CAR.h - 0.3, CAR.w, (s * CAR.w) / 2, CAR.h / 2, 0, GLASS, { kind: 'glass' });
  }
  c.box(CAR.w, 0.05, 0.05, 0, 1.0, CAR.w / 2 + 0.02, PAL.brass);
  car.add(c.build(reg, mats));
  car.position.set(SHAFT_X, 0.3, 0);
  scene.add(car);
  return car;
}

function buildRoof(reg: Registry, mats: Materials): Group {
  const g = GEO[5];
  if (g === undefined) throw new Error('GEO[5]');
  const roof = new Group();
  const b = new Batch();
  b.rbox(g.w + 0.4, 0.3, g.d + 0.4, 0, 0.15, 0, WOODC, {}, 0.05);
  // sàn gỗ ngoài trời (ván so le) và thảm cỏ
  b.raw(plankFloor(0, 0, g.w - 0.5, g.d - 0.5, 0.31, ['#B98A5E', '#C99A6E', '#C29467', '#AF8257'], 55, 0.3));
  for (const [x, z, w, d] of [[-4.6, 3.4, 4.2, 2.2], [5.2, -1.0, 3.4, 4.4]] as const) b.rbox(w, 0.04, d, x, 0.33, z, '#7FAE6F', {}, 0.02);
  // chậu cây dài ven mép mái: gỗ + bụi nhiều xanh + hoa
  const planter = (x: number, z: number, len: number, ry: number, i: number) => {
    b.rbox(len, 0.5, 0.7, x, 0.55, z, '#9C7048', { ry }, 0.05);
    b.box(len - 0.12, 0.04, 0.58, x, 0.82, z, '#4A3B30', { ry });
    const n = Math.max(2, Math.round(len / 0.55));
    for (let k = 0; k < n; k++) {
      const t = (k + 0.5) / n - 0.5;
      const px = x + Math.cos(ry) * len * t;
      const pz = z - Math.sin(ry) * len * t;
      b.sph(0.32, px, 1.1, pz, LEAVES[(i + k) % LEAVES.length] ?? '#3F8F63', { ao: 0.2 }, 0.9, 8);
      if (k % 2 === 0) b.sph(0.08, px + 0.1, 1.38, pz + 0.05, ['#F4A6B5', '#FFD166', '#FFFFFF'][(k + i) % 3] ?? '#FFFFFF', {}, 1, 5);
    }
  };
  planter(-g.w / 2 + 1.2, g.d / 2 - 0.9, 5.0, 0, 0);
  planter(g.w / 2 - 1.2, g.d / 2 - 0.9, 5.0, 0, 2);
  planter(-g.w / 2 + 0.7, -0.4, 4.2, Math.PI / 2, 1);
  planter(g.w / 2 - 0.7, -0.4, 4.2, Math.PI / 2, 3);
  // hai cây nhỏ trong chậu, ghế lười, ô che nắng
  for (const [x, z] of [[-6.2, 2.4], [6.4, 2.6]] as const) {
    b.cyl(0.32, 0.25, 0.5, x, 0.58, z, PAL.terracotta, {}, 10);
    b.cyl(0.04, 0.05, 1.0, x, 1.1, z, '#6B4E3A', {}, 6);
    for (let k = 0; k < 4; k++) b.sph(0.5 - k * 0.08, x + Math.cos(k * 2.1) * 0.18, 1.6 + k * 0.22, z + Math.sin(k * 2.1) * 0.18, LEAVES[(k + 1) % LEAVES.length] ?? '#3F8F63', {}, 0.8, 8);
  }
  for (const [x, z, c, ry] of [[-2.0, 2.6, PAL.mustard, 0.3], [0.2, 2.8, PAL.terracotta, -0.2], [2.4, 2.6, PAL.teal, 0.1]] as const) {
    b.sph(0.45, x, 0.7, z, c, { sx: 1.1, ry, ao: 0.25 }, 0.7, 10);
    b.sph(0.32, x, 0.92, z + 0.12, mix(c, '#FFFFFF', 0.15), { ry }, 0.5, 8);
  }
  b.cyl(0.04, 0.04, 2.0, 0.3, 1.3, 1.2, '#4B4540', {}, 6);
  b.cyl(0.0, 1.5, 0.35, 0.3, 2.45, 1.2, '#F3ECE0', {}, 16);
  // Giá đỡ biển: hai trụ gỗ.
  for (const x of [-4.2, 4.2]) b.rbox(0.18, 1.7, 0.18, x, 1.2, -g.d / 2 + 0.7, WOODC, {}, 0.04);
  // dây đèn: hai cột và 17 bóng ấm võng xuống
  for (const x of [-g.w / 2 + 0.6, g.w / 2 - 0.6]) b.cyl(0.03, 0.04, 2.6, x, 1.6, g.d / 2 - 1.8, '#4B4540', {}, 6);
  for (let i = 0; i < 17; i++) {
    const t = i / 16;
    const x = -g.w / 2 + 0.6 + t * (g.w - 1.2);
    b.sph(0.07, x, 2.85 - Math.sin(t * Math.PI) * 0.5, g.d / 2 - 1.8, '#FFE2A8', { kind: 'glow' }, 1, 6);
  }
  b.link([-g.w / 2 + 0.6, 2.9, g.d / 2 - 1.8], [g.w / 2 - 0.6, 2.9, g.d / 2 - 1.8], 0.008, '#4B4540', {}, 4);
  roof.add(b.build(reg, mats));
  const sign = canvasTexture(reg, 1024, 160, (c, w, h) => drawSign(c, w, h, 'ARCTURUS', '#FFB020', '#1E1610'));
  const signMesh = new Mesh(reg.add(new PlaneGeometry(9.2, 1.44)), reg.add(new MeshBasicMaterial({ map: sign, toneMapped: false })));
  signMesh.position.set(0, 2.4, -g.d / 2 + 0.8);
  roof.add(signMesh);
  const back = new Mesh(reg.add(new BoxGeometry(9.4, 1.6, 0.12)), reg.add(new MeshLambertMaterial({ color: DARK })));
  back.position.set(0, 2.4, -g.d / 2 + 0.72);
  back.castShadow = true;
  roof.add(back);
  roof.position.set(g.cx, ROOF_Y, g.cz);
  return roof;
}

export function buildWorld(reg: Registry): World {
  const scene = new Scene();
  const mats = makeMaterials(reg);
  const windows = reg.add(new MeshBasicMaterial({ color: '#FFD98A' }));
  const skyTextures = {
    day: gradient(reg, '#7DB6EA', '#F6F0E2'),
    dusk: gradient(reg, '#4E4585', '#FFB88C'),
    night: gradient(reg, '#0A0F22', '#2B3158'),
  };
  const fog = new Fog('#F0EBDD', 140, 320);
  scene.fog = fog;

  const hemi = new HemisphereLight('#FFF4E2', '#B7A58E', 1.2);
  scene.add(hemi);
  scene.add(new AmbientLight('#FFF1DC', 0.2));
  const warm = new PointLight('#FFC27A', 0, 0, 2);
  scene.add(warm);
  const sun = new DirectionalLight('#FFE8C8', 2.2);
  sun.position.set(30, 60, 40);
  sun.castShadow = true;
  sun.shadow.mapSize.set(1024, 1024);
  sun.shadow.camera.left = -34;
  sun.shadow.camera.right = 34;
  sun.shadow.camera.top = 34;
  sun.shadow.camera.bottom = -34;
  sun.shadow.camera.near = 10;
  sun.shadow.camera.far = 160;
  sun.shadow.bias = -0.0008;
  sun.shadow.normalBias = 0.03;
  sun.target.position.set(0, 12, 0);
  scene.add(sun);
  scene.add(sun.target);

  buildGround(reg, mats, scene, windows);
  const stars = buildStars(reg);
  scene.add(stars);
  const floors = Array.from({ length: FLOOR_COUNT }, (_, n) => buildFloorShell(reg, mats, n));
  for (const f of floors) scene.add(f.group);
  const roof = buildRoof(reg, mats);
  scene.add(roof);
  const car = buildShaftAndCar(reg, mats, scene);

  const world: World = {
    scene, mats, floors, roof, car, hemi, sun, warm, windows, stars, skyTextures, fog, exposure: 1,
    setTime(mode) {
      scene.background = skyTextures[mode];
      const cfg = {
        day: { sky: '#FFF6E8', ground: '#B7A58E', hemi: 1.05, sun: 2.0, sunColor: '#FFEBD0', win: '#7C8DA5', fog: '#EDE8DA', stars: false, exposure: 0.92, warm: 0, glow: '#000000' },
        dusk: { sky: '#FFE4D2', ground: '#8A7384', hemi: 1.0, sun: 1.45, sunColor: '#FFC49C', win: '#FFC870', fog: '#D8B09E', stars: false, exposure: 1.0, warm: 60, glow: '#3A2410' },
        night: { sky: '#A9B6FF', ground: '#4A4268', hemi: 0.95, sun: 0.5, sunColor: '#A9BBFF', win: '#FFE29A', fog: '#161C36', stars: true, exposure: 1.2, warm: 110, glow: '#5A3A18' },
      }[mode];
      hemi.color.set(cfg.sky);
      hemi.groundColor.set(cfg.ground);
      hemi.intensity = cfg.hemi;
      sun.intensity = cfg.sun;
      sun.color.set(cfg.sunColor);
      windows.color.set(cfg.win);
      fog.color.set(cfg.fog);
      stars.visible = cfg.stars;
      warm.intensity = cfg.warm;
      world.exposure = cfg.exposure;
      // Kính phát ấm nhẹ khi trời tối (ánh đèn trong nhà lộ qua kính).
      for (const f of floors) f.glass.emissive.set(cfg.glow);
      if (mats.glass instanceof MeshLambertMaterial) mats.glass.emissive.set(cfg.glow);
    },
  };
  return world;
}
