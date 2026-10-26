// Popup Lite (docs/PLAN.md T12): hàng đợi duyệt, duyệt/từ chối, duyệt gộp, phím tắt trong popup (↑/↓ chọn, Enter đồng
// ý, Esc từ chối). Đóng popup thì app hủy cả webview, nên không có gì cần dọn ngoài socket.
import { useEffect, useRef } from 'react';
import { Character } from '../components/Character';
import { QueueList } from '../components/QueueList';
import { approvalInput } from '../hub';
import type { ClientMsg } from '../protocol';
import type { HubState } from '../store';
import { DEFAULT_MIN_REVIEW_MS, useApprovalQueue } from '../useApprovalQueue';
import { useHubConnection } from '../useHubConnection';
export { DEFAULT_MIN_REVIEW_MS } from '../useApprovalQueue';

export interface LitePopupViewProps {
  state: HubState;
  send: (msg: ClientMsg) => boolean;
  loadFull: (requestId: string) => Promise<string>;
  minReviewMs: number;
}

export function LitePopupView({ state, send, loadFull, minReviewMs }: LitePopupViewProps) {
  const { queue, handoffs, agents, team, feed } = state;
  const { now, selected, setSelectedId, gateOf, decide, decideGroup, setReviewed, notice } =
    useApprovalQueue(state, send, minReviewMs);

  // Phím tắt chỉ trong popup (cửa sổ riêng, không đăng ký phím toàn cục).
  const latest = useRef({ queue, selected, gateOf, decide });
  latest.current = { queue, selected, gateOf, decide };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const { queue: q, selected: sel, gateOf: gate, decide: act } = latest.current;
      const target = e.target;
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return;
      if (sel === undefined) return;
      const i = q.findIndex((a) => a.request_id === sel.request_id);
      switch (e.key) {
        case 'ArrowDown': {
          const next = q[Math.min(q.length - 1, i + 1)];
          if (next !== undefined) setSelectedId(next.request_id);
          e.preventDefault();
          break;
        }
        case 'ArrowUp': {
          const prev = q[Math.max(0, i - 1)];
          if (prev !== undefined) setSelectedId(prev.request_id);
          e.preventDefault();
          break;
        }
        case 'Enter':
          // Nút đang có focus tự xử lý Enter của nó.
          if (target instanceof HTMLButtonElement) return;
          if (gate(sel).ok) act(sel.request_id, 'allow');
          e.preventDefault();
          break;
        case 'Escape':
          act(sel.request_id, 'deny');
          e.preventDefault();
          break;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const lastFeed = feed[0];
  const teamAgents = Object.values(agents).filter((a) => a.parent_id === null);

  return (
    <main className="popup">
      <header className="popup__head">
        <h1>Arcturus</h1>
        <span className="muted" role="status">
          {queue.length > 0 ? `${queue.length} yêu cầu đang chờ` : 'Không có yêu cầu nào đang chờ'}
        </span>
      </header>

      {state.connection !== 'open' && (
        <p className="banner banner--error" role="alert">
          {state.error ?? 'Đang kết nối hub…'}
        </p>
      )}
      {team.paused && <p className="banner banner--paused">Đội đang tạm dừng: không có gì được tự duyệt.</p>}
      {notice !== null && <p className="banner banner--error">{notice}</p>}
      {lastFeed !== undefined && (
        <p className="banner" role="status" aria-live="polite">
          {lastFeed.text}
        </p>
      )}

      <QueueList
        queue={queue}
        handoffs={handoffs}
        agents={agents}
        selectedId={selected?.request_id ?? null}
        now={now}
        gateOf={gateOf}
        onSelect={setSelectedId}
        onDecide={decide}
        onDecideGroup={decideGroup}
        loadFull={loadFull}
        onReviewed={(id) => setReviewed((r) => ({ ...r, [id]: true }))}
      />

      {teamAgents.length > 0 && (
        <section className="team" aria-label="Đội">
          {teamAgents.map((a) => (
            <div key={a.id} className="team__member">
              <Character
                mood={a.mood}
                width={28}
                hair={a.palette.hair}
                outfit={a.palette.outfit}
                acc={a.palette.acc}
                label={a.name}
              />
              <span className="team__name">{a.name}</span>
              <span className="muted team__act">{a.act_text}</span>
            </div>
          ))}
        </section>
      )}
    </main>
  );
}

export function LitePopup({ minReviewMs = DEFAULT_MIN_REVIEW_MS }: { minReviewMs?: number }) {
  const { state, send } = useHubConnection();
  return <LitePopupView state={state} send={send} loadFull={approvalInput} minReviewMs={minReviewMs} />;
}
