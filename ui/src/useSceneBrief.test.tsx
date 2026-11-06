import { act, cleanup, render } from '@testing-library/react';
import { StrictMode } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import type { SceneBrief } from './sceneBrief';
import { initialState, type HubState } from './store';
import { agent, approval } from './test/fixtures';
import { useSceneBrief } from './useSceneBrief';

afterEach(cleanup);

function Probe({ state, out, enabled = true }: { state: HubState; out: { brief: SceneBrief | null }; enabled?: boolean }) {
  out.brief = useSceneBrief(state, enabled);
  return null;
}

const open: HubState = { ...initialState, connection: 'open', synced: true, agents: { agt_s1: agent({ mood: 'working' }) } };

describe('useSceneBrief', () => {
  it('speaks_a_new_request_even_under_StrictMode', () => {
    const out: { brief: SceneBrief | null } = { brief: null };
    const view = render(<StrictMode><Probe state={open} out={out} /></StrictMode>);
    const withRequest: HubState = { ...open, queue: [approval({ risk: 'high' })] };
    act(() => view.rerender(<StrictMode><Probe state={withRequest} out={out} /></StrictMode>));
    expect(out.brief?.speech?.text).toContain('xin chạy Bash');
    expect(out.brief?.pending.get('agt_s1')?.count).toBe(1);
  });

  it('starts_with_an_empty_brief_and_never_uses_timers', () => {
    const out: { brief: SceneBrief | null } = { brief: null };
    render(<Probe state={initialState} out={out} />);
    expect(out.brief?.connected).toBe(false);
    expect(out.brief?.speech).toBeNull();
  });

  it('disabled_never_runs_think_and_keeps_the_brief_empty', () => {
    const out: { brief: SceneBrief | null } = { brief: null };
    const view = render(<Probe state={open} out={out} enabled={false} />);
    view.rerender(<Probe state={{ ...open, queue: [approval({ risk: 'high' })] }} out={out} enabled={false} />);
    expect(out.brief?.speech).toBeNull();
    expect(out.brief?.pending.size).toBe(0);
  });

  it('keeps_the_same_reference_when_the_content_is_unchanged', () => {
    const out: { brief: SceneBrief | null } = { brief: null };
    const view = render(<Probe state={open} out={out} />);
    const first = out.brief;
    view.rerender(<Probe state={{ ...open }} out={out} />);
    expect(out.brief).toBe(first);
  });
});
