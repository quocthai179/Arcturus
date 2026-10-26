import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ClientMsg, TeamView } from '../protocol';
import { TeamControls } from './TeamControls';

afterEach(cleanup);

const TEAM: TeamView = { paused: false, focus: false, roster_max: 5 };

function controls(team: TeamView = TEAM, connected = true, result = true) {
  const send = vi.fn((_: ClientMsg) => result);
  render(<TeamControls team={team} connected={connected} send={send} />);
  return send;
}

describe('TeamControls', () => {
  it('pause_button_sends_team_pause_then_resume_by_team_state', () => {
    let send = controls();
    fireEvent.click(screen.getByRole('button', { name: 'Tạm dừng cả đội' }));
    expect(send).toHaveBeenCalledWith({ t: 'team_pause' });
    cleanup();
    send = controls({ ...TEAM, paused: true });
    const resume = screen.getByRole('button', { name: 'Tiếp tục cả đội' });
    expect(resume.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(resume);
    expect(send).toHaveBeenCalledWith({ t: 'team_resume' });
  });

  it('pause_tooltip_states_the_v0_limit', () => {
    controls();
    const title = screen.getByRole('button', { name: 'Tạm dừng cả đội' }).getAttribute('title') ?? '';
    expect(title).toContain('tắt tự duyệt');
    expect(title).toContain('không dừng được Claude Code');
  });

  it('focus_button_toggles_focus_set_by_team_state', () => {
    let send = controls();
    const off = screen.getByRole('button', { name: /Giờ tập trung/ });
    expect(off.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(off);
    expect(send).toHaveBeenCalledWith({ t: 'focus_set', on: true });
    cleanup();
    send = controls({ ...TEAM, focus: true });
    const on = screen.getByRole('button', { name: /Giờ tập trung/ });
    expect(on.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(on);
    expect(send).toHaveBeenCalledWith({ t: 'focus_set', on: false });
  });

  it('disabled_and_silent_when_disconnected', () => {
    const send = controls(TEAM, false);
    for (const b of screen.getAllByRole('button')) {
      expect((b as HTMLButtonElement).disabled).toBe(true);
      fireEvent.click(b);
    }
    expect(send).not.toHaveBeenCalled();
  });

  it('shows_error_when_send_fails', () => {
    controls(TEAM, true, false);
    fireEvent.click(screen.getByRole('button', { name: 'Tạm dừng cả đội' }));
    expect(screen.getByRole('alert').textContent).toContain('Chưa gửi được');
  });
});
