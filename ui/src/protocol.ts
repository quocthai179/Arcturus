// Kiểu thông điệp WebSocket, viết tay khớp crates/hub-core/src/protocol.rs (docs/PROTOCOL.md). Đổi giao thức phải sửa
// cả ba nơi cùng commit.

export type AgentId = string;
export type RequestId = string;

export type Mood = 'idle' | 'working' | 'approval' | 'error' | 'done';
export type Activity = 'typing' | 'reading' | 'running' | 'thinking' | 'waiting' | 'idle';
export type AgentKind = 'session' | 'subagent' | 'teammate';
export type Source = 'claude-code';
export type Risk = 'low' | 'medium' | 'high' | 'critical';
export type Decision = 'allow' | 'deny';
export type DecidedBy = 'user' | 'countdown' | 'policy' | 'timeout' | 'cancelled';
export type Suspicion = 'hidden_chars' | 'subshell' | 'eval' | 'encoded';
export type DecideOrigin = 'user' | 'countdown';

export interface Palette {
  hair: string;
  outfit: string;
  acc: string;
}

export interface AgentView {
  id: AgentId;
  parent_id: AgentId | null;
  kind: AgentKind;
  name: string;
  role: string;
  palette: Palette;
  mood: Mood;
  activity: Activity;
  act_text: string;
  host: string;
  source: Source;
  cwd: string;
  context_pct: number | null;
  last_event_at: string;
}

export interface ApprovalView {
  request_id: RequestId;
  agent_id: AgentId;
  agent_name: string;
  tool: string;
  command_original: string;
  tool_options: Record<string, string>;
  /** Cơ sở duyệt (quy tắc bất biến 3): ký tự ẩn đã hiện thành `⟨U+XXXX⟩`. */
  command_normalized: string;
  /** `[start, end)` theo chỉ số ký tự Unicode (code point) trong `command_normalized`. */
  danger_spans: [number, number][];
  suspicions: Suspicion[];
  truncated: boolean;
  risk: Risk;
  reversible: boolean;
  group_key: string;
  created_at: string;
  deadline_at: string;
  countdown_ms: number | null;
}

export interface HandoffView {
  handoff_id: string;
  agent_id: AgentId;
  agent_name: string;
  tool: string;
  bytes: number;
  lines: number;
  preview_head: string;
  preview_tail: string;
  suspicions: Suspicion[];
  tool_options: Record<string, string>;
  message: string;
  created_at: string;
}

export interface TeamView {
  paused: boolean;
  focus: boolean;
  roster_max: number;
}

export type ServerMsg =
  | { t: 'snapshot'; agents: AgentView[]; queue: ApprovalView[]; handoffs: HandoffView[]; team: TeamView }
  | { t: 'feed'; text: string; ts: string }
  | { t: 'agent_updated'; agent: AgentView }
  | { t: 'agent_removed'; id: AgentId }
  | { t: 'approval_requested'; approval: ApprovalView }
  | { t: 'approval_resolved'; request_id: RequestId; decision: Decision | null; decided_by: DecidedBy }
  | { t: 'team_updated'; team: TeamView }
  | { t: 'approval_handed_off'; handoff: HandoffView }
  | { t: 'handoff_cleared'; handoff_id: string };

export type ClientMsg =
  | { t: 'decide'; request_id: RequestId; decision: Decision; message?: string; origin: DecideOrigin }
  | { t: 'decide_group'; group_key: string; decision: Decision }
  | { t: 'team_pause' }
  | { t: 'team_resume' }
  | { t: 'focus_set'; on: boolean };

const SERVER_TYPES: ReadonlySet<string> = new Set([
  'snapshot',
  'feed',
  'agent_updated',
  'agent_removed',
  'approval_requested',
  'approval_resolved',
  'team_updated',
  'approval_handed_off',
  'handoff_cleared',
]);

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/**
 * Đọc một frame từ hub. Chỉ kiểm khung ngoài (`t` biết được, các trường bắt buộc có đúng kiểu cơ bản); `null` khi sai,
 * client bỏ qua frame đó. Hub là nguồn tin cậy về hình dạng, nhưng nội dung chuỗi luôn là dữ liệu không tin cậy.
 */
export function parseServerMsg(raw: string): ServerMsg | null {
  let v: unknown;
  try {
    v = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isRecord(v) || typeof v.t !== 'string' || !SERVER_TYPES.has(v.t)) {
    return null;
  }
  switch (v.t) {
    case 'snapshot':
      if (!Array.isArray(v.agents) || !Array.isArray(v.queue) || !Array.isArray(v.handoffs) || !isRecord(v.team)) {
        return null;
      }
      break;
    case 'feed':
      if (typeof v.text !== 'string') return null;
      break;
    case 'agent_updated':
      if (!isRecord(v.agent) || typeof v.agent.id !== 'string') return null;
      break;
    case 'agent_removed':
      if (typeof v.id !== 'string') return null;
      break;
    case 'approval_requested':
      if (!isRecord(v.approval) || typeof v.approval.request_id !== 'string') return null;
      break;
    case 'approval_resolved':
      if (typeof v.request_id !== 'string') return null;
      break;
    case 'team_updated':
      if (!isRecord(v.team)) return null;
      break;
    case 'approval_handed_off':
      if (!isRecord(v.handoff) || typeof v.handoff.handoff_id !== 'string') return null;
      break;
    case 'handoff_cleared':
      if (typeof v.handoff_id !== 'string') return null;
      break;
  }
  return v as ServerMsg;
}
