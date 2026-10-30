// Cảnh lưới bàn (T13): mỗi agent một thẻ. Cũng là phần thẻ chi tiết nằm dưới sàn của cảnh văn phòng sống.
import type { AgentView } from '../protocol';
import { AgentDesk } from './AgentDesk';

export interface OfficeSceneProps {
  /** Đội hình chính (đã sắp xếp), mỗi agent một bàn. */
  desks: readonly AgentView[];
  /** Agent rảnh vượt đội hình chính (phòng hỗ trợ). */
  support: readonly AgentView[];
  /** Agent này có thẻ duyệt/handoff để nhảy tới không. */
  canSelect: (agent: AgentView) => boolean;
  onSelect: (agentId: string) => void;
  /** Được phép chuyển động (trang hiện, không giảm chuyển động; `useMotionPolicy`). Mặc định `true`. */
  motion?: boolean;
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
