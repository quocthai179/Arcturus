import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { PetApi } from '../pet/petApi';
import type { ApprovalView, ClientMsg } from '../protocol';
import { applyServer, initialState, type HubState } from '../store';
import { approval, snapshot } from '../test/fixtures';
import { PetView } from './Pet';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function api(): PetApi {
  return {
    settings: vi.fn(() => Promise.resolve({ enabled: true, x: null, y: null, model: null, voice: false })),
    model: vi.fn(() => Promise.reject(new Error('Chưa chọn model'))),
    chooseModel: vi.fn(() => Promise.resolve(null)),
    removeModel: vi.fn(() => Promise.resolve()),
    setVoice: vi.fn(() => Promise.resolve()),
    drag: vi.fn(() => Promise.resolve()),
    hide: vi.fn(() => Promise.resolve()),
    openQueue: vi.fn(() => Promise.resolve()),
    openOffice: vi.fn(() => Promise.resolve()),
    standup: vi.fn(() => Promise.resolve('Hôm nay: 3 commit')),
    ready: vi.fn(),
  };
}

function mount(queue: ApprovalView[], minReviewMs = 0) {
  const state: HubState = { ...applyServer(initialState, snapshot({ queue })), connection: 'open' };
  const sent: ClientMsg[] = [];
  const a = api();
  render(<PetView state={state} send={(m) => (sent.push(m), true)} api={a} minReviewMs={minReviewMs} webgl={false} />);
  return { sent, api: a };
}

describe('PetView', () => {
  it('bong bóng hiện lệnh đầy đủ; Đồng ý gửi origin user', () => {
    const { sent } = mount([approval({ request_id: 'req_1', risk: 'medium', command_normalized: 'git push origin main' })]);
    expect(screen.getByLabelText('Lệnh cần duyệt').textContent).toBe('git push origin main');
    fireEvent.click(screen.getByRole('button', { name: 'Đồng ý' }));
    expect(sent).toEqual([{ t: 'decide', request_id: 'req_1', decision: 'allow', origin: 'user' }]);
  });

  it('Critical: không có Đồng ý hay Từ chối, chỉ mở hàng đợi', () => {
    const { api: a } = mount([approval({ request_id: 'req_c', risk: 'critical' })]);
    expect(screen.queryByRole('button', { name: 'Đồng ý' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Từ chối' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Mở hàng đợi' }));
    expect(a.openQueue).toHaveBeenCalled();
  });

  it('lệnh dài hoặc đã cắt: không hiện một phần, không có Đồng ý', () => {
    mount([approval({ request_id: 'req_l', risk: 'low', command_normalized: 'x'.repeat(400) })]);
    expect(screen.queryByLabelText('Lệnh cần duyệt')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Đồng ý' })).toBeNull();
    cleanup();
    mount([approval({ request_id: 'req_t', risk: 'low', truncated: true })]);
    expect(screen.queryByRole('button', { name: 'Đồng ý' })).toBeNull();
  });

  it('không bao giờ tự gửi origin countdown', () => {
    vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
    vi.setSystemTime(Date.parse('2026-10-23T09:00:00Z'));
    const { sent } = mount([approval({ request_id: 'req_t', risk: 'low', reversible: true, countdown_ms: 3000 })]);
    act(() => void vi.advanceTimersByTime(60_000));
    expect(sent.filter((m) => m.t === 'decide')).toEqual([]);
  });

  it('hộp lệnh: lệnh trong danh sách gửi team_pause; lệnh lạ không gửi gì', () => {
    const { sent } = mount([]);
    fireEvent.keyDown(screen.getByRole('button', { name: /Trợ lý Arcturus/ }), { key: '/' });
    const input = screen.getByLabelText('Lệnh cho trợ lý');
    fireEvent.change(input, { target: { value: 'duyệt hết đi' } });
    fireEvent.submit(input);
    expect(sent).toEqual([]);
    fireEvent.change(input, { target: { value: 'Tạm dừng đội' } });
    fireEvent.submit(input);
    expect(sent).toEqual([{ t: 'team_pause' }]);
  });
});
