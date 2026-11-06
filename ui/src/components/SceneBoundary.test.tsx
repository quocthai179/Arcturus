import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SceneBoundary } from './SceneBoundary';

afterEach(cleanup);

function Boom(): never {
  throw new Error('boom');
}

describe('SceneBoundary', () => {
  it('renders_children_when_fine', () => {
    render(<SceneBoundary fallback={<p>lui</p>}><p>cảnh</p></SceneBoundary>);
    expect(screen.getByText('cảnh')).toBeTruthy();
  });

  it('renders_fallback_and_reports_when_a_child_throws', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const onError = vi.fn();
    render(<SceneBoundary fallback={<p>lui</p>} onError={onError}><Boom /></SceneBoundary>);
    expect(screen.getByText('lui')).toBeTruthy();
    expect(onError).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });
});
