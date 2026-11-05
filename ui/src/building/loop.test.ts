import { describe, expect, it, vi } from 'vitest';
import { createLoop } from './loop';

function fakeRaf() {
  let next = 1;
  const queue = new Map<number, FrameRequestCallback>();
  return {
    raf: (cb: FrameRequestCallback) => {
      const id = next++;
      queue.set(id, cb);
      return id;
    },
    caf: (id: number) => void queue.delete(id),
    pending: () => queue.size,
    step(time: number) {
      const cbs = [...queue.values()];
      queue.clear();
      for (const cb of cbs) cb(time);
    },
  };
}

describe('createLoop', () => {
  it('invalidate_many_times_is_one_frame_and_nothing_waits_afterwards', () => {
    const f = fakeRaf();
    const render = vi.fn();
    const loop = createLoop(render, f.raf, f.caf);
    expect(f.pending()).toBe(0);
    loop.invalidate();
    loop.invalidate();
    loop.invalidate();
    expect(f.pending()).toBe(1);
    f.step(0);
    expect(render).toHaveBeenCalledTimes(1);
    expect(f.pending()).toBe(0);
  });

  it('tween_runs_at_30fps_skipping_alternate_60hz_frames', () => {
    const f = fakeRaf();
    const render = vi.fn();
    const loop = createLoop(render, f.raf, f.caf);
    const seen: number[] = [];
    let done = false;
    void loop.tween(200, 30, (p) => seen.push(p)).then(() => (done = true));
    for (let t = 0, i = 0; t <= 400 && f.pending() > 0; t += 1000 / 60, i++) f.step(t);
    expect(seen[seen.length - 1]).toBe(1);
    // 200 ms ở 30 FPS ≈ 6-7 lần vẽ, không phải 12.
    expect(render.mock.calls.length).toBeGreaterThanOrEqual(6);
    expect(render.mock.calls.length).toBeLessThanOrEqual(8);
    expect(f.pending()).toBe(0);
    return Promise.resolve().then(() => expect(done).toBe(true));
  });

  it('tween_at_15fps_renders_about_every_66ms', () => {
    const f = fakeRaf();
    const render = vi.fn();
    const loop = createLoop(render, f.raf, f.caf);
    void loop.tween(400, 15, () => {});
    for (let t = 0; t <= 600 && f.pending() > 0; t += 1000 / 60) f.step(t);
    expect(render.mock.calls.length).toBeLessThanOrEqual(7);
    expect(render.mock.calls.length).toBeGreaterThanOrEqual(5);
  });

  it('faster_tween_wins_the_frame_rate', () => {
    const f = fakeRaf();
    const render = vi.fn();
    const loop = createLoop(render, f.raf, f.caf);
    void loop.tween(1000, 15, () => {});
    void loop.tween(1000, 30, () => {});
    for (let t = 0; t < 500; t += 1000 / 60) f.step(t);
    expect(render.mock.calls.length).toBeGreaterThan(12);
  });

  it('motion_off_runs_fn_1_immediately_without_a_waiting_raf_tween', async () => {
    const f = fakeRaf();
    const render = vi.fn();
    const loop = createLoop(render, f.raf, f.caf);
    loop.setMotion(false);
    const fn = vi.fn();
    await loop.tween(1000, 30, fn);
    expect(fn).toHaveBeenCalledWith(1);
    expect(fn).toHaveBeenCalledTimes(1);
    // chỉ có đúng một khung (invalidate), không vòng lặp
    expect(f.pending()).toBe(1);
    f.step(0);
    expect(f.pending()).toBe(0);
  });

  it('turning_motion_off_finishes_running_tweens', async () => {
    const f = fakeRaf();
    const loop = createLoop(vi.fn(), f.raf, f.caf);
    const fn = vi.fn();
    const p = loop.tween(1000, 30, fn);
    loop.setMotion(false);
    await p;
    expect(fn).toHaveBeenLastCalledWith(1);
  });

  it('same_key_replaces_the_old_tween', async () => {
    const f = fakeRaf();
    const loop = createLoop(vi.fn(), f.raf, f.caf);
    const first = vi.fn();
    const p1 = loop.tween(1000, 30, first, 'cam');
    void loop.tween(1000, 30, () => {}, 'cam');
    await p1;
    f.step(0);
    expect(first).not.toHaveBeenCalled();
  });

  it('dispose_cancels_the_waiting_frame', () => {
    const f = fakeRaf();
    const render = vi.fn();
    const loop = createLoop(render, f.raf, f.caf);
    loop.invalidate();
    void loop.tween(1000, 30, () => {});
    loop.dispose();
    expect(f.pending()).toBe(0);
    loop.invalidate();
    expect(f.pending()).toBe(0);
    expect(render).not.toHaveBeenCalled();
  });
});
