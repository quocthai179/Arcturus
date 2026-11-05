// Thế giới tĩnh của cảnh Tòa nhà (docs/OFFICE_BUILDING.md §4.1, §9.2): khu đất, trời, vỏ 6 tầng (sàn, tường, kính, nan nhôm, trần),
// mái + biển ARCTURUS, trục thang máy kính, cầu nối, cabin. Nội thất từng tầng dựng riêng (`interiors.ts`).
import {
  AmbientLight, BoxGeometry, type CanvasTexture, CylinderGeometry, DirectionalLight, Fog, Group, HemisphereLight, InstancedMesh,
  Mesh, MeshBasicMaterial, MeshLambertMaterial, Object3D, PlaneGeometry, Points, PointsMaterial, Float32BufferAttribute, BufferGeometry, Scene, SphereGeometry,
} from 'three';
import { FLOORS, FLOOR_COUNT } from '../floors';
import { BASE, CAR, GEO, ROOF_Y, SHAFT_X } from '../layout';
import type { TimeMode } from '../model';
import { Batch, makeMaterials, Registry, type Materials } from './kit';
import { canvasTexture, drawSign } from './labels';

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
  windows: MeshBasicMaterial;
  stars: Points;
  skyTextures: Record<TimeMode, CanvasTexture>;
  fog: Fog;
  setTime(mode: TimeMode): void;
}

const WALL = '#E9ECF4';
const SLAB = '#CBD0DD';
const GREY = '#8A92A6';
const DARK = '#2A2F3C';

function gradient(reg: Registry, top: string, bottom: string): CanvasTexture {
  return canvasTexture(reg, 2, 256, (g, w, h) => {
    const grad = g.createLinearGradient(0, 0, 0, h);
    grad.addColorStop(0, top);
    grad.addColorStop(1, bottom);
    g.fillStyle = grad;
    g.fillRect(0, 0, w, h);
  });
}

function instanced(reg: Registry, geo: BoxGeometry | CylinderGeometry | SphereGeometry, mat: MeshLambertMaterial | MeshBasicMaterial, items: { x: number; y: number; z: number; sx?: number; sy?: number; sz?: number }[], shadow = false): InstancedMesh {
  const m = new InstancedMesh(reg.add(geo), mat, Math.max(1, items.length));
  const o = new Object3D();
  items.forEach((it, i) => {
    o.position.set(it.x, it.y, it.z);
    o.scale.set(it.sx ?? 1, it.sy ?? 1, it.sz ?? 1);
    o.updateMatrix();
    m.setMatrixAt(i, o.matrix);
  });
  m.count = items.length;
  m.instanceMatrix.needsUpdate = true;
  m.castShadow = shadow;
  m.frustumCulled = false;
  return m;
}

function buildGround(reg: Registry, mats: Materials, scene: Scene, windows: MeshBasicMaterial): void {
  const ground = new Mesh(reg.add(new PlaneGeometry(260, 260)), reg.add(new MeshLambertMaterial({ color: '#2B3140' })));
  ground.rotation.x = -Math.PI / 2;
  ground.position.y = -0.06;
  ground.receiveShadow = true;
  scene.add(ground);

  const b = new Batch();
  b.box(46, 0.1, 16, 0, -0.02, 15, '#4A5266'); // sân trước sảnh
  b.box(260, 0.06, 7, 0, -0.03, 31, '#1D212B'); // đường
  for (let i = -6; i <= 6; i++) b.box(2.4, 0.07, 0.3, i * 9, 0.01, 31, '#CFD3DE');
  b.box(7, 0.12, 22, 38, -0.02, -2, '#3A4254'); // lối bên
  // Khối nhà hàng xóm (thấy qua kính Phòng CEO) và cửa sổ sáng.
  const blocks: [number, number, number, number, number][] = [
    [-46, -26, 16, 26, 14], [-30, -42, 14, 34, 12], [-8, -52, 22, 20, 12], [22, -50, 18, 30, 14],
    [46, -34, 16, 22, 14], [62, -8, 14, 32, 16], [-62, 4, 14, 18, 18], [-52, 24, 12, 14, 10],
  ];
  const wins: { x: number; y: number; z: number; sx: number; sy: number; sz: number }[] = [];
  let seed = 7;
  const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  for (const [x, z, w, h, d] of blocks) {
    b.box(w, h, d, x, h / 2, z, '#4A5370');
    b.box(w + 0.4, 0.4, d + 0.4, x, h + 0.2, z, '#363C4E');
    for (let row = 0; row < Math.floor(h / 3); row++) {
      for (let col = 0; col < Math.floor(w / 3); col++) {
        if (rnd() < 0.55) wins.push({ x: x - w / 2 + 1.5 + col * 3, y: 2 + row * 3, z: z + d / 2 + 0.03, sx: 1.6, sy: 1.3, sz: 1 });
      }
    }
  }
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

  // Cây, đèn đường, ghế sân trước.
  const trunkMat = reg.add(new MeshLambertMaterial({ color: '#6B4E3A' }));
  const leafMat = reg.add(new MeshLambertMaterial({ color: '#3F8F63' }));
  const poleMat = reg.add(new MeshLambertMaterial({ color: '#59607A' }));
  const lampMat = reg.add(new MeshBasicMaterial({ color: '#FFE9A8' }));
  const trees = [-20, -14, 14, 20, 30, -30, 40].map((x, i) => ({ x, z: 17 + (i % 3) * 3 }));
  scene.add(instanced(reg, new CylinderGeometry(0.18, 0.24, 1.6, 8), trunkMat, trees.map((t) => ({ x: t.x, y: 0.8, z: t.z })), true));
  scene.add(instanced(reg, new SphereGeometry(1.4, 10, 8), leafMat, trees.map((t) => ({ x: t.x, y: 2.6, z: t.z })), true));
  const lamps = [-24, -12, 12, 24, 36].map((x) => ({ x, z: 22 }));
  scene.add(instanced(reg, new CylinderGeometry(0.06, 0.08, 4, 6), poleMat, lamps.map((l) => ({ x: l.x, y: 2, z: l.z }))));
  scene.add(instanced(reg, new SphereGeometry(0.22, 8, 6), lampMat, lamps.map((l) => ({ x: l.x, y: 4.1, z: l.z }))));
  scene.add(instanced(reg, new BoxGeometry(1.6, 0.12, 0.5), poleMat, [-6, 6].map((x) => ({ x, y: 0.5, z: 19 }))));
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
  const group = new Group();
  const glass = reg.add(new MeshLambertMaterial({ vertexColors: true, transparent: true, opacity: n === 5 ? 0.14 : 0.3, depthWrite: false }));
  const edge = reg.add(new MeshBasicMaterial({ vertexColors: true }));
  const own: Materials = { lit: mats.lit, glow: mats.glow, glass };

  const shell = new Batch();
  shell.box(w, 0.3, d, 0, 0.15, 0, SLAB);
  shell.box(w - 0.4, 0.04, d - 0.4, 0, 0.3, 0, '#E4E7F0');
  // tường sau và trái đặc, dải đèn nhấn
  shell.box(w, H - 0.3, 0.2, 0, 0.3 + (H - 0.3) / 2, -d / 2 + 0.1, WALL);
  shell.box(0.2, H - 0.3, d, -w / 2 + 0.1, 0.3 + (H - 0.3) / 2, 0, WALL);
  shell.box(w - 0.4, 0.08, 0.04, 0, H - 0.55, -d / 2 + 0.22, accent, { kind: 'glow' });
  shell.box(0.04, 0.08, d - 0.4, -w / 2 + 0.22, H - 0.55, 0, accent, { kind: 'glow' });
  shell.box(w - 0.4, 0.06, 0.04, 0, 0.5, -d / 2 + 0.22, accent, { kind: 'glow' });
  // chân tường kính (mặt trước và bên phải)
  shell.box(w, 0.35, 0.12, 0, 0.3 + 0.175, d / 2 - 0.06, '#B7BDCC');
  shell.box(0.12, 0.35, d, w / 2 - 0.06, 0.3 + 0.175, 0, '#B7BDCC');
  // kính mặt dựng
  shell.box(w, H - 0.9, 0.04, 0, 0.65 + (H - 0.9) / 2, d / 2, '#BFD8FF', { kind: 'glass' });
  shell.box(0.04, H - 0.9, d, w / 2, 0.65 + (H - 0.9) / 2, 0, '#BFD8FF', { kind: 'glass' });
  // cột góc và thanh ngang
  shell.box(0.16, H, 0.16, w / 2, H / 2, d / 2, GREY);
  shell.box(w, 0.1, 0.12, 0, H - 0.05, d / 2, GREY);
  shell.box(0.12, 0.1, d, w / 2, H - 0.05, 0, GREY);
  if (n === 0) {
    // Cửa kính chính và mái đón trước sảnh.
    shell.box(7, 0.22, 3.6, 0, H - 0.9, d / 2 + 1.8, '#F4F5FA');
    shell.box(7, 0.06, 3.6, 0, H - 0.78, d / 2 + 1.8, accent, { kind: 'glow' });
    for (const x of [-3.3, 3.3]) shell.box(0.16, H - 1.0, 0.16, x, (H - 1.0) / 2 + 0.3, d / 2 + 3.4, GREY);
    shell.box(4, 0.04, 0.05, 0, 2.6, d / 2 + 0.02, '#FFFFFF', { kind: 'glow' });
  }
  // Cầu nối sang trục thang máy (z thế giới 0 = cục bộ −cz).
  const bridgeLen = SHAFT_X - 1.4 - (g.cx + w / 2);
  if (bridgeLen > 0) {
    shell.box(bridgeLen + 0.4, 0.3, 1.5, w / 2 + bridgeLen / 2, 0.15, -g.cz, SLAB);
    shell.box(bridgeLen + 0.4, 0.04, 1.2, w / 2 + bridgeLen / 2, 0.32, -g.cz, '#E4E7F0');
    for (const s of [-1, 1]) shell.box(bridgeLen + 0.4, 0.9, 0.04, w / 2 + bridgeLen / 2, 0.8, -g.cz + s * 0.75, '#BFD8FF', { kind: 'glass' });
  }
  const shellGroup = shell.build(reg, own);
  group.add(shellGroup);

  const top = new Batch();
  top.box(w, 0.2, d, 0, H - 0.1, 0, '#F2F3F8');
  const cols = Math.floor(w / 4);
  const rows = Math.floor(d / 4);
  for (let i = 0; i < cols; i++) for (let j = 0; j < rows; j++) top.box(1.6, 0.03, 0.5, -w / 2 + 2 + i * 4, H - 0.22, -d / 2 + 2 + j * 4, '#FFF8E0', { kind: 'glow' });
  for (let x = -w / 2 + 1.5; x < w / 2; x += 1.5) top.box(0.06, H - 0.9, 0.14, x, 0.65 + (H - 0.9) / 2, d / 2 + 0.07, '#C7CCD9');
  for (let z = -d / 2 + 1.5; z < d / 2; z += 1.5) top.box(0.14, H - 0.9, 0.06, w / 2 + 0.07, 0.65 + (H - 0.9) / 2, z, '#C7CCD9');
  const topGroup = top.build(reg, own);
  group.add(topGroup);

  const edgeBatch = new Batch();
  edgeBatch.box(w + 0.1, 0.06, 0.1, 0, 0.27, d / 2 + 0.05, accent, { kind: 'glow' });
  edgeBatch.box(0.1, 0.06, d + 0.1, w / 2 + 0.05, 0.27, 0, accent, { kind: 'glow' });
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
  for (const [x, z] of [[-hw, -hw], [hw, -hw], [-hw, hw], [hw, hw]] as const) b.box(0.14, ROOF_Y + 0.3, 0.14, sx + x, (ROOF_Y + 0.3) / 2, z, GREY);
  for (const y of [0.15, ROOF_Y + 0.2]) b.box(2 * hw + 0.3, 0.3, 2 * hw + 0.3, sx, y, 0, '#D5D9E4');
  for (const s of [-1, 1]) {
    b.box(2 * hw, ROOF_Y, 0.04, sx, ROOF_Y / 2, s * hw, '#BFD8FF', { kind: 'glass' });
    b.box(0.04, ROOF_Y, 2 * hw, sx + s * hw, ROOF_Y / 2, 0, '#BFD8FF', { kind: 'glass' });
  }
  for (let i = 0; i < FLOOR_COUNT; i++) b.box(2 * hw, 0.08, 0.1, sx, (BASE[i] ?? 0) + 3, hw + 0.02, FLOORS[i]?.accent ?? GREY, { kind: 'glow' });
  scene.add(b.build(reg, mats, false));

  const car = new Group();
  const c = new Batch();
  c.box(CAR.w, 0.12, CAR.w, 0, 0.06, 0, '#E4E7F0');
  c.box(CAR.w, 0.12, CAR.w, 0, CAR.h, 0, '#F2F3F8');
  c.box(CAR.w - 0.4, 0.04, CAR.w - 0.4, 0, CAR.h - 0.08, 0, '#FFF8E0', { kind: 'glow' });
  for (const [x, z] of [[-1, -1], [1, -1], [-1, 1], [1, 1]] as const) c.box(0.1, CAR.h, 0.1, (x * CAR.w) / 2, CAR.h / 2, (z * CAR.w) / 2, '#59607A');
  for (const s of [-1, 1]) {
    c.box(CAR.w, CAR.h - 0.3, 0.03, 0, CAR.h / 2, (s * CAR.w) / 2, '#BFD8FF', { kind: 'glass' });
    c.box(0.03, CAR.h - 0.3, CAR.w, (s * CAR.w) / 2, CAR.h / 2, 0, '#BFD8FF', { kind: 'glass' });
  }
  c.box(CAR.w, 0.05, 0.05, 0, 1.0, CAR.w / 2 + 0.02, '#FFB020', { kind: 'glow' });
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
  b.box(g.w + 0.4, 0.3, g.d + 0.4, 0, 0.15, 0, '#D5D9E4');
  for (let i = 0; i < 14; i++) b.box(g.w - 0.6, 0.06, 0.5, 0, 0.32, -g.d / 2 + 0.8 + i * 0.66, i % 2 === 0 ? '#B98A5E' : '#C99A6E');
  for (const [x, z] of [[-6.5, 3.5], [6.5, 3.5], [-6.5, -3.5]] as const) {
    b.box(1.2, 0.5, 0.6, x, 0.6, z, '#8A92A6');
    b.sph(0.45, x, 1.1, z, '#3F8F63', {}, 0.9);
  }
  // Giá đỡ biển: hai trụ.
  for (const x of [-4.2, 4.2]) b.box(0.18, 1.7, 0.18, x, 1.2, -g.d / 2 + 0.7, GREY);
  roof.add(b.build(reg, mats));
  const sign = canvasTexture(reg, 1024, 160, (c, w, h) => drawSign(c, w, h, 'ARCTURUS', '#FFB020', '#12151C'));
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
    day: gradient(reg, '#5E9BE3', '#D6E9FF'),
    dusk: gradient(reg, '#2F2A5A', '#FF9A6B'),
    night: gradient(reg, '#070B18', '#1C2640'),
  };
  const fog = new Fog('#D6E9FF', 140, 320);
  scene.fog = fog;

  const hemi = new HemisphereLight('#FFFFFF', '#6B7288', 1.0);
  scene.add(hemi);
  scene.add(new AmbientLight('#FFFFFF', 0.25));
  const sun = new DirectionalLight('#FFFFFF', 1.5);
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
    scene, mats, floors, roof, car, hemi, sun, windows, stars, skyTextures, fog,
    setTime(mode) {
      scene.background = skyTextures[mode];
      const cfg = {
        day: { sky: '#FFFFFF', ground: '#8791A8', hemi: 1.05, sun: 1.5, sunColor: '#FFF6E6', win: '#5E6A88', fog: '#D6E9FF', stars: false },
        dusk: { sky: '#FFD6B8', ground: '#5A4B73', hemi: 0.8, sun: 1.0, sunColor: '#FFB27A', win: '#FFC870', fog: '#D58A78', stars: false },
        night: { sky: '#8FA3FF', ground: '#2C3350', hemi: 0.75, sun: 0.35, sunColor: '#9DB4FF', win: '#FFE29A', fog: '#0F1528', stars: true },
      }[mode];
      hemi.color.set(cfg.sky);
      hemi.groundColor.set(cfg.ground);
      hemi.intensity = cfg.hemi;
      sun.intensity = cfg.sun;
      sun.color.set(cfg.sunColor);
      windows.color.set(cfg.win);
      fog.color.set(cfg.fog);
      stars.visible = cfg.stars;
    },
  };
  return world;
}
