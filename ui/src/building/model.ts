// Hợp đồng vỏ DOM → canvas three (docs/OFFICE_BUILDING.md §11). Kiểu thuần, không import three. Canvas không biết `onSelect`
// hay `brief`: chỉ báo vật được bấm; vỏ là nơi duy nhất gọi `onSelect` (và chỉ sau thao tác của người dùng).
import type { AgentId } from '../protocol';
import type { PendingBrief } from '../sceneBrief';
import type { View } from './camera';
import type { AgentFloorId, FloorId } from './floors';
import type { SecMemory } from './secretary';
import type { FloorRoster } from './seating';

export type TimeMode = 'day' | 'dusk' | 'night';

export interface BuildingModel {
  view: View;
  /** Tầng 0…4. */
  rosters: readonly FloorRoster[];
  /** Canvas gọi `routePosAt`/`secretaryPose` để vẽ. */
  secretary: { mem: SecMemory; startAt: number };
  /** = `canSelect`, dùng cho sprite "!". */
  selectable: ReadonlySet<AgentId>;
  /** Tường trạng thái. */
  pending: ReadonlyMap<AgentId, PendingBrief>;
  timeMode: TimeMode;
}

export type PickHit =
  | { k: 'floor'; floor: FloorId }
  | { k: 'agent'; agentId: AgentId }
  | { k: 'bang'; agentId: AgentId }
  | { k: 'secretary' }
  | { k: 'status-screen'; floor: AgentFloorId };

export interface BuildingCanvasProps {
  model: BuildingModel;
  motion: boolean;
  onHover: (hit: PickHit | null, x: number, y: number) => void;
  /** Vỏ quyết: goTo / onSelect (chỉ khi agent ∈ selectable) / tóm tắt. */
  onPick: (hit: PickHit) => void;
  onReady: () => void;
  onError: (message: string) => void;
}

/** Giờ ngày/hoàng hôn/đêm theo giờ máy (không timer: tính lại khi vẽ do sự kiện). */
export function timeModeAt(date: Date): TimeMode {
  const h = date.getHours();
  if (h >= 7 && h < 17) return 'day';
  if (h >= 17 && h < 19) return 'dusk';
  return 'night';
}
