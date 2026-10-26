import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { agent } from '../test/fixtures';
import { DEFAULT_SCENE, OFFICE_SCENES, type OfficeSceneProps } from './scenes';

afterEach(cleanup);

describe('office scenes', () => {
  it('default_scene_is_the_desk_grid_and_renders_desks_and_support_room', () => {
    expect(DEFAULT_SCENE).toBe('grid');
    const Scene = OFFICE_SCENES[DEFAULT_SCENE];
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
