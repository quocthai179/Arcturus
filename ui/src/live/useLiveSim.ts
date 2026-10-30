// Nối mô phỏng với React (docs/OFFICE_LIVE.md §4): đúng một `setTimeout` trỏ tới mốc gần nhất; không còn mốc hoặc không
// được chuyển động (trang ẩn, giảm chuyển động) thì không có timer nào. Không dùng requestAnimationFrame.
import { useCallback, useEffect, useReducer, useRef } from 'react';
import type { AgentView } from '../protocol';
import { advance, createSim, nextWakeAt, seeded, setMotion, syncAgents, wakeAll, type Sim } from './sim';

export interface LiveSim {
  sim: Sim;
  /** Tăng mỗi lần mô phỏng đổi (để vẽ lại). */
  version: number;
  /** "Giải lao tiếp": đánh thức các agent rảnh đang ngủ gật. */
  wake: () => void;
}

export function useLiveSim(agents: readonly AgentView[], motion: boolean): LiveSim {
  const simRef = useRef<Sim | null>(null);
  if (simRef.current === null) simRef.current = createSim({ rng: seeded(Date.now()), motion });
  const sim = simRef.current;
  const [version, bump] = useReducer((n: number) => n + 1, 0);

  // Chỉ đồng bộ khi danh sách id/mood đổi: Office vẽ lại thường xuyên (đồng hồ đếm ngược) với mảng agent mới.
  const key = agents.map((a) => `${a.id}\u0000${a.mood}\u0000${a.parent_id === null ? 0 : 1}`).join('\u0001');
  const latest = useRef(agents);
  latest.current = agents;
  useEffect(() => {
    const now = Date.now();
    const moved = setMotion(sim, motion, now);
    const synced = syncAgents(
      sim,
      latest.current.map((a) => ({ id: a.id, mood: a.mood, small: a.parent_id !== null })),
      now,
    );
    if (moved || synced) bump();
  }, [sim, key, motion]);

  useEffect(() => {
    if (!motion) return undefined;
    const wakeAt = nextWakeAt(sim);
    if (wakeAt === Infinity) return undefined;
    const timer = setTimeout(() => {
      advance(sim, Date.now());
      bump();
    }, Math.max(0, wakeAt - Date.now()));
    return () => clearTimeout(timer);
  }, [sim, version, motion]);

  const wake = useCallback(() => {
    if (wakeAll(sim, Date.now())) bump();
  }, [sim]);

  return { sim, version, wake };
}
