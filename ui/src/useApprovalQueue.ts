// Chung cho popup và Office: mọi quyết định dùng cùng điều kiện duyệt.
import { useCallback, useEffect, useMemo, useState } from 'react';
import { allowGate, type Gate } from './components/ApprovalCard';
import type { GroupAction } from './components/QueueList';
import type { ApprovalView, ClientMsg, Decision } from './protocol';
import type { HubState } from './store';
import { useNow } from './useNow';

export const DEFAULT_MIN_REVIEW_MS = 1000;

export function useApprovalQueue(state: HubState, send: (msg: ClientMsg) => boolean, minReviewMs: number) {
  const { queue } = state;
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

  return { now, selected, setSelectedId, gateOf, decide, decideGroup, setReviewed, notice };
}
