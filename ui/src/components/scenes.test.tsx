import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { agent } from '../test/fixtures';
import { DEFAULT_SCENE, OFFICE_SCENES, type OfficeSceneProps } from './scenes';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('office scenes', () => {
  it('grid_scene_renders_desks_and_support_room', () => {
    const Scene = OFFICE_SCENES.grid;
    const props: OfficeSceneProps = {
      desks: [agent({ id: 'agt_a', name: 'Khoa' })],
      support: [agent({ id: 'agt_b', name: 'Linh', mood: 'idle' })],
      canSelect: () => false,
      onSelect: () => {},
    };
    render(<Scene {...props} />);
    expect(screen.getByLabelText('Bàn của Khoa')).toBeTruthy();
    expect(screen.getByLabelText('Phòng hỗ trợ').textContent).toContain('Linh');
  });

  it('scene_only_gets_data_and_callbacks_selection_goes_through_onSelect', () => {
    const onSelect = vi.fn();
    const Scene = OFFICE_SCENES.grid;
    render(
      <Scene
        desks={[agent({ id: 'agt_a', name: 'Khoa', mood: 'approval' })]}
        support={[]}
        canSelect={(a) => a.id === 'agt_a'}
        onSelect={onSelect}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Xem yêu cầu của Khoa' }));
    expect(onSelect).toHaveBeenCalledWith('agt_a');
  });

  it('empty_office_shows_the_hint_instead_of_a_blank_scene', () => {
    const Scene = OFFICE_SCENES.grid;
    render(<Scene desks={[]} support={[]} canSelect={() => false} onSelect={() => {}} />);
    expect(screen.getByText(/Chưa có agent nào/)).toBeTruthy();
  });
});

function floor() {
  return screen.getByRole('region', { name: 'Sàn văn phòng' });
}

describe('live office scene', () => {
  it('is_the_default_and_keeps_the_detail_cards_below_the_floor', () => {
    expect(DEFAULT_SCENE).toBe('live');
    const Scene = OFFICE_SCENES[DEFAULT_SCENE];
    render(
      <Scene
        desks={[agent({ id: 'agt_a', name: 'Khoa', mood: 'working' })]}
        support={[agent({ id: 'agt_b', name: 'Linh', mood: 'idle' })]}
        canSelect={() => false}
        onSelect={() => {}}
      />,
    );
    expect(within(floor()).getByRole('img', { name: /^Khoa: Đang làm việc, ở bàn làm việc$/ })).toBeTruthy();
    expect(within(floor()).getByRole('img', { name: /^Linh: Đang rảnh, đi dạo ở / })).toBeTruthy();
    expect(screen.getByLabelText('Bàn của Khoa')).toBeTruthy();
    expect(screen.getByLabelText('Phòng hỗ trợ').textContent).toContain('Linh');
    // Nhãn tên trên sàn kèm tên dự án (fixture cwd = /w/proj): tên sao một mình khó nhận ra.
    const tags = [...floor().querySelectorAll('.actor__name')].map((el) => el.textContent);
    expect(tags).toContain('Khoa · proj');
  });

  it('raised_hand_on_the_floor_opens_the_approval_card_via_onSelect', () => {
    const onSelect = vi.fn();
    render(
      <OFFICE_SCENES.live
        desks={[agent({ id: 'agt_a', name: 'Khoa', mood: 'approval' }), agent({ id: 'agt_b', name: 'Linh', mood: 'approval' })]}
        support={[]}
        canSelect={(a) => a.id === 'agt_a'}
        onSelect={onSelect}
      />,
    );
    fireEvent.click(within(floor()).getByRole('button', { name: 'Khoa đang giơ tay: mở thẻ duyệt' }));
    expect(onSelect).toHaveBeenCalledWith('agt_a');
    // Không có gì để mở thì chỉ là dấu hiệu, không phải nút.
    expect(within(floor()).queryByRole('button', { name: /Linh/ })).toBeNull();
  });

  it('idle_agents_walk_with_css_transitions_and_working_ones_need_no_timer', () => {
    vi.useFakeTimers();
    const working = [agent({ id: 'agt_a', name: 'Khoa', mood: 'working' })];
    const view = render(<OFFICE_SCENES.live desks={working} support={[]} canSelect={() => false} onSelect={() => {}} />);
    expect(vi.getTimerCount()).toBe(0);

    const idle = [agent({ id: 'agt_b', name: 'Linh', mood: 'idle' }), agent({ id: 'agt_c', name: 'Mai', mood: 'idle' })];
    view.rerender(<OFFICE_SCENES.live desks={[...working, ...idle]} support={[]} canSelect={() => false} onSelect={() => {}} />);
    expect(vi.getTimerCount()).toBe(1);
    let walked = false;
    for (let i = 0; i < 40 && !walked; i++) {
      act(() => { vi.advanceTimersToNextTimer(); });
      walked = [...document.querySelectorAll<HTMLElement>('.actor')].some(
        (el) => el.dataset.pose === 'walk' && el.style.transitionDuration !== '0ms',
      );
    }
    expect(walked).toBe(true);
    expect(vi.getTimerCount()).toBe(1);
  });

  it('motion_off_stops_the_simulation_and_places_everyone', () => {
    vi.useFakeTimers();
    const agents = [agent({ id: 'agt_b', name: 'Linh', mood: 'idle' }), agent({ id: 'agt_c', name: 'Mai', mood: 'working' })];
    const view = render(<OFFICE_SCENES.live desks={agents} support={[]} canSelect={() => false} onSelect={() => {}} />);
    expect(vi.getTimerCount()).toBe(1);
    view.rerender(<OFFICE_SCENES.live desks={agents} support={[]} canSelect={() => false} onSelect={() => {}} motion={false} />);
    expect(vi.getTimerCount()).toBe(0);
    expect(within(floor()).getByRole('img', { name: /^Linh: Đang rảnh, ngủ gật$/ })).toBeTruthy();
    expect(within(floor()).getByRole('img', { name: /^Mai: Đang làm việc, ở bàn làm việc$/ })).toBeTruthy();
    for (const el of document.querySelectorAll<HTMLElement>('.actor')) expect(el.style.transitionDuration).toBe('0ms');
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('agent_names_are_rendered_as_text_not_markup', () => {
    render(
      <OFFICE_SCENES.live
        desks={[agent({ id: 'agt_x', name: '<img src=x onerror=alert(1)>', mood: 'working' })]}
        support={[]}
        canSelect={() => false}
        onSelect={() => {}}
      />,
    );
    expect(document.querySelector('img')).toBeNull();
    expect(within(floor()).getByText('<img src=x onerror=alert(1)>')).toBeTruthy();
  });
});
