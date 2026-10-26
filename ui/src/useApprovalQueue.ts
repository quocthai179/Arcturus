// Chung cho popup và Office: mọi quyết định dùng cùng điều kiện duyệt.
import { useCallback, useEffect, useMemo, useState } from 'react';
import { allowGate, type Gate } from './components/ApprovalCard';
import type { GroupAction } from './components/QueueList';
import type { ApprovalView, ClientMsg, Decision } from './protocol';
import type { HubState } from './store';
import { useNow } from './useNow';

export const DEFAULT_MIN_REVIEW_MS = 1000;

/** Sau khi một thẻ rời hàng đợi (thẻ dưới trượt lên dưới con trỏ/phím), chưa cho Đồng ý chừng này để người dùng kịp thấy. */
export const LIST_SETTLE_MS = 500;

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

  // Thẻ biến mất khỏi hàng đợi thì thẻ sau trượt lên: khóa Đồng ý một lúc (phím Enter/click kịp lúc không rơi vào thẻ mới).
  const idsKey = queue.map((a) => a.request_id).join(',');
  const [seen, setSeen] = useState({ key: idsKey, ids: queue.map((a) => a.request_id) });
  const [calmUntil, setCalmUntil] = useState(0);
  if (seen.key !== idsKey) {
    const ids = queue.map((a) => a.request_id);
    if (seen.ids.some((id) => !ids.includes(id))) setCalmUntil(Date.now() + LIST_SETTLE_MS);
    setSeen({ key: idsKey, ids });
  }

  const gateOf = useCallback(
    (a: ApprovalView): Gate =>
      allowGate(a, {
        connected,
        shownAt: shownAt[a.request_id],
        now: Math.max(now, Date.now()),
        minReviewMs,
        reviewed: reviewed[a.request_id] === true,
        calmUntil,
      }),
    [connected, shownAt, now, minReviewMs, reviewed, calmUntil],
  );

  const decide = useCallback(
    (requestId: string, decision: Decision): boolean => {
      const ok = send({ t: 'decide', request_id: requestId, decision, origin: 'user' });
      setNotice(ok ? null : 'Chưa kết nối hub, chưa gửi được');
      return ok;
    },
    [send],
  );

  const decideGroup = useCallback(
    (g: GroupAction) => {
      // T9 chưa có `decide_group` ở hub: gửi từng `decide`, hub vẫn kiểm điều kiện cho từng yêu cầu (allow_gate).
      // Dừng ở lần gửi lỗi đầu tiên và nói rõ đã gửi được bao nhiêu (không để thông báo của lần cuối che mất).
      let sent = 0;
      for (const a of g.items) {
        if (!decide(a.request_id, 'allow')) {
          setNotice(`Mới gửi ${sent}/${g.items.length} yêu cầu, mất kết nối hub. Kiểm tra lại hàng đợi.`);
          return;
        }
        sent += 1;
      }
    },
    [decide],
  );

  return { now, selected, setSelectedId, gateOf, decide, decideGroup, setReviewed, notice };
}
