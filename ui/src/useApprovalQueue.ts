// Chung cho popup và Office: mọi quyết định dùng cùng điều kiện duyệt.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { allowGate, type Gate } from './components/ApprovalCard';
import type { GroupAction } from './components/QueueList';
import { countdownEndsAt } from './countdown';
import type { ApprovalView, ClientMsg, Decision, TrustSuggestionView } from './protocol';
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
      // Một thông điệp `decide_group` (T9): hub áp cho mọi yêu cầu cùng nhóm trừ Critical, kiểm điều kiện cho từng yêu cầu
      // (allow_gate) và trả `Feed` báo đã duyệt bao nhiêu, bao nhiêu bị giữ lại. Không còn trường hợp gửi dở nửa chừng.
      const ok = send({ t: 'decide_group', group_key: g.groupKey, decision: 'allow' });
      setNotice(ok ? null : 'Chưa kết nối hub, chưa gửi được');
    },
    [send],
  );

  // Đề xuất nâng mức tin cậy (T10): trả lời bằng một thông điệp; thẻ biến mất ngay ở cửa sổ này (hub chỉ báo cho client khác
  // qua Feed), nên giữ danh sách id đã trả lời cục bộ.
  const [answered, setAnswered] = useState<ReadonlySet<string>>(new Set());
  const suggestions = useMemo<TrustSuggestionView[]>(
    () => state.suggestions.filter((s) => !answered.has(s.id)),
    [state.suggestions, answered],
  );
  const answerSuggestion = useCallback(
    (id: string, accept: boolean) => {
      const ok = send({ t: 'suggestion_answer', id, accept });
      setNotice(ok ? null : 'Chưa kết nối hub, chưa gửi được');
      if (ok) setAnswered((prev) => new Set(prev).add(id));
    },
    [send],
  );

  // Đếm ngược phủ quyết (T14). Mốc hết giờ tính từ lúc thẻ hiện; đội tạm dừng thì không có đồng hồ nào.
  const paused = state.team.paused;
  const countdownOf = useCallback(
    (a: ApprovalView): number | null => {
      const end = countdownEndsAt(a, shownAt[a.request_id], paused);
      return end === null ? null : Math.max(0, end - Math.max(now, Date.now()));
    },
    [shownAt, paused, now],
  );

  // Hết giờ thì gửi `allow` với `origin: countdown`, mỗi yêu cầu đúng một lần. Chỉ khi đã kết nối và trang đang hiện
  // (người dùng có cơ hội thấy và phủ quyết); hub vẫn kiểm lại mọi điều kiện (allow_gate, §3.4).
  const fired = useRef(new Set<string>());
  useEffect(() => {
    if (!connected || document.hidden) return;
    for (const a of queue) {
      if (fired.current.has(a.request_id) || countdownOf(a) !== 0) continue;
      if (send({ t: 'decide', request_id: a.request_id, decision: 'allow', origin: 'countdown' })) {
        fired.current.add(a.request_id);
      }
    }
  }, [queue, connected, countdownOf, send]);

  return {
    now,
    selected,
    setSelectedId,
    gateOf,
    decide,
    decideGroup,
    setReviewed,
    notice,
    countdownOf,
    suggestions,
    answerSuggestion,
  };
}
