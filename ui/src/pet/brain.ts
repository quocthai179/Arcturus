// Tầng 0 của pet (docs/PLAN.md §7.4): suy tâm trạng và lời nói từ trạng thái đội. Hàm thuần, 0 token: mọi câu là văn bản
// mẫu ghép từ dữ liệu có cấu trúc (quy tắc bất biến 3 và 5). Chuỗi từ agent (tên, act_text) là dữ liệu không tin cậy: ký tự
// ẩn hiện thành `⟨U+XXXX⟩`, cắt độ dài cho lời nói; lệnh chờ duyệt KHÔNG nằm trong lời nói (bong bóng hiện lệnh đầy đủ).
import { RISK_LABELS } from '../components/ApprovalCard';
import type { AgentId, ApprovalView, Mood, Risk } from '../protocol';
import type { HubState } from '../store';
import { revealHidden } from '../text';
import type { Chatter, PetMood } from './types';

export type SpeechPriority = 'low' | 'normal' | 'high';

export type SpeechKind = 'done' | 'error' | 'request' | 'conn' | 'team';

export interface Speech {
  /** Khóa chống lặp: cùng khóa chỉ nói một lần. */
  key: string;
  /** Câu hiện trong bong bóng. */
  text: string;
  priority: SpeechPriority;
  kind: SpeechKind;
  /** Tên agent (đã làm sạch) với câu về một agent. */
  agent?: string;
}

/** Một mục trong "Lời nhắc gần đây" (PLAN §7.11): mọi câu pet định nói, kể cả câu bị bỏ qua. */
export interface JournalEntry {
  at: number;
  text: string;
  priority: SpeechPriority;
  kind: SpeechKind;
  agent?: string;
  /** Pet đã nói câu này ra (bong bóng/giọng) hay bỏ qua vì mức nhắc, giờ tập trung, giãn cách. */
  spoken: boolean;
}

export interface BrainMemory {
  /** Khóa đã nói (giữ có giới hạn). */
  said: readonly string[];
  lastSpokeAt: number;
  agentMoods: Readonly<Record<AgentId, Mood>>;
  /** Lần cuối đội có thay đổi (để quyết định ngủ). */
  lastChangeAt: number;
  /** Lần gọi trước đang kết nối (báo mất kết nối đúng một lần lúc chuyển). */
  wasConnected: boolean;
  /** Đã từng kết nối (mất kết nối sau đó thì pet lo lắng; chưa từng thì chỉ đang chờ). */
  everConnected: boolean;
  wasPaused: boolean;
  /** Nhật ký lời nhắc, cũ trước, tối đa `JOURNAL_MAX`. Chỉ trong bộ nhớ của cửa sổ pet. */
  journal: readonly JournalEntry[];
}

export interface BrainOutput {
  mood: PetMood;
  /** Yêu cầu duyệt pet đang chỉ tới (cũ nhất trong số được báo); `null` khi không có. */
  approval: ApprovalView | null;
  /** Số yêu cầu đang chờ mà pet được phép báo (giờ tập trung chỉ tính High/Critical). */
  visibleCount: number;
  speech: Speech | null;
  memory: BrainMemory;
}

export const SLEEP_AFTER_MS = 10 * 60_000;
/** Khoảng tối thiểu giữa hai câu không khẩn (chống nói liên tục). */
export const SPEAK_GAP_MS = 6_000;
const SAID_MAX = 200;
export const JOURNAL_MAX = 30;
/** Người dùng không tương tác với pet chừng này thì lần tương tác sau được nghe tóm tắt lúc vắng. */
export const AWAY_MS = 10 * 60_000;
const NAME_MAX = 32;
const ACT_MAX = 80;

export function initialMemory(now: number): BrainMemory {
  return { said: [], lastSpokeAt: 0, agentMoods: {}, lastChangeAt: now, wasConnected: false, everConnected: false, wasPaused: false, journal: [] };
}

/** Chuỗi không tin cậy → văn bản an toàn để nói/hiện: ký tự ẩn hiện ra, gộp khoảng trắng, cắt theo code point. */
export function cleanText(s: string, max: number): string {
  const flat = revealHidden(s).replace(/\s+/g, ' ').trim();
  const chars = Array.from(flat);
  return chars.length <= max ? flat : `${chars.slice(0, max - 1).join('')}…`;
}

const URGENT: ReadonlySet<Risk> = new Set<Risk>(['high', 'critical']);

/** Yêu cầu pet được báo: giờ tập trung chỉ High/Critical (giống chấm báo trên tray, T14). Cũ trước. */
export function visibleQueue(state: HubState): ApprovalView[] {
  return state.team.focus ? state.queue.filter((a) => URGENT.has(a.risk)) : state.queue;
}

function moodOfTeam(state: HubState): PetMood {
  const moods = Object.values(state.agents).map((a) => a.mood);
  if (moods.includes('error')) return 'error';
  if (moods.includes('working') || moods.includes('approval')) return 'working';
  if (moods.includes('done')) return 'done';
  return 'idle';
}

/** Mức nhắc thật sự: giờ tập trung của hub ép tối đa `urgent`. */
export function effectiveChatter(chatter: Chatter, focus: boolean): Chatter {
  return focus && chatter === 'normal' ? 'urgent' : chatter;
}

const RISK_RANK: Record<ApprovalView['risk'], number> = { critical: 3, high: 2, medium: 1, low: 0 };

/** Yêu cầu rủi ro cao nhất; cùng mức thì giữ thứ tự hàng đợi (cái đến trước). */
export function mostUrgent(queue: readonly ApprovalView[]): ApprovalView | null {
  let best: ApprovalView | null = null;
  for (const a of queue) {
    if (best === null || RISK_RANK[a.risk] > RISK_RANK[best.risk]) best = a;
  }
  return best;
}

export function think(state: HubState, mem: BrainMemory, now: number, sleepAfterMs = SLEEP_AFTER_MS, chatter: Chatter = 'normal'): BrainOutput {
  const connected = state.connection === 'open' && state.synced;
  const said = new Set(mem.said);
  const candidates: Speech[] = [];
  let lastChangeAt = mem.lastChangeAt;

  // Thay đổi của từng agent (chỉ báo lúc chuyển sang lỗi/xong, mỗi lần chuyển một câu).
  const agentMoods: Record<AgentId, Mood> = {};
  for (const a of Object.values(state.agents)) {
    agentMoods[a.id] = a.mood;
    const before = mem.agentMoods[a.id];
    if (before === a.mood) continue;
    lastChangeAt = now;
    const name = cleanText(a.name, NAME_MAX);
    if (a.mood === 'error') {
      const act = cleanText(a.act_text, ACT_MAX);
      candidates.push({ key: `err:${a.id}:${a.last_event_at}`, text: act ? `${name} gặp lỗi: ${act}` : `${name} gặp lỗi.`, priority: 'normal', kind: 'error', agent: name });
    } else if (a.mood === 'done' && before !== undefined) {
      candidates.push({ key: `done:${a.id}:${a.last_event_at}`, text: `${name} xong việc rồi.`, priority: 'low', kind: 'done', agent: name });
    }
  }
  if (Object.keys(mem.agentMoods).some((id) => !(id in agentMoods))) lastChangeAt = now;

  const queue = visibleQueue(state);
  // Thẻ của pet hiện yêu cầu gấp nhất (cùng mức thì cái đến trước), khớp với câu pet nói: lời nói ưu tiên rủi ro cao, nên
  // thẻ lấy `queue[0]` từng nói về vùng khóa của agent này trong khi thẻ là lệnh rủi ro thấp của agent khác.
  const approval = connected ? mostUrgent(queue) : null;
  for (const a of connected ? queue : []) {
    const key = `req:${a.request_id}`;
    if (said.has(key)) continue;
    lastChangeAt = now;
    const name = cleanText(a.agent_name, NAME_MAX);
    const tool = cleanText(a.tool, 24);
    candidates.push({
      key,
      text: a.risk === 'critical' ? `${name} xin chạy ${tool} trong vùng khóa. Bạn mở hàng đợi xem kỹ nhé.` : `${name} xin chạy ${tool} (${RISK_LABELS[a.risk].toLowerCase()}).`,
      priority: URGENT.has(a.risk) ? 'high' : 'normal',
      kind: 'request',
      agent: name,
    });
  }

  if (!connected && mem.wasConnected) {
    candidates.push({ key: `conn:lost:${now}`, text: 'Mất kết nối hub. Yêu cầu duyệt vẫn hiện trong terminal.', priority: 'high', kind: 'conn' });
  }
  if (connected && state.team.paused && !mem.wasPaused) {
    candidates.push({ key: `paused:${now}`, text: 'Đội đang tạm dừng: không có gì được tự duyệt.', priority: 'normal', kind: 'team' });
  }
  if (connected && !state.team.paused && mem.wasPaused) {
    candidates.push({ key: `resumed:${now}`, text: 'Đội làm việc tiếp rồi.', priority: 'low', kind: 'team' });
  }

  // Chọn câu: khẩn trước; mức `urgent` (hoặc giờ tập trung) chỉ nói câu khẩn; `quiet` không nói gì chủ động; câu thường phải
  // cách câu trước SPEAK_GAP_MS.
  const level = effectiveChatter(chatter, state.team.focus);
  let speech: Speech | null = null;
  const order: SpeechPriority[] = ['high', 'normal', 'low'];
  for (const p of level === 'quiet' ? [] : order) {
    const c = candidates.find((s) => s.priority === p && !said.has(s.key));
    if (c === undefined) continue;
    if (p !== 'high' && (level === 'urgent' || now - mem.lastSpokeAt < SPEAK_GAP_MS)) break;
    speech = c;
    break;
  }
  // Câu bị bỏ qua thì không nói lại sau (tin cũ) nhưng vẫn vào nhật ký. Yêu cầu duyệt vẫn được chỉ tới qua `approval`.
  const fresh = candidates.filter((c) => !said.has(c.key));
  const nextSaid = [...mem.said, ...fresh.map((c) => c.key)].slice(-SAID_MAX);
  const journal =
    fresh.length === 0
      ? mem.journal
      : [
          ...mem.journal,
          ...fresh.map((c): JournalEntry => ({ at: now, text: c.text, priority: c.priority, kind: c.kind, ...(c.agent !== undefined ? { agent: c.agent } : {}), spoken: c.key === speech?.key })),
        ].slice(-JOURNAL_MAX);

  let mood: PetMood;
  if (!connected) mood = mem.everConnected ? 'error' : 'idle';
  else if (approval !== null) mood = approval.risk === 'critical' ? 'critical' : 'approval';
  else if (state.team.paused) mood = 'paused';
  else mood = moodOfTeam(state);
  if ((mood === 'idle' || mood === 'done') && now - lastChangeAt >= sleepAfterMs) mood = 'sleep';

  return {
    mood,
    approval,
    visibleCount: connected ? queue.length : 0,
    speech,
    memory: {
      said: nextSaid,
      lastSpokeAt: speech !== null ? now : mem.lastSpokeAt,
      agentMoods,
      lastChangeAt,
      wasConnected: connected,
      everConnected: mem.everConnected || connected,
      wasPaused: connected ? state.team.paused : mem.wasPaused,
      journal,
    },
  };
}

// --- chip trạng thái (PLAN §7.11) ---

export type GlanceTone = 'critical' | 'attention' | 'error' | 'offline' | 'paused' | 'working' | 'done' | 'calm';

export interface Glance {
  tone: GlanceTone;
  /** Ký hiệu đứng trước chữ (trang trí; nghĩa nằm ở `text`). */
  icon: string;
  text: string;
  /** Bấm chip làm gì. */
  action: 'open_queue' | 'open_office' | 'status';
}

/** Một dòng nhìn là hiểu, ưu tiên điều cần người dùng nhất. Hàm thuần, 0 token. */
export function glance(state: HubState, out: Pick<BrainOutput, 'approval' | 'visibleCount' | 'memory'>): Glance {
  const connected = state.connection === 'open' && state.synced;
  if (!connected) {
    return out.memory.everConnected
      ? { tone: 'offline', icon: '⚡', text: 'Mất kết nối hub', action: 'status' }
      : { tone: 'calm', icon: '…', text: 'Đang kết nối hub', action: 'status' };
  }
  if (out.approval !== null) {
    const n = out.visibleCount;
    return out.approval.risk === 'critical'
      ? { tone: 'critical', icon: '⛔', text: n > 1 ? `Vùng khóa chờ duyệt · ${n}` : 'Vùng khóa chờ duyệt', action: 'open_queue' }
      : { tone: 'attention', icon: '⏳', text: `Cần bạn duyệt · ${n}`, action: 'open_queue' };
  }
  const team = Object.values(state.agents);
  const failed = team.filter((a) => a.mood === 'error');
  if (failed.length > 0) {
    const first = failed[0];
    const name = first === undefined ? '' : cleanText(first.name, 20);
    return { tone: 'error', icon: '!', text: failed.length > 1 ? `${name} lỗi · +${failed.length - 1}` : `${name} gặp lỗi`, action: 'open_office' };
  }
  if (state.team.paused) return { tone: 'paused', icon: '⏸', text: 'Đội đang tạm dừng', action: 'status' };
  const working = team.filter((a) => a.parent_id === null && (a.mood === 'working' || a.mood === 'approval')).length;
  if (working > 0) return { tone: 'working', icon: '▶', text: `${working} agent đang làm`, action: 'status' };
  if (team.some((a) => a.mood === 'done')) return { tone: 'done', icon: '✓', text: 'Xong hết việc', action: 'status' };
  return { tone: 'calm', icon: '·', text: team.length === 0 ? 'Chưa có agent nào' : 'Đội đang rảnh', action: 'status' };
}

// --- tóm tắt lúc vắng (PLAN §7.11) ---

/** Một câu tóm tắt các tin từ sau `since`; `null` khi không có gì mới. Yêu cầu chờ tính theo hàng đợi hiện tại. */
export function awayDigest(journal: readonly JournalEntry[], since: number, pending: number): string | null {
  const news = journal.filter((e) => e.at > since);
  if (news.length === 0) return null;
  const done = news.filter((e) => e.kind === 'done').length;
  const errors = news.filter((e) => e.kind === 'error');
  const names = [...new Set(errors.map((e) => e.agent).filter((n): n is string => n !== undefined))];
  const other = news.filter((e) => e.kind === 'conn' || e.kind === 'team').length;
  const parts: string[] = [];
  if (done > 0) parts.push(`${done} việc xong`);
  if (errors.length > 0) parts.push(`${errors.length} lỗi${names.length > 0 ? ` (${names.slice(0, 3).join(', ')}${names.length > 3 ? '…' : ''})` : ''}`);
  if (pending > 0) parts.push(`${pending} yêu cầu đang chờ duyệt`);
  if (other > 0) parts.push(`${other} tin về đội`);
  if (parts.length === 0) return 'Trong lúc bạn vắng có vài yêu cầu, giờ đã xử lý hết.';
  return `Trong lúc bạn vắng: ${parts.join(', ')}.`;
}
