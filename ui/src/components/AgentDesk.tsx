import type { AgentView } from '../protocol';
import { AgentAvatar } from './AgentAvatar';
import { MOOD_LABELS } from './Character';

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
      <p className="muted">{a.role || 'Chưa có vai trò'}</p>
      <span className="desk__mood">{MOOD_LABELS[a.mood]}</span>
      <p className="desk__activity">{a.act_text}</p>
      <p className="muted small">{a.source} · {a.host}</p>
      <div className="desk__context">
        <span>Context {context === null ? '— chưa có dữ liệu' : `${Math.round(context)}%`}</span>
        {context !== null && (
          <progress aria-label={`Context của ${a.name}`} value={context} max={100} />
        )}
      </div>
    </article>
  );
}
