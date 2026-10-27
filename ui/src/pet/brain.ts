// Tầng 0 của pet (docs/PLAN.md §7.4): suy tâm trạng và lời nói từ trạng thái đội. Hàm thuần, 0 token: mọi câu là văn bản
// mẫu ghép từ dữ liệu có cấu trúc (quy tắc bất biến 3 và 5). Chuỗi từ agent (tên, act_text) là dữ liệu không tin cậy: ký tự
// ẩn hiện thành `⟨U+XXXX⟩`, cắt độ dài cho lời nói; lệnh chờ duyệt KHÔNG nằm trong lời nói (bong bóng hiện lệnh đầy đủ).
import { RISK_LABELS } from '../components/ApprovalCard';
import type { AgentId, ApprovalView, Mood, Risk } from '../protocol';
import type { HubState } from '../store';
import { revealHidden } from '../text';
import type { PetMood } from './types';

export type SpeechPriority = 'low' | 'normal' | 'high';

export interface Speech {
  /** Khóa chống lặp: cùng khóa chỉ nói một lần. */
  key: string;
  /** Câu hiện trong bong bóng. */
  text: string;
  priority: SpeechPriority;
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
const NAME_MAX = 32;
const ACT_MAX = 80;

export function initialMemory(now: number): BrainMemory {
  return { said: [], lastSpokeAt: 0, agentMoods: {}, lastChangeAt: now, wasConnected: false, everConnected: false, wasPaused: false };
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

export function think(state: HubState, mem: BrainMemory, now: number, sleepAfterMs = SLEEP_AFTER_MS): BrainOutput {
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
      candidates.push({ key: `err:${a.id}:${a.last_event_at}`, text: act ? `${name} gặp lỗi: ${act}` : `${name} gặp lỗi.`, priority: 'normal' });
    } else if (a.mood === 'done' && before !== undefined) {
      candidates.push({ key: `done:${a.id}:${a.last_event_at}`, text: `${name} xong việc rồi.`, priority: 'low' });
    }
  }
  if (Object.keys(mem.agentMoods).some((id) => !(id in agentMoods))) lastChangeAt = now;

  const queue = visibleQueue(state);
  const approval = connected ? (queue[0] ?? null) : null;
  for (const a of connected ? queue : []) {
    const key = `req:${a.request_id}`;
    if (said.has(key)) continue;
    lastChangeAt = now;
    const name = cleanText(a.agent_name, NAME_MAX);
    const tool = cleanText(a.tool, 24);
    candidates.push({
      key,
      text: a.risk === 'critical' ? `${name} xin chạy ${tool} trong vùng khóa. Anh mở hàng đợi xem kỹ nhé.` : `${name} xin chạy ${tool} (${RISK_LABELS[a.risk].toLowerCase()}).`,
      priority: URGENT.has(a.risk) ? 'high' : 'normal',
    });
  }

  if (!connected && mem.wasConnected) {
    candidates.push({ key: `conn:lost:${now}`, text: 'Mất kết nối hub. Yêu cầu duyệt vẫn hiện trong terminal.', priority: 'high' });
  }
  if (connected && state.team.paused && !mem.wasPaused) {
    candidates.push({ key: `paused:${now}`, text: 'Đội đang tạm dừng: không có gì được tự duyệt.', priority: 'normal' });
  }
  if (connected && !state.team.paused && mem.wasPaused) {
    candidates.push({ key: `resumed:${now}`, text: 'Đội làm việc tiếp rồi.', priority: 'low' });
  }

  // Chọn câu: khẩn trước; giờ tập trung chỉ nói câu khẩn; câu thường phải cách câu trước SPEAK_GAP_MS.
  let speech: Speech | null = null;
  const order: SpeechPriority[] = ['high', 'normal', 'low'];
  for (const p of order) {
    const c = candidates.find((s) => s.priority === p && !said.has(s.key));
    if (c === undefined) continue;
    if (p !== 'high' && (state.team.focus || now - mem.lastSpokeAt < SPEAK_GAP_MS)) break;
    speech = c;
    break;
  }
  // Câu bị bỏ qua vì giờ tập trung/giãn cách thì không nói lại sau (tin cũ). Yêu cầu duyệt vẫn được chỉ tới qua `approval`.
  const nextSaid = [...mem.said, ...candidates.map((c) => c.key).filter((k) => !said.has(k))].slice(-SAID_MAX);

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
    },
  };
}
