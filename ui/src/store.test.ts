import { describe, expect, it } from 'vitest';
import { parseServerMsg } from './protocol';
import { applyServer, FEED_MAX, initialState, reducer } from './store';
import { agent, approval, handoff, snapshot } from './test/fixtures';

describe('store', () => {
  it('snapshot_replaces_everything', () => {
    let s = applyServer(initialState, snapshot({ queue: [approval({ request_id: 'req_old' })] }));
    s = applyServer(s, { t: 'agent_updated', agent: agent({ id: 'agt_x', name: 'Lạ' }) });
    s = applyServer(
      s,
      snapshot({
        agents: [agent({ id: 'agt_s2', name: 'Linh' })],
        queue: [
          approval({ request_id: 'req_b', created_at: '2026-10-23T09:00:02Z' }),
          approval({ request_id: 'req_a', created_at: '2026-10-23T09:00:01Z' }),
        ],
        handoffs: [handoff()],
        team: { paused: true, focus: false, roster_max: 5 },
      }),
    );
    expect(s.synced).toBe(true);
    expect(Object.keys(s.agents)).toEqual(['agt_s2']);
    expect(s.queue.map((a) => a.request_id)).toEqual(['req_a', 'req_b']);
    expect(s.handoffs).toHaveLength(1);
    expect(s.team.paused).toBe(true);
  });

  it('pause_clears_countdown_everywhere_and_resume_does_not_restore', () => {
    const timed = (id: string) => approval({ request_id: id, countdown_ms: 5000, reversible: true, risk: 'low' });
    let s = applyServer(initialState, snapshot({ queue: [timed('req_1'), timed('req_2')] }));
    expect(s.queue.map((a) => a.countdown_ms)).toEqual([5000, 5000]);
    s = applyServer(s, { t: 'team_updated', team: { paused: true, focus: false, roster_max: 5 } });
    expect(s.queue.map((a) => a.countdown_ms)).toEqual([null, null]);
    s = applyServer(s, { t: 'team_updated', team: { paused: false, focus: false, roster_max: 5 } });
    expect(s.queue.map((a) => a.countdown_ms)).toEqual([null, null]);
    // Yêu cầu tới sau khi tiếp tục thì được có đếm ngược.
    s = applyServer(s, { t: 'approval_requested', approval: timed('req_3') });
    expect(s.queue.find((a) => a.request_id === 'req_3')?.countdown_ms).toBe(5000);
  });

  it('countdown_is_dropped_while_paused_even_if_snapshot_or_delta_carries_one', () => {
    const timed = approval({ request_id: 'req_1', countdown_ms: 5000, reversible: true, risk: 'low' });
    const paused = { paused: true, focus: false, roster_max: 5 };
    let s = applyServer(initialState, snapshot({ queue: [timed], team: paused }));
    expect(s.queue[0]?.countdown_ms).toBeNull();
    s = applyServer(s, { t: 'approval_requested', approval: { ...timed, request_id: 'req_2' } });
    expect(s.queue.map((a) => a.countdown_ms)).toEqual([null, null]);
  });

  it('deltas_upsert_and_remove_by_id', () => {
    let s = applyServer(initialState, snapshot());
    s = applyServer(s, { t: 'approval_requested', approval: approval({ request_id: 'req_2', created_at: '2026-10-23T09:00:05Z' }) });
    s = applyServer(s, { t: 'approval_requested', approval: approval({ request_id: 'req_1', created_at: '2026-10-23T09:00:01Z' }) });
    expect(s.queue.map((a) => a.request_id)).toEqual(['req_1', 'req_2']);
    // Trùng ngay sau snapshot: thay, không nhân đôi.
    s = applyServer(s, { t: 'approval_requested', approval: approval({ request_id: 'req_1', risk: 'high' }) });
    expect(s.queue).toHaveLength(2);
    expect(s.queue[0]?.risk).toBe('high');
    s = applyServer(s, { t: 'approval_resolved', request_id: 'req_1', decision: 'allow', decided_by: 'user' });
    expect(s.queue.map((a) => a.request_id)).toEqual(['req_2']);

    s = applyServer(s, { t: 'agent_updated', agent: agent({ mood: 'working' }) });
    expect(s.agents['agt_s1']?.mood).toBe('working');
    s = applyServer(s, { t: 'agent_removed', id: 'agt_s1' });
    expect(s.agents).toEqual({});

    s = applyServer(s, { t: 'approval_handed_off', handoff: handoff() });
    expect(s.handoffs).toHaveLength(1);
    s = applyServer(s, { t: 'handoff_cleared', handoff_id: 'hof_1' });
    expect(s.handoffs).toHaveLength(0);

    s = applyServer(s, { t: 'team_updated', team: { paused: true, focus: true, roster_max: 3 } });
    expect(s.team).toEqual({ paused: true, focus: true, roster_max: 3 });
  });

  it('unknown_ids_are_ignored_without_new_state', () => {
    const s = applyServer(initialState, snapshot());
    expect(applyServer(s, { t: 'approval_resolved', request_id: 'req_x', decision: null, decided_by: 'timeout' })).toBe(s);
    expect(applyServer(s, { t: 'agent_removed', id: 'agt_x' })).toBe(s);
    expect(applyServer(s, { t: 'handoff_cleared', handoff_id: 'hof_x' })).toBe(s);
  });

  it('feed_keeps_latest_first_and_capped', () => {
    let s = initialState;
    for (let i = 0; i < FEED_MAX + 2; i++) s = applyServer(s, { t: 'feed', text: `tin ${i}`, ts: '' });
    expect(s.feed).toHaveLength(FEED_MAX);
    expect(s.feed[0]?.text).toBe(`tin ${FEED_MAX + 1}`);
  });

  it('connection_action_sets_status_and_error', () => {
    const s = reducer(initialState, { type: 'connection', status: 'closed', error: 'Mất kết nối hub' });
    expect(s.connection).toBe('closed');
    expect(s.error).toBe('Mất kết nối hub');
    expect(reducer(s, { type: 'connection', status: 'open' }).error).toBeNull();
  });

  it('parse_server_msg_rejects_bad_frames', () => {
    expect(parseServerMsg('không phải json')).toBeNull();
    expect(parseServerMsg('{"t":"la_lam"}')).toBeNull();
    expect(parseServerMsg('{"t":"approval_requested"}')).toBeNull();
    expect(parseServerMsg('[1,2]')).toBeNull();
    expect(parseServerMsg(JSON.stringify(snapshot()))?.t).toBe('snapshot');
  });
});
