// Dựng thế giới lỗi sau khi đã có renderer: phải giải phóng context và báo `onError` (không để lọt ngoại lệ ra React).
import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

const h = vi.hoisted(() => ({ made: [] as { dispose: ReturnType<typeof vi.fn>; forceContextLoss: ReturnType<typeof vi.fn>; domElement: HTMLCanvasElement }[] }));

vi.mock('three', async (importOriginal) => {
  const actual = await importOriginal<typeof import('three')>();
  class FakeRenderer {
    domElement = document.createElement('canvas');
    shadowMap = { enabled: false, type: 0, autoUpdate: true };
    dispose = vi.fn();
    forceContextLoss = vi.fn();
    constructor() {
      h.made.push(this);
    }
    setPixelRatio(): void {}
  }
  return { ...actual, WebGLRenderer: FakeRenderer };
});
vi.mock('./world', () => ({
  buildWorld: () => {
    throw new Error('world exploded');
  },
}));

import BuildingCanvas from './BuildingCanvas';

afterEach(cleanup);

describe('BuildingCanvas init failure', () => {
  it('disposes_the_renderer_forces_context_loss_and_calls_onError', () => {
    const onError = vi.fn();
    render(<BuildingCanvas model={{} as never} motion={false} onHover={vi.fn()} onPick={vi.fn()} onReady={vi.fn()} onError={onError} />);
    expect(onError).toHaveBeenCalledWith('world exploded');
    expect(h.made[0]?.dispose).toHaveBeenCalled();
    expect(h.made[0]?.forceContextLoss).toHaveBeenCalled();
    expect(h.made[0]?.domElement.isConnected).toBe(false);
  });
});
