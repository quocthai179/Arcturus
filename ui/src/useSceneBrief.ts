// Giữ bộ nhớ của `think()` (pet/brain.ts) và dựng `SceneBrief` cho cảnh. `think` chạy trong effect, KHÔNG trong render/useMemo:
// StrictMode gọi render hai lần, lần hai sẽ thấy khóa đã nói và mất câu (docs/OFFICE_BUILDING.md §2.2). Không có timer.
import { useEffect, useRef, useState } from 'react';
import { initialMemory, SLEEP_AFTER_MS, think, type BrainMemory } from './pet/brain';
import { sceneBrief, type SceneBrief } from './sceneBrief';
import type { HubState } from './store';

export function useSceneBrief(state: HubState): SceneBrief {
  const memory = useRef<BrainMemory | null>(null);
  const seen = useRef<HubState | null>(null);
  const [brief, setBrief] = useState<SceneBrief>(() => sceneBrief(state, { speech: null, mood: 'idle' }));
  useEffect(() => {
    // StrictMode chạy lại effect với cùng `state`: lần hai `think` thấy khóa đã nói và trả về không có câu, làm mất câu.
    if (seen.current === state) return;
    seen.current = state;
    const now = Date.now();
    memory.current ??= initialMemory(now);
    const out = think(state, memory.current, now, SLEEP_AFTER_MS, 'normal');
    memory.current = out.memory;
    setBrief(sceneBrief(state, out));
  }, [state]);
  return brief;
}
