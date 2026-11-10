// Hình học từng phần của nhân vật (art pass 2026-11-06). Mỗi phần là MỘT geometry dùng chung cho mọi instance; màu từng người đi qua
// `instanceColor`, còn màu đỉnh ở đây chỉ là hệ số tối/sáng (AO giả ở chân tóc, gấu áo…). Hình tùy biến gộp nhiều khối nhỏ bằng `Batch`.
import { BufferGeometry, CapsuleGeometry, CylinderGeometry, SphereGeometry, TorusGeometry } from 'three';
import { capsuleCylinderLength, PART_SPECS, type PartId } from '../figure';
import { Batch } from './kit';

const W = '#FFFFFF';
const TILT = 0.5;

function hairCap(): BufferGeometry {
  return new Batch().geo(new SphereGeometry(0.205, 22, 12, 0, Math.PI * 2, 0, 1.5), 0, 0, 0, W, { rx: TILT, ao: 0.28 }).geometry();
}

function fringe(): BufferGeometry {
  return new Batch()
    .sph(1, 0.035, 0.118, -0.125, W, { sx: 0.13, sy: 0.05, sz: 0.075, rz: -0.25 }, 1, 10)
    .sph(1, -0.075, 0.108, -0.13, W, { sx: 0.1, sy: 0.045, sz: 0.07, rz: 0.3 }, 1, 10)
    .geometry();
}

function hairBob(): BufferGeometry {
  const b = new Batch();
  for (const s of [-1, 1]) b.sph(1, s * 0.168, -0.035, 0.03, W, { sx: 0.06, sy: 0.15, sz: 0.14, ao: 0.25 }, 1, 10);
  return b.sph(1, 0, -0.03, 0.1, W, { sx: 0.19, sy: 0.15, sz: 0.1, ao: 0.25 }, 1, 10).geometry();
}

function hairLong(): BufferGeometry {
  const b = new Batch();
  for (const s of [-1, 1]) b.sph(1, s * 0.172, -0.1, 0.03, W, { sx: 0.045, sy: 0.27, sz: 0.12, ao: 0.3 }, 1, 10);
  return b.sph(1, 0, -0.2, 0.1, W, { sx: 0.2, sy: 0.34, sz: 0.1, ao: 0.3 }, 1, 10).geometry();
}

function hairCurly(): BufferGeometry {
  const b = new Batch();
  b.sph(1, 0, 0.09, 0.06, W, { sx: 0.225, sy: 0.17, sz: 0.2 }, 1, 12);
  for (let i = 0; i < 12; i++) {
    const a = (i / 12) * Math.PI * 2;
    // góc 0 = phía trước (−z): chừa mặt, chỉ gắn búp tóc ở hai bên và sau
    if (Math.abs(Math.atan2(Math.sin(a), Math.cos(a))) < 0.9) continue;
    b.sph(0.09, Math.sin(a) * 0.19, 0.0 + (i % 2) * 0.06, 0.04 + Math.cos(a) * 0.19, W, { ao: 0.2 }, 1, 8);
  }
  for (const [x, y, z] of [[-0.09, 0.2, 0.0], [0.09, 0.2, 0.0], [0, 0.22, 0.07], [-0.075, 0.165, -0.105], [0.075, 0.165, -0.105], [0, 0.185, -0.1]] as const) b.sph(0.085, x, y, z, W, {}, 1, 8);
  return b.geometry();
}

function hairPony(): BufferGeometry {
  return new Batch()
    .sph(0.032, 0, 0.07, 0.195, '#E0E0E0', {}, 1, 8)
    .link([0, 0.07, 0.2], [0, -0.16, 0.3], 0.05, W, { ao: 0 }, 8, 0.022)
    .sph(0.028, 0, -0.17, 0.305, W, {}, 1, 8)
    .geometry();
}

function beanie(): BufferGeometry {
  return new Batch()
    .geo(new SphereGeometry(0.22, 22, 12, 0, Math.PI * 2, 0, 1.45), 0, 0, 0, W, { rx: TILT })
    .geo(new TorusGeometry(0.205, 0.034, 8, 28), 0, 0.027, 0.014, '#E6E6E6', { rx: -(Math.PI / 2 - TILT) })
    .sph(0.05, 0, 0.215, 0.118, '#F2F2F2', {}, 1, 8)
    .geometry();
}

function glasses(): BufferGeometry {
  const b = new Batch();
  for (const s of [-1, 1]) {
    b.geo(new TorusGeometry(0.05, 0.0075, 6, 20), s * 0.068, 0.014, -0.18, W, {});
    b.link([s * 0.118, 0.016, -0.172], [s * 0.178, 0.012, -0.04], 0.006, W, {}, 4);
  }
  return b.box(0.03, 0.008, 0.008, 0, 0.026, -0.182, W).geometry();
}

function headphones(): BufferGeometry {
  const b = new Batch().geo(new TorusGeometry(0.208, 0.013, 6, 26, Math.PI), 0, 0, 0.005, W, {});
  for (const s of [-1, 1]) b.cyl(0.062, 0.062, 0.045, s * 0.208, -0.01, 0.005, W, { rz: Math.PI / 2 }, 14);
  return b.geometry();
}

function headset(): BufferGeometry {
  const b = new Batch().geo(new TorusGeometry(0.205, 0.008, 6, 26, Math.PI), 0, 0, 0.005, W, {});
  for (const s of [-1, 1]) b.cyl(0.048, 0.048, 0.035, s * 0.205, -0.01, 0.005, W, { rz: Math.PI / 2 }, 12);
  return b.link([0.205, -0.015, 0.0], [0.17, -0.09, -0.13], 0.006, W, {}, 5).sph(0.015, 0.165, -0.095, -0.14, '#8A8F9C', {}, 1, 6).geometry();
}

function lanyard(): BufferGeometry {
  const b = new Batch();
  for (const s of [-1, 1]) {
    b.link([s * 0.062, 0.16, 0.094], [s * 0.036, 0.08, 0.016], 0.006, W, {}, 4);
    b.link([s * 0.036, 0.08, 0.016], [0, 0.03, 0.0], 0.006, W, {}, 4);
  }
  return b.geometry();
}

function collar(): BufferGeometry {
  const b = new Batch().cyl(0.074, 0.078, 0.034, 0, 0, 0.01, W, {}, 12);
  for (const s of [-1, 1]) b.rbox(0.1, 0.012, 0.075, s * 0.05, -0.006, -0.07, W, { ry: -s * 0.55, rz: s * 0.3 }, 0.005);
  return b.geometry();
}

function hood(): BufferGeometry {
  return new Batch()
    .geo(new TorusGeometry(0.09, 0.035, 8, 20), 0, -0.01, 0, W, { rx: Math.PI / 2 })
    .sph(1, 0, -0.05, 0.1, W, { sx: 0.1, sy: 0.08, sz: 0.06 }, 1, 8)
    .geometry();
}

function shoe(): BufferGeometry {
  return new Batch().sph(0.06, 0, 0, 0, W, { sx: 0.95, sy: 0.66, sz: 1.75, ao: 0.35 }, 1, 10).geometry();
}

function clipboard(): BufferGeometry {
  return new Batch()
    .rbox(0.2, 0.28, 0.022, 0, 0, 0, W, {}, 0.008)
    .box(0.09, 0.035, 0.03, 0, 0.14, -0.004, '#6A6F7C')
    .rbox(0.15, 0.2, 0.004, 0, -0.015, -0.013, '#FFFFFF', {}, 0.001)
    .geometry();
}

function smile(): BufferGeometry {
  return new Batch().geo(new TorusGeometry(0.032, 0.0065, 6, 14, Math.PI), 0, 0, 0, W, {}).geometry();
}

const CUSTOM: Record<string, () => BufferGeometry> = {
  hairCap, fringe, hairBob, hairLong, hairCurly, hairPony, beanie, glasses, headphones, headset, lanyard, collar, hood, shoe, clipboard, smile,
};

function primitive(p: PartId): BufferGeometry {
  const s = PART_SPECS[p].shape;
  const b = new Batch();
  switch (s.k) {
    case 'sphere': return b.sph(s.r, 0, 0, 0, W, {}, 1, 14).geometry();
    case 'box': return b.box(s.w, s.h, s.d, 0, 0, 0, W).geometry();
    case 'capsule': return b.geo(new CapsuleGeometry(s.r, capsuleCylinderLength(p), 4, 10), 0, 0, 0, W, { ao: p === 'torso' ? 0.22 : 0 }).geometry();
    case 'cylinder': return b.geo(new CylinderGeometry(s.r, s.r, s.h, 14), 0, 0, 0, W).geometry();
    case 'custom': return (CUSTOM[s.name] ?? (() => new BufferGeometry()))();
  }
}

/** Geometry dùng chung của một phần (người gọi đăng ký vào `Registry`). Luôn có `position`, `normal`, `color`. */
export function partGeometry(p: PartId): BufferGeometry {
  return primitive(p);
}

/** Phần nào đổ bóng lên sàn: chỉ các khối lớn (phần nhỏ chỉ tốn draw call trong lượt bóng). */
export const CASTS_SHADOW: ReadonlySet<PartId> = new Set<PartId>(['head', 'torso', 'armL', 'armR', 'foreL', 'foreR', 'thighL', 'thighR', 'shinL', 'shinR', 'hairCap', 'hairLong', 'hairBob', 'hairCurly', 'beanie']);
