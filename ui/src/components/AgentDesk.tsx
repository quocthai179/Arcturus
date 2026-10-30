import type { AgentView } from '../protocol';
import { AgentAvatar } from './AgentAvatar';
import { MOOD_LABELS } from './Character';
import { projectName } from '../text';

/** Vai trò mặc định khi chưa đặt persona (`crates/hub-core/src/persona.rs` `DEFAULT_ROLE`): không mang thông tin nên
 * không hiện. */
export const DEFAULT_ROLE = 'thành viên';

export function AgentDesk({ agent: a, onSelect }: { agent: AgentView; onSelect: (() => void) | undefined }) {
  const context = a.context_pct !== null && Number.isFinite(a.context_pct)
    ? Math.min(100, Math.max(0, a.context_pct)) : null;
  return (
    <article className={`desk desk--${a.mood}`} aria-label={`Bàn của ${a.name}`}>
      <div className="desk__avatar">
        <AgentAvatar mood={a.mood} palette={a.palette} size={88} label={`${a.name}: ${MOOD_LABELS[a.mood]}`} />
      </div>
      <h3>
        {onSelect ? (
          <button className="desk__select" onClick={onSelect} aria-label={`Xem yêu cầu của ${a.name}`}>
            {a.name}
          </button>
        ) : a.name}
      </h3>
      {a.role !== '' && a.role !== DEFAULT_ROLE && <p className="muted">{a.role}</p>}
      <span className="desk__mood">{MOOD_LABELS[a.mood]}</span>
      {/* "Đang rảnh" / "Đang rảnh": hoạt động trùng nhãn trạng thái thì không lặp lại. */}
      <p className="desk__activity">{a.act_text === MOOD_LABELS[a.mood] ? '' : a.act_text}</p>
      {projectName(a.cwd) !== '' && (
        <p className="muted small desk__project" title={a.cwd}>
          {projectName(a.cwd)}
        </p>
      )}
      {context !== null && (
        <div className="desk__context">
          <span>Context {Math.round(context)}%</span>
          <progress aria-label={`Context của ${a.name}`} value={context} max={100} />
        </div>
      )}
    </article>
  );
}
