// Điểm thay thế duy nhất cho "cảnh" của Văn phòng (docs/UI_EXTENSION.md). `Office` chỉ chọn cảnh trong `OFFICE_SCENES` và
// đưa dữ liệu + callback; cảnh chỉ vẽ. Cảnh mới (isometric, 3D) = thêm một khóa, không sửa `Office`.
//
// Hợp đồng: cảnh KHÔNG đọc store/WebSocket, không gửi lệnh; chọn một agent đang giơ tay thì gọi `onSelect(agentId)`
// (Office lo cuộn tới thẻ duyệt). Hàng đợi duyệt không thuộc cảnh: thẻ duyệt luôn ở cột riêng để duyệt không phụ thuộc
// cảnh có tải được hay không (cảnh 3D lỗi thì vẫn duyệt được).
import type { ComponentType } from 'react';
import { BuildingScene } from '../building/BuildingScene';
import { LiveOfficeScene } from '../live/LiveOfficeScene';
import { DeskGridScene, type OfficeSceneProps } from './DeskGridScene';

export { DeskGridScene, type OfficeSceneProps };

/**
 * `live`: văn phòng sống có hoạt ảnh (docs/OFFICE_LIVE.md); `grid`: lưới thẻ tĩnh (T13); `building`: tòa nhà 3D 6 tầng có
 * thư ký trưởng (docs/OFFICE_BUILDING.md, opt-in: three.js nạp muộn, chưa đo ngân sách nên không phải mặc định).
 */
export type SceneId = 'live' | 'grid' | 'building';

export const DEFAULT_SCENE: SceneId = 'live';

export const SCENE_LABELS: Record<SceneId, string> = { live: 'Sống', grid: 'Lưới', building: 'Tòa nhà' };

export const OFFICE_SCENES: Record<SceneId, ComponentType<OfficeSceneProps>> = {
  live: LiveOfficeScene,
  grid: DeskGridScene,
  building: BuildingScene,
};

export function isSceneId(value: unknown): value is SceneId {
  return value === 'live' || value === 'grid' || value === 'building';
}
