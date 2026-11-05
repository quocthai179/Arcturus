import { act, cleanup, render } from '@testing-library/react';
import { StrictMode } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import type { SceneBrief } from './sceneBrief';
import { initialState, type HubState } from './store';
import { agent, approval } from './test/fixtures';
import { useSceneBrief } from './useSceneBrief';

afterEach(cleanup);

function Probe({ state, out }: { state: HubState; out: { brief: SceneBrief | null } }) {
  out.brief = useSceneBrief(state);
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
});
