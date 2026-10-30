import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SelfTestResult, SetupApi, SetupPreview, SetupStatus } from '../setup/setupApi';
import { applyServer, initialState, type HubState } from '../store';
import { agent, snapshot } from '../test/fixtures';
import { SetupView, UI_WAIT_MS } from './Setup';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const NOT_INSTALLED: SetupStatus = {
  path: '/home/u/.claude/settings.json',
  exists: true,
  up_to_date: false,
  arc_hooks: 0,
  problems: ['Chưa cài hook của Arcturus'],
  error: null,
  hub_port: 7878,
};

const INSTALLED: SetupStatus = { ...NOT_INSTALLED, up_to_date: true, arc_hooks: 12, problems: [] };

const PREVIEW: SetupPreview = {
  action: 'install',
  path: NOT_INSTALLED.path,
  changed: true,
  diff: [
    { op: 'ctx', text: '{' },
    { op: 'add', text: '  "hooks": { "Authorization": "Bearer <token …cdef>" }' },
    { op: 'skip', text: '… 4 dòng không đổi' },
  ],
  digest: 'abc123',
};

const PASSED: SelfTestResult = {
  steps: [
    { id: 'settings', state: 'ok', detail: 'Đã cài 12 hook' },
    { id: 'hub', state: 'ok', detail: 'Hub đang chạy ở 127.0.0.1:7878' },
    { id: 'hook', state: 'ok', detail: 'Hub nhận sự kiện thử' },
  ],
  agent_id: 'agt_arcturus-selftest-0123456789abcdef',
  nonce: '0123456789abcdef',
};

function api(over: Partial<SetupApi> = {}): SetupApi {
  return {
    status: vi.fn(() => Promise.resolve(NOT_INSTALLED)),
    preview: vi.fn(() => Promise.resolve(PREVIEW)),
    apply: vi.fn(() => Promise.resolve({ changed: true, backup: '/home/u/.claude/settings.json.bak-1' })),
    selfTest: vi.fn(() => Promise.resolve(PASSED)),
    cleanup: vi.fn(() => Promise.resolve()),
    ...over,
  };
}

function hub(): HubState {
  return { ...applyServer(initialState, snapshot()), connection: 'open' };
}

async function mount(a: SetupApi, state: HubState = hub()) {
  const r = render(<SetupView state={state} api={a} />);
  await act(async () => {});
  return r;
}

describe('SetupView', () => {
  it('hiện vấn đề, xem trước diff rồi mới cài đúng digest đã xem', async () => {
    const a = api();
    await mount(a);
    expect(screen.getByLabelText('Vấn đề').textContent).toContain('Chưa cài hook');
    expect(screen.getByRole('button', { name: 'Gỡ hook…' })).toHaveProperty('disabled', true);
    expect(a.apply).not.toHaveBeenCalled();

    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Xem trước khi cài' })));
    expect(a.preview).toHaveBeenCalledWith('install');
    const diff = screen.getByLabelText('Thay đổi');
    expect(diff.textContent).toContain('+   "hooks"');
    expect(diff.textContent).toContain('<token …cdef>');
    expect(diff.querySelector('.diff--add')).not.toBeNull();

    vi.mocked(a.status).mockResolvedValue(INSTALLED);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Đồng ý cài' })));
    expect(a.apply).toHaveBeenCalledWith('install', 'abc123');
    expect(screen.getByRole('status').textContent).toContain('settings.json.bak-1');
    expect(screen.getByText(/Đã cài 12 hook/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Gỡ hook…' })).toHaveProperty('disabled', false);
  });

  it('hủy xem trước thì không ghi gì', async () => {
    const a = api();
    await mount(a);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Xem trước khi cài' })));
    fireEvent.click(screen.getByRole('button', { name: 'Hủy' }));
    expect(a.apply).not.toHaveBeenCalled();
    expect(screen.queryByLabelText('Thay đổi')).toBeNull();
  });

  it('gỡ cũng phải qua xem trước', async () => {
    const a = api({
      status: vi.fn(() => Promise.resolve(INSTALLED)),
      preview: vi.fn(() => Promise.resolve({ ...PREVIEW, action: 'uninstall' as const })),
    });
    await mount(a);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Gỡ hook…' })));
    expect(a.preview).toHaveBeenCalledWith('uninstall');
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Đồng ý gỡ' })));
    expect(a.apply).toHaveBeenCalledWith('uninstall', 'abc123');
  });

  it('settings.json hỏng: báo lỗi, khóa nút cài', async () => {
    await mount(api({ status: vi.fn(() => Promise.resolve({ ...NOT_INSTALLED, error: 'settings.json không phải JSON hợp lệ' })) }));
    expect(screen.getByText(/không phải JSON hợp lệ/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Xem trước khi cài' })).toHaveProperty('disabled', true);
  });

  it('lỗi khi ghi (file vừa đổi) hiện cho người dùng', async () => {
    const a = api({ apply: vi.fn(() => Promise.reject(new Error('settings.json vừa thay đổi sau lúc xem trước'))) });
    await mount(a);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Xem trước khi cài' })));
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Đồng ý cài' })));
    expect(screen.getByRole('alert').textContent).toContain('vừa thay đổi');
  });

  it('tự kiểm tra: đạt khi agent thử tới giao diện, rồi dọn phiên thử', async () => {
    const a = api({ status: vi.fn(() => Promise.resolve(INSTALLED)) });
    const r = await mount(a);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Chạy tự kiểm tra' })));
    const ui = () => document.querySelector('[data-step="ui"]');
    expect(ui()?.className).toContain('step--running');
    expect(a.cleanup).not.toHaveBeenCalled();

    const seen = applyServer(hub(), { t: 'agent_updated', agent: agent({ id: 'agt_arcturus-selftest-0123456789abcdef' }) });
    await act(async () => r.rerender(<SetupView state={seen} api={a} />));
    expect(ui()?.className).toContain('step--ok');
    expect(a.cleanup).toHaveBeenCalledWith('0123456789abcdef');
    expect(a.cleanup).toHaveBeenCalledTimes(1);
    expect(screen.getByText(/Mọi thứ đã nối/)).toBeTruthy();
  });

  it('tự kiểm tra: giao diện không thấy sự kiện thì báo hỏng sau thời hạn, vẫn dọn', async () => {
    vi.useFakeTimers();
    const a = api({ status: vi.fn(() => Promise.resolve(INSTALLED)) });
    await mount(a);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Chạy tự kiểm tra' })));
    await act(async () => {
      vi.advanceTimersByTime(UI_WAIT_MS);
    });
    const ui = document.querySelector('[data-step="ui"]');
    expect(ui?.className).toContain('step--fail');
    expect(ui?.textContent).toContain('giao diện không thấy');
    expect(a.cleanup).toHaveBeenCalledWith('0123456789abcdef');
    expect(screen.queryByText(/Mọi thứ đã nối/)).toBeNull();
  });

  it('tự kiểm tra: hook hỏng thì bỏ qua bước giao diện, không dọn gì', async () => {
    const a = api({
      selfTest: vi.fn(() =>
        Promise.resolve<SelfTestResult>({
          steps: [
            { id: 'settings', state: 'ok', detail: '' },
            { id: 'hub', state: 'ok', detail: '' },
            { id: 'hook', state: 'fail', detail: 'Hub từ chối token của hook (401)' },
          ],
          agent_id: null,
          nonce: null,
        }),
      ),
    });
    await mount(a);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Chạy tự kiểm tra' })));
    expect(document.querySelector('[data-step="hook"]')?.textContent).toContain('401');
    expect(document.querySelector('[data-step="ui"]')?.className).toContain('step--skip');
    expect(a.cleanup).not.toHaveBeenCalled();
    expect(screen.queryByText(/Mọi thứ đã nối/)).toBeNull();
  });
});
