// Trạng thái UI dựng từ snapshot + delta của hub (docs/PROTOCOL.md "Vòng đời"): snapshot thay toàn bộ; delta là upsert
// theo id; xóa id không biết thì bỏ qua. UI không bao giờ tự suy luận trạng thái ngoài các thông điệp này.
import type {
  AgentId,
  AgentView,
  ApprovalView,
  HandoffView,
  ServerMsg,
  TeamView,
  TrustSuggestionView,
} from './protocol';

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
  /** Đề xuất nâng mức tin cậy đang chờ trả lời (T10). */
  suggestions: TrustSuggestionView[];
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
  suggestions: [],
};

export type Action =
  | { type: 'server'; msg: ServerMsg }
  | { type: 'connection'; status: ConnectionStatus; error?: string | null };

function byCreated<T extends { created_at: string }>(a: T, b: T): number {
  return a.created_at < b.created_at ? -1 : a.created_at > b.created_at ? 1 : 0;
}

/** Đội tạm dừng: không yêu cầu nào được có đếm ngược (hub cũng xóa ở phía mình). */
function withoutCountdown(queue: ApprovalView[]): ApprovalView[] {
  return queue.some((a) => a.countdown_ms !== null)
    ? queue.map((a) => (a.countdown_ms === null ? a : { ...a, countdown_ms: null }))
    : queue;
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
      const queue = [...msg.queue].sort(byCreated);
      return {
        ...state,
        synced: true,
        // Hub gửi lại các đề xuất còn chờ ngay sau snapshot.
        suggestions: [],
        agents,
        queue: msg.team.paused ? withoutCountdown(queue) : queue,
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
    case 'approval_requested': {
      const approval = state.team.paused && msg.approval.countdown_ms !== null ? { ...msg.approval, countdown_ms: null } : msg.approval;
      return { ...state, queue: upsert(state.queue, approval, (a) => a.request_id, byCreated) };
    }
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
      // Tạm dừng xóa đếm ngược của mọi yêu cầu; tiếp tục không khôi phục (chỉ yêu cầu tới sau mới có).
      return { ...state, team: msg.team, queue: msg.team.paused ? withoutCountdown(state.queue) : state.queue };
    case 'suggestion':
      return { ...state, suggestions: upsert(state.suggestions, msg.suggestion, (x) => x.id) };
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
