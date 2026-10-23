import { useEffect, useState } from 'react';

/**
 * Giờ hiện tại (ms), cập nhật mỗi `intervalMs` **chỉ khi** `active` và trang đang hiện (Page Visibility API). Không có
 * yêu cầu nào đang chờ thì không có timer nào chạy (CLAUDE.md: không animation/timer khi rảnh).
 */
export function useNow(active: boolean, intervalMs = 250): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return undefined;
    let timer: ReturnType<typeof setInterval> | null = null;
    const startStop = () => {
      if (document.hidden) {
        if (timer !== null) clearInterval(timer);
        timer = null;
      } else if (timer === null) {
        setNow(Date.now());
        timer = setInterval(() => setNow(Date.now()), intervalMs);
      }
    };
    startStop();
    document.addEventListener('visibilitychange', startStop);
    return () => {
      document.removeEventListener('visibilitychange', startStop);
      if (timer !== null) clearInterval(timer);
    };
  }, [active, intervalMs]);
  return now;
}
