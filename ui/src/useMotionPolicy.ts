// Chính sách chuyển động của cả giao diện, một chỗ duy nhất (docs/UI_EXTENSION.md, CLAUDE.md "Quy ước code").
// Chuyển động chỉ được chạy khi trang đang hiện và người dùng không bật "giảm chuyển động". Hook đặt `data-motion="on|off"`
// lên `<html>`; `index.css` tắt mọi `animation`/`transition` khi `off`. Animation mới (CSS, Web Animations, canvas, WebGL)
// phải đọc cùng tín hiệu này: CSS thì tự được tắt, JS thì dùng `motionAllowed`/`useMotionPolicy`.
import { useEffect, useState } from 'react';

export interface MotionInputs {
  hidden: boolean;
  reduced: boolean;
}

export function motionAllowed({ hidden, reduced }: MotionInputs): boolean {
  return !hidden && !reduced;
}

const REDUCED_QUERY = '(prefers-reduced-motion: reduce)';

function read(): MotionInputs {
  const reduced = typeof window.matchMedia === 'function' ? window.matchMedia(REDUCED_QUERY).matches : false;
  return { hidden: document.hidden, reduced };
}

/** Gắn một lần ở gốc ứng dụng; trả về trạng thái hiện tại cho mã JS cần quyết định (ví dụ có chạy vòng vẽ hay không). */
export function useMotionPolicy(): boolean {
  const [allowed, setAllowed] = useState(() => motionAllowed(read()));
  useEffect(() => {
    const apply = () => {
      const ok = motionAllowed(read());
      document.documentElement.dataset.motion = ok ? 'on' : 'off';
      setAllowed(ok);
    };
    apply();
    const media = typeof window.matchMedia === 'function' ? window.matchMedia(REDUCED_QUERY) : null;
    document.addEventListener('visibilitychange', apply);
    media?.addEventListener('change', apply);
    return () => {
      document.removeEventListener('visibilitychange', apply);
      media?.removeEventListener('change', apply);
    };
  }, []);
  return allowed;
}
