import { describe, expect, it } from 'vitest';
import type { AgentView, ApprovalView, Risk } from '../protocol';
import { initialState, type HubState } from '../store';
import { SLEEP_AFTER_MS, SPEAK_GAP_MS, cleanText, initialMemory, think, type BrainMemory } from './brain';

function agent(id: string, mood: AgentView['mood'], extra: Partial<AgentView> = {}): AgentView {
  return {
    id,
    parent_id: null,
    kind: 'session',
    name: id,
    role: '',
    palette: { hair: '#000000', outfit: '#111111', acc: '#222222' },
    mood,
    activity: 'idle',
    act_text: '',
    host: 'h',
    source: 'claude-code',
    cwd: '/w',
    context_pct: null,
    last_event_at: '2026-10-28T00:00:00Z',
    ...extra,
  };
}

function approval(id: string, risk: Risk, created = '2026-10-28T00:00:01Z'): ApprovalView {
  return {
    request_id: id,
    agent_id: 'vega',
    agent_name: 'Vega',
    tool: 'Bash',
    command_original: 'git push origin main',
    tool_options: {},
    command_normalized: 'git push origin main',
    danger_spans: [],
    suspicions: [],
    truncated: false,
    risk,
    reversible: false,
    group_key: 'g',
    created_at: created,
    deadline_at: '2026-10-28T00:02:00Z',
    countdown_ms: null,
  };
}

function hub(patch: Partial<HubState> = {}): HubState {
  return { ...initialState, connection: 'open', synced: true, ...patch };
}

/** Chạy liên tiếp nhiều trạng thái, trả lời nói và tâm trạng của từng bước. */
function run(states: HubState[], start = 1_000_000, step = SPEAK_GAP_MS) {
  let mem: BrainMemory = initialMemory(start);
  return states.map((s, i) => {
    const out = think(s, mem, start + i * step);
    mem = out.memory;
    return out;
  });
}

describe('think', () => {
  it('có yêu cầu duyệt: giơ tay, chỉ tới yêu cầu cũ nhất, báo đúng một lần, lệnh không nằm trong lời nói', () => {
    const q = hub({ queue: [approval('req_1', 'medium'), approval('req_2', 'low', '2026-10-28T00:00:05Z')] });
    const [a, b] = run([q, q]);
    expect(a?.mood).toBe('approval');
    expect(a?.approval?.request_id).toBe('req_1');
    expect(a?.visibleCount).toBe(2);
    expect(a?.speech?.text).toBe('Vega xin chạy Bash (rủi ro vừa).');
    expect(a?.speech?.text).not.toContain('git push');
    expect(b?.speech).toBeNull();
    expect(b?.approval?.request_id).toBe('req_1');
  });

  it('Critical: tâm trạng nghiêm, câu khẩn, hướng người dùng mở hàng đợi', () => {
    const [a] = run([hub({ queue: [approval('req_9', 'critical')] })]);
    expect(a?.mood).toBe('critical');
    expect(a?.speech?.priority).toBe('high');
    expect(a?.speech?.text).toContain('vùng khóa');
  });

  it('giờ tập trung: chỉ báo High/Critical, các yêu cầu khác không làm pet giơ tay', () => {
    const team = { paused: false, focus: true, roster_max: 5 };
    const [low] = run([hub({ team, queue: [approval('req_1', 'low')], agents: { a: agent('a', 'working') } })]);
    expect(low?.approval).toBeNull();
    expect(low?.mood).toBe('working');
    expect(low?.speech).toBeNull();
    const [high] = run([hub({ team, queue: [approval('req_1', 'low'), approval('req_2', 'high', '2026-10-28T00:00:09Z')] })]);
    expect(high?.approval?.request_id).toBe('req_2');
    expect(high?.speech?.priority).toBe('high');
  });

  it('agent lỗi / xong: nói một lần lúc chuyển, chuỗi không tin cậy được làm sạch', () => {
    const base = hub({ agents: { vega: agent('vega', 'working') } });
    const err = hub({ agents: { vega: agent('vega', 'error', { name: 'Ve‮ga', act_text: 'clippy   báo\n2 cảnh báo' }) } });
    const [, e, again] = run([base, err, err]);
    expect(e?.mood).toBe('error');
    expect(e?.speech?.text).toBe('Ve⟨U+202E⟩ga gặp lỗi: clippy báo 2 cảnh báo');
    expect(again?.speech).toBeNull();
    const done = hub({ agents: { vega: agent('vega', 'done') } });
    const [, d] = run([base, done]);
    expect(d?.mood).toBe('done');
    expect(d?.speech?.text).toBe('vega xong việc rồi.');
  });

  it('giãn cách câu thường; câu khẩn không phải chờ', () => {
    let mem = initialMemory(0);
    const s1 = hub({ agents: { a: agent('a', 'error') } });
    const o1 = think(s1, mem, 10_000);
    expect(o1.speech).not.toBeNull();
    mem = o1.memory;
    const o2 = think(hub({ agents: { a: agent('a', 'error'), b: agent('b', 'error') } }), mem, 11_000);
    expect(o2.speech).toBeNull();
    const o3 = think(hub({ queue: [approval('req_x', 'high')] }), o2.memory, 11_500);
    expect(o3.speech?.priority).toBe('high');
  });

  it('mất kết nối: báo đúng một lần, pet lo lắng, không chỉ tới yêu cầu nào', () => {
    const on = hub({ queue: [approval('req_1', 'low')] });
    const off: HubState = { ...on, connection: 'closed', synced: true };
    const [, a, b] = run([on, off, off]);
    expect(a?.mood).toBe('error');
    expect(a?.approval).toBeNull();
    expect(a?.speech?.text).toContain('Mất kết nối hub');
    expect(b?.speech).toBeNull();
  });

  it('chưa từng kết nối: chỉ chờ, không lo lắng', () => {
    const [a] = run([{ ...initialState }]);
    expect(a?.mood).toBe('idle');
    expect(a?.speech).toBeNull();
  });

  it('tạm dừng: báo lúc chuyển, tâm trạng paused khi không có yêu cầu', () => {
    const paused = hub({ team: { paused: true, focus: false, roster_max: 5 } });
    const [, p, again] = run([hub(), paused, paused]);
    expect(p?.mood).toBe('paused');
    expect(p?.speech?.text).toContain('tạm dừng');
    expect(again?.speech).toBeNull();
  });

  it('rảnh lâu thì ngủ; có thay đổi thì thức', () => {
    const idle = hub({ agents: { a: agent('a', 'idle') } });
    let mem = initialMemory(0);
    mem = think(idle, mem, 0).memory;
    expect(think(idle, mem, SLEEP_AFTER_MS - 1).mood).toBe('idle');
    const asleep = think(idle, mem, SLEEP_AFTER_MS);
    expect(asleep.mood).toBe('sleep');
    expect(think(hub({ agents: { a: agent('a', 'working') } }), asleep.memory, SLEEP_AFTER_MS + 1).mood).toBe('working');
  });
});

describe('cleanText', () => {
  it('cắt theo code point, không cắt đôi emoji', () => {
    expect(cleanText('😀'.repeat(10), 4)).toBe('😀😀😀…');
  });
});
