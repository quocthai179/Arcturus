import { Vector3 } from 'three';
import { describe, expect, it } from 'vitest';
import { PET_MOODS } from '../types';
import { HANDS_UP, MOOD_POSES } from './moods';
import { ELBOW_MAX_DEG, armQuats, forwardKinematics, headQuat, measureRig, type RigPoints, type Side } from './pose';

// Rig tổng hợp: VRM1 quay mặt +Z (tay trái ở +X), VRM0 quay mặt −Z (tay trái ở −X), và một rig A-pose xoay lệch.
const vrm1: RigPoints = {
  leftUpperArm: new Vector3(0.13, 1.23, 0),
  rightUpperArm: new Vector3(-0.13, 1.23, 0),
  leftLowerArm: new Vector3(0.38, 1.23, 0),
  rightLowerArm: new Vector3(-0.38, 1.23, 0),
};
const vrm0: RigPoints = {
  leftUpperArm: new Vector3(-0.13, 1.23, 0),
  rightUpperArm: new Vector3(0.13, 1.23, 0),
  leftLowerArm: new Vector3(-0.38, 1.23, 0),
  rightLowerArm: new Vector3(0.38, 1.23, 0),
};
const aPose: RigPoints = {
  leftUpperArm: new Vector3(0.1, 1.3, 0.05),
  rightUpperArm: new Vector3(-0.1, 1.3, -0.05),
  leftLowerArm: new Vector3(0.31, 1.14, 0.08),
  rightLowerArm: new Vector3(-0.31, 1.14, -0.08),
};
const RIGS = { vrm1, vrm0, aPose };
const SIDES: Side[] = ['L', 'R'];

describe('measureRig', () => {
  it('tìm hướng mặt từ vị trí vai (VRM1 +Z, VRM0 −Z)', () => {
    expect(measureRig(vrm1).forward.z).toBeCloseTo(1);
    expect(measureRig(vrm0).forward.z).toBeCloseTo(-1);
  });

  it('gập dương quanh trục bản lề đưa cẳng tay ra trước', () => {
    for (const p of Object.values(RIGS)) {
      const rig = measureRig(p);
      for (const s of SIDES) {
        const moved = rig[s].d.clone().applyAxisAngle(rig[s].hinge, 0.5);
        expect(moved.dot(rig.forward) - rig[s].d.dot(rig.forward)).toBeGreaterThan(0.2);
      }
    }
  });
});

describe('armQuats: không bao giờ ngược tay', () => {
  for (const [name, p] of Object.entries(RIGS)) {
    const rig = measureRig(p);
    for (const mood of PET_MOODS) {
      for (const s of SIDES) {
        it(`${name} · ${mood} · tay ${s}`, () => {
          const a = MOOD_POSES[mood].arms[s];
          const m = forwardKinematics(rig, s, armQuats(rig, s, a));
          expect(m.flexDeg).toBeGreaterThanOrEqual(-0.5);
          expect(m.flexDeg).toBeCloseTo(Math.min(a.elbow, ELBOW_MAX_DEG), 0);
          expect(m.offDeg).toBeLessThan(1);
          // Hướng cánh tay trên đúng góc nâng mong muốn.
          expect(m.upper.dot(rig.up)).toBeCloseTo(Math.sin((a.elev * Math.PI) / 180), 3);
          // Bàn tay (đoạn trên 0.25 + cẳng tay 0.22 tính từ vai) cao/thấp hơn vai đúng trạng thái.
          const hand = m.upper.clone().multiplyScalar(0.25).addScaledVector(m.fore, 0.22);
          expect(hand.y > 0, `${mood} tay ${s}`).toBe(HANDS_UP[mood][s]);
        });
      }
    }
  }

  it('khuỷu âm bị kẹp về 0, khuỷu quá lớn bị kẹp về tối đa', () => {
    const rig = measureRig(vrm1);
    expect(forwardKinematics(rig, 'L', armQuats(rig, 'L', { elev: -70, az: 0, elbow: -40, hint: 'forward' })).flexDeg).toBeCloseTo(0, 1);
    expect(forwardKinematics(rig, 'R', armQuats(rig, 'R', { elev: -70, az: 0, elbow: 200, hint: 'forward' })).flexDeg).toBeCloseTo(
      ELBOW_MAX_DEG,
      1,
    );
  });

  it('hint "up" dựng cẳng tay lên, "forward" đưa cẳng tay ra trước', () => {
    for (const p of [vrm1, vrm0]) {
      const rig = measureRig(p);
      const up = forwardKinematics(rig, 'R', armQuats(rig, 'R', { elev: 0, az: 0, elbow: 90, hint: 'up' }));
      expect(up.fore.dot(rig.up)).toBeGreaterThan(0.95);
      const fwd = forwardKinematics(rig, 'R', armQuats(rig, 'R', { elev: -80, az: 0, elbow: 90, hint: 'forward' }));
      expect(fwd.fore.dot(rig.forward)).toBeGreaterThan(0.9);
    }
  });
});

describe('headQuat', () => {
  it('gật dương là cúi xuống ở cả VRM0 và VRM1', () => {
    for (const p of [vrm1, vrm0]) {
      const rig = measureRig(p);
      const look = rig.forward.clone().applyQuaternion(headQuat(rig, { nod: 20, roll: 0 }));
      expect(look.y).toBeLessThan(-0.3);
    }
  });
});
