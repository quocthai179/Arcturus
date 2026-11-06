// Lớp three không có GPU trong jsdom: thay `WebGLRenderer` bằng bản giả (phần còn lại của three là thật) để kiểm vòng đời,
// đường lui khi mất context và giải phóng tài nguyên.
import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const h = vi.hoisted(() => ({ renderers: [] as { dispose: ReturnType<typeof vi.fn>; forceContextLoss: ReturnType<typeof vi.fn>; domElement: HTMLCanvasElement; renders: number }[] }));

vi.mock('three', async (importOriginal) => {
  const actual = await importOriginal<typeof import('three')>();
  class FakeRenderer {
    domElement = document.createElement('canvas');
    shadowMap = { enabled: false, type: 0, autoUpdate: true, needsUpdate: false };
    outputColorSpace = '';
    dispose = vi.fn();
    forceContextLoss = vi.fn();
    renders = 0;
    constructor() {
      h.renderers.push(this);
    }
    setPixelRatio(): void {}
    setSize(): void {}
    render(): void {
      this.renders++;
    }
  }
  return { ...actual, WebGLRenderer: FakeRenderer };
});

import { agent } from '../../test/fixtures';
import { DEFAULT_VIEW } from '../camera';
import type { BuildingCanvasProps, BuildingModel } from '../model';
import { initialSecretary } from '../secretary';
import { assignSeats, EMPTY_SEATS, rosters } from '../seating';
import BuildingCanvas from './BuildingCanvas';

const agents = [agent({ id: 'agt_a', kind: 'subagent', role: 'dev-squad:developer', mood: 'working', activity: 'typing' })];
const model: BuildingModel = {
  view: DEFAULT_VIEW,
  rosters: rosters(assignSeats(EMPTY_SEATS, agents), agents, undefined, Date.now()),
  secretary: { mem: initialSecretary(), startAt: 0 },
  selectable: new Set(),
  pending: new Map(),
  timeMode: 'day',
};

function props(over: Partial<BuildingCanvasProps> = {}): BuildingCanvasProps {
  return { model, motion: false, onHover: vi.fn(), onPick: vi.fn(), onReady: vi.fn(), onError: vi.fn(), ...over };
}

beforeEach(() => {
  h.renderers.length = 0;
  vi.stubGlobal('ResizeObserver', class { observe(): void {} disconnect(): void {} unobserve(): void {} });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('BuildingCanvas lifecycle', () => {
  it('renders_on_demand_and_reports_ready', async () => {
    const p = props();
    render(<BuildingCanvas {...p} />);
    await vi.waitFor(() => expect(p.onReady).toHaveBeenCalledTimes(1));
    expect(h.renderers[0]?.renders).toBeGreaterThan(0);
    expect(p.onError).not.toHaveBeenCalled();
  });

  it('webglcontextlost_leads_to_onError', () => {
    const p = props();
    render(<BuildingCanvas {...p} />);
    const canvas = h.renderers[0]?.domElement;
    const ev = new Event('webglcontextlost', { cancelable: true });
    canvas?.dispatchEvent(ev);
    expect(p.onError).toHaveBeenCalledWith('webglcontextlost');
    expect(ev.defaultPrevented).toBe(true);
  });

  it('unmount_disposes_the_renderer_forces_context_loss_and_removes_the_canvas', () => {
    const view = render(<BuildingCanvas {...props()} />);
    const r = h.renderers[0];
    expect(r?.domElement.isConnected).toBe(true);
    view.unmount();
    expect(r?.dispose).toHaveBeenCalledTimes(1);
    expect(r?.forceContextLoss).toHaveBeenCalledTimes(1);
    expect(r?.domElement.isConnected).toBe(false);
  });

  it('two_quick_navigations_end_on_the_last_view_without_errors', async () => {
    const p = props();
    const view = render(<BuildingCanvas {...p} />);
    view.rerender(<BuildingCanvas {...p} model={{ ...model, view: 1 }} />);
    view.rerender(<BuildingCanvas {...p} model={{ ...model, view: 2 }} />);
    await new Promise((r) => setTimeout(r, 30));
    expect(p.onError).not.toHaveBeenCalled();
    expect(h.renderers[0]?.dispose).not.toHaveBeenCalled();
  });
});
