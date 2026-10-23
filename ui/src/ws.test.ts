import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ServerMsg } from './protocol';
import type { ConnectionStatus } from './store';
import { FakeSocket } from './test/FakeSocket';
import { snapshot } from './test/fixtures';
import { HubClient } from './ws';

beforeEach(() => {
  FakeSocket.reset();
  vi.useFakeTimers();
});
afterEach(() => vi.useRealTimers());

function client(token = 'tok/en+1') {
  const msgs: ServerMsg[] = [];
  const statuses: [ConnectionStatus, string | null][] = [];
  let calls = 0;
  const c = new HubClient({
    connection: () => {
      calls += 1;
      return Promise.resolve({ wsUrl: 'ws://127.0.0.1:7878/ws', token: `${token}${calls}` });
    },
    onMessage: (m) => msgs.push(m),
    onStatus: (s, e) => statuses.push([s, e]),
    factory: FakeSocket.factory,
    backoff: [100, 1000],
  });
  return { c, msgs, statuses };
}

describe('HubClient', () => {
  it('connects_with_encoded_token_and_forwards_messages', async () => {
    const { c, msgs, statuses } = client();
    c.start();
    await vi.runAllTimersAsync();
    const s = FakeSocket.last();
    expect(s.url).toBe('ws://127.0.0.1:7878/ws?token=tok%2Fen%2B11');
    s.open();
    s.receive(snapshot());
    s.receive('rác');
    expect(msgs.map((m) => m.t)).toEqual(['snapshot']);
    expect(statuses.at(-1)).toEqual(['open', null]);
    c.stop();
  });

  it('send_only_when_open_and_serializes_client_msg', async () => {
    const { c } = client();
    expect(c.send({ t: 'team_pause' })).toBe(false);
    c.start();
    await vi.runAllTimersAsync();
    const s = FakeSocket.last();
    expect(c.send({ t: 'team_pause' })).toBe(false);
    s.open();
    expect(c.send({ t: 'decide', request_id: 'req_1', decision: 'deny', origin: 'user' })).toBe(true);
    expect(s.sentJson()).toEqual([{ t: 'decide', request_id: 'req_1', decision: 'deny', origin: 'user' }]);
    c.stop();
  });

  it('reconnects_with_backoff_and_fresh_token', async () => {
    const { c, statuses } = client();
    c.start();
    await vi.runAllTimersAsync();
    FakeSocket.last().open();
    FakeSocket.last().drop();
    expect(statuses.at(-1)?.[0]).toBe('closed');
    expect(FakeSocket.all).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(99);
    expect(FakeSocket.all).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeSocket.all).toHaveLength(2);
    // Lấy lại thông tin kết nối mỗi lần (token đổi khi hub khởi động lại).
    expect(FakeSocket.last().url.endsWith('2')).toBe(true);
    c.stop();
  });

  it('connection_info_failure_reports_error_and_retries', async () => {
    let fail = true;
    const statuses: [ConnectionStatus, string | null][] = [];
    const c = new HubClient({
      connection: () => (fail ? Promise.reject(new Error('Hub chưa chạy')) : Promise.resolve({ wsUrl: 'ws://x/ws', token: 't' })),
      onMessage: () => {},
      onStatus: (s, e) => statuses.push([s, e]),
      factory: FakeSocket.factory,
      backoff: [50],
    });
    c.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(statuses.at(-1)).toEqual(['closed', 'Hub chưa chạy']);
    fail = false;
    await vi.advanceTimersByTimeAsync(50);
    expect(FakeSocket.all).toHaveLength(1);
    c.stop();
  });

  it('stop_closes_socket_and_never_reconnects', async () => {
    const { c } = client();
    c.start();
    await vi.runAllTimersAsync();
    const s = FakeSocket.last();
    s.open();
    c.stop();
    expect(s.closed).toBe(true);
    s.drop();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(FakeSocket.all).toHaveLength(1);
  });
});
