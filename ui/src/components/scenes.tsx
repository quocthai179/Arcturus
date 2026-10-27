// Điểm thay thế duy nhất cho "cảnh" của Văn phòng (docs/UI_EXTENSION.md). `Office` chỉ chọn cảnh trong `OFFICE_SCENES` và
// đưa dữ liệu + callback; cảnh chỉ vẽ. Cảnh mới (văn phòng động, isometric, 3D) = thêm một khóa, không sửa `Office`.
//
// Hợp đồng: cảnh KHÔNG đọc store/WebSocket, không gửi lệnh; chọn một agent đang giơ tay thì gọi `onSelect(agentId)`
// (Office lo cuộn tới thẻ duyệt). Hàng đợi duyệt không thuộc cảnh: thẻ duyệt luôn ở cột riêng để duyệt không phụ thuộc
// cảnh có tải được hay không (cảnh 3D lỗi thì vẫn duyệt được).
import type { ComponentType } from 'react';
import type { AgentView } from '../protocol';
import { AgentDesk } from './AgentDesk';

export type SceneId = 'grid';

export const DEFAULT_SCENE: SceneId = 'grid';

export interface OfficeSceneProps {
  /** Đội hình chính (đã sắp xếp), mỗi agent một bàn. */
  desks: readonly AgentView[];
  /** Agent rảnh vượt đội hình chính (phòng hỗ trợ). */
  support: readonly AgentView[];
  /** Agent này có thẻ duyệt/handoff để nhảy tới không. */
  canSelect: (agent: AgentView) => boolean;
  onSelect: (agentId: string) => void;
}

const HEX = /^#[0-9a-fA-F]{6}$/;
const SUPPORT_FALLBACK = '#8B7CF6';

export function DeskGridScene({ desks, support, canSelect, onSelect }: OfficeSceneProps) {
  return (
    <section aria-labelledby="desks-heading">
      <h2 id="desks-heading">Đội hình chính</h2>
      {desks.length === 0 && <p className="muted">Chưa có agent nào. Các phiên kết nối hub sẽ xuất hiện ở đây.</p>}
      <div className="office__desks">
        {desks.map((a) => (
          <AgentDesk key={a.id} agent={a} onSelect={canSelect(a) ? () => onSelect(a.id) : undefined} />
        ))}
        {support.length > 0 && (
          <article className="desk support" aria-label="Phòng hỗ trợ">
            <h3>Phòng hỗ trợ</h3>
            <p>{support.length} agent đang rảnh</p>
            <ul className="support__agents">
              {support.map((a) => (
                <li key={a.id}>
                  <span
                    className="support__dot"
                    style={{ backgroundColor: HEX.test(a.palette.outfit) ? a.palette.outfit : SUPPORT_FALLBACK }}
                    aria-hidden="true"
                  />
                  {a.name}
                </li>
              ))}
            </ul>
          </article>
        )}
      </div>
    </section>
  );
}

export const OFFICE_SCENES: Record<SceneId, ComponentType<OfficeSceneProps>> = {
  grid: DeskGridScene,
};
