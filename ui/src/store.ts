// Trạng thái UI dựng từ snapshot + delta của hub (docs/PROTOCOL.md "Vòng đời"): snapshot thay toàn bộ; delta là upsert
// theo id; xóa id không biết thì bỏ qua. UI không bao giờ tự suy luận trạng thái ngoài các thông điệp này.
import type { AgentId, AgentView, ApprovalView, HandoffView, ServerMsg, TeamView } from './protocol';

export type ConnectionStatus = 'connecting' | 'open' | 'closed';

export interface FeedItem {
  text: string;
  ts: string;
}

export interface HubState {
  connection: ConnectionStatus;
  /** Lý do không kết nối được (hiện cho người dùng), `null` khi đang ổn. */
  error: string | null;
  /** Đã nhận ít nhất một snapshot. */
  synced: boolean;
  agents: Record<AgentId, AgentView>;
  /** Cũ trước (theo `created_at`). */
  queue: ApprovalView[];
  handoffs: HandoffView[];
  team: TeamView;
  /** Mới nhất trước, tối đa [`FEED_MAX`]. */
  feed: FeedItem[];
}

export const FEED_MAX = 5;

export const initialState: HubState = {
  connection: 'connecting',
  error: null,
  synced: false,
  agents: {},
  queue: [],
  handoffs: [],
  team: { paused: false, focus: false, roster_max: 5 },
  feed: [],
};

export type Action =
  | { type: 'server'; msg: ServerMsg }
  | { type: 'connection'; status: ConnectionStatus; error?: string | null };

function byCreated<T extends { created_at: string }>(a: T, b: T): number {
  return a.created_at < b.created_at ? -1 : a.created_at > b.created_at ? 1 : 0;
}

function upsert<T>(list: readonly T[], item: T, key: (x: T) => string, sort?: (a: T, b: T) => number): T[] {
  const k = key(item);
  const i = list.findIndex((x) => key(x) === k);
  const next = i >= 0 ? list.map((x, j) => (j === i ? item : x)) : [...list, item];
  return sort !== undefined && i < 0 ? next.sort(sort) : next;
}

export function applyServer(state: HubState, msg: ServerMsg): HubState {
  switch (msg.t) {
    case 'snapshot': {
      const agents: Record<AgentId, AgentView> = {};
      for (const a of msg.agents) agents[a.id] = a;
      return {
        ...state,
        synced: true,
        agents,
        queue: [...msg.queue].sort(byCreated),
        handoffs: [...msg.handoffs].sort(byCreated),
        team: msg.team,
      };
    }
    case 'agent_updated':
      return { ...state, agents: { ...state.agents, [msg.agent.id]: msg.agent } };
    case 'agent_removed': {
      if (!(msg.id in state.agents)) return state;
      const agents = { ...state.agents };
      delete agents[msg.id];
      return { ...state, agents };
    }
    case 'approval_requested':
      return { ...state, queue: upsert(state.queue, msg.approval, (a) => a.request_id, byCreated) };
    case 'approval_resolved': {
      const queue = state.queue.filter((a) => a.request_id !== msg.request_id);
      return queue.length === state.queue.length ? state : { ...state, queue };
    }
    case 'approval_handed_off':
      return { ...state, handoffs: upsert(state.handoffs, msg.handoff, (h) => h.handoff_id, byCreated) };
    case 'handoff_cleared': {
      const handoffs = state.handoffs.filter((h) => h.handoff_id !== msg.handoff_id);
      return handoffs.length === state.handoffs.length ? state : { ...state, handoffs };
    }
    case 'team_updated':
      return { ...state, team: msg.team };
    case 'feed':
      return { ...state, feed: [{ text: msg.text, ts: msg.ts }, ...state.feed].slice(0, FEED_MAX) };
  }
}

export function reducer(state: HubState, action: Action): HubState {
  switch (action.type) {
    case 'server':
      return applyServer(state, action.msg);
    case 'connection':
      return { ...state, connection: action.status, error: action.error ?? null };
  }
}
