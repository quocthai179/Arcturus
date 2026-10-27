// Tư thế và biểu cảm theo trạng thái của pet (docs/PLAN.md §7.5). Chỉ dữ liệu: góc theo giải phẫu (pose.ts), không góc xương.
import type { PetMood } from '../types';
import type { ArmPose, HeadPose } from './pose';

export type ExpressionName = 'happy' | 'sad' | 'surprised' | 'relaxed' | 'angry' | 'blink';
export const EXPRESSIONS: readonly ExpressionName[] = ['happy', 'sad', 'surprised', 'relaxed', 'angry', 'blink'];

export interface MoodPose {
  arms: { L: ArmPose; R: ArmPose };
  head: HeadPose;
  expr: Partial<Record<ExpressionName, number>>;
  /** Tay vẫy (chỉ trong khoảng hoạt động ngắn sau khi đổi trạng thái). */
  wave?: 'L' | 'R';
}

const rest: ArmPose = { elev: -72, az: 8, elbow: 12, hint: 'forward' };

export const MOOD_POSES: Record<PetMood, MoodPose> = {
  idle: { arms: { L: rest, R: rest }, head: { nod: 2, roll: 3 }, expr: { relaxed: 0.35 } },
  working: {
    arms: { L: { elev: -55, az: 40, elbow: 80, hint: 'forward' }, R: { elev: -55, az: 40, elbow: 80, hint: 'forward' } },
    head: { nod: 14, roll: 0 },
    expr: { relaxed: 0.1 },
  },
  approval: {
    arms: { L: rest, R: { elev: 25, az: 15, elbow: 95, hint: 'up' } },
    head: { nod: -3, roll: -5 },
    expr: { surprised: 0.3, happy: 0.35 },
    wave: 'R',
  },
  critical: {
    arms: { L: rest, R: { elev: 10, az: 30, elbow: 100, hint: 'up' } },
    head: { nod: -2, roll: 0 },
    expr: { surprised: 0.2, angry: 0.25 },
  },
  done: {
    arms: { L: { elev: 50, az: 15, elbow: 35, hint: 'up' }, R: { elev: 50, az: 15, elbow: 35, hint: 'up' } },
    head: { nod: -4, roll: 3 },
    expr: { happy: 0.9 },
  },
  error: {
    arms: { L: { elev: -62, az: 45, elbow: 105, hint: 'in' }, R: { elev: -62, az: 45, elbow: 105, hint: 'in' } },
    head: { nod: 10, roll: -6 },
    expr: { sad: 0.75 },
  },
  paused: {
    arms: { L: { elev: -66, az: 30, elbow: 60, hint: 'in' }, R: { elev: -66, az: 30, elbow: 60, hint: 'in' } },
    head: { nod: 4, roll: 0 },
    expr: { relaxed: 0.5 },
  },
  sleep: { arms: { L: { ...rest, elev: -80, elbow: 6 }, R: { ...rest, elev: -80, elbow: 6 } }, head: { nod: 18, roll: 12 }, expr: { relaxed: 0.6, blink: 1 } },
};

/** Tay nào phải cao hơn vai trong từng trạng thái (kiểm tự động). */
export const HANDS_UP: Record<PetMood, { L: boolean; R: boolean }> = {
  idle: { L: false, R: false },
  working: { L: false, R: false },
  approval: { L: false, R: true },
  critical: { L: false, R: true },
  done: { L: true, R: true },
  error: { L: false, R: false },
  paused: { L: false, R: false },
  sleep: { L: false, R: false },
};
