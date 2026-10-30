import { describe, expect, it } from 'vitest';
import type { Mood } from '../protocol';
import { SEATS } from './layout';
import {
  advance,
  createSim,
  MAX_ON_FLOOR,
  nextWakeAt,
  partnerOf,
  seeded,
  setMotion,
  syncAgents,
  wakeAll,
  type Actor,
  type Sim,
  type SimAgent,
} from './sim';

const ag = (id: string, mood: Mood = 'idle', small = false): SimAgent => ({ id, mood, small });

/** Chạy mô phỏng theo đúng cách bộ vẽ chạy: nhảy tới mốc kế tiếp. */
function runUntil(sim: Sim, from: number, until: number, onStep?: (now: number) => void): number {
  let now = from;
  for (let i = 0; i < 100_000; i++) {
    const wake = nextWakeAt(sim);
    if (!Number.isFinite(wake) && wake > 0) return now;
    now = Math.max(now, wake);
    if (now > until) return until;
    advance(sim, now);
    onStep?.(now);
  }
  throw new Error('mô phỏng không dừng');
}

function actor(sim: Sim, id: string): Actor {
  const a = sim.actors.get(id);
  if (a === undefined) throw new Error(`không có ${id}`);
  return a;
}

describe('live office sim', () => {
  it('working_agent_sits_at_its_own_desk_on_first_open', () => {
    const sim = createSim({ rng: seeded(1) });
    syncAgents(sim, [ag('a', 'working'), ag('b', 'approval')], 0);
    const a = actor(sim, 'a');
    expect(a.seat).toBe(0);
    expect({ x: a.x, y: a.y }).toEqual(SEATS[0]?.at);
    expect(a.pose).toBe('sit');
    const b = actor(sim, 'b');
    expect(b.pose).toBe('raise');
    expect(b.bubble).toEqual({ text: '!', kind: 'alert' });
    // Không ai rảnh: không có mốc nào, không timer.
    expect(nextWakeAt(sim)).toBe(Infinity);
  });

  it('idle_agent_wanders_then_is_called_back_and_walks_to_its_desk', () => {
    const sim = createSim({ rng: seeded(2) });
    syncAgents(sim, [ag('a', 'idle')], 0);
    const seen = new Set<string>();
    runUntil(sim, 0, 60_000, () => seen.add(actor(sim, 'a').pose));
    expect(seen.has('walk')).toBe(true);
    expect(actor(sim, 'a').task?.k).not.toBe('desk');

    syncAgents(sim, [ag('a', 'working')], 60_000);
    const a = actor(sim, 'a');
    expect(a.task?.k).toBe('desk');
    expect(a.bubble?.kind).toBe('say');
    runUntil(sim, 60_000, 120_000);
    expect(a.pose).toBe('sit');
    expect({ x: a.x, y: a.y }).toEqual(SEATS[a.seat ?? -1]?.at);
  });

  it('desk_is_stable_across_mood_changes_and_freed_when_agent_leaves', () => {
    const sim = createSim({ rng: seeded(3) });
    syncAgents(sim, [ag('a', 'working'), ag('b', 'working')], 0);
    expect(actor(sim, 'b').seat).toBe(1);
    syncAgents(sim, [ag('a', 'idle'), ag('b', 'approval')], 1000);
    expect(actor(sim, 'b').seat).toBe(1);
    syncAgents(sim, [ag('b', 'approval')], 2000);
    expect(sim.seats[0]).toBeNull();
    syncAgents(sim, [ag('b', 'approval'), ag('c', 'working')], 3000);
    expect(actor(sim, 'c').seat).toBe(0);
  });

  it('two_idle_agents_end_up_chatting_with_alternating_bubbles', () => {
    const sim = createSim({ rng: seeded(4) });
    syncAgents(sim, [ag('a'), ag('b')], 0);
    const speakers: string[] = [];
    let chatted = false;
    runUntil(sim, 0, 4 * 60_000, () => {
      const a = actor(sim, 'a');
      const b = actor(sim, 'b');
      if (partnerOf(sim, a) === 'b' && partnerOf(sim, b) === 'a') chatted = true;
      for (const x of [a, b]) {
        if (x.task?.k === 'chat' && x.bubble?.kind === 'say' && x.bubbleUntil === null && speakers.at(-1) !== x.id) {
          speakers.push(x.id);
        }
      }
    });
    expect(chatted).toBe(true);
    expect(speakers.length).toBeGreaterThanOrEqual(2);
    // Luân phiên: không ai nói hai câu liền nhau.
    for (let i = 1; i < speakers.length; i++) expect(speakers[i]).not.toBe(speakers[i - 1]);
  });

  it('called_to_work_mid_chat_partner_is_not_left_waiting', () => {
    const sim = createSim({ rng: seeded(5) });
    syncAgents(sim, [ag('a'), ag('b')], 0);
    let now = 0;
    for (let i = 0; i < 20_000 && sim.convs.size === 0; i++) {
      now = Math.max(now, nextWakeAt(sim));
      advance(sim, now);
    }
    expect(sim.convs.size).toBe(1);
    syncAgents(sim, [ag('a', 'working'), ag('b')], now);
    expect(sim.convs.size).toBe(0);
    expect(actor(sim, 'a').task?.k).toBe('desk');
    expect(actor(sim, 'b').task?.k).not.toBe('chat');
    runUntil(sim, now, now + 30_000);
    expect(actor(sim, 'a').pose).toBe('sit');
  });

  it('after_the_break_everyone_naps_and_no_timer_is_left', () => {
    const sim = createSim({ rng: seeded(6), wanderMs: 60_000 });
    syncAgents(sim, [ag('a'), ag('b'), ag('c'), ag('d', 'done')], 0);
    const end = runUntil(sim, 0, 10 * 60_000);
    expect(end).toBeLessThan(10 * 60_000);
    expect(nextWakeAt(sim)).toBe(Infinity);
    for (const a of sim.actors.values()) {
      expect(a.task?.k).toBe('nap');
      expect(a.pose).toBe('sleep');
    }
    const naps = [...sim.actors.values()].map((a) => a.napSpot);
    expect(new Set(naps).size).toBe(naps.length);

    expect(wakeAll(sim, end)).toBe(true);
    expect(Number.isFinite(nextWakeAt(sim))).toBe(true);
  });

  it('busy_agents_at_their_desk_keep_no_timer_while_idle_ones_nap', () => {
    const sim = createSim({ rng: seeded(7), wanderMs: 30_000 });
    syncAgents(sim, [ag('a', 'working'), ag('b')], 0);
    runUntil(sim, 0, 5 * 60_000);
    expect(nextWakeAt(sim)).toBe(Infinity);
    expect(actor(sim, 'a').pose).toBe('sit');
  });

  it('motion_off_snaps_everyone_to_their_destination_without_timers', () => {
    const sim = createSim({ rng: seeded(8) });
    syncAgents(sim, [ag('a', 'working'), ag('b'), ag('c')], 0);
    runUntil(sim, 0, 20_000);
    expect(setMotion(sim, false, 20_000)).toBe(true);
    expect(nextWakeAt(sim)).toBe(Infinity);
    expect(sim.convs.size).toBe(0);
    for (const a of sim.actors.values()) {
      expect(a.legMs).toBe(0);
      expect(a.pose === 'sit' || a.pose === 'sleep').toBe(true);
    }
    // Mood đổi khi đang tắt chuyển động: đặt thẳng vào bàn.
    syncAgents(sim, [ag('a', 'working'), ag('b', 'working'), ag('c')], 21_000);
    const b = actor(sim, 'b');
    expect(b.pose).toBe('sit');
    expect({ x: b.x, y: b.y }).toEqual(SEATS[b.seat ?? -1]?.at);
    expect(nextWakeAt(sim)).toBe(Infinity);
    // Bật lại: người rảnh tiếp tục giải lao.
    setMotion(sim, true, 22_000);
    expect(Number.isFinite(nextWakeAt(sim))).toBe(true);
  });

  it('caps_the_floor_and_counts_the_rest_as_overflow', () => {
    const sim = createSim({ rng: seeded(9) });
    const agents = Array.from({ length: MAX_ON_FLOOR + 3 }, (_, i) => ag(`a${String(i).padStart(2, '0')}`, 'working'));
    syncAgents(sim, agents, 0);
    expect(sim.actors.size).toBe(MAX_ON_FLOOR);
    expect(sim.overflow).toBe(3);
    // Agent không có chỗ ngồi vẫn làm việc (đứng cạnh bảng trắng), không có mốc treo.
    expect([...sim.actors.values()].filter((a) => a.seat === null)).toHaveLength(MAX_ON_FLOOR - SEATS.length);
    expect(nextWakeAt(sim)).toBe(Infinity);
  });

  it('new_agent_walks_in_from_the_door_after_the_office_is_open', () => {
    const sim = createSim({ rng: seeded(10) });
    syncAgents(sim, [ag('a', 'working')], 0);
    syncAgents(sim, [ag('a', 'working'), ag('b', 'working')], 1000);
    const b = actor(sim, 'b');
    expect(b.pose).toBe('walk');
    expect(b.bubble?.text).toBe('Chào cả nhà!');
    runUntil(sim, 1000, 60_000);
    expect(b.pose).toBe('sit');
  });

  it('done_agent_celebrates_at_the_desk_then_goes_on_break', () => {
    const sim = createSim({ rng: seeded(11) });
    syncAgents(sim, [ag('a', 'working')], 0);
    syncAgents(sim, [ag('a', 'done')], 1000);
    const a = actor(sim, 'a');
    expect(a.pose).toBe('stand');
    expect(a.bubble?.kind).toBe('say');
    runUntil(sim, 1000, 30_000);
    expect(a.task?.k).not.toBe('desk');
  });
});
