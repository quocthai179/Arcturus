// Nhân vật thủ tục (docs/OFFICE_BUILDING.md §6): kích thước, bảng tư thế, động học thuận (FK) thuần. Không import three:
// `three/figures.ts` chỉ biến `PartTransform[]` thành InstancedMesh. Nhân vật đứng cao 1,50 m, gốc ở chân, MẶT HƯỚNG −z.
// Quy ước góc: pitch dương = vai/hông gập ra trước (về −z); bend dương = khuỷu/gối gập; head dương = cúi; torso dương = nghiêng trước.
import type { Mood, Palette } from '../protocol';
import { DEFAULT_PALETTE } from '../components/Character';
import type { Vec3 } from './layout';

export type PoseId = 'stand' | 'walk' | 'sit_type' | 'sit_read' | 'sit_think' | 'sit_error' | 'raise' | 'cheer' | 'lounge_sit' | 'sleep';
export type HairStyle = 'short' | 'bob' | 'bun' | 'long' | 'curly' | 'ponytail' | 'beanie';
export type TopStyle = 'tee' | 'hoodie' | 'shirt' | 'sweater' | 'blazer';
export type BottomStyle = 'jeans' | 'chino' | 'slacks' | 'charcoal';

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

export const HAIR_STYLES: readonly HairStyle[] = ['short', 'bob', 'bun', 'long', 'curly', 'ponytail', 'beanie'];
export const TOP_STYLES: readonly TopStyle[] = ['tee', 'hoodie', 'shirt', 'sweater', 'blazer'];
export const BOTTOM_STYLES: readonly BottomStyle[] = ['jeans', 'chino', 'slacks', 'charcoal'];
/** Tông da từ sáng đến sẫm (chọn theo id, độc lập với màu tóc/áo của agent). */
export const SKIN_TONES: readonly string[] = ['#FFE0CC', '#F5CBA7', '#E8B48A', '#C98B5E', '#9C6444', '#6B4330'];
export const SHOE_COLORS: readonly string[] = ['#F2EFE8', '#C9795A', '#2B2B30', '#8A6A4C', '#7C9CBF'];

/** Kiểu dáng của một nhân vật: toàn bộ suy ra từ id, không có chuỗi nào từ agent đi vào hình học. */
export interface Look {
  hair: HairStyle;
  skin: number;
  top: TopStyle;
  bottom: BottomStyle;
  shoe: number;
  glasses: boolean;
  headphones: boolean;
  lanyard: boolean;
}

export const DEFAULT_LOOK: Look = { hair: 'short', skin: 1, top: 'tee', bottom: 'jeans', shoe: 0, glasses: false, headphones: false, lanyard: false };
/** Thư ký: vest thanh lịch, quần tây, bob gọn, thẻ đeo cổ (tai nghe, ăng-ten, bìa kẹp hồ sơ thêm ở `partTransforms`). */
export const SECRETARY_LOOK: Look = { hair: 'bob', skin: 1, top: 'blazer', bottom: 'slacks', shoe: 2, glasses: false, headphones: false, lanyard: true };

/** Bộ trộn bit murmur3 (fmix32): id gần nhau (agt_1, agt_2…) vẫn cho kiểu dáng khác hẳn nhau. */
function fmix(x: number): number {
  let h = x >>> 0;
  h ^= h >>> 16;
  h = Math.imul(h, 0x85ebca6b);
  h ^= h >>> 13;
  h = Math.imul(h, 0xc2b2ae35);
  h ^= h >>> 16;
  return h >>> 0;
}

/** Mỗi thuộc tính một số riêng, suy ra từ hash của id (không tương quan với nhau). */
function stream(id: string): () => number {
  const seed = fnv1a(id);
  let k = 0;
  return () => fmix(seed + Math.imul(++k, 0x9e3779b9));
}

/** Ổn định theo id (mọi chuỗi, kể cả rỗng/unicode), không dựa vào tên. */
export function lookFor(agentId: string): Look {
  const next = stream(agentId);
  const pick = <T>(list: readonly T[], fallback: T): T => list[next() % list.length] ?? fallback;
  const hair = pick(HAIR_STYLES, 'short');
  const skin = next() % SKIN_TONES.length;
  const t = next() % 9;
  const top: TopStyle = t < 2 ? 'tee' : t < 4 ? 'hoodie' : t < 6 ? 'shirt' : t < 8 ? 'sweater' : 'blazer';
  const bottom = pick(BOTTOM_STYLES, 'jeans');
  const shoe = next() % SHOE_COLORS.length;
  return { hair, skin, top, bottom, shoe, glasses: next() % 100 < 30, headphones: next() % 100 < 22, lanyard: next() % 100 < 45 };
}

/** Kiểu tóc (giữ cho nơi chỉ cần tóc). */
export function hairStyle(agentId: string): HairStyle {
  return lookFor(agentId).hair;
}

/** Khóa bộ nhớ đệm cho các thuộc tính ảnh hưởng tới hình học/tư thế (da, quần, giày chỉ ảnh hưởng màu). */
export function shapeKey(look: Look): string {
  return `${look.hair}.${look.top}.${look.glasses ? 1 : 0}${look.headphones ? 1 : 0}${look.lanyard ? 1 : 0}`;
}

export interface Face {
  /** Độ nghiêng đuôi mày trong (rad): dương = lo lắng, âm = cau có/tập trung. */
  browTilt: number;
  /** Nâng mày (m). */
  browLift: number;
  mouth: 'smile' | 'flat' | 'open' | 'frown';
  mouthScale: number;
  cheeks: boolean;
}
export const MOOD_FACE: Record<Mood, Face> = {
  idle: { browTilt: 0, browLift: 0, mouth: 'smile', mouthScale: 0.8, cheeks: false },
  working: { browTilt: -0.15, browLift: -0.004, mouth: 'flat', mouthScale: 0.8, cheeks: false },
  approval: { browTilt: 0.12, browLift: 0.014, mouth: 'open', mouthScale: 1, cheeks: true },
  error: { browTilt: 0.42, browLift: 0.006, mouth: 'frown', mouthScale: 0.9, cheeks: false },
  done: { browTilt: 0, browLift: 0.008, mouth: 'smile', mouthScale: 1.3, cheeks: true },
};

// --- các phần ---

export type ColorKey =
  | 'skin' | 'skinDark' | 'hair' | 'brow' | 'outfit' | 'outfitDark' | 'collar' | 'pants' | 'shoe' | 'ink' | 'mouth' | 'cheek' | 'acc' | 'badge'
  | 'gold' | 'headset' | 'folder' | 'frame' | 'lanyard' | 'belt' | 'hat';
/** Hình học tùy biến (nhiều khối gộp) do `three/figures.ts` dựng theo tên. */
export type CustomShape =
  | 'hairCap' | 'fringe' | 'hairBob' | 'hairLong' | 'hairCurly' | 'hairPony' | 'beanie' | 'glasses' | 'headphones' | 'headset' | 'lanyard'
  | 'collar' | 'hood' | 'shoe' | 'clipboard' | 'smile';
export type PartShape =
  | { k: 'sphere'; r: number }
  | { k: 'box'; w: number; h: number; d: number }
  | { k: 'capsule'; r: number; len: number }
  | { k: 'cylinder'; r: number; h: number }
  | { k: 'custom'; name: CustomShape };

export type PartId =
  | 'head' | 'earL' | 'earR' | 'nose' | 'eyeL' | 'eyeR' | 'browL' | 'browR' | 'mouth' | 'smile' | 'cheekL' | 'cheekR'
  | 'hairCap' | 'fringe' | 'hairBob' | 'hairLong' | 'hairCurly' | 'hairPony' | 'hairBun' | 'beanie' | 'glasses' | 'headphones'
  | 'neck' | 'torso' | 'belt' | 'badge' | 'lanyard' | 'collar' | 'hood' | 'pocket' | 'hem'
  | 'armL' | 'armR' | 'foreL' | 'foreR' | 'cuffL' | 'cuffR' | 'handL' | 'handR' | 'thighL' | 'thighR' | 'shinL' | 'shinR' | 'shoeL' | 'shoeR'
  | 'antStem' | 'antBall' | 'headset' | 'folder';

/** `len` của capsule là độ dài khớp-tới-khớp (gồm hai nắp cầu). */
export const PART_SPECS: Record<PartId, { shape: PartShape; color: ColorKey }> = {
  head: { shape: { k: 'sphere', r: 0.185 }, color: 'skin' },
  earL: { shape: { k: 'sphere', r: 0.036 }, color: 'skin' },
  earR: { shape: { k: 'sphere', r: 0.036 }, color: 'skin' },
  nose: { shape: { k: 'sphere', r: 0.02 }, color: 'skinDark' },
  eyeL: { shape: { k: 'sphere', r: 0.024 }, color: 'ink' },
  eyeR: { shape: { k: 'sphere', r: 0.024 }, color: 'ink' },
  browL: { shape: { k: 'box', w: 0.062, h: 0.013, d: 0.012 }, color: 'brow' },
  browR: { shape: { k: 'box', w: 0.062, h: 0.013, d: 0.012 }, color: 'brow' },
  mouth: { shape: { k: 'box', w: 0.05, h: 0.012, d: 0.01 }, color: 'mouth' },
  smile: { shape: { k: 'custom', name: 'smile' }, color: 'mouth' },
  cheekL: { shape: { k: 'cylinder', r: 0.027, h: 0.004 }, color: 'cheek' },
  cheekR: { shape: { k: 'cylinder', r: 0.027, h: 0.004 }, color: 'cheek' },
  hairCap: { shape: { k: 'custom', name: 'hairCap' }, color: 'hair' },
  fringe: { shape: { k: 'custom', name: 'fringe' }, color: 'hair' },
  hairBob: { shape: { k: 'custom', name: 'hairBob' }, color: 'hair' },
  hairLong: { shape: { k: 'custom', name: 'hairLong' }, color: 'hair' },
  hairCurly: { shape: { k: 'custom', name: 'hairCurly' }, color: 'hair' },
  hairPony: { shape: { k: 'custom', name: 'hairPony' }, color: 'hair' },
  hairBun: { shape: { k: 'sphere', r: 0.078 }, color: 'hair' },
  beanie: { shape: { k: 'custom', name: 'beanie' }, color: 'hat' },
  glasses: { shape: { k: 'custom', name: 'glasses' }, color: 'frame' },
  headphones: { shape: { k: 'custom', name: 'headphones' }, color: 'hat' },
  neck: { shape: { k: 'cylinder', r: 0.055, h: 0.06 }, color: 'skin' },
  torso: { shape: { k: 'capsule', r: 0.17, len: 0.46 }, color: 'outfit' },
  belt: { shape: { k: 'cylinder', r: 0.172, h: 0.04 }, color: 'belt' },
  badge: { shape: { k: 'box', w: 0.06, h: 0.08, d: 0.01 }, color: 'badge' },
  lanyard: { shape: { k: 'custom', name: 'lanyard' }, color: 'lanyard' },
  collar: { shape: { k: 'custom', name: 'collar' }, color: 'collar' },
  hood: { shape: { k: 'custom', name: 'hood' }, color: 'outfitDark' },
  pocket: { shape: { k: 'box', w: 0.15, h: 0.07, d: 0.03 }, color: 'outfitDark' },
  hem: { shape: { k: 'cylinder', r: 0.17, h: 0.05 }, color: 'outfitDark' },
  armL: { shape: { k: 'capsule', r: 0.05, len: 0.26 }, color: 'outfit' },
  armR: { shape: { k: 'capsule', r: 0.05, len: 0.26 }, color: 'outfit' },
  foreL: { shape: { k: 'capsule', r: 0.045, len: 0.24 }, color: 'outfitDark' },
  foreR: { shape: { k: 'capsule', r: 0.045, len: 0.24 }, color: 'outfitDark' },
  cuffL: { shape: { k: 'cylinder', r: 0.049, h: 0.035 }, color: 'outfitDark' },
  cuffR: { shape: { k: 'cylinder', r: 0.049, h: 0.035 }, color: 'outfitDark' },
  handL: { shape: { k: 'sphere', r: 0.052 }, color: 'skin' },
  handR: { shape: { k: 'sphere', r: 0.052 }, color: 'skin' },
  thighL: { shape: { k: 'capsule', r: 0.065, len: 0.36 }, color: 'pants' },
  thighR: { shape: { k: 'capsule', r: 0.065, len: 0.36 }, color: 'pants' },
  shinL: { shape: { k: 'capsule', r: 0.058, len: 0.34 }, color: 'pants' },
  shinR: { shape: { k: 'capsule', r: 0.058, len: 0.34 }, color: 'pants' },
  shoeL: { shape: { k: 'custom', name: 'shoe' }, color: 'shoe' },
  shoeR: { shape: { k: 'custom', name: 'shoe' }, color: 'shoe' },
  antStem: { shape: { k: 'cylinder', r: 0.01, h: 0.3 }, color: 'gold' },
  antBall: { shape: { k: 'sphere', r: 0.06 }, color: 'gold' },
  headset: { shape: { k: 'custom', name: 'headset' }, color: 'headset' },
  folder: { shape: { k: 'custom', name: 'clipboard' }, color: 'folder' },
};
export const PART_IDS = Object.keys(PART_SPECS) as PartId[];

export interface FigureExtras {
  mood: Mood;
  /** Thư ký: ăng-ten, tai nghe có mic (và bìa kẹp hồ sơ khi đi/đứng). */
  secretary: boolean;
  folder: boolean;
  /** Kiểu dáng (mặc định `DEFAULT_LOOK`; thư ký dùng `SECRETARY_LOOK`). */
  look?: Look;
}
export const NO_EXTRAS: FigureExtras = { mood: 'idle', secretary: false, folder: false };

export interface PartTransform {
  part: PartId;
  pos: Vec3;
  /** Euler XYZ, radian. */
  rot: Vec3;
  scale: Vec3;
  /** Thay khóa màu của phần (vd. cẳng tay trần của áo thun dùng màu da). */
  tint?: ColorKey;
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

/** FK thuần: gốc ở chân, mặt hướng −z. `hair` ghi đè `extras.look.hair`. */
export function partTransforms(a: JointAngles, hair: HairStyle, extras: FigureExtras): PartTransform[] {
  const out: PartTransform[] = [];
  const put = (part: PartId, pos: Vec3, rot: Vec3 = ZERO, scale: Vec3 = SCALE1, tint?: ColorKey) =>
    out.push(tint === undefined ? { part, pos, rot: V(rot.x, rot.y, rot.z), scale } : { part, pos, rot: V(rot.x, rot.y, rot.z), scale, tint });
  const hide = (part: PartId) => put(part, ZERO, ZERO, ZERO);
  const show = (part: PartId, on: boolean, pos: Vec3, rot: Vec3 = ZERO, scale: Vec3 = SCALE1) => (on ? put(part, pos, rot, scale) : hide(part));
  const look = extras.look ?? DEFAULT_LOOK;
  const top = look.top;

  const hip = V(0, a.pelvisY, 0);
  const Rt = rx(-rad(a.torso)); // nghiêng trước = đỉnh thân ra −z
  const Rh = mul(Rt, rx(-rad(a.head)));
  const up = (v: Vec3) => add(hip, app(Rt, v));
  const rt = euler(Rt);

  put('torso', up(V(0, 0.24, 0)), rt, V(1.05, 1, 0.8));
  show('belt', top === 'shirt' || top === 'blazer', up(V(0, 0.04, 0)), rt, V(1.03, 1, 0.82));
  show('hem', top === 'sweater' || top === 'hoodie', up(V(0, 0.03, 0)), rt, V(1.1, 1, 0.84));
  show('pocket', top === 'hoodie', up(V(0, 0.12, -0.133)), rt);
  show('hood', top === 'hoodie', up(V(0, 0.5, 0.0)), rt);
  show('collar', top === 'shirt' || top === 'blazer', up(V(0, 0.5, -0.01)), rt);
  show('lanyard', look.lanyard, up(V(0, 0.3, -0.14)), rt);
  put('badge', up(look.lanyard ? V(0, 0.3, -0.147) : V(-0.08, 0.31, -0.14)), rt);
  put('neck', up(V(0, 0.5, 0)), rt);

  const neck = up(V(0, 0.5, 0));
  const inHead = (v: Vec3) => add(neck, app(Rh, add(V(0, 0.075, 0), v)));
  const hr = euler(Rh);
  put('head', inHead(ZERO), hr, V(1, 1.02, 0.97));
  put('earL', inHead(V(-0.178, -0.01, 0.01)), hr, V(0.55, 1, 0.8));
  put('earR', inHead(V(0.178, -0.01, 0.01)), hr, V(0.55, 1, 0.8));
  put('nose', inHead(V(0, -0.02, -0.18)), hr, V(1, 1, 0.8));

  // Tóc: mũ len thay tóc ngắn; chỉ hiện đúng phần của kiểu đã chọn.
  const beanie = hair === 'beanie';
  show('hairCap', !beanie && hair !== 'curly', inHead(V(0, 0.01, 0.02)), hr);
  show('fringe', hair === 'short' || hair === 'bob' || hair === 'long' || hair === 'ponytail' || hair === 'bun', inHead(ZERO), hr);
  show('hairBob', hair === 'bob', inHead(ZERO), hr);
  show('hairLong', hair === 'long', inHead(ZERO), hr);
  show('hairCurly', hair === 'curly', inHead(ZERO), hr);
  show('hairPony', hair === 'ponytail', inHead(ZERO), hr);
  show('hairBun', hair === 'bun', inHead(V(0, 0.215, 0.07)), hr);
  show('beanie', beanie, inHead(ZERO), hr);

  // Mặt: mắt, mày, miệng theo tâm trạng.
  const face = MOOD_FACE[extras.mood];
  put('eyeL', inHead(V(-0.064, 0.012, -0.17)), hr, V(1, 1.3, 0.55));
  put('eyeR', inHead(V(0.064, 0.012, -0.17)), hr, V(1, 1.3, 0.55));
  put('browL', inHead(V(-0.064, 0.072 + face.browLift, -0.172)), euler(mul(Rh, rz(face.browTilt))));
  put('browR', inHead(V(0.064, 0.072 + face.browLift, -0.172)), euler(mul(Rh, rz(-face.browTilt))));
  const ms = face.mouthScale;
  show('mouth', face.mouth === 'flat' || face.mouth === 'open', inHead(V(0, -0.078, -0.172)), hr, face.mouth === 'open' ? V(0.5, 3.4, 1) : V(ms, 1, 1));
  show('smile', face.mouth === 'smile' || face.mouth === 'frown', inHead(V(0, face.mouth === 'smile' ? -0.058 : -0.1, -0.172)), face.mouth === 'smile' ? euler(mul(Rh, rz(Math.PI))) : hr, V(ms, ms, 1));
  const blush = euler(mul(Rh, rx(Math.PI / 2)));
  // Đĩa má: trục y mặc định của cylinder cần xoay về +z (rx 90°).
  show('cheekL', face.cheeks, inHead(V(-0.1, -0.04, -0.15)), blush);
  show('cheekR', face.cheeks, inHead(V(0.1, -0.04, -0.15)), blush);
  show('glasses', look.glasses, inHead(ZERO), hr);
  show('headphones', look.headphones && !extras.secretary, inHead(ZERO), hr);

  // Tay
  const sleeves = top !== 'tee';
  for (const side of ['L', 'R'] as const) {
    const sx = side === 'R' ? 1 : -1;
    const limb = side === 'R' ? a.armR : a.armL;
    const shoulder = up(V(0.2 * sx, 0.41, 0));
    const Ru = mul(mul(Rt, rx(rad(limb.pitch))), rz(rad(limb.roll) * sx));
    const upper = segment(shoulder, Ru, SPEC_LEN('armL'));
    put(side === 'R' ? 'armR' : 'armL', upper.center, euler(Ru));
    const Rf = mul(Ru, rx(rad(limb.bend)));
    const fore = segment(upper.end, Rf, SPEC_LEN('foreL'));
    put(side === 'R' ? 'foreR' : 'foreL', fore.center, euler(Rf), SCALE1, sleeves ? undefined : 'skin');
    show(side === 'R' ? 'cuffR' : 'cuffL', sleeves, add(fore.end, app(Rf, V(0, 0.03, 0))), euler(Rf));
    put(side === 'R' ? 'handR' : 'handL', add(fore.end, app(Rf, V(0, -0.03, 0))), ZERO);
    if (side === 'L' && extras.folder) put('folder', add(fore.end, app(Rf, V(0.0, -0.02, -0.07))), euler(Rf));
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

  show('antStem', extras.secretary, inHead(V(0, 0.36, 0)), hr);
  show('antBall', extras.secretary, inHead(V(0, 0.53, 0)), hr);
  show('headset', extras.secretary, inHead(ZERO), hr);
  return out;
}

/** Màu từng phần theo bảng màu + màu nhấn của tầng + kiểu dáng (da, quần, giày). */
export function partColors(p: Palette, accent: string, look: Look = DEFAULT_LOOK): Record<ColorKey, string> {
  const shade = (hex: string, k: number) => {
    const n = parseInt(hex.slice(1), 16);
    const ch = (s: number) => Math.min(255, Math.round(((n >> s) & 255) * k)).toString(16).padStart(2, '0');
    return `#${ch(16)}${ch(8)}${ch(0)}`;
  };
  const skin = SKIN_TONES[look.skin % SKIN_TONES.length] ?? '#F5CBA7';
  const pants = { jeans: '#41648F', chino: '#B79F7C', slacks: '#3A3F4B', charcoal: '#2C2F36' }[look.bottom];
  const badge = HEX.test(accent) ? accent : p.acc;
  return {
    skin, skinDark: shade(skin, 0.86), hair: p.hair, brow: shade(p.hair, 0.75), outfit: p.outfit, outfitDark: shade(p.outfit, 0.86), collar: '#F7F4EE', pants,
    shoe: SHOE_COLORS[look.shoe % SHOE_COLORS.length] ?? '#2B2B30', ink: '#2A2030', mouth: '#A8465A', cheek: '#FF9AA8', acc: p.acc, badge, gold: '#FFD166',
    headset: '#2B2F3C', folder: '#E8EAF0', frame: '#2E2B33', lanyard: badge, belt: '#4B3A2E', hat: p.acc,
  };
}
