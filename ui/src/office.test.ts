import { describe, expect, it } from 'vitest';
import { officeRoster } from './office';
import { agent } from './test/fixtures';

describe('officeRoster', () => {
  it('sorts_moods_then_newest_timestamp_then_id_without_mutating_input', () => {
    const agents = [
      agent({ id: 'idle', mood: 'idle' }), agent({ id: 'done', mood: 'done' }),
      agent({ id: 'work', mood: 'working' }), agent({ id: 'error', mood: 'error' }),
      agent({ id: 'b', last_event_at: '2026-10-26T10:00:00Z' }),
      agent({ id: 'a', last_event_at: '2026-10-26T10:00:00Z' }), agent({ id: 'old' }),
    ];
    const before = [...agents];
    expect(officeRoster(agents, 20).desks.map((a) => a.id)).toEqual(['a', 'b', 'old', 'error', 'work', 'done', 'idle']);
    expect(agents).toEqual(before);
  });

  it('never_groups_non_idle_agents_even_above_roster_limit_including_subagents', () => {
    const agents = Array.from({ length: 7 }, (_, i) => agent({ id: `work-${i}`, mood: 'working', parent_id: i ? 'work-0' : null }));
    agents.push(agent({ id: 'idle', mood: 'idle' }));
    const result = officeRoster(agents, 5);
    expect(result.desks).toHaveLength(7);
    expect(result.support.map((a) => a.id)).toEqual(['idle']);
  });

  it('fills_remaining_slots_with_idle_and_recomputes_on_mood_or_limit_change', () => {
    const working = agent({ id: 'work', mood: 'working' });
    const idle = [agent({ id: 'a', mood: 'idle' }), agent({ id: 'b', mood: 'idle' })];
    expect(officeRoster([working, ...idle], 2).support.map((a) => a.id)).toEqual(['b']);
    expect(officeRoster([working, ...idle], 3).support).toEqual([]);
    expect(officeRoster([working, idle[0]!, { ...idle[1]!, mood: 'error' }], 1).desks.map((a) => a.id)).toEqual(['b', 'work']);
    expect(officeRoster([], 5)).toEqual({ desks: [], support: [] });
  });
});
