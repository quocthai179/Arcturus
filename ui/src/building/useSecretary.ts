// Nối `stepSecretary` (thuần) với React: chạy khi `brief`/rosters/motion đổi và tại `wakeAt` bằng đúng một `setTimeout`; không
// có mốc (`Infinity`) thì không timer (đúng luật "0 khi rảnh"). Hủy khi unmount hoặc đầu vào đổi.
import { useCallback, useEffect, useRef, useState } from 'react';
import type { AgentView } from '../protocol';
import type { SceneBrief } from '../sceneBrief';
import { initialSecretary, sayBubble, stepSecretary, type Bubble, type SecMemory } from './secretary';
import type { FloorRoster } from './seating';

export interface UseSecretaryInput {
  rosters: readonly FloorRoster[];
  brief: SceneBrief | undefined;
  canSelect: (a: AgentView) => boolean;
  motion: boolean;
}

export interface SecretaryHandle {
  mem: SecMemory;
  /** Đặt một bong bóng do người dùng gọi (ví dụ tóm tắt khi bấm thư ký). */
  say: (b: Bubble) => void;
}

export function useSecretary({ rosters, brief, canSelect, motion }: UseSecretaryInput): SecretaryHandle {
  const [mem, setMem] = useState<SecMemory>(initialSecretary);
  const memRef = useRef(mem);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latest = useRef({ rosters, brief, canSelect, motion });
  latest.current = { rosters, brief, canSelect, motion };

  const run = useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
    const now = Date.now();
    const out = stepSecretary(memRef.current, { ...latest.current, now });
    if (out.mem !== memRef.current) {
      memRef.current = out.mem;
      setMem(out.mem);
    }
    if (Number.isFinite(out.wakeAt)) timer.current = setTimeout(run, Math.max(0, out.wakeAt - Date.now()));
  }, []);

  useEffect(() => {
    run();
  }, [rosters, brief, motion, run]);

  useEffect(
    () => () => {
      if (timer.current !== null) clearTimeout(timer.current);
      timer.current = null;
    },
    [],
  );

  const say = useCallback(
    (b: Bubble) => {
      memRef.current = sayBubble(memRef.current, b, Date.now());
      setMem(memRef.current);
      run();
    },
    [run],
  );

  return { mem, say };
}
