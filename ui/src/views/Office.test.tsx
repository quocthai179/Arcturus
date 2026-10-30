import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { applyServer, initialState } from '../store';
import { agent, approval, handoff, snapshot } from '../test/fixtures';
import { FakeSocket } from '../test/FakeSocket';
import { Office, OfficeView } from './Office';

vi.mock('../hub', () => ({
  connectionInfo: () => Promise.resolve({ wsUrl: 'ws://127.0.0.1:7878/ws', token: 'test-ui-token' }),
  approvalInput: () => Promise.resolve('{}'),
}));

const scroll = vi.fn();
beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: scroll });
  FakeSocket.reset();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  scroll.mockClear();
});

function props() {
  return {
    state: { ...applyServer(initialState, snapshot({ agents: [agent({ context_pct: 42 })], queue: [approval()] })), connection: 'open' as const },
    send: vi.fn(() => true), loadFull: vi.fn(() => Promise.resolve('{"full":true}')), minReviewMs: 0,
  };
}

describe('OfficeView', () => {
  it('renders_agent_metadata_context_and_unknown_context_without_false_zero', () => {
    const p = props();
    const view = render(<OfficeView {...p} />);
    const desk = screen.getByRole('article', { name: 'Bàn của Khoa' });
    expect(within(desk).getByText('backend')).toBeTruthy();
    expect(within(desk).getByText('proj').getAttribute('title')).toBe('/w/proj');
    expect(within(desk).queryByText(/laptop|claude-code/)).toBeNull();
    expect(within(desk).getByRole('progressbar').getAttribute('value')).toBe('42');
    view.rerender(<OfficeView {...p} state={{ ...p.state, agents: { agt_s1: agent({ context_pct: null }) } }} />);
    expect(screen.queryByRole('progressbar')).toBeNull();
    expect(screen.queryByText(/Context/)).toBeNull();
    // Vai trò mặc định của hub không mang thông tin: không hiện.
    view.rerender(<OfficeView {...p} state={{ ...p.state, agents: { agt_s1: agent({ role: 'thành viên' }) } }} />);
    expect(screen.queryByText('thành viên')).toBeNull();
    view.rerender(<OfficeView {...p} state={{ ...p.state, agents: { agt_s1: agent({ mood: 'idle', act_text: 'Đang rảnh' }) } }} />);
    expect(within(screen.getByRole('article', { name: 'Bàn của Khoa' })).getAllByText('Đang rảnh')).toHaveLength(1);
  });

  it('shows_support_names_and_moves_newly_active_agent_to_a_desk', () => {
    const p = props();
    const idle = agent({ id: 'idle', name: 'Linh', mood: 'idle' });
    const state = { ...p.state, agents: { ...p.state.agents, idle }, team: { ...p.state.team, roster_max: 1 } };
    const view = render(<OfficeView {...p} state={state} />);
    expect(within(screen.getByRole('article', { name: 'Phòng hỗ trợ' })).getByText('Linh')).toBeTruthy();
    view.rerender(<OfficeView {...p} state={{ ...state, agents: { ...state.agents, idle: { ...idle, mood: 'working' } } }} />);
    expect(screen.queryByRole('article', { name: 'Phòng hỗ trợ' })).toBeNull();
    expect(screen.getByRole('article', { name: 'Bàn của Linh' })).toBeTruthy();
  });

  it('selecting_agent_scrolls_and_focuses_oldest_pending_card', () => {
    const p = props();
    const a = agent({ id: 'second', name: 'Linh' });
    const state = applyServer(p.state, snapshot({ agents: [agent(), a], queue: [
      approval(),
      approval({ request_id: 'req_2', agent_id: a.id, agent_name: a.name }),
      approval({ request_id: 'req_3', agent_id: a.id, agent_name: a.name, created_at: '2026-10-23T10:00:00Z' }),
    ] }));
    render(<OfficeView {...p} state={state} />);
    fireEvent.click(screen.getByRole('button', { name: 'Xem yêu cầu của Linh' }));
    const card = document.querySelector('[data-request-id="req_2"]')!;
    expect(card.getAttribute('aria-current')).toBe('true');
    expect(document.activeElement).toBe(card);
    expect(scroll).toHaveBeenCalledOnce();
  });

  it('agent_with_only_handoff_focuses_terminal_card_without_decision_buttons', () => {
    const p = props();
    render(<OfficeView {...p} state={{ ...p.state, queue: [], handoffs: [handoff()] }} />);
    fireEvent.click(screen.getByRole('button', { name: 'Xem yêu cầu của Khoa' }));
    expect(document.activeElement).toBe(screen.getByRole('article', { name: 'Duyệt trong terminal: Khoa' }));
    expect(screen.queryByRole('button', { name: /Đồng ý|Từ chối/ })).toBeNull();
  });

  it('preserves_review_floor_truncation_disconnect_and_deny_behavior', async () => {
    const p = props();
    const view = render(<OfficeView {...p} minReviewMs={60000} />);
    expect((screen.getByRole('button', { name: /Đồng ý/ }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: /Từ chối/ }));
    expect(p.send).toHaveBeenCalledWith({ t: 'decide', request_id: 'req_1', decision: 'deny', origin: 'user' });
    const truncated = { ...p.state, queue: [approval({ truncated: true })] };
    view.rerender(<OfficeView {...p} state={truncated} />);
    expect((screen.getByRole('button', { name: /Đồng ý/ }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Xem đầy đủ' }));
    await waitFor(() => expect((screen.getByRole('button', { name: /Đồng ý/ }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(screen.getByRole('button', { name: /Đồng ý/ }));
    expect(p.send).toHaveBeenCalledWith({ t: 'decide', request_id: 'req_1', decision: 'allow', origin: 'user' });
    view.rerender(<OfficeView {...p} state={{ ...truncated, connection: 'closed', error: 'Offline' }} />);
    expect(screen.getByRole('alert').textContent).toBe('Offline');
    expect((screen.getByRole('button', { name: /Đồng ý/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('stops_ui_timers_while_hidden_and_cleans_up_on_unmount', () => {
    vi.useFakeTimers();
    let hidden = false;
    vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden);
    const view = render(<OfficeView {...props()} />);
    expect(vi.getTimerCount()).toBe(1);
    hidden = true;
    fireEvent(document, new Event('visibilitychange'));
    expect(vi.getTimerCount()).toBe(0);
    expect(document.querySelector('.office')?.getAttribute('data-hidden')).toBe('true');
    hidden = false;
    fireEvent(document, new Event('visibilitychange'));
    expect(vi.getTimerCount()).toBe(1);
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
    fireEvent(document, new Event('visibilitychange'));
    expect(vi.getTimerCount()).toBe(0);
  });

  it('idle_office_has_no_timer_and_shows_empty_states', () => {
    vi.useFakeTimers();
    const p = props();
    render(<OfficeView {...p} state={{ ...p.state, agents: {}, queue: [] }} />);
    expect(vi.getTimerCount()).toBe(0);
    expect(screen.getByText(/Chưa có agent nào/)).toBeTruthy();
    expect(screen.getByText('Không có yêu cầu nào đang chờ.')).toBeTruthy();
  });
});

describe('Office connection', () => {
  it('handles_snapshot_deltas_reconnect_and_closes_socket_on_unmount', async () => {
    vi.stubGlobal('WebSocket', FakeSocket);
    const view = render(<Office />);
    await waitFor(() => expect(FakeSocket.all).toHaveLength(1));
    const socket = FakeSocket.last();
    act(() => { socket.open(); socket.receive(snapshot()); });
    expect(screen.getByRole('article', { name: 'Bàn của Khoa' })).toBeTruthy();
    act(() => socket.receive({ t: 'agent_updated', agent: agent({ name: 'Linh' }) }));
    expect(screen.getByRole('article', { name: 'Bàn của Linh' })).toBeTruthy();
    act(() => socket.receive({ t: 'approval_requested', approval: approval() }));
    expect(screen.getByRole('article', { name: 'Yêu cầu của Khoa: Bash' })).toBeTruthy();
    act(() => socket.receive({ t: 'approval_resolved', request_id: 'req_1', decision: null, decided_by: 'timeout' }));
    expect(screen.queryByRole('article', { name: 'Yêu cầu của Khoa: Bash' })).toBeNull();
    act(() => socket.receive({ t: 'agent_removed', id: 'agt_s1' }));
    expect(screen.queryByRole('article', { name: 'Bàn của Linh' })).toBeNull();
    act(() => socket.receive({ t: 'agent_updated', agent: agent() }));
    act(() => socket.drop());
    await waitFor(() => expect(FakeSocket.all).toHaveLength(2));
    const reconnected = FakeSocket.last();
    act(() => { reconnected.open(); reconnected.receive(snapshot({ agents: [] })); });
    expect(screen.queryByRole('article', { name: /Bàn của/ })).toBeNull();
    view.unmount();
    expect(reconnected.closed).toBe(true);
  });
});
