import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ApprovalView, ClientMsg } from '../protocol';
import { applyServer, initialState, type HubState } from '../store';
import { approval, handoff, snapshot } from '../test/fixtures';
import { LIST_SETTLE_MS, LitePopupView } from './LitePopup';

const noScroll = Element.prototype.scrollIntoView;
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  Element.prototype.scrollIntoView = noScroll;
});

function stateWith(queue: ApprovalView[], extra: Partial<HubState> = {}): HubState {
  return { ...applyServer(initialState, snapshot({ queue })), connection: 'open', ...extra };
}

function popup(state: HubState, minReviewMs = 0) {
  const sent: ClientMsg[] = [];
  const send = vi.fn((m: ClientMsg) => {
    sent.push(m);
    return true;
  });
  const r = render(
    <LitePopupView state={state} send={send} loadFull={() => Promise.resolve('{}')} minReviewMs={minReviewMs} />,
  );
  return { ...r, sent };
}

const THREE = [
  approval({ request_id: 'req_1', created_at: '2026-10-23T09:00:01Z', command_normalized: 'npm test' }),
  approval({ request_id: 'req_2', created_at: '2026-10-23T09:00:02Z', command_normalized: 'cargo test' }),
  approval({ request_id: 'req_3', created_at: '2026-10-23T09:00:03Z', command_normalized: 'ls' }),
];

describe('LitePopupView', () => {
  it('keyboard_enter_allows_and_esc_denies_selected', () => {
    const { sent } = popup(stateWith(THREE));
    fireEvent.keyDown(window, { key: 'Enter' });
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'Escape' });
    fireEvent.keyDown(window, { key: 'ArrowUp' });
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(sent).toEqual([
      { t: 'decide', request_id: 'req_1', decision: 'allow', origin: 'user' },
      { t: 'decide', request_id: 'req_3', decision: 'deny', origin: 'user' },
      { t: 'decide', request_id: 'req_2', decision: 'deny', origin: 'user' },
    ]);
  });

  it('held_enter_or_escape_decides_only_once', () => {
    const { sent } = popup(stateWith(THREE));
    fireEvent.keyDown(window, { key: 'Enter', repeat: true });
    fireEvent.keyDown(window, { key: 'Escape', repeat: true });
    expect(sent).toEqual([]);
    fireEvent.keyDown(window, { key: 'Enter' });
    fireEvent.keyDown(window, { key: 'Enter', repeat: true });
    expect(sent).toEqual([{ t: 'decide', request_id: 'req_1', decision: 'allow', origin: 'user' }]);
  });

  it('allow_blocked_briefly_after_a_card_leaves_the_queue', () => {
    vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
    const sent: ClientMsg[] = [];
    const send = (m: ClientMsg) => (sent.push(m), true);
    const view = (queue: ApprovalView[]) => (
      <LitePopupView state={stateWith(queue)} send={send} loadFull={() => Promise.resolve('{}')} minReviewMs={0} />
    );
    const r = render(view(THREE));
    act(() => void vi.advanceTimersByTime(1000));
    // req_1 được xử lý ở nơi khác (terminal): req_2 trượt lên đúng chỗ con trỏ/phím đang nhắm.
    r.rerender(view(THREE.slice(1)));
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(sent).toEqual([]);
    for (const b of screen.getAllByRole('button', { name: /Đồng ý/ })) {
      expect((b as HTMLButtonElement).disabled).toBe(true);
    }
    // Từ chối vẫn được; sau LIST_SETTLE_MS thì Đồng ý mở lại.
    act(() => void vi.advanceTimersByTime(LIST_SETTLE_MS + 50));
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(sent).toEqual([{ t: 'decide', request_id: 'req_2', decision: 'allow', origin: 'user' }]);
  });

  it('new_card_appended_does_not_block_allow', () => {
    vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
    const sent: ClientMsg[] = [];
    const send = (m: ClientMsg) => (sent.push(m), true);
    const view = (queue: ApprovalView[]) => (
      <LitePopupView state={stateWith(queue)} send={send} loadFull={() => Promise.resolve('{}')} minReviewMs={0} />
    );
    const r = render(view(THREE.slice(0, 2)));
    act(() => void vi.advanceTimersByTime(1000));
    r.rerender(view(THREE));
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(sent).toHaveLength(1);
  });

  it('arrow_selection_scrolls_card_into_view', () => {
    const scrolled: (string | undefined)[] = [];
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push((this as HTMLElement).dataset.requestId);
    };
    popup(stateWith(THREE));
    scrolled.length = 0;
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    expect(scrolled).toEqual(['req_2', 'req_3']);
  });

  it('enter_does_nothing_before_min_review', () => {
    const { sent } = popup(stateWith(THREE), 60_000);
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(sent).toEqual([]);
    // Từ chối không bị chặn.
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(sent).toEqual([{ t: 'decide', request_id: 'req_1', decision: 'deny', origin: 'user' }]);
  });

  it('enter_blocked_for_truncated_until_full_viewed', () => {
    const { sent } = popup(stateWith([approval({ request_id: 'req_w', tool: 'Write', truncated: true })]));
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(sent).toEqual([]);
  });

  it('group_button_allows_each_non_critical_request', () => {
    const queue = [
      approval({ request_id: 'req_a', group_key: 'Bash:npm test', risk: 'low', created_at: '2026-10-23T09:00:01Z' }),
      approval({ request_id: 'req_b', group_key: 'Bash:npm test', risk: 'medium', created_at: '2026-10-23T09:00:02Z' }),
    ];
    const { sent } = popup(stateWith(queue));
    fireEvent.click(screen.getByRole('button', { name: /Duyệt cả 2/ }));
    expect(sent).toEqual([
      { t: 'decide', request_id: 'req_a', decision: 'allow', origin: 'user' },
      { t: 'decide', request_id: 'req_b', decision: 'allow', origin: 'user' },
    ]);
  });

  it('group_button_reports_partial_send_when_connection_drops', () => {
    const queue = ['a', 'b', 'c'].map((s, i) =>
      approval({ request_id: `req_${s}`, group_key: 'Bash:npm test', risk: 'low', created_at: `2026-10-23T09:00:0${i}Z` }),
    );
    const sent: ClientMsg[] = [];
    const send = vi.fn((m: ClientMsg) => {
      sent.push(m);
      return sent.length <= 1; // lần gửi thứ hai trở đi: socket đã đứt
    });
    render(<LitePopupView state={stateWith(queue)} send={send} loadFull={() => Promise.resolve('{}')} minReviewMs={0} />);
    fireEvent.click(screen.getByRole('button', { name: /Duyệt cả 3/ }));
    expect(send).toHaveBeenCalledTimes(2); // dừng ở lần lỗi đầu tiên, không thử nốt yêu cầu thứ ba
    expect(screen.getByText(/Mới gửi 1\/3 yêu cầu/)).toBeTruthy();
  });

  it('critical_group_has_no_batch_button', () => {
    const queue = [
      approval({ request_id: 'req_a', group_key: 'Bash', risk: 'critical', created_at: '2026-10-23T09:00:01Z' }),
      approval({ request_id: 'req_b', group_key: 'Bash', risk: 'critical', created_at: '2026-10-23T09:00:02Z' }),
      approval({ request_id: 'req_c', group_key: 'Bash', risk: 'medium', created_at: '2026-10-23T09:00:03Z' }),
    ];
    popup(stateWith(queue));
    // Chỉ còn 1 yêu cầu không Critical trong nhóm: không có nút gộp, Critical không bao giờ được gộp.
    expect(screen.queryByRole('button', { name: /Duyệt cả/ })).toBeNull();
  });

  it('disconnected_disables_allow_and_shows_error', () => {
    popup(stateWith(THREE, { connection: 'closed', error: 'Mất kết nối hub, đang thử lại…' }));
    expect(screen.getByRole('alert').textContent).toBe('Mất kết nối hub, đang thử lại…');
    for (const b of screen.getAllByRole('button', { name: /Đồng ý/ })) {
      expect((b as HTMLButtonElement).disabled).toBe(true);
    }
  });

  it('handoff_cards_have_no_decision_buttons', () => {
    popup(stateWith([], { handoffs: [handoff()] }));
    expect(screen.getByText('Duyệt trong terminal')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Đồng ý|Từ chối/ })).toBeNull();
  });

  it('shows_pause_banner_feed_and_count', () => {
    popup(
      stateWith(THREE, {
        team: { paused: true, focus: false, roster_max: 5 },
        feed: [{ text: 'Duyệt quá nhanh: xem lại thẻ rồi bấm lại', ts: '' }],
      }),
    );
    expect(screen.getByText('3 yêu cầu đang chờ')).toBeTruthy();
    expect(screen.getByText(/Đội đang tạm dừng/)).toBeTruthy();
    expect(screen.getByText('Duyệt quá nhanh: xem lại thẻ rồi bấm lại')).toBeTruthy();
  });
});
