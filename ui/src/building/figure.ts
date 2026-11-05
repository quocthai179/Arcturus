// Nhân vật thủ tục (docs/OFFICE_BUILDING.md §6): kích thước, bảng tư thế, động học thuận (FK) thuần. Không import three:
// `three/figures.ts` chỉ biến `PartTransform[]` thành InstancedMesh. Nhân vật đứng cao 1,50 m, gốc ở chân, MẶT HƯỚNG −z.
// Quy ước góc: pitch dương = vai/hông gập ra trước (về −z); bend dương = khuỷu/gối gập; head dương = cúi; torso dương = nghiêng trước.
import type { Mood, Palette } from '../protocol';
import { DEFAULT_PALETTE } from '../components/Character';
import type { Vec3 } from './layout';

export type PoseId = 'stand' | 'walk' | 'sit_type' | 'sit_read' | 'sit_think' | 'sit_error' | 'raise' | 'cheer' | 'lounge_sit' | 'sleep';
export type HairStyle = 'short' | 'bob' | 'bun';

export interface Limb {
  pitch: number;
  roll: number;
  bend: number;
}
export interface JointAngles {
  armL: Limb;
  armR: Limb;
  legL: Limb;
  legR: Limb;
  head: number;
  torso: number;
  pelvisY: number;
}

/** Người ngồi ở bàn được kéo về phía bàn chừng này (so với ghế) để tay chạm bàn phím. */
export const SIT_FORWARD = 0.25;

const L = (pitch: number, roll: number, bend: number): Limb => ({ pitch, roll, bend });
const J = (armL: Limb, armR: Limb, legL: Limb, legR: Limb, head: number, torso: number, pelvisY: number): JointAngles => ({
  armL, armR, legL, legR, head, torso, pelvisY,
});
const SIT_LEGS = [L(90, 0, 90), L(90, 0, 90)] as const;

export const POSES: Record<Exclude<PoseId, 'walk'>, JointAngles> = {
  stand: J(L(0, 8, 10), L(0, 8, 10), L(0, 0, 0), L(0, 0, 0), 0, 0, 0.74),
  sit_type: J(L(50, 10, 70), L(50, 10, 70), ...SIT_LEGS, 12, 5, 0.45),
  sit_read: J(L(30, 10, 100), L(45, 10, 60), ...SIT_LEGS, 18, 4, 0.45),
  sit_think: J(L(40, 10, 60), L(70, 5, 140), ...SIT_LEGS, 5, 8, 0.45),
  sit_error: J(L(50, 10, 70), L(150, 40, 130), ...SIT_LEGS, 15, 10, 0.45),
  raise: J(L(0, 8, 10), L(170, 15, 15), L(0, 0, 0), L(0, 0, 0), -10, 0, 0.74),
  cheer: J(L(160, 30, 20), L(160, 30, 20), L(0, 0, 0), L(0, 0, 0), -8, -3, 0.74),
  lounge_sit: J(L(10, 12, 30), L(10, 12, 30), L(80, 0, 85), L(80, 0, 85), 0, -12, 0.42),
  sleep: J(L(40, 10, 90), L(40, 10, 90), L(85, 0, 90), L(85, 0, 90), 35, 20, 0.45),
};

/** Tư thế ngồi (hông ở mặt ghế): người được kéo về bàn `SIT_FORWARD`. */
export function isSeated(pose: PoseId): boolean {
  return pose === 'sit_type' || pose === 'sit_read' || pose === 'sit_think' || pose === 'sit_error' || pose === 'sleep';
}

/** Bước đi một chu kỳ 2 bước, `phase` 0…1 (tuần hoàn: 0 = 1). */
export function walkAngles(phase: number): JointAngles {
  const a = 2 * Math.PI * phase;
  const s = Math.sin(a);
  const c = Math.cos(a);
  const bendL = 10 + 20 * Math.max(0, c);
  const bendR = 10 + 20 * Math.max(0, -c);
  return J(L(-25 * s, 6, 20), L(25 * s, 6, 20), L(25 * s, 0, bendL), L(-25 * s, 0, bendR), 0, 3, 0.74 + 0.02 * Math.abs(s));
}

// --- màu ---

const HEX = /^#[0-9a-fA-F]{6}$/;
export const SECRETARY_PALETTE: Palette = { hair: '#2E2A5C', outfit: '#8B7CF6', acc: '#FFD166' };

/** Chỉ nhận `#RRGGBB`; sai thì dùng bảng màu mặc định (màu đi vào vật liệu, không nhận chuỗi tùy ý). */
export function safePalette(p: Partial<Palette> | undefined): Palette {
  const pick = (v: string | undefined, d: string) => (v !== undefined && HEX.test(v) ? v : d);
  return { hair: pick(p?.hair, DEFAULT_PALETTE.hair), outfit: pick(p?.outfit, DEFAULT_PALETTE.outfit), acc: pick(p?.acc, DEFAULT_PALETTE.acc) };
}

export function fnv1a(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

const HAIR_STYLES: readonly HairStyle[] = ['short', 'bob', 'bun'];
/** Ổn định theo id, không dựa vào tên. */
export function hairStyle(agentId: string): HairStyle {
  return HAIR_STYLES[fnv1a(agentId) % 3] ?? 'short';
}

// --- các phần ---

export type ColorKey = 'skin' | 'hair' | 'outfit' | 'outfitDark' | 'pants' | 'shoe' | 'ink' | 'mouth' | 'cheek' | 'acc' | 'badge' | 'gold' | 'headset' | 'folder';
export type PartShape =
  | { k: 'sphere'; r: number }
  | { k: 'box'; w: number; h: number; d: number }
  | { k: 'capsule'; r: number; len: number }
  | { k: 'cylinder'; r: number; h: number }
  | { k: 'torus'; r: number; tube: number };

export type PartId =
  | 'head' | 'hairCap' | 'hairBob' | 'hairBun' | 'eyeL' | 'eyeR' | 'mouth' | 'cheekL' | 'cheekR' | 'neck' | 'torso' | 'belt' | 'badge'
  | 'armL' | 'armR' | 'foreL' | 'foreR' | 'handL' | 'handR' | 'thighL' | 'thighR' | 'shinL' | 'shinR' | 'shoeL' | 'shoeR'
  | 'antStem' | 'antBall' | 'headset' | 'folder';

/** `len` của capsule là độ dài khớp-tới-khớp (gồm hai nắp cầu). */
export const PART_SPECS: Record<PartId, { shape: PartShape; color: ColorKey }> = {
  head: { shape: { k: 'sphere', r: 0.165 }, color: 'skin' },
  hairCap: { shape: { k: 'sphere', r: 0.178 }, color: 'hair' },
  hairBob: { shape: { k: 'box', w: 0.36, h: 0.22, d: 0.2 }, color: 'hair' },
  hairBun: { shape: { k: 'sphere', r: 0.07 }, color: 'hair' },
  eyeL: { shape: { k: 'sphere', r: 0.022 }, color: 'ink' },
  eyeR: { shape: { k: 'sphere', r: 0.022 }, color: 'ink' },
  mouth: { shape: { k: 'box', w: 0.05, h: 0.012, d: 0.01 }, color: 'mouth' },
  cheekL: { shape: { k: 'cylinder', r: 0.025, h: 0.004 }, color: 'cheek' },
  cheekR: { shape: { k: 'cylinder', r: 0.025, h: 0.004 }, color: 'cheek' },
  neck: { shape: { k: 'cylinder', r: 0.055, h: 0.06 }, color: 'skin' },
  torso: { shape: { k: 'capsule', r: 0.17, len: 0.46 }, color: 'outfit' },
  belt: { shape: { k: 'cylinder', r: 0.172, h: 0.05 }, color: 'acc' },
  badge: { shape: { k: 'box', w: 0.06, h: 0.08, d: 0.01 }, color: 'badge' },
  armL: { shape: { k: 'capsule', r: 0.05, len: 0.26 }, color: 'outfit' },
  armR: { shape: { k: 'capsule', r: 0.05, len: 0.26 }, color: 'outfit' },
  foreL: { shape: { k: 'capsule', r: 0.045, len: 0.24 }, color: 'outfitDark' },
  foreR: { shape: { k: 'capsule', r: 0.045, len: 0.24 }, color: 'outfitDark' },
  handL: { shape: { k: 'sphere', r: 0.05 }, color: 'skin' },
  handR: { shape: { k: 'sphere', r: 0.05 }, color: 'skin' },
  thighL: { shape: { k: 'capsule', r: 0.065, len: 0.36 }, color: 'pants' },
  thighR: { shape: { k: 'capsule', r: 0.065, len: 0.36 }, color: 'pants' },
  shinL: { shape: { k: 'capsule', r: 0.058, len: 0.34 }, color: 'pants' },
  shinR: { shape: { k: 'capsule', r: 0.058, len: 0.34 }, color: 'pants' },
  shoeL: { shape: { k: 'box', w: 0.1, h: 0.07, d: 0.17 }, color: 'shoe' },
  shoeR: { shape: { k: 'box', w: 0.1, h: 0.07, d: 0.17 }, color: 'shoe' },
  antStem: { shape: { k: 'cylinder', r: 0.01, h: 0.3 }, color: 'gold' },
  antBall: { shape: { k: 'sphere', r: 0.06 }, color: 'gold' },
  headset: { shape: { k: 'torus', r: 0.17, tube: 0.012 }, color: 'headset' },
  folder: { shape: { k: 'box', w: 0.2, h: 0.28, d: 0.03 }, color: 'folder' },
};
export const PART_IDS = Object.keys(PART_SPECS) as PartId[];

export interface FigureExtras {
  mood: Mood;
  /** Thư ký: ăng-ten, tai nghe (và kẹp hồ sơ khi đi). */
  secretary: boolean;
  folder: boolean;
}
export const NO_EXTRAS: FigureExtras = { mood: 'idle', secretary: false, folder: false };

export interface PartTransform {
  part: PartId;
  pos: Vec3;
  /** Euler XYZ, radian. */
  rot: Vec3;
  scale: Vec3;
}

// --- toán 3x3 tối thiểu ---

type M3 = readonly [number, number, number, number, number, number, number, number, number];
const rad = (d: number) => (d * Math.PI) / 180;
function rx(a: number): M3 {
  const c = Math.cos(a);
  const s = Math.sin(a);
  return [1, 0, 0, 0, c, -s, 0, s, c];
}
function rz(a: number): M3 {
  const c = Math.cos(a);
  const s = Math.sin(a);
  return [c, -s, 0, s, c, 0, 0, 0, 1];
}
function mul(a: M3, b: M3): M3 {
  const o: number[] = [];
  for (let r = 0; r < 3; r++) for (let c = 0; c < 3; c++) o.push((a[r * 3] ?? 0) * (b[c] ?? 0) + (a[r * 3 + 1] ?? 0) * (b[3 + c] ?? 0) + (a[r * 3 + 2] ?? 0) * (b[6 + c] ?? 0));
  return o as unknown as M3;
}
function app(m: M3, v: Vec3): Vec3 {
  return {
    x: m[0] * v.x + m[1] * v.y + m[2] * v.z,
    y: m[3] * v.x + m[4] * v.y + m[5] * v.z,
    z: m[6] * v.x + m[7] * v.y + m[8] * v.z,
  };
}
const add = (a: Vec3, b: Vec3): Vec3 => ({ x: a.x + b.x, y: a.y + b.y, z: a.z + b.z });
const V = (x: number, y: number, z: number): Vec3 => ({ x, y, z });

/** Euler XYZ (như three.js `setFromRotationMatrix`). */
function euler(m: M3): Vec3 {
  const y = Math.asin(Math.max(-1, Math.min(1, m[2])));
  if (Math.abs(m[2]) < 0.9999999) return V(Math.atan2(-m[5], m[8]), y, Math.atan2(-m[1], m[0]));
  return V(Math.atan2(m[7], m[4]), y, 0);
}

const SCALE1 = V(1, 1, 1);
const ZERO = V(0, 0, 0);

/** Đầu một đoạn chi: tâm đoạn = khớp + R·(0, −len/2, 0); khớp kế = khớp + R·(0, −len, 0). */
function segment(joint: Vec3, r: M3, len: number): { center: Vec3; end: Vec3 } {
  return { center: add(joint, app(r, V(0, -len / 2, 0))), end: add(joint, app(r, V(0, -len, 0))) };
}

const SPEC_LEN = (p: PartId): number => {
  const s = PART_SPECS[p].shape;
  return s.k === 'capsule' ? s.len : 0;
};
/** CapsuleGeometry nhận độ dài phần trụ: trừ hai nắp cầu. */
export function capsuleCylinderLength(p: PartId): number {
  const s = PART_SPECS[p].shape;
  return s.k === 'capsule' ? Math.max(0.001, s.len - 2 * s.r) : 0;
}

/** FK thuần: gốc ở chân, mặt hướng −z. */
export function partTransforms(a: JointAngles, hair: HairStyle, extras: FigureExtras): PartTransform[] {
  const out: PartTransform[] = [];
  const put = (part: PartId, pos: Vec3, rot: Vec3 = ZERO, scale: Vec3 = SCALE1) => out.push({ part, pos, rot: V(rot.x, rot.y, rot.z), scale });
  const hide = (part: PartId) => put(part, ZERO, ZERO, ZERO);

  const hip = V(0, a.pelvisY, 0);
  const Rt = rx(-rad(a.torso)); // nghiêng trước = đỉnh thân ra −z
  const Rh = mul(Rt, rx(-rad(a.head)));
  const up = (v: Vec3) => add(hip, app(Rt, v));

  put('torso', up(V(0, 0.24, 0)), euler(Rt), V(1.05, 1, 0.8));
  put('belt', up(V(0, 0.04, 0)), euler(Rt));
  put('badge', up(V(-0.08, 0.31, -0.14)), euler(Rt));
  put('neck', up(V(0, 0.51, 0)), euler(Rt));

  const neck = up(V(0, 0.51, 0));
  const inHead = (v: Vec3) => add(neck, app(Rh, add(V(0, 0.08, 0), v)));
  const hr = euler(Rh);
  put('head', inHead(ZERO), hr, V(1, 1.04, 0.96));
  put('hairCap', inHead(V(0, 0.04, 0.03)), hr, V(1, 0.8, 1));
  if (hair === 'bob') put('hairBob', inHead(V(0, -0.02, 0.1)), hr); else hide('hairBob');
  if (hair === 'bun' || extras.secretary) {
    if (hair === 'bun') put('hairBun', inHead(V(0, 0.13, 0.1)), hr); else hide('hairBun');
  } else hide('hairBun');
  put('eyeL', inHead(V(-0.058, 0.01, -0.15)), hr);
  put('eyeR', inHead(V(0.058, 0.01, -0.15)), hr);
  const m = extras.mood;
  const mouthScale = m === 'done' || m === 'approval' ? V(1.6, 1.3, 1) : m === 'error' ? V(0.8, 0.8, 1) : m === 'working' ? V(0.7, 0.8, 1) : V(1, 1, 1);
  put('mouth', inHead(V(0, -0.07, -0.155)), hr, mouthScale);
  if (m === 'done' || m === 'approval') {
    // Đĩa má: trục y mặc định của cylinder cần xoay về +z (rx 90°).
    put('cheekL', inHead(V(-0.09, -0.04, -0.13)), euler(mul(Rh, rx(Math.PI / 2))));
    put('cheekR', inHead(V(0.09, -0.04, -0.13)), euler(mul(Rh, rx(Math.PI / 2))));
  } else {
    hide('cheekL');
    hide('cheekR');
  }

  // Tay
  for (const side of ['L', 'R'] as const) {
    const sx = side === 'R' ? 1 : -1;
    const limb = side === 'R' ? a.armR : a.armL;
    const shoulder = up(V(0.2 * sx, 0.41, 0));
    const Ru = mul(mul(Rt, rx(rad(limb.pitch))), rz(rad(limb.roll) * sx));
    const upper = segment(shoulder, Ru, SPEC_LEN('armL'));
    put(side === 'R' ? 'armR' : 'armL', upper.center, euler(Ru));
    const Rf = mul(Ru, rx(rad(limb.bend)));
    const fore = segment(upper.end, Rf, SPEC_LEN('foreL'));
    put(side === 'R' ? 'foreR' : 'foreL', fore.center, euler(Rf));
    put(side === 'R' ? 'handR' : 'handL', add(fore.end, app(Rf, V(0, -0.03, 0))), ZERO);
    if (side === 'L' && extras.folder) put('folder', add(fore.end, app(Rf, V(0.0, -0.02, -0.06))), euler(Rf));
  }
  if (!extras.folder) hide('folder');

  // Chân (hông không nghiêng theo thân)
  for (const side of ['L', 'R'] as const) {
    const sx = side === 'R' ? 1 : -1;
    const limb = side === 'R' ? a.legR : a.legL;
    const hipJoint = add(hip, V(0.085 * sx, 0, 0));
    const Rth = mul(rx(rad(limb.pitch)), rz(rad(limb.roll) * sx));
    const thigh = segment(hipJoint, Rth, SPEC_LEN('thighL'));
    put(side === 'R' ? 'thighR' : 'thighL', thigh.center, euler(Rth));
    const Rsh = mul(Rth, rx(-rad(limb.bend)));
    const shin = segment(thigh.end, Rsh, SPEC_LEN('shinL'));
    put(side === 'R' ? 'shinR' : 'shinL', shin.center, euler(Rsh));
    put(side === 'R' ? 'shoeR' : 'shoeL', add(shin.end, V(0, -0.005, -0.03)), ZERO);
  }

  if (extras.secretary) {
    put('antStem', inHead(V(0, 0.33, 0)), hr);
    put('antBall', inHead(V(0, 0.5, 0)), hr);
    put('headset', inHead(V(0, 0.0, 0)), euler(mul(Rh, rz(Math.PI / 2))), V(1, 1, 1));
  } else {
    hide('antStem');
    hide('antBall');
    hide('headset');
  }
  return out;
}

/** Màu từng phần theo bảng màu + màu nhấn của tầng. */
export function partColors(p: Palette, accent: string): Record<ColorKey, string> {
  const dark = (hex: string, k: number) => {
    const n = parseInt(hex.slice(1), 16);
    const ch = (s: number) => Math.round(((n >> s) & 255) * k).toString(16).padStart(2, '0');
    return `#${ch(16)}${ch(8)}${ch(0)}`;
  };
  return {
    skin: '#FFE6D8', hair: p.hair, outfit: p.outfit, outfitDark: dark(p.outfit, 0.85), pants: '#2C3140', shoe: '#1A1D25', ink: '#241B4D',
    mouth: '#B4445E', cheek: '#FF8FA8', acc: p.acc, badge: HEX.test(accent) ? accent : p.acc, gold: '#FFD166', headset: '#2B2F3C', folder: '#E8EAF0',
  };
}
