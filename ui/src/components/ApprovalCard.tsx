// Thẻ duyệt (docs/PLAN.md T12, quy tắc bất biến 3). Cơ sở duyệt là `command_normalized` hiện ĐẦY ĐỦ (không rút gọn,
// không cắt bằng CSS), render bằng text node. Mọi chuỗi từ agent là dữ liệu không tin cậy.
import { useState } from 'react';
import type { AgentView, ApprovalView, Decision, Risk, Suspicion } from '../protocol';
import { revealHidden, splitSpans } from '../text';
import { AgentAvatar } from './AgentAvatar';

export const RISK_LABELS: Record<Risk, string> = {
  low: 'Rủi ro thấp',
  medium: 'Rủi ro vừa',
  high: 'Rủi ro cao',
  critical: 'Vùng khóa',
};

export const SUSPICION_LABELS: Record<Suspicion, string> = {
  hidden_chars: 'Chứa ký tự ẩn',
  subshell: 'Có lệnh con $(…)',
  eval: 'Có eval/source',
  encoded: 'Có dữ liệu mã hóa',
};

export interface GateInput {
  connected: boolean;
  /** Lúc thẻ được hiện trên màn hình (ms); `undefined` khi chưa ghi nhận. */
  shownAt: number | undefined;
  now: number;
  minReviewMs: number;
  /** Đã mở bản đầy đủ (bắt buộc với thẻ `truncated`). */
  reviewed: boolean;
  /** Danh sách vừa mất một thẻ (thẻ bên dưới trượt lên): chưa cho Đồng ý trước thời điểm này (ms). */
  calmUntil?: number | undefined;
}

export interface Gate {
  ok: boolean;
  /** Lý do chưa cho Đồng ý (hiện cho người dùng). */
  reason: string | null;
}

/**
 * Điều kiện bật nút Đồng ý, khớp `approvals::allow_gate` của hub: sàn thời gian `min_review_ms` tính từ lúc **hiện** thẻ
 * (kể cả thẻ tới qua snapshot), và thẻ đã cắt phải mở bản đầy đủ trước. Từ chối luôn được.
 */
export function allowGate(a: ApprovalView, g: GateInput): Gate {
  if (!g.connected) return { ok: false, reason: 'Mất kết nối hub' };
  if (a.truncated && !g.reviewed) return { ok: false, reason: 'Cần mở bản đầy đủ trước khi duyệt' };
  if (g.calmUntil !== undefined && g.now < g.calmUntil) {
    return { ok: false, reason: 'Danh sách vừa đổi, xem lại thẻ rồi hãy duyệt' };
  }
  if (g.shownAt === undefined) return { ok: false, reason: 'Xem kỹ lệnh trước khi duyệt' };
  const left = g.minReviewMs - Math.max(0, g.now - g.shownAt);
  if (left > 0) return { ok: false, reason: `Xem kỹ lệnh trước khi duyệt (${Math.ceil(left / 1000)} s)` };
  return { ok: true, reason: null };
}

export interface ApprovalCardProps {
  approval: ApprovalView;
  agent: AgentView | undefined;
  selected: boolean;
  gate: Gate;
  now: number;
  onSelect: () => void;
  onDecide: (decision: Decision) => void;
  /** Lấy `tool_input` đầy đủ (`GET /approvals/{id}/input` qua phía Rust). */
  loadFull: (requestId: string) => Promise<string>;
  /** Gọi khi đã lấy được bản đầy đủ (mở khóa Đồng ý cho thẻ `truncated`). */
  onReviewed: () => void;
  /** Đếm ngược phủ quyết còn lại (ms), `null` khi thẻ không có (T14). Hết giờ thì UI tự gửi `origin: countdown`. */
  countdownLeftMs?: number | null;
}

type FullState = { kind: 'none' } | { kind: 'loading' } | { kind: 'ok'; text: string } | { kind: 'error'; text: string };

export function ApprovalCard({
  approval: a,
  agent,
  selected,
  gate,
  now,
  onSelect,
  onDecide,
  loadFull,
  onReviewed,
  countdownLeftMs = null,
}: ApprovalCardProps) {
  const [showOriginal, setShowOriginal] = useState(false);
  const [full, setFull] = useState<FullState>({ kind: 'none' });
  const pieces = splitSpans(a.command_normalized, a.danger_spans);
  const secondsLeft = Math.max(0, Math.ceil((Date.parse(a.deadline_at) - now) / 1000));
  const options = Object.entries(a.tool_options);

  const viewFull = async () => {
    setFull({ kind: 'loading' });
    try {
      const text = await loadFull(a.request_id);
      setFull({ kind: 'ok', text });
      onReviewed();
    } catch (e) {
      setFull({ kind: 'error', text: e instanceof Error ? e.message : 'Không lấy được bản đầy đủ' });
    }
  };

  return (
    <article
      className={`card risk-${a.risk}${selected ? ' card--selected' : ''}`}
      aria-label={`Yêu cầu của ${a.agent_name}: ${a.tool}`}
      aria-current={selected ? 'true' : undefined}
      data-request-id={a.request_id}
      tabIndex={-1}
      onClick={onSelect}
    >
      <header className="card__head">
        <AgentAvatar
          mood="approval"
          palette={agent?.palette}
          size={40}
          label={`${a.agent_name} đang xin duyệt`}
        />
        <div className="card__who">
          <strong>{a.agent_name}</strong>
          {agent !== undefined && (
            <span className="muted">
              {agent.host} · {agent.source}
            </span>
          )}
        </div>
        <span className={`risk-badge risk-badge--${a.risk}`}>{RISK_LABELS[a.risk]}</span>
      </header>

      <div className="card__meta">
        <span className="tool">{a.tool}</span>
        <span className="muted">còn {secondsLeft} s</span>
      </div>

      {a.suspicions.length > 0 && (
        <ul className="flags" aria-label="Cảnh báo">
          {a.suspicions.map((s) => (
            <li key={s} className="flag">
              {SUSPICION_LABELS[s]}
            </li>
          ))}
        </ul>
      )}

      {options.length > 0 && (
        <dl className="options" aria-label="Tùy chọn chạy lệnh">
          {options.map(([k, v]) => (
            <div key={k} className={k === 'dangerouslyDisableSandbox' && v === 'true' ? 'option option--danger' : 'option'}>
              <dt>{k}</dt>
              <dd>{v}</dd>
            </div>
          ))}
        </dl>
      )}

      <pre className="command" aria-label="Lệnh cần duyệt">
        {pieces.map((p, i) =>
          p.danger ? (
            <mark key={i} className="danger">
              {p.text}
            </mark>
          ) : (
            <span key={i}>{p.text}</span>
          ),
        )}
      </pre>

      {a.suspicions.includes('hidden_chars') && (
        <div className="extra">
          <button type="button" className="link" onClick={() => setShowOriginal((v) => !v)}>
            {showOriginal ? 'Ẩn bản gốc' : 'Xem bản gốc'}
          </button>
          {showOriginal && (
            <pre className="command command--original" aria-label="Bản gốc, ký tự ẩn hiện bằng mã">
              {revealHidden(a.command_original)}
            </pre>
          )}
        </div>
      )}

      {a.truncated && (
        <div className="extra">
          <span className="muted">Nội dung đã cắt</span>{' '}
          <button
            type="button"
            className="link"
            disabled={full.kind === 'loading'}
            onClick={() => void viewFull()}
          >
            Xem đầy đủ
          </button>
          {full.kind === 'ok' && (
            <pre className="command command--full" aria-label="Bản đầy đủ">
              {revealHidden(full.text)}
            </pre>
          )}
          {full.kind === 'error' && <p className="error">{full.text}</p>}
        </div>
      )}

      {countdownLeftMs !== null && (
        <p className="countdown">
          {countdownLeftMs > 0
            ? `Tự chạy sau ${Math.ceil(countdownLeftMs / 1000)} s — bấm "Dừng lại" để hủy`
            : 'Hết giờ đếm ngược: đang chờ bạn bấm'}
        </p>
      )}

      <p className="muted small">Cũng đang hỏi trong terminal của agent.</p>

      <footer className="card__actions">
        <button type="button" className="btn btn--deny" onClick={() => onDecide('deny')}>
          {countdownLeftMs !== null ? 'Dừng lại' : 'Từ chối'} <kbd>Esc</kbd>
        </button>
        <button
          type="button"
          className="btn btn--allow"
          disabled={!gate.ok}
          title={gate.reason ?? undefined}
          onClick={() => gate.ok && onDecide('allow')}
        >
          {countdownLeftMs !== null ? 'Chạy ngay' : 'Đồng ý'} <kbd>Enter</kbd>
        </button>
      </footer>
      {gate.reason !== null && <p className="muted small gate-reason">{gate.reason}</p>}
    </article>
  );
}
