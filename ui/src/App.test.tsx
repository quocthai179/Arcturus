import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { App } from './App';

afterEach(cleanup);

describe('App', () => {
  it('office_route_renders_real_office', async () => {
    window.history.replaceState({}, '', '/?view=office');
    try {
      render(<App />);
      expect(screen.getByRole('heading', { name: 'Văn phòng 2D' })).toBeTruthy();
      expect(screen.getByRole('heading', { name: 'Đội hình chính' })).toBeTruthy();
      await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy());
    } finally {
      window.history.replaceState({}, '', '/');
    }
  });
  it('outside_tauri_shows_popup_with_connection_error', async () => {
    render(<App />);
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Arcturus');
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Chỉ chạy được trong app Arcturus'));
  });
});
