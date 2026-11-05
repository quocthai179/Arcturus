// Thư ký trưởng (docs/OFFICE_BUILDING.md §7, ADR 0003). Thuần, 0 token: mọi câu là văn bản mẫu. Thư ký CHỈ quan sát và đi lại:
// không nhận, không trả `ClientMsg`; hành động của bong bóng chỉ là `select` (người dùng bấm thì vỏ gọi `onSelect`) hoặc `view`
// (đổi góc nhìn). Không hiện lệnh (chỉ `tool` + mức rủi ro), không gợi ý "nên đồng ý", không giao việc.
import { RISK_LABELS } from '../components/ApprovalCard';
import { cleanText, type SpeechPriority } from '../pet/brain';
import type { AgentId, AgentView } from '../protocol';
import { RISK_RANK, type PendingBrief, type SceneBrief } from '../sceneBrief';
import type { View } from './camera';
import type { PoseId } from './figure';
import { CEO_FLOOR, FLOORS, roleShort, type AgentFloorId, type FloorId } from './floors';
import { briefSpot, deskAt, ELEVATOR_DOOR, LOUNGE_SPOTS, SECRETARY_HOME, STANDING_SPOTS, type Vec3 } from './layout';
import { planRoute, routePosAt, type Place, type Route } from './route';
import type { FloorRoster, Spot } from './seating';

export type GoalKind = 'request' | 'handoff' | 'error';
export interface Goal {
  kind: GoalKind;
  agentId: AgentId;
  floor: AgentFloorId;
  key: string;
  rank: number;
}

export type BubbleAction = { kind: 'select'; agentId: AgentId; label: 'Xem thẻ duyệt' } | { kind: 'view'; view: View; label: string };
export interface Bubble {
  key: string;
  text: string;
  priority: SpeechPriority;
  until: number;
  actions: readonly BubbleAction[];
}

export type SecState =
  | { k: 'home' }
  | { k: 'moving'; route: Route; startAt: number; goal: Goal | null }
  | { k: 'briefing'; goal: Goal; since: number; clearedAt: number | null; at: Place }
  | { k: 'asleep' };

export interface SecMemory {
  state: SecState;
  /** Khóa đã nói, tối đa `SAID_MAX`. */
  said: readonly string[];
  bubble: Bubble | null;
  lastTripAt: number;
}

export interface SecInput {
  rosters: readonly FloorRoster[];
  brief: SceneBrief | undefined;
  canSelect: (a: AgentView) => boolean;
  motion: boolean;
  now: number;
}

export const BRIEF_RETURN_MS = 4_000;
export const TRIP_GAP_MS = 3_000;
export const SAY_MS = 8_000;
const SAID_MAX = 100;
const NAME_MAX = 32;
const TOOL_MAX = 24;
const ACT_MAX = 80;
/** Rank cách nhau chừng này thì đổi đích ngay, không chờ `TRIP_GAP_MS`. */
const MUCH_LOWER = 2;
const HOME: Place = { floor: CEO_FLOOR, pos: SECRETARY_HOME };

export function initialSecretary(): SecMemory {
  return { state: { k: 'home' }, said: [], bubble: null, lastTripAt: -Infinity };
}

// --- mục tiêu ---

interface Candidate extends Goal {
  at: number;
  agent: AgentView;
  pending: PendingBrief | undefined;
}

function candidates(rosters: readonly FloorRoster[], brief: SceneBrief | undefined, canSelect: (a: AgentView) => boolean): Candidate[] {
  const out: Candidate[] = [];
  for (const r of rosters) {
    for (const { agent } of r.figures) {
      const base = { agentId: agent.id, floor: r.floor, agent };
      const last = Date.parse(agent.last_event_at);
      const pending = brief?.pending.get(agent.id);
      if (pending !== undefined) {
        const rank = pending.risk === 'critical' || pending.risk === 'high' ? RISK_RANK[pending.risk] : 2;
        out.push({ ...base, kind: 'request', rank, key: `request:${agent.id}:${pending.oldestAt}`, at: pending.oldestAt, pending });
      } else if (brief === undefined && agent.mood === 'approval' && canSelect(agent)) {
        out.push({ ...base, kind: 'request', rank: 2, key: `request:${agent.id}:${last}`, at: last, pending: undefined });
      }
      if (brief?.handoffs.has(agent.id) === true) {
        out.push({ ...base, kind: 'handoff', rank: 3, key: `handoff:${agent.id}`, at: last, pending: undefined });
      }
      if (agent.mood === 'error') {
        out.push({ ...base, kind: 'error', rank: 4, key: `error:${agent.id}:${last}`, at: last, pending: undefined });
      }
    }
  }
  return out.sort((a, b) => a.rank - b.rank || a.at - b.at || (a.agentId < b.agentId ? -1 : a.agentId > b.agentId ? 1 : 0));
}

const toGoal = (c: Candidate): Goal => ({ kind: c.kind, agentId: c.agentId, floor: c.floor, key: c.key, rank: c.rank });

/** Mục tiêu tốt nhất mà thư ký được phép đi tới (giờ tập trung chỉ Critical/High; mất kết nối: không đi). */
export function pickGoal(rosters: readonly FloorRoster[], brief: SceneBrief | undefined, canSelect: (a: AgentView) => boolean): Goal | null {
  if (brief !== undefined && !brief.connected) return null;
  const list = candidates(rosters, brief, canSelect).filter((c) => brief?.focus !== true || c.rank <= 1);
  const first = list[0];
  return first === undefined ? null : toGoal(first);
}

// --- vị trí ---

function spotPos(spot: Spot): Vec3 {
  if (spot.kind === 'desk') return briefSpot(deskAt(spot.floor, spot.index).seat);
  const p = (spot.kind === 'lounge' ? LOUNGE_SPOTS : STANDING_SPOTS)[spot.floor]?.[spot.index] ?? SECRETARY_HOME;
  return { ...p, x: p.x + 0.75 };
}

function goalPlace(goal: Goal, rosters: readonly FloorRoster[]): Place | null {
  const fig = rosters.find((r) => r.floor === goal.floor)?.figures.find((f) => f.agent.id === goal.agentId);
  return fig === undefined ? null : { floor: goal.floor, pos: spotPos(fig.spot) };
}

function currentPlace(state: SecState, now: number): Place {
  if (state.k === 'briefing') return state.at;
  if (state.k !== 'moving') return HOME;
  const p = routePosAt(state.route, now - state.startAt);
  if (p.floor !== 'car') return { floor: p.floor, pos: p.pos };
  const ride = state.route.legs.find((l) => l.k === 'ride');
  const to: FloorId = ride?.k === 'ride' ? ride.toFloor : state.route.end.floor;
  return { floor: to, pos: ELEVATOR_DOOR[to] ?? state.route.end.pos };
}

// --- lời nói ---

const nameOf = (a: AgentView) => cleanText(a.name, NAME_MAX);
const floorName = (f: FloorId) => FLOORS[f]?.name ?? '';

function roleLabel(c: Candidate, rosters: readonly FloorRoster[]): string {
  const fig = rosters.find((r) => r.floor === c.floor)?.figures.find((f) => f.agent.id === c.agentId);
  const p = fig?.placement;
  if (p === undefined) return 'chung';
  return roleShort(p.floor, p.roleId) ?? (p.zone === 'session' ? 'phiên chính' : p.zone === 'guest' ? 'khách' : 'chung');
}

export const SECRETARY_LINES = {
  goCritical: (n: number, floor: string) => `Có việc vùng khóa ở tầng ${n} · ${floor}. Em xuống ngay.`,
  go: (n: number, floor: string, name: string) => `Em xuống tầng ${n} · ${floor} xem ${name} nhé.`,
  arriveCritical: (name: string, tool: string) => `${name} xin chạy ${tool} trong vùng khóa. Anh mở thẻ xem kỹ nhé.`,
  arriveRequest: (name: string, role: string, tool: string, risk: string, count: number) =>
    `${name} (${role}) xin chạy ${tool} · ${risk}.${count > 1 ? ` Còn ${count - 1} yêu cầu nữa.` : ''}`,
  arriveWaiting: (name: string, role: string) => `${name} (${role}) đang chờ anh duyệt.`,
  arriveHandoff: (name: string) => `Lệnh của ${name} dài quá, anh duyệt trong terminal nhé.`,
  arriveError: (name: string, floor: string, act: string) => (act === '' ? `${name} ở tầng ${floor} gặp lỗi.` : `${name} ở tầng ${floor} gặp lỗi: ${act}.`),
  home: 'Hết việc chờ rồi. Em về Phòng CEO.',
  summaryIdle: 'Cả tòa đang rảnh.',
  summary: (w: number, r: number, e: number, n: number, floor: string) => `Cả tòa: ${w} đang làm, ${r} chờ duyệt, ${e} lỗi. Bận nhất: tầng ${n} · ${floor}.`,
} as const;

function bubble(key: string, text: string, priority: SpeechPriority, now: number, actions: readonly BubbleAction[] = []): Bubble {
  return { key, text, priority, until: now + SAY_MS, actions };
}

const PRIO: Record<SpeechPriority, number> = { low: 0, normal: 1, high: 2 };

/** Đặt bong bóng mới (chống lặp theo khóa; câu `low` không đè câu quan trọng hơn còn hiện). */
function say(mem: SecMemory, b: Bubble, now: number): SecMemory {
  if (mem.said.includes(b.key)) return mem;
  const cur = mem.bubble !== null && mem.bubble.until > now ? mem.bubble : null;
  const said = [...mem.said, b.key].slice(-SAID_MAX);
  if (cur !== null && PRIO[b.priority] < PRIO[cur.priority]) return { ...mem, said };
  return { ...mem, said, bubble: b };
}

/** Người dùng bấm thư ký: tóm tắt cả tòa (câu mới mỗi lần bấm, không qua khóa chống lặp). */
export function summaryBubble(rosters: readonly FloorRoster[], now: number): Bubble {
  let w = 0;
  let r = 0;
  let e = 0;
  let busiest: { floor: AgentFloorId; n: number } | null = null;
  for (const f of rosters) {
    w += f.stats.busy;
    r += f.stats.raised;
    e += f.stats.errors;
    const n = f.stats.busy + f.stats.raised + f.stats.errors;
    if (n > 0 && (busiest === null || n > busiest.n)) busiest = { floor: f.floor, n };
  }
  const key = `summary:${now}`;
  if (busiest === null) return bubble(key, SECRETARY_LINES.summaryIdle, 'normal', now);
  return bubble(key, SECRETARY_LINES.summary(w, r, e, busiest.floor, floorName(busiest.floor)), 'normal', now, [
    { kind: 'view', view: busiest.floor, label: `Xuống tầng ${busiest.floor}` },
  ]);
}

/** Đặt một bong bóng do vỏ dựng (ví dụ `summaryBubble`). */
export function sayBubble(mem: SecMemory, b: Bubble, now: number): SecMemory {
  return say(mem, b, now);
}

// --- máy trạng thái ---

function arriveLine(c: Candidate | undefined, goal: Goal, input: SecInput): { text: string; actions: BubbleAction[] } | null {
  if (c === undefined) return null;
  const name = nameOf(c.agent);
  const actions: BubbleAction[] = input.canSelect(c.agent) ? [{ kind: 'select', agentId: c.agentId, label: 'Xem thẻ duyệt' }] : [];
  if (goal.kind === 'request' && c.pending !== undefined) {
    const tool = cleanText(c.pending.tool, TOOL_MAX);
    return goal.rank === 0
      ? { text: SECRETARY_LINES.arriveCritical(name, tool), actions }
      : { text: SECRETARY_LINES.arriveRequest(name, roleLabel(c, input.rosters), tool, RISK_LABELS[c.pending.risk].toLowerCase(), c.pending.count), actions };
  }
  if (goal.kind === 'handoff') return { text: SECRETARY_LINES.arriveHandoff(name), actions };
  if (goal.kind === 'error') return { text: SECRETARY_LINES.arriveError(name, floorName(goal.floor), cleanText(c.agent.act_text, ACT_MAX)), actions: [] };
  return { text: SECRETARY_LINES.arriveWaiting(name, roleLabel(c, input.rosters)), actions };
}

function startTrip(mem: SecMemory, goal: Goal | null, input: SecInput): SecMemory {
  const { now } = input;
  const from = currentPlace(mem.state, now);
  let dest: Place | null = HOME;
  if (goal !== null) {
    dest = goalPlace(goal, input.rosters);
    if (dest === null) return mem;
  }
  const route = planRoute(from, dest, goal?.rank === 0, input.motion);
  let next: SecMemory = { ...mem, state: { k: 'moving', route, startAt: now, goal }, lastTripAt: goal === null ? mem.lastTripAt : now };
  if (goal !== null) {
    const c = candidates(input.rosters, input.brief, input.canSelect).find((x) => x.key === goal.key);
    const n = goal.floor;
    const text = goal.rank === 0 ? SECRETARY_LINES.goCritical(n, floorName(n)) : SECRETARY_LINES.go(n, floorName(n), c === undefined ? '' : nameOf(c.agent));
    next = say(next, bubble(`go:${goal.key}`, text, goal.rank === 0 ? 'high' : 'normal', now, [{ kind: 'view', view: n, label: `Xuống tầng ${n}` }]), now);
  }
  return next;
}

/** Một bước của máy trạng thái; gọi khi `brief`/rosters đổi và khi tới `wakeAt`. `wakeAt = Infinity` khi không cần timer. */
export function stepSecretary(input0: SecMemory, input: SecInput): { mem: SecMemory; wakeAt: number } {
  const { now, brief } = input;
  let mem = input0;
  if (mem.bubble !== null && mem.bubble.until <= now) mem = { ...mem, bubble: null };

  const connected = brief?.connected ?? true;
  const cands = candidates(input.rosters, brief, input.canSelect);
  const best = pickGoal(input.rosters, brief, input.canSelect);

  // Câu từ `think()` (lỗi, xong việc, yêu cầu mới, mất kết nối…): nguyên văn.
  if (brief?.speech != null) {
    mem = say(mem, bubble(brief.speech.key, brief.speech.text, brief.speech.priority, now), now);
  }

  for (let guard = 0; guard < 6; guard++) {
    const s = mem.state;
    if (!connected && (s.k === 'moving' ? s.goal !== null : s.k === 'briefing')) {
      mem = startTrip(mem, null, input);
      continue;
    }
    if (s.k === 'asleep') {
      const woke = best !== null || (brief?.teamMood ?? 'idle') !== 'sleep' || (brief?.speech != null);
      if (!woke) break;
      mem = { ...mem, state: { k: 'home' } };
      continue;
    }
    if (s.k === 'home') {
      if (best !== null) {
        const before = mem;
        mem = startTrip(mem, best, input);
        if (mem === before) break;
        continue;
      }
      if (brief?.teamMood === 'sleep') mem = { ...mem, state: { k: 'asleep' } };
      break;
    }
    if (s.k === 'moving') {
      if (now >= s.startAt + s.route.totalMs || !input.motion) {
        if (s.goal === null) {
          mem = { ...mem, state: { k: 'home' } };
          if (brief?.focus !== true) mem = say(mem, bubble(`home:${s.startAt}`, SECRETARY_LINES.home, 'low', now), now);
        } else {
          const goal = s.goal;
          const c = cands.find((x) => x.key === goal.key);
          mem = { ...mem, state: { k: 'briefing', goal, since: now, clearedAt: null, at: s.route.end } };
          const line = arriveLine(c, goal, input);
          if (line !== null) mem = say(mem, bubble(`arrive:${goal.key}`, line.text, goal.rank === 0 ? 'high' : 'normal', now, line.actions), now);
        }
        continue;
      }
      if (best !== null && connected && (s.goal === null || (best.rank < s.goal.rank && best.key !== s.goal.key))) {
        const cur = s.goal?.rank ?? Infinity;
        if (s.goal === null || now - mem.lastTripAt >= TRIP_GAP_MS || best.rank + MUCH_LOWER <= cur) {
          mem = startTrip(mem, best, input);
          if (mem.state === s) break;
        }
      }
      break;
    }
    // briefing
    const alive = cands.some((c) => c.key === s.goal.key);
    if (alive) {
      if (s.clearedAt !== null) {
        mem = { ...mem, state: { ...s, clearedAt: null } };
        continue;
      }
      if (best !== null && best.rank < s.goal.rank && best.key !== s.goal.key && (now - mem.lastTripAt >= TRIP_GAP_MS || best.rank + MUCH_LOWER <= s.goal.rank)) {
        mem = startTrip(mem, best, input);
        if (mem.state === s) break;
        continue;
      }
      break;
    }
    if (s.clearedAt === null) {
      // Câu về yêu cầu đã xử lý xong thì hết giá trị: bỏ để câu kế tiếp (ví dụ "về Phòng CEO") hiện được.
      const stale = mem.bubble?.key === `arrive:${s.goal.key}`;
      mem = { ...mem, state: { ...s, clearedAt: now }, bubble: stale ? null : mem.bubble };
      break;
    }
    if (now >= s.clearedAt + BRIEF_RETURN_MS) {
      mem = startTrip(mem, best, input);
      if (mem.state === s) mem = startTrip(mem, null, input);
      continue;
    }
    break;
  }

  return { mem, wakeAt: wakeAtOf(mem, now) };
}

function wakeAtOf(mem: SecMemory, now: number): number {
  let wake = Infinity;
  const s = mem.state;
  if (s.k === 'moving') wake = Math.min(wake, s.startAt + s.route.totalMs);
  if (s.k === 'briefing' && s.clearedAt !== null) wake = Math.min(wake, s.clearedAt + BRIEF_RETURN_MS);
  if (mem.bubble !== null) wake = Math.min(wake, mem.bubble.until);
  return wake <= now ? now + 1 : wake;
}

// --- tư thế để vẽ ---

export function secretaryPose(mem: SecMemory, now: number): { floor: FloorId | 'car'; pos: Vec3; pose: PoseId; facing: number } {
  const s = mem.state;
  if (s.k === 'asleep') return { floor: CEO_FLOOR, pos: SECRETARY_HOME, pose: 'sleep', facing: 0 };
  if (s.k === 'home') return { floor: CEO_FLOOR, pos: SECRETARY_HOME, pose: 'sit_type', facing: 0 };
  if (s.k === 'briefing') return { floor: s.at.floor, pos: s.at.pos, pose: 'stand', facing: Math.PI / 2 };
  const t = now - s.startAt;
  const p = routePosAt(s.route, t);
  if (p.floor === 'car') return { floor: 'car', pos: p.pos, pose: 'stand', facing: Math.PI };
  if (!p.walking) {
    const done = t >= s.route.totalMs;
    const end = s.route.end;
    const atHome = done && s.goal === null && end.floor === CEO_FLOOR;
    return { floor: p.floor, pos: p.pos, pose: atHome ? 'sit_type' : 'stand', facing: atHome ? 0 : Math.PI / 2 };
  }
  const q = routePosAt(s.route, t + 60);
  const dx = q.floor === p.floor ? q.pos.x - p.pos.x : 0;
  const dz = q.floor === p.floor ? q.pos.z - p.pos.z : 0;
  return { floor: p.floor, pos: p.pos, pose: 'walk', facing: dx === 0 && dz === 0 ? 0 : Math.atan2(-dx, -dz) };
}
