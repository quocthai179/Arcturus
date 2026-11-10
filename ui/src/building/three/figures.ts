// Nhân vật trong cảnh Tòa nhà (docs/OFFICE_BUILDING.md §6.3): mỗi phần một InstancedMesh dùng chung geometry (chỉ các phần đang dùng mới tốn draw call cho
// tầng đang xem) + hình rút gọn (thân, đầu, tóc) cho các tầng khác. Tư thế và FK tính ở `../figure.ts` (thuần).
import {
  Color, CylinderGeometry, Euler, Group, InstancedMesh, Matrix4, Mesh, MeshBasicMaterial, MeshLambertMaterial, PlaneGeometry, Quaternion, Sprite,
  Vector3, type CanvasTexture,
} from 'three';
import { cleanText } from '../../pet/brain';
import type { AgentId, Mood } from '../../protocol';
import {
  lookFor, partColors, partTransforms, PART_IDS, PART_SPECS, POSES, safePalette, SECRETARY_LOOK, SECRETARY_PALETTE, shapeKey, SIT_FORWARD, isSeated, walkAngles,
  type ColorKey, type FigureExtras, type Look, type PartId, type PartTransform, type PoseId,
} from '../figure';
import { FLOORS, roleShort, type FloorId } from '../floors';
import { BASE, deskAt, FLOOR_SLAB, geo, LOUNGE_SOFA_SPOTS, LOUNGE_SPOTS, STANDING_SPOTS, type Vec3 } from '../layout';
import type { FloorRoster, Figure } from '../seating';
import { blobTexture } from './decor';
import { CASTS_SHADOW, partGeometry } from './figureGeo';
import { Registry } from './kit';
import { canvasTexture, drawBang, drawDeskLabel, drawDots, fontsReady, redraw, spriteMaterial } from './labels';

/** Số chỗ tối đa trong một InstancedMesh (24 agent của tầng đang xem + thư ký). */
export const MAX_INSTANCES = 25;

export interface SecretaryDraw {
  floor: FloorId | 'car';
  pos: Vec3;
  pose: PoseId;
  facing: number;
  phase: number;
  speaking: boolean;
}

export interface FigureFrame {
  /** Tầng đang xem chi tiết (`null` = ngoại cảnh hoặc Phòng CEO không có agent). */
  detailFloor: FloorId | null;
  rosters: readonly FloorRoster[];
  lifts: readonly number[];
  selectable: ReadonlySet<AgentId>;
  secretary: SecretaryDraw;
}

export interface Anchors {
  agents: Map<AgentId, Vector3>;
  secretary: Vector3 | null;
  bangs: Sprite[];
}

const colorCache = new Map<string, Color>();
/** Màu dùng lại theo mã hex (palette do agent gửi nên có giới hạn kích thước bộ nhớ đệm). */
function colorOf(hex: string): Color {
  let c = colorCache.get(hex);
  if (c === undefined) {
    if (colorCache.size > 256) colorCache.clear();
    c = new Color(hex);
    colorCache.set(hex, c);
  }
  return c;
}

const q = new Quaternion();
const e = new Euler();
const m4 = new Matrix4();
const base = new Matrix4();
const pt = new Matrix4();
const v3 = new Vector3();
const one = new Vector3(1, 1, 1);
const sc = new Vector3();

function partMatrix(out: Matrix4, world: Matrix4, t: PartTransform): Matrix4 {
  e.set(t.rot.x, t.rot.y, t.rot.z, 'XYZ');
  q.setFromEuler(e);
  pt.compose(v3.set(t.pos.x, t.pos.y, t.pos.z), q, sc.set(t.scale.x, t.scale.y, t.scale.z));
  return out.multiplyMatrices(world, pt);
}

export interface Figures {
  group: Group;
  halo: Mesh;
  update(frame: FigureFrame): Anchors;
  /** Vẽ lại nhãn (sau khi phông nạp). */
  relabel(): void;
  dispose(): void;
}

interface Label {
  sprite: Sprite;
  tex: CanvasTexture;
  key: string;
  draw: (g: CanvasRenderingContext2D, w: number, h: number) => void;
}

export function createFigures(): Figures {
  const reg = new Registry();
  const group = new Group();
  const detail = new Map<PartId, InstancedMesh>();
  const detailMat = reg.add(new MeshLambertMaterial({ color: '#FFFFFF', vertexColors: true }));
  const geometries = new Map<PartId, ReturnType<typeof partGeometry>>();
  const geometryOf = (id: PartId) => {
    let g = geometries.get(id);
    if (g === undefined) {
      g = reg.add(partGeometry(id));
      geometries.set(id, g);
    }
    return g;
  };
  for (const id of PART_IDS) {
    const mesh = new InstancedMesh(geometryOf(id), detailMat, MAX_INSTANCES);
    mesh.castShadow = CASTS_SHADOW.has(id);
    mesh.frustumCulled = false;
    mesh.count = 0;
    mesh.visible = false;
    mesh.instanceColor = null;
    for (let i = 0; i < MAX_INSTANCES; i++) mesh.setColorAt(i, colorOf('#FFFFFF'));
    detail.set(id, mesh);
    group.add(mesh);
  }
  // Hình rút gọn: thân, đầu, tóc chỏm cho mọi người (chung cả tòa).
  const reducedParts: PartId[] = ['torso', 'head', 'hairCap'];
  const reduced = new Map<PartId, InstancedMesh>();
  const reducedMax = 5 * 24 + 1;
  for (const id of reducedParts) {
    const mesh = new InstancedMesh(geometryOf(id), detailMat, reducedMax);
    mesh.frustumCulled = false;
    mesh.count = 0;
    for (let i = 0; i < reducedMax; i++) mesh.setColorAt(i, colorOf('#FFFFFF'));
    reduced.set(id, mesh);
    group.add(mesh);
  }

  const halo = new Mesh(reg.add(new CylinderGeometry(0.42, 0.42, 0.015, 24)), reg.add(new MeshBasicMaterial({ color: '#FFD166', transparent: true, opacity: 0.55, depthWrite: false })));
  halo.visible = false;
  group.add(halo);

  // Bóng đổ giả dưới chân người của tầng đang xem: một InstancedMesh, một draw call.
  const blobGeo = reg.add(new PlaneGeometry(0.8, 0.7));
  blobGeo.rotateX(-Math.PI / 2);
  const blobMat = reg.add(new MeshBasicMaterial({ map: blobTexture(reg), transparent: true, depthWrite: false, toneMapped: false, polygonOffset: true, polygonOffsetFactor: -2 }));
  const blobs = new InstancedMesh(blobGeo, blobMat, MAX_INSTANCES);
  blobs.frustumCulled = false;
  blobs.renderOrder = 1;
  blobs.count = 0;
  group.add(blobs);

  // Sprite "!" dùng chung một texture; "…" của thư ký.
  const bangTex = canvasTexture(reg, 64, 64, drawBang);
  const bangMat = spriteMaterial(reg, bangTex);
  const dotsTex = canvasTexture(reg, 96, 72, drawDots);
  const dotsSprite = new Sprite(spriteMaterial(reg, dotsTex));
  dotsSprite.scale.set(0.6, 0.45, 1);
  dotsSprite.visible = false;
  dotsSprite.renderOrder = 10;
  group.add(dotsSprite);
  const bangPool: Sprite[] = [];
  for (let i = 0; i < MAX_INSTANCES; i++) {
    const s = new Sprite(bangMat);
    s.scale.set(0.38, 0.38, 1);
    s.visible = false;
    s.renderOrder = 10;
    bangPool.push(s);
    group.add(s);
  }

  const labels = new Map<string, Label>();
  const labelGroup = new Group();
  group.add(labelGroup);
  let fontsOk = false;
  void fontsReady().then(() => {
    fontsOk = true;
    relabel();
  });

  const cache = new Map<string, readonly PartTransform[]>();
  const transforms = (pose: PoseId, hair: Look['hair'], extras: FigureExtras, phase: number): readonly PartTransform[] => {
    if (pose === 'walk') return partTransforms(walkAngles(phase), hair, extras);
    const key = `${pose}|${hair}|${shapeKey(extras.look ?? SECRETARY_LOOK)}|${extras.mood}|${extras.secretary}|${extras.folder}`;
    let t = cache.get(key);
    if (t === undefined) {
      if (cache.size > 512) cache.clear();
      t = partTransforms(POSES[pose], hair, extras);
      cache.set(key, t);
    }
    return t;
  };

  function localPos(f: Figure): { pos: Vec3; yaw: number } {
    const { spot, pose } = f;
    if (spot.kind === 'desk') {
      const seat = deskAt(spot.floor, spot.index).seat;
      if (pose === 'raise' || pose === 'cheer') return { pos: { x: seat.x + 0.6, y: seat.y, z: seat.z }, yaw: 0 };
      return { pos: { x: seat.x, y: seat.y, z: seat.z - (isSeated(pose) ? SIT_FORWARD : 0) }, yaw: 0 };
    }
    const list = spot.kind === 'lounge' ? LOUNGE_SPOTS : STANDING_SPOTS;
    const p = list[spot.floor]?.[spot.index] ?? { x: 0, y: FLOOR_SLAB, z: 0 };
    const sofa = spot.kind === 'lounge' && spot.index < LOUNGE_SOFA_SPOTS;
    return { pos: sofa ? { ...p, z: p.z - 0.05 } : p, yaw: sofa ? Math.PI : 0 };
  }

  function worldOf(floor: FloorId, pos: Vec3, lifts: readonly number[]): Vec3 {
    const g = geo(floor);
    return { x: g.cx + pos.x, y: (BASE[floor] ?? 0) + (lifts[floor] ?? 0) + pos.y, z: g.cz + pos.z };
  }

  let nReduced = 0;
  const anchors: Anchors = { agents: new Map(), secretary: null, bangs: [] };

  /** Số instance đã ghi của từng phần: mỗi phần có danh sách dày riêng (chỉ người dùng phần đó), nên kiểu tóc/phụ kiện hiếm gần như không tốn gì. */
  const used = new Map<PartId, number>();
  const colorsFor = new Map<string, Record<ColorKey, string>>();

  function place(agentId: AgentId | null, w: Vec3, yaw: number, pose: PoseId, look: Look, mood: Mood, palette: { hair: string; outfit: string; acc: string }, accent: string, asDetail: boolean, secretary: boolean, phase: number, bang: boolean): void {
    const folder = secretary && (pose === 'walk' || pose === 'stand');
    const extras: FigureExtras = { mood, secretary, folder, look };
    const parts = transforms(pose, look.hair, extras, phase);
    base.compose(v3.set(w.x, w.y, w.z), q.setFromEuler(e.set(0, yaw, 0)), one);
    const ckey = `${palette.hair}${palette.outfit}${palette.acc}${accent}${look.skin}${look.bottom}${look.shoe}`;
    let colors = colorsFor.get(ckey);
    if (colors === undefined) {
      if (colorsFor.size > 128) colorsFor.clear();
      colors = partColors(palette, accent, look);
      colorsFor.set(ckey, colors);
    }
    const head = parts.find((p) => p.part === 'head');
    const headWorld = new Vector3(head?.pos.x ?? 0, head?.pos.y ?? 1.33, head?.pos.z ?? 0).applyMatrix4(base);
    if (asDetail) {
      for (const t of parts) {
        const mesh = detail.get(t.part);
        const i = used.get(t.part) ?? 0;
        if (mesh === undefined || t.scale.x === 0 || i >= MAX_INSTANCES) continue;
        mesh.setMatrixAt(i, partMatrix(m4, base, t));
        mesh.setColorAt(i, colorOf(colors[t.tint ?? PART_SPECS[t.part].color]));
        used.set(t.part, i + 1);
      }
      if (nBlob < MAX_INSTANCES) blobs.setMatrixAt(nBlob++, m4.copy(base).setPosition(w.x, w.y + 0.02, w.z));
      if (agentId !== null) anchors.agents.set(agentId, headWorld);
      if (bang && agentId !== null) {
        const s = bangPool[nBang++];
        if (s !== undefined) {
          s.position.set(headWorld.x, headWorld.y + 0.55, headWorld.z);
          s.userData['agentId'] = agentId;
          s.visible = true;
          anchors.bangs.push(s);
        }
      }
    } else {
      const i = nReduced++;
      if (i >= reducedMax) return;
      for (const id of reducedParts) {
        let t = parts.find((p) => p.part === id);
        const mesh = reduced.get(id);
        if (t === undefined || mesh === undefined) continue;
        // Tóc kiểu mũ len/xoăn không có chỏm: hình rút gọn vẫn cần một chỏm tóc để khỏi trọc.
        if (id === 'hairCap' && t.scale.x === 0 && head !== undefined) t = { ...t, pos: head.pos, rot: head.rot, scale: { x: 1, y: 1, z: 1 } };
        mesh.setMatrixAt(i, partMatrix(m4, base, t));
        mesh.setColorAt(i, colorOf(colors[t.tint ?? PART_SPECS[id].color]));
      }
    }
    if (secretary) anchors.secretary = headWorld;
  }
  let nBang = 0;
  let nBlob = 0;
  const secretaryHead = (): Vector3 | null => anchors.secretary;

  const frameLabels = new Map<string, { key: string; draw: Label['draw']; at: Vector3 }>();

  function syncLabels(): void {
    for (const [key, l] of labels) {
      if (!frameLabels.has(key)) {
        labelGroup.remove(l.sprite);
        l.tex.dispose();
        l.sprite.material.dispose();
        labels.delete(key);
      }
    }
    for (const [key, want] of frameLabels) {
      let l = labels.get(key);
      if (l === undefined || l.key !== want.key) {
        if (l !== undefined) {
          labelGroup.remove(l.sprite);
          l.tex.dispose();
          l.sprite.material.dispose();
        }
        const tex = canvasTexture(new Registry(), 256, 64, want.draw);
        const sprite = new Sprite(spriteMaterial(new Registry(), tex));
        sprite.scale.set(1.7, 0.425, 1);
        sprite.renderOrder = 9;
        labelGroup.add(sprite);
        l = { sprite, tex, key: want.key, draw: want.draw };
        labels.set(key, l);
      }
      l.draw = want.draw;
      l.sprite.position.copy(want.at);
    }
  }

  function relabel(): void {
    if (!fontsOk) return;
    for (const l of labels.values()) redraw(l.tex, l.draw);
  }

  return {
    group,
    halo,
    update(frame) {
      nReduced = 0;
      nBang = 0;
      nBlob = 0;
      used.clear();
      anchors.agents.clear();
      anchors.secretary = null;
      anchors.bangs = [];
      frameLabels.clear();
      for (const s of bangPool) s.visible = false;
      for (const r of frame.rosters) {
        for (const f of r.figures) {
          const { pos, yaw } = localPos(f);
          const w = worldOf(r.floor, pos, frame.lifts);
          const asDetail = frame.detailFloor === r.floor;
          const palette = safePalette(f.agent.palette);
          const accent = FLOORS[r.floor]?.accent ?? '#9AA3B5';
          place(f.agent.id, w, yaw, f.pose, lookFor(f.agent.id), f.agent.mood, palette, accent, asDetail, false, 0, f.pose === 'raise' && frame.selectable.has(f.agent.id));
          if (asDetail) {
            const role = roleShort(f.placement.floor, f.placement.roleId) ?? (f.placement.zone === 'session' ? 'phiên chính' : f.placement.zone === 'guest' ? 'khách' : f.placement.zone === 'extra' ? 'bàn linh hoạt' : 'chung');
            const name = f.agent.name;
            const head = anchors.agents.get(f.agent.id);
            if (head !== undefined) {
              frameLabels.set(f.agent.id, {
                key: `${role}|${name}`,
                draw: (g, cw, ch) => drawDeskLabel(g, cw, ch, role, cleanText(name, 16), accent),
                at: new Vector3(head.x, head.y + 0.42, head.z),
              });
            }
          }
        }
      }
      // Thư ký
      const s = frame.secretary;
      let secWorld: Vec3;
      const yaw = s.facing;
      let pos = s.pos;
      if (s.floor === 'car') {
        secWorld = pos;
      } else {
        if (s.pose === 'sit_type' || s.pose === 'sleep') pos = { ...pos, z: pos.z - SIT_FORWARD };
        secWorld = worldOf(s.floor, pos, frame.lifts);
      }
      const asDetail = s.floor === 'car' || frame.detailFloor === s.floor;
      place(null, secWorld, yaw, s.pose, SECRETARY_LOOK, 'idle', SECRETARY_PALETTE, '#FFB020', asDetail, true, s.phase, false);
      halo.visible = true;
      halo.position.set(secWorld.x, secWorld.y + 0.01, secWorld.z);
      const sh = secretaryHead();
      dotsSprite.visible = s.speaking && sh !== null;
      if (sh !== null) dotsSprite.position.set(sh.x, sh.y + 0.85, sh.z);

      for (const [id, mesh] of detail) {
        const n = used.get(id) ?? 0;
        mesh.count = n;
        mesh.visible = n > 0;
        mesh.instanceMatrix.needsUpdate = true;
        if (mesh.instanceColor !== null) mesh.instanceColor.needsUpdate = true;
      }
      for (const mesh of reduced.values()) {
        mesh.count = nReduced;
        mesh.visible = nReduced > 0;
        mesh.instanceMatrix.needsUpdate = true;
        if (mesh.instanceColor !== null) mesh.instanceColor.needsUpdate = true;
      }
      blobs.count = nBlob;
      blobs.visible = nBlob > 0;
      blobs.instanceMatrix.needsUpdate = true;
      syncLabels();
      relabel();
      return anchors;
    },
    relabel,
    dispose() {
      for (const l of labels.values()) {
        l.tex.dispose();
        l.sprite.material.dispose();
      }
      labels.clear();
      reg.disposeAll();
    },
  };
}
