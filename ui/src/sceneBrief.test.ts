import { describe, expect, it } from 'vitest';
import { RISK_RANK, sceneBrief } from './sceneBrief';
import { initialState, type HubState } from './store';
import { agent, approval, handoff } from './test/fixtures';

const base: HubState = { ...initialState, connection: 'open', synced: true };

describe('sceneBrief', () => {
  it('collapses_the_queue_to_highest_risk_and_oldest_tool_per_agent', () => {
    const state: HubState = {
      ...base,
      queue: [
        approval({ request_id: 'r1', agent_id: 'agt_a', tool: 'Read', risk: 'low', created_at: '2026-10-23T09:00:00Z' }),
        approval({ request_id: 'r2', agent_id: 'agt_a', tool: 'Bash', risk: 'critical', created_at: '2026-10-23T09:00:05Z' }),
        approval({ request_id: 'r3', agent_id: 'agt_b', tool: 'Edit', risk: 'medium', created_at: '2026-10-23T09:00:02Z' }),
      ],
    };
    const brief = sceneBrief(state, { speech: null, mood: 'working' });
    expect(brief.pending.get('agt_a')).toEqual({ count: 2, risk: 'critical', tool: 'Read', oldestAt: Date.parse('2026-10-23T09:00:00Z') });
    expect(brief.pending.get('agt_b')?.count).toBe(1);
    expect(brief.pending.size).toBe(2);
  });

  it('carries_handoffs_flags_speech_and_mood', () => {
    const state: HubState = { ...base, handoffs: [handoff({ agent_id: 'agt_h' })], team: { paused: true, focus: true, roster_max: 5 }, agents: { a: agent() } };
    const brief = sceneBrief(state, { speech: { key: 'k', text: 't', priority: 'high', kind: 'conn' }, mood: 'sleep' });
    expect(brief.handoffs.has('agt_h')).toBe(true);
    expect(brief).toMatchObject({ connected: true, paused: true, focus: true, teamMood: 'sleep', speech: { key: 'k', text: 't', priority: 'high' } });
    expect(sceneBrief({ ...base, synced: false }, { speech: null, mood: 'idle' }).connected).toBe(false);
  });

  it('has_no_command_fields_anywhere', () => {
    const state: HubState = { ...base, queue: [approval({ command_original: 'SECRET-CMD', command_normalized: 'SECRET-CMD' })] };
    expect(JSON.stringify([...sceneBrief(state, { speech: null, mood: 'idle' }).pending])).not.toContain('SECRET-CMD');
  });

  it('risk_rank_order', () => {
    expect(RISK_RANK).toEqual({ critical: 0, high: 1, medium: 2, low: 3 });
  });
});
