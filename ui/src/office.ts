import type { AgentView, Mood } from './protocol';

const PRIORITY: Record<Mood, number> = { approval: 0, error: 1, working: 2, done: 3, idle: 4 };

/** Mới nhất trước trong mỗi mood; id phá hòa để bàn không đổi chỗ ngẫu nhiên. */
export function officeRoster(agents: readonly AgentView[], rosterMax: number) {
  const sorted = [...agents].sort((a, b) =>
    PRIORITY[a.mood] - PRIORITY[b.mood] ||
    Date.parse(b.last_event_at) - Date.parse(a.last_event_at) || a.id.localeCompare(b.id),
  );
  const desks: AgentView[] = [];
  const support: AgentView[] = [];
  for (const a of sorted) {
    if (a.mood !== 'idle' || desks.length < rosterMax) desks.push(a);
    else support.push(a);
  }
  return { desks, support };
}
