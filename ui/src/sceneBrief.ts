// Tóm tắt chỉ đọc cho cảnh Văn phòng (docs/OFFICE_BUILDING.md §2.2). Hàm thuần, không import three. KHÔNG chứa
// `command_original`/`command_normalized` (quy tắc bất biến 3): thư ký chỉ biết `tool` và mức rủi ro.
import type { AgentId, Risk } from './protocol';
import type { BrainOutput, SpeechPriority } from './pet/brain';
import type { PetMood } from './pet/types';
import type { HubState } from './store';

export interface PendingBrief {
  /** Số yêu cầu đang chờ của agent. */
  count: number;
  /** Rủi ro cao nhất. */
  risk: Risk;
  /** Tool của yêu cầu cũ nhất (dữ liệu không tin cậy: chỉ hiện qua `cleanText`). */
  tool: string;
  /** `Date.parse(created_at)` của yêu cầu cũ nhất. */
  oldestAt: number;
}

export interface SceneBrief {
  connected: boolean;
  paused: boolean;
  focus: boolean;
  pending: ReadonlyMap<AgentId, PendingBrief>;
  /** Agent có thẻ "Duyệt trong terminal". */
  handoffs: ReadonlySet<AgentId>;
  speech: { key: string; text: string; priority: SpeechPriority } | null;
  /** `sleep` = đội rảnh lâu. */
  teamMood: PetMood;
}

export const RISK_RANK: Record<Risk, number> = { critical: 0, high: 1, medium: 2, low: 3 };

export function sceneBrief(state: HubState, out: Pick<BrainOutput, 'speech' | 'mood'>): SceneBrief {
  const pending = new Map<AgentId, PendingBrief>();
  // `queue` đã cũ trước: yêu cầu đầu tiên của mỗi agent là cũ nhất.
  for (const a of state.queue) {
    const at = Date.parse(a.created_at);
    const before = pending.get(a.agent_id);
    if (before === undefined) {
      pending.set(a.agent_id, { count: 1, risk: a.risk, tool: a.tool, oldestAt: at });
    } else {
      const older = at < before.oldestAt;
      pending.set(a.agent_id, {
        count: before.count + 1,
        risk: RISK_RANK[a.risk] < RISK_RANK[before.risk] ? a.risk : before.risk,
        tool: older ? a.tool : before.tool,
        oldestAt: older ? at : before.oldestAt,
      });
    }
  }
  return {
    connected: state.connection === 'open' && state.synced,
    paused: state.team.paused,
    focus: state.team.focus,
    pending,
    handoffs: new Set(state.handoffs.map((h) => h.agent_id)),
    speech: out.speech === null ? null : { key: out.speech.key, text: out.speech.text, priority: out.speech.priority },
    teamMood: out.mood,
  };
}
