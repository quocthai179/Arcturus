import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ApprovalView, ClientMsg } from '../protocol';
import { applyServer, initialState, type HubState } from '../store';
import { approval, handoff, snapshot } from '../test/fixtures';
import { LitePopupView } from './LitePopup';

afterEach(cleanup);

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
