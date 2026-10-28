// Tạo dáng tay và đầu theo giải phẫu (docs/PLAN.md §7.10). Không viết góc xương thô: tư thế mô tả bằng góc nâng, hướng quanh
// thân, độ gập khuỷu và hướng gập; trục lấy từ chính model lúc nạp, nên đúng cho cả VRM0 (mặt về −Z) và VRM1 (mặt về +Z).
//
// Khung tham chiếu là khung của xương chuẩn hóa (normalized humanoid) của three-vrm: lúc nghỉ mọi xương có quaternion đơn vị
// và trục thẳng hàng với gốc rig, nên một quaternion cục bộ của cánh tay trên cũng là phép quay trong khung thân.
import { MathUtils, Quaternion, Vector3 } from 'three';

export type Side = 'L' | 'R';
export type BendHint = 'forward' | 'up' | 'in';

export interface ArmPose {
  /** Góc nâng cánh tay so với mặt phẳng ngang (độ): −90 buông thẳng, 0 dang ngang, 90 giơ thẳng. */
  elev: number;
  /** Hướng quanh thân (độ): 0 sang ngang, 90 ra trước. */
  az: number;
  /** Độ gập khuỷu (độ), bị kẹp vào [0, ELBOW_MAX_DEG]: không bao giờ gập ngược. */
  elbow: number;
  /** Hướng cẳng tay đi tới khi gập. */
  hint: BendHint;
}

export interface HeadPose {
  /** Gật (độ), dương = cúi. */
  nod: number;
  /** Nghiêng (độ). */
  roll: number;
}

export interface ArmRig {
  /** Hướng cánh tay lúc nghỉ (vai → khuỷu). */
  d: Vector3;
  /** Trục bản lề khuỷu: quay dương quanh trục này thì cẳng tay đi ra trước. */
  hinge: Vector3;
  /** Hướng ra phía bên đó của thân. */
  lateral: Vector3;
}

export interface Rig {
  up: Vector3;
  forward: Vector3;
  /** Phía tay trái của nhân vật. */
  lateralL: Vector3;
  L: ArmRig;
  R: ArmRig;
}

/** Vị trí lúc nghỉ của các khớp trong khung rig. */
export interface RigPoints {
  leftUpperArm: Vector3;
  rightUpperArm: Vector3;
  leftLowerArm: Vector3;
  rightLowerArm: Vector3;
}

export const ELBOW_MAX_DEG = 145;
export const TWIST_MAX_DEG = 110;
const D = MathUtils.degToRad;

export function measureRig(p: RigPoints): Rig {
  const up = new Vector3(0, 1, 0);
  const lateralL = p.leftUpperArm.clone().sub(p.rightUpperArm).setY(0).normalize();
  const forward = new Vector3().crossVectors(lateralL, up).normalize();
  const side = (upper: Vector3, lower: Vector3, lateral: Vector3): ArmRig => {
    const d = lower.clone().sub(upper).normalize();
    return { d, hinge: new Vector3().crossVectors(d, forward).normalize(), lateral };
  };
  return {
    up,
    forward,
    lateralL,
    L: side(p.leftUpperArm, p.leftLowerArm, lateralL.clone()),
    R: side(p.rightUpperArm, p.rightLowerArm, lateralL.clone().negate()),
  };
}

function hintVector(rig: Rig, s: ArmRig, hint: BendHint): Vector3 {
  if (hint === 'up') return rig.up.clone();
  if (hint === 'in') return s.lateral.clone().negate();
  return rig.forward.clone();
}

/** Hướng đích của cánh tay trên trong khung thân. */
export function armTarget(rig: Rig, side: Side, a: ArmPose): Vector3 {
  const s = rig[side];
  const e = D(a.elev);
  const az = D(a.az);
  return s.lateral
    .clone()
    .multiplyScalar(Math.cos(e) * Math.cos(az))
    .addScaledVector(rig.forward, Math.cos(e) * Math.sin(az))
    .addScaledVector(rig.up, Math.sin(e))
    .normalize();
}

/**
 * Swing–twist: quay hướng nghỉ tới hướng đích (swing), rồi xoay quanh trục cánh tay (twist) để mặt phẳng bản lề khuỷu hướng
 * về `hint`; khuỷu chỉ quay quanh trục bản lề, góc kẹp trong [0, ELBOW_MAX_DEG].
 */
export function armQuats(rig: Rig, side: Side, a: ArmPose): { upper: Quaternion; lower: Quaternion } {
  const s = rig[side];
  const t = armTarget(rig, side, a);
  const swing = new Quaternion().setFromUnitVectors(s.d, t);
  // Quay quanh trục h thì vectơ t đi theo h × t: đó là hướng cẳng tay đi khi gập.
  const flexDir = s.hinge.clone().applyQuaternion(swing).cross(t);
  const want = hintVector(rig, s, a.hint);
  want.addScaledVector(t, -want.dot(t));
  let twist = 0;
  if (want.lengthSq() > 1e-6 && flexDir.lengthSq() > 1e-9) {
    want.normalize();
    flexDir.normalize();
    twist = Math.atan2(new Vector3().crossVectors(flexDir, want).dot(t), flexDir.dot(want));
    twist = MathUtils.clamp(twist, D(-TWIST_MAX_DEG), D(TWIST_MAX_DEG));
  }
  const upper = new Quaternion().setFromAxisAngle(t, twist).multiply(swing);
  const lower = new Quaternion().setFromAxisAngle(s.hinge, MathUtils.clamp(D(a.elbow), 0, D(ELBOW_MAX_DEG)));
  return { upper, lower };
}

export function headQuat(rig: Rig, h: HeadPose): Quaternion {
  return new Quaternion()
    .setFromAxisAngle(rig.lateralL, D(h.nod))
    .multiply(new Quaternion().setFromAxisAngle(rig.forward, D(h.roll)));
}

export interface ElbowMetrics {
  /** Độ gập khuỷu có dấu theo trục bản lề của chính cánh tay (âm = gập ngược). */
  flexDeg: number;
  /** Góc cẳng tay rời khỏi mặt phẳng bản lề (lớn = khuỷu bẻ ngang). */
  offDeg: number;
  upper: Vector3;
  fore: Vector3;
}

/** Đo khuỷu từ hai hướng (cánh tay trên, cẳng tay) và trục bản lề hiện tại. */
export function elbowMetrics(upper: Vector3, fore: Vector3, hinge: Vector3): ElbowMetrics {
  const u = upper.clone().normalize();
  const f = fore.clone().normalize();
  const h = hinge.clone().normalize();
  const flexDeg = MathUtils.radToDeg(Math.atan2(new Vector3().crossVectors(u, f).dot(h), u.dot(f)));
  const offDeg = MathUtils.radToDeg(Math.asin(Math.min(1, Math.abs(f.dot(h)))));
  return { flexDeg, offDeg, upper: u, fore: f };
}

/** Động học thuận trong khung thân (cẳng tay lúc nghỉ thẳng hàng với cánh tay trên): dùng cho test và kiểm tự động. */
export function forwardKinematics(rig: Rig, side: Side, q: { upper: Quaternion; lower: Quaternion }): ElbowMetrics {
  const s = rig[side];
  const upper = s.d.clone().applyQuaternion(q.upper);
  const fore = s.d.clone().applyQuaternion(q.lower).applyQuaternion(q.upper);
  const hinge = s.hinge.clone().applyQuaternion(q.upper);
  return elbowMetrics(upper, fore, hinge);
}
