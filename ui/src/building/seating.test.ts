import { describe, expect, it } from 'vitest';
import { agent } from '../test/fixtures';
import type { AgentView } from '../protocol';
import { FLOORS } from './floors';
import { assignSeats, EMPTY_SEATS, IDLE_SLEEP_MS, MAX_FIGURES_PER_FLOOR, poseFor, rosters, type Spot } from './seating';
import type { SceneBrief } from '../sceneBrief';

const sub = (id: string, role: string, over: Partial<AgentView> = {}) => agent({ id, kind: 'subagent', parent_id: 'agt_s0', role, name: id, mood: 'working', activity: 'typing', ...over });
const NOW = Date.parse('2026-10-23T09:00:30Z');
const spotOf = (seats: ReturnType<typeof assignSeats>, id: string) => {
  const s = seats.byAgent.get(id);
  if (s === undefined) throw new Error(`no seat for ${id}`);
  return s;
};

describe('assignSeats', () => {
  it('role_desk_then_hot_desk_for_two_developers', () => {
    const seats = assignSeats(EMPTY_SEATS, [sub('agt_a', 'dev-squad:developer'), sub('agt_b', 'dev-squad:developer')]);
    const dev = FLOORS[1]?.roles.findIndex((r) => r.id === 'developer') ?? -1;
    expect(spotOf(seats, 'agt_a')).toEqual({ floor: 1, kind: 'desk', index: dev });
    expect(spotOf(seats, 'agt_b')).toEqual({ floor: 1, kind: 'desk', index: 15 });
  });

  it('seats_are_stable_when_mood_changes_and_freed_when_removed', () => {
    const a = sub('agt_a', 'dev-squad:developer');
    const b = sub('agt_b', 'dev-squad:architect');
    const s1 = assignSeats(EMPTY_SEATS, [a, b]);
    const s2 = assignSeats(s1, [{ ...a, mood: 'idle' }, { ...b, mood: 'error' }]);
    expect(spotOf(s2, 'agt_a')).toEqual(spotOf(s1, 'agt_a'));
    expect(spotOf(s2, 'agt_b')).toEqual(spotOf(s1, 'agt_b'));
    const s3 = assignSeats(s2, [b]);
    expect(s3.byAgent.has('agt_a')).toBe(false);
    const s4 = assignSeats(s3, [b, sub('agt_c', 'dev-squad:developer')]);
    expect(spotOf(s4, 'agt_c')).toEqual(spotOf(s1, 'agt_a'));
    expect(s4.order).toEqual(['agt_b', 'agt_c']);
  });

  it('guests_take_the_lobby_sofa_then_standing_then_overflow', () => {
    const guests = Array.from({ length: 10 }, (_, i) => sub(`agt_g${i}`, `evil:x${i}`));
    const seats = assignSeats(EMPTY_SEATS, guests);
    const kinds = guests.map((g) => seats.byAgent.get(g.id)?.kind);
    expect(kinds.slice(0, 4)).toEqual(['lounge', 'lounge', 'lounge', 'lounge']);
    expect(kinds.slice(4, 8)).toEqual(['standing', 'standing', 'standing', 'standing']);
    expect(seats.byAgent.has('agt_g8')).toBe(false);
    const r = rosters(seats, guests, undefined, NOW);
    expect(r[0]?.overflow).toBe(2);
    expect(r[0]?.figures.length).toBe(8);
  });

  it('sessions_and_general_use_their_own_lobby_desks', () => {
    const seats = assignSeats(EMPTY_SEATS, [agent({ id: 'agt_s1' }), sub('agt_a', 'general-purpose'), sub('agt_b', 'Explore')]);
    expect(spotOf(seats, 'agt_s1')).toEqual({ floor: 0, kind: 'desk', index: 0 });
    expect(spotOf(seats, 'agt_a')).toEqual({ floor: 0, kind: 'desk', index: 6 });
    expect(spotOf(seats, 'agt_b')).toEqual({ floor: 0, kind: 'desk', index: 7 });
  });

  it('never_more_than_the_figure_cap_per_floor_and_never_floor_5', () => {
    const roles = FLOORS[1]?.roles ?? [];
    const many = Array.from({ length: 60 }, (_, i) => sub(`agt_${String(i).padStart(3, '0')}`, `dev-squad:${roles[i]?.id ?? `extra-${i}`}`));
    const seats = assignSeats(EMPTY_SEATS, many);
    const r = rosters(seats, many, undefined, NOW);
    expect(r).toHaveLength(5);
    for (const f of r) expect(f.figures.length).toBeLessThanOrEqual(MAX_FIGURES_PER_FLOOR);
    expect(r[1]?.figures.length).toBe(23);
    expect(r[1]?.overflow).toBe(60 - 23);
    expect(r[1]?.stats.present).toBe(23);
  });

  it('agent_with_ceo_role_is_never_on_floor_5', () => {
    const seats = assignSeats(EMPTY_SEATS, [sub('agt_a', 'ceo:x')]);
    expect(spotOf(seats, 'agt_a').floor).toBe(0);
  });
});

describe('poses and rosters', () => {
  const desk: Spot = { floor: 1, kind: 'desk', index: 0 };
  const at = (over: Partial<AgentView>) => poseFor(sub('agt_a', 'dev-squad:developer', over), desk, NOW);

  it('mood_activity_table', () => {
    expect(at({ mood: 'working', activity: 'typing' })).toBe('sit_type');
    expect(at({ mood: 'working', activity: 'running' })).toBe('sit_type');
    expect(at({ mood: 'working', activity: 'reading' })).toBe('sit_read');
    for (const activity of ['thinking', 'waiting', 'idle'] as const) expect(at({ mood: 'working', activity })).toBe('sit_think');
    expect(at({ mood: 'approval' })).toBe('raise');
    expect(at({ mood: 'error' })).toBe('sit_error');
    expect(at({ mood: 'done' })).toBe('cheer');
    expect(at({ mood: 'idle' })).toBe('sit_think');
  });

  it('long_idle_sleeps', () => {
    const old = new Date(NOW - IDLE_SLEEP_MS - 1000).toISOString();
    expect(at({ mood: 'idle', last_event_at: old })).toBe('sleep');
    expect(poseFor(sub('agt_a', 'x', { mood: 'idle', last_event_at: old }), { floor: 0, kind: 'lounge', index: 1 }, NOW)).toBe('sleep');
  });

  it('idle_agents_go_to_the_lounge_when_there_is_room_else_stay_at_the_desk', () => {
    const idle = Array.from({ length: 8 }, (_, i) => sub(`agt_${i}`, `dev-squad:${FLOORS[1]?.roles[i]?.id ?? 'x'}`, { mood: 'idle', activity: 'idle' }));
    const seats = assignSeats(EMPTY_SEATS, idle);
    const f = rosters(seats, idle, undefined, NOW)[1];
    const kinds = f?.figures.map((x) => x.spot.kind) ?? [];
    expect(kinds.filter((k) => k === 'lounge')).toHaveLength(6);
    expect(kinds.filter((k) => k === 'desk')).toHaveLength(idle.length - 6);
    expect(f?.figures[0]?.pose).toBe('lounge_sit');
    expect(f?.figures[3]?.pose).toBe('stand');
    expect(f?.figures[7]?.pose).toBe('sit_think');
  });

  it('stats_count_busy_raised_errors', () => {
    const list = [
      sub('agt_a', 'dev-squad:developer'),
      sub('agt_b', 'dev-squad:architect', { mood: 'approval' }),
      sub('agt_c', 'dev-squad:debugger', { mood: 'error' }),
      sub('agt_d', 'dev-squad:tech-writer', { mood: 'done' }),
    ];
    const r = rosters(assignSeats(EMPTY_SEATS, list), list, undefined, NOW)[1];
    expect(r?.stats).toEqual({ present: 4, busy: 1, raised: 1, errors: 1 });
    const brief = { pending: new Map([['agt_d', { count: 1, risk: 'low', tool: 'Bash', oldestAt: 0 }]]) } as unknown as SceneBrief;
    expect(rosters(assignSeats(EMPTY_SEATS, list), list, brief, NOW)[1]?.stats.raised).toBe(2);
  });
});
