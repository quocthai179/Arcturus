// Chính sách giữ nội thất theo tầng (docs/OFFICE_BUILDING.md §9.2): giữ Phòng CEO, tầng đang xem (luôn luôn, kể cả khi đang
// chuyển cảnh) và tối đa `KEEP_FLOORS` tầng xem gần nhất. Thuần để test được; `three/BuildingCanvas` chỉ áp kết quả.
import type { View } from './camera';
import { CEO_FLOOR, type FloorId } from './floors';

export const KEEP_FLOORS = 2;

export interface PrunePlan {
  recent: FloorId[];
  dispose: FloorId[];
}

/** `navigatedTo`: tầng vừa chuyển tới; `current`: góc nhìn hiện hành của model (có thể khác nếu người dùng bấm tiếp). */
export function planPrune(built: Iterable<FloorId>, recent: readonly FloorId[], navigatedTo: View, current: View): PrunePlan {
  const next = typeof navigatedTo === 'number' && navigatedTo !== CEO_FLOOR ? [navigatedTo, ...recent.filter((x) => x !== navigatedTo)].slice(0, KEEP_FLOORS) : [...recent];
  const dispose: FloorId[] = [];
  for (const f of built) {
    if (f === CEO_FLOOR || f === current || f === navigatedTo || next.includes(f)) continue;
    dispose.push(f);
  }
  return { recent: next, dispose };
}
