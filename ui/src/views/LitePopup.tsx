// Popup Lite (docs/PLAN.md T12): hàng đợi duyệt, duyệt/từ chối, duyệt gộp, phím tắt trong popup (↑/↓ chọn, Enter đồng
// ý, Esc từ chối). Đóng popup thì app hủy cả webview, nên không có gì cần dọn ngoài socket.
import { useEffect, useRef } from 'react';
import { AgentAvatar } from '../components/AgentAvatar';
import { QueueList } from '../components/QueueList';
import { SuggestionList } from '../components/SuggestionList';
import { TeamControls } from '../components/TeamControls';
import { approvalInput, reportPopupReady } from '../hub';
import type { ClientMsg } from '../protocol';
import type { HubState } from '../store';
import { DEFAULT_MIN_REVIEW_MS, useApprovalQueue } from '../useApprovalQueue';
import { useHubConnection } from '../useHubConnection';
export { DEFAULT_MIN_REVIEW_MS, LIST_SETTLE_MS } from '../useApprovalQueue';

export interface LitePopupViewProps {
  state: HubState;
  send: (msg: ClientMsg) => boolean;
  loadFull: (requestId: string) => Promise<string>;
  minReviewMs: number;
}

export function LitePopupView({ state, send, loadFull, minReviewMs }: LitePopupViewProps) {
  const { queue, handoffs, agents, team, feed } = state;
  const { now, selected, setSelectedId, gateOf, decide, decideGroup, setReviewed, notice, countdownOf, suggestions, answerSuggestion } =
    useApprovalQueue(state, send, minReviewMs);

  // Thẻ được chọn (↑/↓) phải nằm trong vùng nhìn thấy: Enter chỉ duyệt thẻ người dùng đang thấy.
  const root = useRef<HTMLElement>(null);
  const selectedId = selected?.request_id;
  useEffect(() => {
    // So thuộc tính dữ liệu trực tiếp: id là dữ liệu, không ghép vào selector.
    const cards = root.current?.querySelectorAll<HTMLElement>('[data-request-id]') ?? [];
    [...cards].find((el) => el.dataset.requestId === selectedId)?.scrollIntoView({ block: 'nearest' });
  }, [selectedId]);

  // Phím tắt chỉ trong popup (cửa sổ riêng, không đăng ký phím toàn cục).
  const latest = useRef({ queue, selected, gateOf, decide });
  latest.current = { queue, selected, gateOf, decide };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const { queue: q, selected: sel, gateOf: gate, decide: act } = latest.current;
      const target = e.target;
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return;
      if (sel === undefined) return;
      // Giữ phím lặp lại sẽ duyệt/từ chối hàng loạt thẻ người dùng chưa kịp đọc: mỗi quyết định là một lần bấm.
      if (e.repeat && (e.key === 'Enter' || e.key === 'Escape')) {
        e.preventDefault();
        return;
      }
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
    <main className="popup" ref={root}>
      <header className="popup__head">
        <h1>Arcturus</h1>
        <span className="muted" role="status">
          {queue.length > 0 ? `${queue.length} yêu cầu đang chờ` : 'Không có yêu cầu nào đang chờ'}
        </span>
      </header>
      <TeamControls team={team} connected={state.connection === 'open' && state.synced} send={send} />

      {state.connection !== 'open' && (
        <p className="banner banner--error" role="alert">
          {state.error ?? 'Đang kết nối hub…'}
        </p>
      )}
      {team.paused && <p className="banner banner--paused">Đội đang tạm dừng: không có gì được tự duyệt.</p>}
      {team.focus && (
        <p className="banner">Giờ tập trung: chỉ báo yêu cầu rủi ro cao và vùng khóa. Các yêu cầu khác vẫn nằm trong hàng đợi.</p>
      )}
      {notice !== null && <p className="banner banner--error">{notice}</p>}
      {lastFeed !== undefined && (
        <p className="banner" role="status" aria-live="polite">
          {lastFeed.text}
        </p>
      )}

      <SuggestionList suggestions={suggestions} onAnswer={answerSuggestion} />

      <QueueList
        queue={queue}
        handoffs={handoffs}
        agents={agents}
        selectedId={selected?.request_id ?? null}
        now={now}
        gateOf={gateOf}
        countdownOf={countdownOf}
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
              <AgentAvatar mood={a.mood} palette={a.palette} size={28} label={a.name} />
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
  // Khung đầu tiên đã vẽ: một lần cho mỗi lần mở popup (webview bị hủy khi đóng).
  useEffect(() => {
    const frame = requestAnimationFrame(reportPopupReady);
    return () => cancelAnimationFrame(frame);
  }, []);
  return <LitePopupView state={state} send={send} loadFull={approvalInput} minReviewMs={minReviewMs} />;
}
