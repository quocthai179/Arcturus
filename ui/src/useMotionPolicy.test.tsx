import { act, cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { motionAllowed, useMotionPolicy } from './useMotionPolicy';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete document.documentElement.dataset.motion;
});

function Probe() {
  useMotionPolicy();
  return null;
}

function stubReducedMotion(initial: boolean) {
  let reduced = initial;
  const listeners = new Set<() => void>();
  vi.stubGlobal('matchMedia', () => ({
    get matches() {
      return reduced;
    },
    addEventListener: (_: string, l: () => void) => listeners.add(l),
    removeEventListener: (_: string, l: () => void) => listeners.delete(l),
  }));
  return (value: boolean) => {
    reduced = value;
    for (const l of listeners) l();
  };
}

describe('motion policy', () => {
  it('allowed_only_when_page_visible_and_user_does_not_prefer_reduced_motion', () => {
    expect(motionAllowed({ hidden: false, reduced: false })).toBe(true);
    expect(motionAllowed({ hidden: true, reduced: false })).toBe(false);
    expect(motionAllowed({ hidden: false, reduced: true })).toBe(false);
  });

  it('sets_data_motion_on_the_root_and_follows_visibility', () => {
    stubReducedMotion(false);
    let hidden = false;
    vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden);
    render(<Probe />);
    expect(document.documentElement.dataset.motion).toBe('on');
    hidden = true;
    act(() => void document.dispatchEvent(new Event('visibilitychange')));
    expect(document.documentElement.dataset.motion).toBe('off');
    hidden = false;
    act(() => void document.dispatchEvent(new Event('visibilitychange')));
    expect(document.documentElement.dataset.motion).toBe('on');
  });

  it('follows_prefers_reduced_motion_changes_live', () => {
    const setReduced = stubReducedMotion(true);
    render(<Probe />);
    expect(document.documentElement.dataset.motion).toBe('off');
    act(() => setReduced(false));
    expect(document.documentElement.dataset.motion).toBe('on');
  });

  it('works_without_matchMedia_and_cleans_up_listeners_on_unmount', () => {
    vi.stubGlobal('matchMedia', undefined);
    const remove = vi.spyOn(document, 'removeEventListener');
    const { unmount } = render(<Probe />);
    expect(document.documentElement.dataset.motion).toBe('on');
    unmount();
    expect(remove).toHaveBeenCalledWith('visibilitychange', expect.any(Function));
  });
});
