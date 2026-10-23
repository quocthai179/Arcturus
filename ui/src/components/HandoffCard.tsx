// Thẻ báo "duyệt trong terminal" (lệnh quá dài, hub đã trả `{}`): KHÔNG có nút duyệt (docs/PROTOCOL.md).
import type { HandoffView } from '../protocol';
import { SUSPICION_LABELS } from './ApprovalCard';

export function HandoffCard({ handoff: h }: { handoff: HandoffView }) {
  return (
    <article className="card card--handoff" aria-label={`Duyệt trong terminal: ${h.agent_name}`}>
      <header className="card__head">
        <div className="card__who">
          <strong>{h.agent_name}</strong>
          <span className="muted">
            {h.tool} · {h.bytes.toLocaleString('vi-VN')} byte · {h.lines} dòng
          </span>
        </div>
        <span className="risk-badge risk-badge--terminal">Duyệt trong terminal</span>
      </header>
      {h.suspicions.length > 0 && (
        <ul className="flags" aria-label="Cảnh báo">
          {h.suspicions.map((s) => (
            <li key={s} className="flag">
              {SUSPICION_LABELS[s]}
            </li>
          ))}
        </ul>
      )}
      <p>{h.message}</p>
      <pre className="command" aria-label="Đầu lệnh">
        {h.preview_head}
      </pre>
      {h.preview_tail !== '' && (
        <>
          <p className="muted small">…</p>
          <pre className="command" aria-label="Cuối lệnh">
            {h.preview_tail}
          </pre>
        </>
      )}
    </article>
  );
}
