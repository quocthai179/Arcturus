// Popup Lite (docs/PLAN.md T12): hàng đợi duyệt, duyệt/từ chối, duyệt gộp, phím tắt trong popup (↑/↓ chọn, Enter đồng
// ý, Esc từ chối). Đóng popup thì app hủy cả webview, nên không có gì cần dọn ngoài socket.
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react';
import { allowGate, type Gate } from '../components/ApprovalCard';
import { Character } from '../components/Character';
import { QueueList, type GroupAction } from '../components/QueueList';
import { approvalInput, connectionInfo } from '../hub';
import type { ApprovalView, ClientMsg, Decision } from '../protocol';
import { initialState, reducer, type HubState } from '../store';
import { useNow } from '../useNow';
import { HubClient } from '../ws';

/** Bằng mặc định `min_review_ms` của hub (`[hub]` trong arcturus.toml). Hub vẫn tự giữ allow sớm hơn sàn của nó. */
export const DEFAULT_MIN_REVIEW_MS = 1000;

export interface LitePopupViewProps {
  state: HubState;
  send: (msg: ClientMsg) => boolean;
  loadFull: (requestId: string) => Promise<string>;
  minReviewMs: number;
}

export function LitePopupView({ state, send, loadFull, minReviewMs }: LitePopupViewProps) {
  const { queue, handoffs, agents, team, feed } = state;
  const connected = state.connection === 'open' && state.synced;
  const now = useNow(queue.length > 0);

  // Lúc mỗi thẻ được hiện (sàn thời gian tính từ đây, kể cả thẻ tới qua snapshot) và thẻ đã mở bản đầy đủ.
  const [shownAt, setShownAt] = useState<Record<string, number>>({});
  const [reviewed, setReviewed] = useState<Record<string, true>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    setShownAt((prev) => {
      const next: Record<string, number> = {};
      const t = Date.now();
      for (const a of queue) next[a.request_id] = prev[a.request_id] ?? t;
      return next;
    });
  }, [queue]);

  const selected = useMemo<ApprovalView | undefined>(
    () => queue.find((a) => a.request_id === selectedId) ?? queue[0],
    [queue, selectedId],
  );

  const gateOf = useCallback(
    (a: ApprovalView): Gate =>
      allowGate(a, {
        connected,
        shownAt: shownAt[a.request_id],
        now: Math.max(now, Date.now()),
        minReviewMs,
        reviewed: reviewed[a.request_id] === true,
      }),
    [connected, shownAt, now, minReviewMs, reviewed],
  );

  const decide = useCallback(
    (requestId: string, decision: Decision) => {
      const ok = send({ t: 'decide', request_id: requestId, decision, origin: 'user' });
      setNotice(ok ? null : 'Chưa kết nối hub, chưa gửi được');
    },
    [send],
  );

  const decideGroup = useCallback(
    (g: GroupAction) => {
      // T9 chưa có `decide_group` ở hub: gửi từng `decide`, hub vẫn kiểm điều kiện cho từng yêu cầu (allow_gate).
      for (const a of g.items) decide(a.request_id, 'allow');
    },
    [decide],
  );

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
  const [state, dispatch] = useReducer(reducer, initialState);
  const client = useRef<HubClient | null>(null);
  useEffect(() => {
    const c = new HubClient({
      connection: connectionInfo,
      onMessage: (msg) => dispatch({ type: 'server', msg }),
      onStatus: (status, error) => dispatch({ type: 'connection', status, error }),
    });
    client.current = c;
    c.start();
    return () => {
      c.stop();
      client.current = null;
    };
  }, []);
  const send = useCallback((msg: ClientMsg) => client.current?.send(msg) ?? false, []);
  return <LitePopupView state={state} send={send} loadFull={approvalInput} minReviewMs={minReviewMs} />;
}
