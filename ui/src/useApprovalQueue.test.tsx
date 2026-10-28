import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ClientMsg } from './protocol';
import { applyServer, initialState, type HubState } from './store';
import { approval, snapshot } from './test/fixtures';
import { useApprovalQueue } from './useApprovalQueue';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function timedState(): HubState {
  const q = [approval({ request_id: 'req_t', risk: 'low', reversible: true, countdown_ms: 5000 })];
  return { ...applyServer(initialState, snapshot({ queue: q })), connection: 'open' };
}

function mount(countdown: boolean | undefined) {
  vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
  vi.setSystemTime(Date.parse('2026-10-23T09:00:00Z'));
  const sent: ClientMsg[] = [];
  const send = (m: ClientMsg) => (sent.push(m), true);
  const state = timedState();
  const opts = countdown === undefined ? undefined : { countdown };
  const r = renderHook(() => useApprovalQueue(state, send, 0, opts));
  return { sent, r };
}
const wait = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

describe('useApprovalQueue countdown option (docs/PLAN.md §7.2: pet không đếm ngược)', () => {
  it('mặc định vẫn đếm ngược như popup/Office', () => {
    const { sent, r } = mount(undefined);
    expect(r.result.current.countdownOf(timedState().queue[0]!)).toBe(5000);
    wait(6000);
    expect(sent).toEqual([{ t: 'decide', request_id: 'req_t', decision: 'allow', origin: 'countdown' }]);
  });

  it('countdown: false không bao giờ gửi origin countdown và không hiện đồng hồ', () => {
    const { sent, r } = mount(false);
    expect(r.result.current.countdownOf(timedState().queue[0]!)).toBeNull();
    wait(60_000);
    expect(sent).toEqual([]);
  });

  it('countdown: false vẫn cho người dùng tự quyết với origin user', () => {
    const { sent, r } = mount(false);
    act(() => void r.result.current.decide('req_t', 'deny'));
    expect(sent).toEqual([{ t: 'decide', request_id: 'req_t', decision: 'deny', origin: 'user' }]);
  });
});
