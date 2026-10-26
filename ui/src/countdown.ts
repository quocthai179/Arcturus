// Đếm ngược phủ quyết (docs/PLAN.md T14, §3.4). Hub chỉ gửi `countdown_ms` cho yêu cầu đảo ngược được và khi đội không
// tạm dừng; UI vẫn tự kiểm lại để không bao giờ hiện hay chạy đồng hồ cho High/Critical (quy tắc bất biến 2).
import type { ApprovalView } from './protocol';

/**
 * Mốc hết giờ (ms epoch) của một yêu cầu, hoặc `null` khi không có đếm ngược. Tính từ lúc thẻ được **hiện** (`shownAt`)
 * nếu muộn hơn `created_at`: thẻ tới UI muộn qua snapshot vẫn cho người dùng đủ thời gian phủ quyết, không tự chạy ngay
 * khi vừa hiện. Không bao giờ sớm hơn `created_at + countdown_ms` (điều kiện hub kiểm khi nhận `origin: countdown`).
 */
export function countdownEndsAt(a: ApprovalView, shownAt: number | undefined, paused: boolean): number | null {
  if (paused || a.countdown_ms === null || shownAt === undefined) return null;
  if (!a.reversible || (a.risk !== 'low' && a.risk !== 'medium')) return null;
  const created = Date.parse(a.created_at);
  return (Number.isNaN(created) ? shownAt : Math.max(created, shownAt)) + a.countdown_ms;
}
