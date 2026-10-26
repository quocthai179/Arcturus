import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { approval, agent } from '../test/fixtures';
import { allowGate, ApprovalCard, type Gate } from './ApprovalCard';

afterEach(cleanup);

const OPEN: Gate = { ok: true, reason: null };

function card(a = approval(), gate: Gate = OPEN, extra: Partial<Parameters<typeof ApprovalCard>[0]> = {}) {
  const onDecide = vi.fn();
  const loadFull = vi.fn(() => Promise.resolve('{"content":"toàn bộ"}'));
  const onReviewed = vi.fn();
  const r = render(
    <ApprovalCard
      approval={a}
      agent={agent()}
      selected={false}
      gate={gate}
      now={Date.parse('2026-10-23T09:00:00Z')}
      onSelect={() => {}}
      onDecide={onDecide}
      loadFull={loadFull}
      onReviewed={onReviewed}
      {...extra}
    />,
  );
  return { ...r, onDecide, loadFull, onReviewed };
}

function commandBlock(): HTMLElement {
  return screen.getByLabelText('Lệnh cần duyệt');
}

describe('ApprovalCard', () => {
  it('shows_full_command_normalized_never_shortened', () => {
    const long = `echo ${'x'.repeat(3000)} && ls`;
    card(approval({ command_normalized: long }));
    expect(commandBlock().textContent).toBe(long);
  });

  it('highlights_danger_spans_by_code_point_with_non_bmp', () => {
    const cmd = 'echo "🚀 thư mục" && rm -rf đầu-ra';
    card(approval({ command_normalized: cmd, danger_spans: [[20, 33]], risk: 'critical' }));
    const marks = commandBlock().querySelectorAll('mark');
    expect(marks).toHaveLength(1);
    expect(marks[0]?.textContent).toBe('rm -rf đầu-ra');
    expect(commandBlock().textContent).toBe(cmd);
    expect(screen.getByText('Vùng khóa')).toBeTruthy();
  });

  it('risk_label_is_text_not_only_color', () => {
    for (const [risk, label] of [
      ['low', 'Rủi ro thấp'],
      ['medium', 'Rủi ro vừa'],
      ['high', 'Rủi ro cao'],
      ['critical', 'Vùng khóa'],
    ] as const) {
      const { unmount } = card(approval({ risk }));
      expect(screen.getByText(label)).toBeTruthy();
      unmount();
    }
  });

  it('hidden_chars_shows_warning_and_original_with_codes', () => {
    card(
      approval({
        command_original: 'r​m -rf x',
        command_normalized: 'r⟨U+200B⟩m -rf x',
        suspicions: ['hidden_chars'],
        risk: 'critical',
      }),
    );
    expect(screen.getByText('Chứa ký tự ẩn')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Xem bản gốc' }));
    const original = screen.getByLabelText('Bản gốc, ký tự ẩn hiện bằng mã');
    expect(original.textContent).toBe('r⟨U+200B⟩m -rf x');
    expect(original.textContent?.includes('​')).toBe(false);
  });

  it('truncated_shows_view_full_which_loads_and_marks_reviewed', async () => {
    const { loadFull, onReviewed } = card(approval({ tool: 'Write', truncated: true }));
    expect(screen.getByText('Nội dung đã cắt')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Xem đầy đủ' }));
    await waitFor(() => expect(screen.getByLabelText('Bản đầy đủ').textContent).toBe('{"content":"toàn bộ"}'));
    expect(loadFull).toHaveBeenCalledWith('req_1');
    expect(onReviewed).toHaveBeenCalledTimes(1);
  });

  it('view_full_reveals_hidden_and_bidi_chars_as_codes', async () => {
    // Hub trả tool_input thô: U+202E (đảo chiều chữ) và U+200B (rộng 0) không được hiện nguyên trên thẻ.
    const raw = '{\n  "command": "echo a‮b​c"\n}';
    const loadFull = vi.fn(() => Promise.resolve(raw));
    card(approval({ tool: 'Write', truncated: true }), OPEN, { loadFull });
    fireEvent.click(screen.getByRole('button', { name: 'Xem đầy đủ' }));
    await waitFor(() => expect(screen.getByLabelText('Bản đầy đủ')).toBeTruthy());
    const text = screen.getByLabelText('Bản đầy đủ').textContent ?? '';
    expect(text).toBe('{\n  "command": "echo a⟨U+202E⟩b⟨U+200B⟩c"\n}');
    expect(text.includes('‮') || text.includes('​')).toBe(false);
  });

  it('view_full_error_is_shown_and_not_reviewed', async () => {
    const loadFull = vi.fn(() => Promise.reject(new Error('Yêu cầu đã hết hạn')));
    const { onReviewed } = card(approval({ truncated: true }), OPEN, { loadFull });
    fireEvent.click(screen.getByRole('button', { name: 'Xem đầy đủ' }));
    await waitFor(() => expect(screen.getByText('Yêu cầu đã hết hạn')).toBeTruthy());
    expect(onReviewed).not.toHaveBeenCalled();
  });

  it('allow_disabled_with_reason_until_gate_opens', () => {
    const { onDecide } = card(approval(), { ok: false, reason: 'Xem kỹ lệnh trước khi duyệt (1 s)' });
    const allow = screen.getByRole('button', { name: /Đồng ý/ });
    expect((allow as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(allow);
    expect(onDecide).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: /Từ chối/ }));
    expect(onDecide).toHaveBeenCalledWith('deny');
  });

  it('sandbox_option_shown_separately_and_flagged', () => {
    const { container } = card(approval({ tool_options: { dangerouslyDisableSandbox: 'true', timeout: '600000' } }));
    expect(container.querySelector('.option--danger')?.textContent).toContain('dangerouslyDisableSandbox');
    expect(commandBlock().textContent).toBe('git push origin main');
  });
});

describe('allowGate', () => {
  const base = { connected: true, shownAt: 1000, now: 1000, minReviewMs: 1000, reviewed: false };

  it('waits_min_review_from_shown_time', () => {
    expect(allowGate(approval(), base).ok).toBe(false);
    expect(allowGate(approval(), { ...base, now: 1999 }).ok).toBe(false);
    expect(allowGate(approval(), { ...base, now: 2000 })).toEqual({ ok: true, reason: null });
    expect(allowGate(approval(), { ...base, shownAt: undefined, now: 9999 }).ok).toBe(false);
  });

  it('blocked_while_list_is_settling', () => {
    const g = { ...base, now: 5000, calmUntil: 5500 };
    expect(allowGate(approval(), g).ok).toBe(false);
    expect(allowGate(approval(), { ...g, now: 5500 }).ok).toBe(true);
    expect(allowGate(approval(), { ...g, calmUntil: undefined }).ok).toBe(true);
  });

  it('truncated_requires_review_and_connection_required', () => {
    const a = approval({ truncated: true });
    expect(allowGate(a, { ...base, now: 5000 }).reason).toBe('Cần mở bản đầy đủ trước khi duyệt');
    expect(allowGate(a, { ...base, now: 5000, reviewed: true }).ok).toBe(true);
    expect(allowGate(approval(), { ...base, now: 5000, connected: false }).reason).toBe('Mất kết nối hub');
  });
});
