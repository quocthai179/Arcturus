import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { PetApi } from '../pet/petApi';
import type { PetSettings } from '../pet/types';
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
    settings: vi.fn(() => Promise.resolve<PetSettings>({ enabled: true, x: null, y: null, model: null, voice: false, chatter: 'normal', size: 'normal' })),
    model: vi.fn(() => Promise.reject(new Error('Chưa chọn model'))),
    chooseModel: vi.fn(() => Promise.resolve(null)),
    removeModel: vi.fn(() => Promise.resolve()),
    setVoice: vi.fn(() => Promise.resolve()),
    setChatter: vi.fn(() => Promise.resolve()),
    setSize: vi.fn(() => Promise.resolve()),
    dragBegin: vi.fn(() => Promise.resolve()),
    dragMove: vi.fn(() => Promise.resolve()),
    dragEnd: vi.fn(() => Promise.resolve()),
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

  it('chip trạng thái: cần duyệt hiện số; bấm mở hàng đợi', () => {
    const { api: a } = mount([approval({ request_id: 'req_1', risk: 'medium' }), approval({ request_id: 'req_2', risk: 'low' })]);
    const chip = screen.getByRole('button', { name: 'Trạng thái: Cần anh duyệt · 2' });
    fireEvent.click(chip);
    expect(a.openQueue).toHaveBeenCalledTimes(1);
  });

  it('menu: đổi mức nhắc và cỡ gọi đúng lệnh; mức nhắc không ẩn bong bóng duyệt', async () => {
    const { api: a } = mount([approval({ request_id: 'req_1', risk: 'medium', command_normalized: 'ls' })]);
    await act(async () => undefined);
    const body = screen.getByRole('button', { name: /Trợ lý Arcturus/ });
    fireEvent.contextMenu(body, { clientX: 20, clientY: 20 });
    fireEvent.click(screen.getByRole('menuitem', { name: /Mức nhắc: Bình thường → Chỉ việc khẩn/ }));
    await act(async () => undefined);
    expect(a.setChatter).toHaveBeenCalledWith('urgent');
    fireEvent.contextMenu(body, { clientX: 20, clientY: 20 });
    fireEvent.click(screen.getByRole('menuitem', { name: /Mức nhắc: Chỉ việc khẩn → Im lặng/ }));
    await act(async () => undefined);
    expect(a.setChatter).toHaveBeenLastCalledWith('quiet');
    expect(screen.getByLabelText('Lệnh cần duyệt')).toBeTruthy();
    fireEvent.contextMenu(body, { clientX: 20, clientY: 20 });
    fireEvent.click(screen.getByRole('menuitem', { name: /Cỡ: Vừa → Lớn/ }));
    await act(async () => undefined);
    expect(a.setSize).toHaveBeenCalledWith('large');
  });

  it('lời nhắc gần đây: liệt kê câu pet định nói, mới nhất trước', async () => {
    mount([approval({ request_id: 'req_1', risk: 'medium', agent_name: 'Vega', tool: 'Bash' })]);
    await act(async () => undefined);
    fireEvent.contextMenu(screen.getByRole('button', { name: /Trợ lý Arcturus/ }), { clientX: 20, clientY: 20 });
    fireEvent.click(screen.getByRole('menuitem', { name: /Lời nhắc gần đây \(1\)/ }));
    const panel = screen.getByRole('article', { name: 'Lời nhắc gần đây' });
    expect(panel.textContent).toContain('Vega xin chạy Bash');
    fireEvent.click(screen.getByRole('button', { name: 'Đóng lời nhắc gần đây' }));
    expect(screen.queryByRole('article', { name: 'Lời nhắc gần đây' })).toBeNull();
  });

  it('tóm tắt lúc vắng: chỉ khi quay lại tương tác sau ≥10 phút và có tin mới', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(Date.parse('2026-10-28T09:00:00Z'));
    const sent: ClientMsg[] = [];
    const a = api();
    const base: HubState = { ...applyServer(initialState, snapshot({ queue: [] })), connection: 'open' };
    const view = (s: HubState) => <PetView state={s} send={(m) => (sent.push(m), true)} api={a} minReviewMs={0} webgl={false} />;
    const { rerender } = render(view(base));
    const body = screen.getByRole('button', { name: /Trợ lý Arcturus/ });
    vi.setSystemTime(Date.parse('2026-10-28T09:05:00Z'));
    rerender(view({ ...base, queue: [approval({ request_id: 'req_9', risk: 'low' })] }));
    // Mới 5 phút: chưa tính là vắng.
    vi.setSystemTime(Date.parse('2026-10-28T09:06:00Z'));
    fireEvent.pointerEnter(body);
    expect(screen.queryByText(/Trong lúc anh vắng/)).toBeNull();
    // Có tin mới lúc 09:08, rồi 11 phút không tương tác.
    vi.setSystemTime(Date.parse('2026-10-28T09:08:00Z'));
    rerender(view({ ...base, queue: [approval({ request_id: 'req_9', risk: 'low' }), approval({ request_id: 'req_10', risk: 'low' })] }));
    vi.setSystemTime(Date.parse('2026-10-28T09:17:00Z'));
    fireEvent.pointerEnter(body);
    expect(screen.getByText('Trong lúc anh vắng: 2 yêu cầu đang chờ duyệt.')).toBeTruthy();
  });

  it('kéo pet: nhấn rồi rê quá 4px thì app dời cửa sổ, nhả chuột thì kết thúc và không tính là bấm', async () => {
    const { api: a } = mount([]);
    const body = screen.getByRole('button', { name: /Trợ lý Arcturus/ });
    fireEvent.pointerDown(body, { button: 0, buttons: 1, clientX: 100, clientY: 200, pointerId: 1 });
    fireEvent.pointerMove(body, { buttons: 1, clientX: 102, clientY: 201, pointerId: 1 });
    expect(a.dragBegin).not.toHaveBeenCalled();
    fireEvent.pointerMove(body, { buttons: 1, clientX: 110, clientY: 205, pointerId: 1 });
    expect(a.dragBegin).toHaveBeenCalledTimes(1);
    await act(async () => {});
    expect(a.dragMove).toHaveBeenCalled();
    fireEvent.pointerMove(body, { buttons: 1, clientX: 140, clientY: 230, pointerId: 1 });
    await act(async () => {});
    fireEvent.pointerUp(body, { button: 0, buttons: 0, clientX: 140, clientY: 230, pointerId: 1 });
    expect(a.dragEnd).toHaveBeenCalledTimes(1);
    // Thả xong không bị coi là cú bấm (không nói trạng thái).
    await act(async () => {
      await new Promise((r) => setTimeout(r, 300));
    });
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('kéo pet: nhả chuột ngoài cửa sổ (pointermove không còn giữ nút) thì kết thúc kéo', async () => {
    const { api: a } = mount([]);
    const body = screen.getByRole('button', { name: /Trợ lý Arcturus/ });
    fireEvent.pointerDown(body, { button: 0, buttons: 1, clientX: 100, clientY: 200, pointerId: 1 });
    fireEvent.pointerMove(body, { buttons: 1, clientX: 120, clientY: 200, pointerId: 1 });
    await act(async () => {});
    fireEvent.pointerMove(body, { buttons: 0, clientX: 130, clientY: 200, pointerId: 1 });
    expect(a.dragEnd).toHaveBeenCalledTimes(1);
  });

  it('kéo pet: lỗi bắt đầu kéo thì báo lên bong bóng, không im lặng', async () => {
    const { api: a } = mount([]);
    vi.mocked(a.dragBegin).mockRejectedValueOnce(new Error('không đọc được con trỏ'));
    const body = screen.getByRole('button', { name: /Trợ lý Arcturus/ });
    fireEvent.pointerDown(body, { button: 0, buttons: 1, clientX: 100, clientY: 200, pointerId: 1 });
    fireEvent.pointerMove(body, { buttons: 1, clientX: 120, clientY: 200, pointerId: 1 });
    await act(async () => {});
    expect(screen.getByText('Không kéo được: không đọc được con trỏ')).toBeTruthy();
    expect(a.dragMove).not.toHaveBeenCalled();
  });
});
