// Vòng vẽ theo nhu cầu (docs/OFFICE_BUILDING.md §9.1). Không import three. KHÔNG có `requestAnimationFrame` thường trực: một khung
// khi `invalidate()`, và chỉ chạy liên tục trong lúc còn tween (FPS tối đa theo tween nhanh nhất đang chạy).
export interface Loop {
  /** Xin vẽ một khung (gộp nhiều lần gọi). */
  invalidate(): void;
  /** Chạy `fn(p)` với p 0…1 trong `ms`. Cùng `key` thì thay tween cũ. `motion=false`: `fn(1)` ngay. */
  tween(ms: number, fps: 15 | 30, fn: (p: number) => void, key?: string): Promise<void>;
  setMotion(on: boolean): void;
  dispose(): void;
}

interface Tween {
  key: string | undefined;
  ms: number;
  fps: 15 | 30;
  fn: (p: number) => void;
  start: number | null;
  done: () => void;
}

export function createLoop(
  render: () => void,
  raf: (cb: FrameRequestCallback) => number = (cb) => requestAnimationFrame(cb),
  caf: (id: number) => void = (id) => cancelAnimationFrame(id),
): Loop {
  let id: number | null = null;
  let dirty = false;
  let motion = true;
  let disposed = false;
  let lastRender = -Infinity;
  let tweens: Tween[] = [];

  const schedule = () => {
    if (disposed || id !== null) return;
    id = raf(frame);
  };

  function frame(time: number): void {
    id = null;
    if (disposed) return;
    const fastest = tweens.reduce((m, t) => Math.max(m, t.fps), 0);
    const finishing = tweens.some((t) => t.start !== null && time - t.start >= t.ms);
    const interval = fastest === 0 ? 0 : 1000 / fastest;
    if (tweens.length > 0 && !dirty && !finishing && time - lastRender < interval - 1) {
      schedule();
      return;
    }
    const finished: Tween[] = [];
    for (const t of tweens) {
      t.start ??= time;
      const p = Math.min(1, (time - t.start) / t.ms);
      t.fn(p);
      if (p >= 1) finished.push(t);
    }
    tweens = tweens.filter((t) => !finished.includes(t));
    dirty = false;
    lastRender = time;
    render();
    for (const t of finished) t.done();
    if (tweens.length > 0 || dirty) schedule();
  }

  return {
    invalidate() {
      dirty = true;
      schedule();
    },
    tween(ms, fps, fn, key) {
      if (disposed) return Promise.resolve();
      if (!motion || ms <= 0) {
        fn(1);
        this.invalidate();
        return Promise.resolve();
      }
      return new Promise<void>((resolve) => {
        if (key !== undefined) {
          for (const old of tweens.filter((t) => t.key === key)) old.done();
          tweens = tweens.filter((t) => t.key !== key);
        }
        tweens.push({ key, ms, fps, fn, start: null, done: resolve });
        schedule();
      });
    },
    setMotion(on) {
      motion = on;
      if (on || tweens.length === 0) return;
      const pending = tweens;
      tweens = [];
      for (const t of pending) {
        t.fn(1);
        t.done();
      }
      this.invalidate();
    },
    dispose() {
      disposed = true;
      if (id !== null) caf(id);
      id = null;
      const pending = tweens;
      tweens = [];
      for (const t of pending) t.done();
    },
  };
}
