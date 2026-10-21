import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { App } from './App';

afterEach(cleanup);

describe('App', () => {
  it('renders_title_and_disconnected_status', () => {
    render(<App />);
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Arcturus');
    expect(screen.getByText('Chưa kết nối hub.')).toBeTruthy();
  });
});
