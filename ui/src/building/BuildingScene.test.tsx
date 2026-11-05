import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { AgentView, Risk } from '../protocol';
import { sceneBrief, type SceneBrief } from '../sceneBrief';
import { initialState, type HubState } from '../store';
import { agent } from '../test/fixtures';
import { BuildingScene } from './BuildingScene';
import type { BuildingCanvasProps, PickHit } from './model';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  window.localStorage.clear();
});

let last: BuildingCanvasProps | null = null;
function FakeCanvas(p: BuildingCanvasProps) {
  last = p;
  return <canvas data-testid="fake" />;
}
function BrokenCanvas(p: BuildingCanvasProps) {
  p.onError('webglcontextlost');
  return null;
}
function ThrowingCanvas(): never {
  throw new Error('chunk failed');
}

const sub = (id: string, role: string, over: Partial<AgentView> = {}) =>
  agent({ id, name: id, kind: 'subagent', parent_id: 'agt_s0', role, mood: 'working', activity: 'typing', ...over });

const base: HubState = { ...initialState, connection: 'open', synced: true };
function briefOf(pending: [string, Risk][] = [], over: Partial<SceneBrief> = {}): SceneBrief {
  const b = sceneBrief(base, { speech: null, mood: 'working' });
  return { ...b, pending: new Map(pending.map(([id, risk], i) => [id, { count: 1, risk, tool: 'Bash', oldestAt: 1000 + i }])), ...over };
}

interface Opts {
  desks?: AgentView[];
  brief?: SceneBrief;
  onSelect?: (id: string) => void;
  canSelect?: (a: AgentView) => boolean;
  Canvas?: BuildingCanvasProps extends never ? never : (p: BuildingCanvasProps) => React.ReactNode;
  webgl?: boolean;
  motion?: boolean;
}
function scene(o: Opts = {}) {
  return (
    <BuildingScene
      desks={o.desks ?? []}
      support={[]}
      canSelect={o.canSelect ?? (() => true)}
      onSelect={o.onSelect ?? (() => {})}
      motion={o.motion ?? false}
      brief={o.brief ?? briefOf()}
      Canvas={o.Canvas ?? FakeCanvas}
      webgl={o.webgl ?? true}
    />
  );
}

const root = () => screen.getByRole('region', { name: /^Tòa nhà Arcturus/ });

describe('BuildingScene shell', () => {
  it('opens_on_the_ceo_room_and_always_offers_the_ceo_button', () => {
    render(scene());
    expect(last?.model.view).toBe(5);
    expect(root().getAttribute('aria-label')).toBe('Tòa nhà Arcturus, đang xem Phòng CEO. Phím 0–5 chọn tầng, E ngoại cảnh.');
    expect(screen.getByRole('button', { name: 'Phòng CEO (5)' }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.keyDown(root(), { key: '2' });
    expect(last?.model.view).toBe(2);
    expect(screen.getByRole('button', { name: 'Phòng CEO (5)' }).getAttribute('aria-pressed')).toBe('false');
  });

  it('digit_and_E_keys_change_the_view_and_R_resets_ctrl_is_ignored', () => {
    render(scene());
    fireEvent.keyDown(root(), { key: '0' });
    expect(last?.model.view).toBe(0);
    fireEvent.keyDown(root(), { key: 'E' });
    expect(last?.model.view).toBe('ext');
    fireEvent.keyDown(root(), { key: '3', ctrlKey: true });
    expect(last?.model.view).toBe('ext');
    fireEvent.keyDown(root(), { key: 'r' });
    expect(last?.model.view).toBe(5);
    fireEvent.keyDown(root(), { key: '6' });
    expect(last?.model.view).toBe(5);
  });

  it('keys_typed_into_the_follow_checkbox_are_ignored', () => {
    render(scene());
    fireEvent.keyDown(screen.getByRole('checkbox', { name: 'Theo dõi thư ký' }), { key: '1' });
    expect(last?.model.view).toBe(5);
  });

  it('follow_is_off_by_default_and_saved', () => {
    render(scene());
    const box = screen.getByRole('checkbox', { name: 'Theo dõi thư ký' }) as HTMLInputElement;
    expect(box.checked).toBe(false);
    fireEvent.click(box);
    expect(window.localStorage.getItem('arcturus.building.follow')).toBe('1');
  });

  it('without_webgl_it_shows_the_2d_office_and_the_notice', () => {
    render(scene({ webgl: false, desks: [sub('agt_a', 'dev-squad:developer')] }));
    expect(screen.getByText('Máy này không dựng được cảnh 3D, đang hiện văn phòng 2D.')).toBeTruthy();
    expect(screen.getByRole('region', { name: 'Sàn văn phòng' })).toBeTruthy();
    expect(screen.queryByTestId('fake')).toBeNull();
  });

  it('canvas_onError_falls_back_to_live', () => {
    render(scene({ Canvas: BrokenCanvas }));
    expect(screen.getByText('Máy này không dựng được cảnh 3D, đang hiện văn phòng 2D.')).toBeTruthy();
    expect(screen.getByRole('region', { name: 'Sàn văn phòng' })).toBeTruthy();
  });

  it('a_throwing_canvas_or_chunk_falls_back_to_live', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(scene({ Canvas: ThrowingCanvas }));
    expect(screen.getByText('Máy này không dựng được cảnh 3D, đang hiện văn phòng 2D.')).toBeTruthy();
    spy.mockRestore();
  });

  it('picking_a_bang_calls_onSelect_exactly_once_only_for_selectable_agents', () => {
    const onSelect = vi.fn();
    const a = sub('agt_a', 'dev-squad:developer', { mood: 'approval' });
    const b = sub('agt_b', 'dev-squad:architect', { mood: 'approval' });
    render(scene({ desks: [a, b], onSelect, canSelect: (x) => x.id === 'agt_a' }));
    const pick = (hit: PickHit) => act(() => last?.onPick(hit));
    pick({ k: 'bang', agentId: 'agt_b' });
    expect(onSelect).not.toHaveBeenCalled();
    pick({ k: 'bang', agentId: 'agt_a' });
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith('agt_a');
    pick({ k: 'agent', agentId: 'agt_ghost' });
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it('picking_a_floor_or_status_screen_changes_the_view_picking_the_secretary_summarises', () => {
    render(scene({ desks: [sub('agt_a', 'dev-squad:developer')] }));
    act(() => last?.onPick({ k: 'floor', floor: 1 }));
    expect(last?.model.view).toBe(1);
    act(() => last?.onPick({ k: 'status-screen', floor: 3 }));
    expect(last?.model.view).toBe(3);
    act(() => last?.onPick({ k: 'secretary' }));
    expect((document.querySelector('.building__sec') as HTMLElement).textContent).toContain('Cả tòa: 1 đang làm, 0 chờ duyệt, 0 lỗi. Bận nhất: tầng 1 · Dev.');
  });

  it('never_calls_onSelect_by_itself_while_the_brief_keeps_changing_and_timers_run', () => {
    vi.useFakeTimers();
    const onSelect = vi.fn();
    const a = sub('agt_a', 'dev-squad:developer', { mood: 'approval' });
    const view = render(scene({ desks: [a], onSelect, motion: true, brief: briefOf([['agt_a', 'critical']]) }));
    for (let i = 0; i < 10; i++) {
      view.rerender(scene({ desks: [a], onSelect, motion: true, brief: briefOf([['agt_a', i % 2 === 0 ? 'high' : 'critical']]) }));
      act(() => { vi.advanceTimersByTime(5_000); });
    }
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('the_secretary_bar_shows_the_line_and_the_view_card_button_calls_onSelect', () => {
    const onSelect = vi.fn();
    const a = sub('agt_a', 'dev-squad:developer', { mood: 'approval' });
    render(scene({ desks: [a], onSelect, brief: briefOf([['agt_a', 'critical']]) }));
    const bar = document.querySelector('.building__sec') as HTMLElement;
    expect(bar.getAttribute('aria-live')).toBe('polite');
    expect(bar.textContent).toContain('agt_a xin chạy Bash trong vùng khóa. Anh mở thẻ xem kỹ nhé.');
    expect(onSelect).not.toHaveBeenCalled();
    fireEvent.click(within(bar).getByRole('button', { name: 'Xem thẻ duyệt' }));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith('agt_a');
  });

  it('no_view_card_button_when_the_agent_cannot_be_selected', () => {
    render(scene({ desks: [sub('agt_a', 'dev-squad:developer', { mood: 'approval' })], canSelect: () => false, brief: briefOf([['agt_a', 'critical']]) }));
    const bar = document.querySelector('.building__sec') as HTMLElement;
    expect(within(bar).queryByRole('button', { name: 'Xem thẻ duyệt' })).toBeNull();
  });

  it('hostile_names_are_text_not_markup', () => {
    const evil = '<img src=x onerror=alert(1)>';
    render(scene({ desks: [sub('agt_a', 'dev-squad:developer', { name: evil, mood: 'approval' })], brief: briefOf([['agt_a', 'medium']]) }));
    expect(document.querySelector('img')).toBeNull();
    expect((document.querySelector('.building__sec') as HTMLElement).textContent).toContain(evil);
  });

  it('lift_panel_shows_counts_overflow_and_the_secretary_dot', () => {
    const desks = [
      sub('agt_a', 'dev-squad:developer'),
      sub('agt_b', 'dev-squad:architect', { mood: 'approval' }),
      ...Array.from({ length: 12 }, (_, i) => sub(`agt_g${String(i).padStart(2, '0')}`, `evil:x${i}`)),
    ];
    render(scene({ desks }));
    const panel = screen.getByRole('navigation', { name: 'Bảng thang máy' });
    const dev = within(panel).getByRole('button', { name: /^1 · Dev/ });
    expect(dev.textContent).toContain('▶ 1 · ! 1');
    const lobby = within(panel).getByRole('button', { name: /^0 · Sảnh/ });
    expect(lobby.textContent).toContain('+4');
    const ceo = within(panel).getByRole('button', { name: /^5 · Phòng CEO/ });
    expect(ceo.textContent).toContain('thư ký · bàn CEO');
    expect(ceo.querySelector('.building__here')).not.toBeNull();
    expect(dev.querySelector('.building__here')).toBeNull();
    fireEvent.click(dev);
    expect(last?.model.view).toBe(1);
  });

  it('an_agent_with_a_ceo_role_never_lands_on_floor_5', () => {
    render(scene({ desks: [sub('agt_a', 'ceo:x', { mood: 'approval' })] }));
    const lobby = screen.getByRole('button', { name: /^0 · Sảnh/ });
    expect(lobby.textContent).toContain('! 1');
    expect(last?.model.rosters.map((r) => r.figures.length)).toEqual([1, 0, 0, 0, 0]);
    expect(last?.model.rosters).toHaveLength(5);
  });

  it('detail_cards_follow_the_viewed_floor', () => {
    render(scene({ desks: [sub('agt_a', 'dev-squad:developer'), sub('agt_t', 'tester-squad:api-tester', { name: 'Tester' })] }));
    expect(screen.queryByLabelText('Bàn của Tester')).toBeNull();
    fireEvent.keyDown(root(), { key: '2' });
    expect(screen.getByLabelText('Bàn của Tester')).toBeTruthy();
    expect(screen.queryByLabelText('Bàn của agt_a')).toBeNull();
  });

  it('hover_shows_a_text_tooltip', () => {
    render(scene({ desks: [sub('agt_a', 'dev-squad:developer', { name: 'Khoa', mood: 'approval', act_text: 'Chờ duyệt: Bash' })] }));
    act(() => last?.onHover({ k: 'agent', agentId: 'agt_a' }, 10, 10));
    const tip = document.querySelector('.building__tip') as HTMLElement;
    expect(tip.textContent).toContain('Khoa · developer · Giơ tay xin duyệt');
    expect(tip.textContent).toContain('Bấm để mở thẻ duyệt');
    act(() => last?.onHover(null, 0, 0));
    expect(document.querySelector('.building__tip')).toBeNull();
  });

  it('follow_moves_the_view_with_the_secretary', () => {
    window.localStorage.setItem('arcturus.building.follow', '1');
    render(scene({ desks: [sub('agt_t', 'tester-squad:api-tester', { mood: 'approval' })], brief: briefOf([['agt_t', 'critical']]) }));
    expect(last?.model.view).toBe(2);
  });
});
