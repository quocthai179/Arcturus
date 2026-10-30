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

  it('enter_never_allows_high_or_critical_only_a_click_does', () => {
    // Hồi quy UX test 2026-10-30: Enter, Enter liên tiếp duyệt cả lệnh vùng khóa vừa trượt lên đầu hàng đợi.
    for (const risk of ['high', 'critical'] as const) {
      cleanup();
      const { sent } = popup(stateWith([approval({ request_id: 'req_x', risk, command_normalized: 'rm -rf ./build' })]));
      fireEvent.keyDown(window, { key: 'Enter' });
      expect(sent).toEqual([]);
      expect(screen.getByText(/Enter không duyệt, hãy bấm nút Đồng ý/)).toBeTruthy();
      const allow = screen.getByRole('button', { name: 'Đồng ý' });
      expect(allow.className).toContain('btn--allow-risky');
      expect(allow.querySelector('kbd')).toBeNull();
      fireEvent.click(allow);
      expect(sent).toEqual([{ t: 'decide', request_id: 'req_x', decision: 'allow', origin: 'user' }]);
    }
  });

  it('team_list_shows_each_agent_project', () => {
    popup(stateWith([]));
    const team = screen.getByRole('region', { name: 'Đội' });
    expect(team.querySelector('.team__project')?.textContent).toBe('proj');
  });

  it('escape_still_denies_critical', () => {
    const { sent } = popup(stateWith([approval({ request_id: 'req_c', risk: 'critical' })]));
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(sent).toEqual([{ t: 'decide', request_id: 'req_c', decision: 'deny', origin: 'user' }]);
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

  it('group_button_sends_one_decide_group_message_and_hub_applies_the_gates', () => {
    const queue = [
      approval({ request_id: 'req_a', group_key: 'Bash:npm test', risk: 'low', created_at: '2026-10-23T09:00:01Z' }),
      approval({ request_id: 'req_b', group_key: 'Bash:npm test', risk: 'medium', created_at: '2026-10-23T09:00:02Z' }),
    ];
    const { sent } = popup(stateWith(queue));
    fireEvent.click(screen.getByRole('button', { name: /Duyệt cả 2/ }));
    expect(sent).toEqual([{ t: 'decide_group', group_key: 'Bash:npm test', decision: 'allow' }]);
  });

  it('group_button_reports_failure_when_not_connected', () => {
    const queue = ['a', 'b'].map((s, i) =>
      approval({ request_id: `req_${s}`, group_key: 'Bash:npm test', risk: 'low', created_at: `2026-10-23T09:00:0${i}Z` }),
    );
    const send = vi.fn(() => false);
    render(<LitePopupView state={stateWith(queue)} send={send} loadFull={() => Promise.resolve('{}')} minReviewMs={0} />);
    fireEvent.click(screen.getByRole('button', { name: /Duyệt cả 2/ }));
    expect(send).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Chưa kết nối hub, chưa gửi được')).toBeTruthy();
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
    // Không nói "không có yêu cầu nào" khi thật ra không biết; trấn an rằng terminal vẫn hỏi.
    expect(screen.getByText('Chưa kết nối hub')).toBeTruthy();
    expect(screen.queryByText('Không có yêu cầu nào đang chờ')).toBeNull();
    expect(screen.getByText(/Claude Code hỏi trong terminal như bình thường/)).toBeTruthy();
    for (const b of screen.getAllByRole('button', { name: /Đồng ý/ })) {
      expect((b as HTMLButtonElement).disabled).toBe(true);
    }
  });

  describe('countdown', () => {
    const timed = (over: Partial<ApprovalView> = {}) =>
      approval({ request_id: 'req_t', risk: 'low', reversible: true, countdown_ms: 5000, ...over });

    function timedPopup(queue: ApprovalView[], extra: Partial<HubState> = {}) {
      vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
      vi.setSystemTime(Date.parse('2026-10-23T09:00:00Z'));
      const sent: ClientMsg[] = [];
      const send = (m: ClientMsg) => (sent.push(m), true);
      const view = (state: HubState) => (
        <LitePopupView state={state} send={send} loadFull={() => Promise.resolve('{}')} minReviewMs={0} />
      );
      const r = render(view(stateWith(queue, extra)));
      return { sent, rerender: (state: HubState) => r.rerender(view(state)) };
    }
    const wait = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

    it('expiry_sends_allow_with_origin_countdown_exactly_once', () => {
      const { sent } = timedPopup([timed()]);
      expect(screen.getByText(/Tự chạy sau/)).toBeTruthy();
      wait(4000);
      expect(sent).toEqual([]);
      wait(1500);
      expect(sent).toEqual([{ t: 'decide', request_id: 'req_t', decision: 'allow', origin: 'countdown' }]);
      wait(5000);
      expect(sent).toHaveLength(1);
    });

    it('stop_button_sends_deny_with_origin_user_and_run_now_allow_with_origin_user', () => {
      const { sent } = timedPopup([timed(), timed({ request_id: 'req_u', created_at: '2026-10-23T09:00:01Z' })]);
      wait(1000);
      const [stop, ,] = screen.getAllByRole('button', { name: /Dừng lại/ });
      const runs = screen.getAllByRole('button', { name: /Chạy ngay/ });
      fireEvent.click(stop as HTMLElement);
      fireEvent.click(runs[1] as HTMLElement);
      expect(sent).toEqual([
        { t: 'decide', request_id: 'req_t', decision: 'deny', origin: 'user' },
        { t: 'decide', request_id: 'req_u', decision: 'allow', origin: 'user' },
      ]);
    });

    it('never_for_high_or_critical_even_if_hub_sends_countdown', () => {
      const { sent } = timedPopup([timed({ risk: 'high' }), timed({ request_id: 'req_c', risk: 'critical' })]);
      expect(screen.queryByText(/Tự chạy sau/)).toBeNull();
      wait(60_000);
      expect(sent).toEqual([]);
    });

    it('pause_stops_the_clock_and_resume_does_not_restart_it', () => {
      const { sent, rerender } = timedPopup([timed()]);
      wait(2000);
      const paused = applyServer(stateWith([timed()]), {
        t: 'team_updated',
        team: { paused: true, focus: false, roster_max: 5 },
      });
      rerender(paused);
      expect(screen.queryByText(/Tự chạy sau/)).toBeNull();
      wait(60_000);
      const resumed = applyServer(paused, { t: 'team_updated', team: { paused: false, focus: false, roster_max: 5 } });
      rerender(resumed);
      wait(60_000);
      expect(sent).toEqual([]);
      expect(screen.queryByText(/Tự chạy sau/)).toBeNull();
    });

    it('no_auto_allow_when_disconnected_or_page_hidden', () => {
      const { sent, rerender } = timedPopup([timed()], { connection: 'closed' });
      wait(10_000);
      expect(sent).toEqual([]);
      Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
      try {
        rerender(stateWith([timed()]));
        wait(10_000);
        expect(sent).toEqual([]);
      } finally {
        Reflect.deleteProperty(document, 'hidden');
      }
    });
  });

  it('team_controls_are_in_the_popup', () => {
    const { sent } = popup(stateWith([]));
    fireEvent.click(screen.getByRole('button', { name: 'Tạm dừng cả đội' }));
    fireEvent.click(screen.getByRole('button', { name: /Giờ tập trung/ }));
    expect(sent).toEqual([{ t: 'team_pause' }, { t: 'focus_set', on: true }]);
  });

  it('suggestion_card_answers_with_one_message_and_disappears', () => {
    const sug = {
      id: 'sug_1',
      subject: 'Khoa',
      pattern: 'Bash:npm test',
      level: 6,
      approved_in_row: 20,
      window_days: 7,
      grant_days: 30,
      text: 'Khoa đã được bạn duyệt 20 lần liên tiếp. Cho phép tự chạy?',
      created_at: '2026-10-27T10:00:00Z',
    };
    const { sent } = popup(stateWith([], { suggestions: [sug] }));
    expect(screen.getByText(/Khoa đã được bạn duyệt 20 lần liên tiếp/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Cho phép' }));
    expect(sent).toEqual([{ t: 'suggestion_answer', id: 'sug_1', accept: true }]);
    expect(screen.queryByText(/Khoa đã được bạn duyệt 20 lần liên tiếp/)).toBeNull();
  });

  it('suggestion_card_stays_when_not_connected_and_reports_it', () => {
    const sug = {
      id: 'sug_1', subject: 'Khoa', pattern: 'Bash:npm test', level: 6, approved_in_row: 20, window_days: 7,
      grant_days: 30, text: 'Đề xuất thử', created_at: '2026-10-27T10:00:00Z',
    };
    const send = vi.fn(() => false);
    render(
      <LitePopupView state={stateWith([], { suggestions: [sug] })} send={send} loadFull={() => Promise.resolve('{}')} minReviewMs={0} />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Để sau' }));
    expect(screen.getByText('Đề xuất thử')).toBeTruthy();
    expect(screen.getByText('Chưa kết nối hub, chưa gửi được')).toBeTruthy();
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
