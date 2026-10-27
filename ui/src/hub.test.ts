import { afterEach, describe, expect, it, vi } from 'vitest';

const core = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => core);

import { reportPopupReady } from './hub';

afterEach(() => {
  core.invoke.mockReset();
  core.isTauri.mockReset();
});

describe('reportPopupReady', () => {
  it('gọi lệnh popup_ready trong app', () => {
    core.isTauri.mockReturnValue(true);
    core.invoke.mockResolvedValue(undefined);
    reportPopupReady();
    expect(core.invoke).toHaveBeenCalledWith('popup_ready');
  });

  it('không làm gì ngoài app (trình duyệt dev)', () => {
    core.isTauri.mockReturnValue(false);
    reportPopupReady();
    expect(core.invoke).not.toHaveBeenCalled();
  });

  it('nuốt lỗi của lệnh để không làm hỏng popup', async () => {
    core.isTauri.mockReturnValue(true);
    core.invoke.mockRejectedValue(new Error('boom'));
    expect(() => reportPopupReady()).not.toThrow();
    await Promise.resolve();
  });
});
