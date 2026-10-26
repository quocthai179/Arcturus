// Hàng đợi duyệt: nút "Duyệt cả N" cho nhóm cùng `group_key`, các thẻ duyệt, rồi các thẻ "duyệt trong terminal".
import type { AgentId, AgentView, ApprovalView, Decision, HandoffView } from '../protocol';
import { ApprovalCard, type Gate } from './ApprovalCard';
import { HandoffCard } from './HandoffCard';

export interface GroupAction {
  groupKey: string;
  /** Yêu cầu trong nhóm được duyệt gộp: mọi yêu cầu không phải Critical (Critical luôn ở lại, duyệt từng cái). */
  items: ApprovalView[];
  /** Mọi yêu cầu trong `items` đều đã qua điều kiện Đồng ý. */
  ready: boolean;
}

/**
 * Nhóm có ≥2 yêu cầu được duyệt gộp. Yêu cầu Critical không bao giờ nằm trong nhóm gộp (quy tắc bất biến 2; khớp T9
 * `DecideGroup` chỉ áp cho risk ≤ High), nên nhóm toàn Critical không có nút.
 */
export function groupActions(queue: readonly ApprovalView[], gateOf: (a: ApprovalView) => Gate): GroupAction[] {
  const groups = new Map<string, ApprovalView[]>();
  for (const a of queue) {
    if (a.risk === 'critical') continue;
    const list = groups.get(a.group_key) ?? [];
    list.push(a);
    groups.set(a.group_key, list);
  }
  return [...groups.entries()]
    .filter(([, items]) => items.length >= 2)
    .map(([groupKey, items]) => ({ groupKey, items, ready: items.every((a) => gateOf(a).ok) }));
}

export interface QueueListProps {
  queue: readonly ApprovalView[];
  handoffs: readonly HandoffView[];
  agents: Record<AgentId, AgentView>;
  selectedId: string | null;
  now: number;
  gateOf: (a: ApprovalView) => Gate;
  onSelect: (requestId: string) => void;
  onDecide: (requestId: string, decision: Decision) => void;
  onDecideGroup: (group: GroupAction) => void;
  loadFull: (requestId: string) => Promise<string>;
  onReviewed: (requestId: string) => void;
  /** Đếm ngược còn lại (ms) của một yêu cầu, `null` khi không có (T14). */
  countdownOf: (a: ApprovalView) => number | null;
}

export function QueueList(p: QueueListProps) {
  const groups = groupActions(p.queue, p.gateOf);
  return (
    <div className="queue">
      {groups.length > 0 && (
        <div className="groups">
          {groups.map((g) => (
            <button
              key={g.groupKey}
              type="button"
              className="btn btn--group"
              disabled={!g.ready}
              title={g.ready ? undefined : 'Mỗi yêu cầu trong nhóm phải đủ điều kiện duyệt trước'}
              onClick={() => g.ready && p.onDecideGroup(g)}
            >
              Duyệt cả {g.items.length} · {g.groupKey}
            </button>
          ))}
        </div>
      )}
      <ul className="cards" aria-label="Yêu cầu đang chờ">
        {p.queue.map((a) => (
          <li key={a.request_id}>
            <ApprovalCard
              approval={a}
              agent={p.agents[a.agent_id]}
              selected={a.request_id === p.selectedId}
              gate={p.gateOf(a)}
              countdownLeftMs={p.countdownOf(a)}
              now={p.now}
              onSelect={() => p.onSelect(a.request_id)}
              onDecide={(d) => p.onDecide(a.request_id, d)}
              loadFull={p.loadFull}
              onReviewed={() => p.onReviewed(a.request_id)}
            />
          </li>
        ))}
      </ul>
      {p.handoffs.length > 0 && (
        <ul className="cards" aria-label="Đang duyệt trong terminal">
          {p.handoffs.map((h) => (
            <li key={h.handoff_id}>
              <HandoffCard handoff={h} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
