// Mô phỏng văn phòng sống (docs/OFFICE_LIVE.md §3–4). Tất định với `rng` cho trước, không đụng DOM hay timer: nơi gọi
// đưa `now` vào, đọc `nextWakeAt` để biết lúc nào cần gọi `advance` lần sau. Không còn mốc nào = `Infinity` = không timer.
//
// Di chuyển là từng đoạn thẳng: nhân vật "đang ở" (x, y) là điểm cuối đoạn hiện tại; bộ vẽ nội suy bằng CSS transition
// dài `legMs`. JS chỉ chạy ở mốc hết đoạn, đổi câu thoại hoặc hết thời gian đứng.
import type { Mood } from '../protocol';
import {
  BACK_TO_IDLE,
  CALLED_TO_WORK,
  CHATS,
  DONE_CHEER,
  ERROR_SIGH,
  MUSINGS,
  PARTNER_LEFT,
  WAKE_UP,
  pick,
} from './chatter';
import { distance, findPath, type Grid, type Point } from './geometry';
import { buildGrid, CHAT_POIS, DOOR, NAP_SPOTS, POIS, SEATS, STANDING_SPOTS, type PoiId } from './layout';

/** Tốc độ đi dạo và chạy về bàn (px logic mỗi giây). */
export const WALK_SPEED = 72;
export const HURRY_SPEED = 150;
/** Giờ giải lao tối đa của một agent rảnh trước khi đi ngủ gật (ngân sách: rảnh lâu thì 0 timer). */
export const WANDER_MS = 5 * 60_000;
/** Số nhân vật tối đa trên sàn (ngân sách DOM); phần còn lại hiện thành "+N". */
export const MAX_ON_FLOOR = 16;
export const LINE_MS = 2400;
export const CHAT_WAIT_MS = 15_000;
export const CELEBRATE_MS = 3500;
const BUBBLE_MS = 2200;

export type Pose = 'stand' | 'walk' | 'sit' | 'sleep' | 'raise';
export type BubbleKind = 'say' | 'think' | 'alert';

export interface Bubble {
  text: string;
  kind: BubbleKind;
}

export type Task =
  | { k: 'desk' }
  | { k: 'stroll'; poi: PoiId; until: number | null }
  | { k: 'chat'; conv: number }
  | { k: 'nap' }
  | { k: 'pause'; until: number };

export interface Actor {
  id: string;
  mood: Mood;
  small: boolean;
  /** Điểm cuối của đoạn đang đi (hoặc chỗ đang đứng). */
  x: number;
  y: number;
  fromX: number;
  fromY: number;
  legStart: number;
  legEnd: number;
  /** Thời lượng transition của đoạn hiện tại; 0 = đặt thẳng, không nội suy. */
  legMs: number;
  path: Point[];
  speed: number;
  facing: 1 | -1;
  pose: Pose;
  task: Task | null;
  bubble: Bubble | null;
  bubbleUntil: number | null;
  /** Bắt đầu giờ giải lao (lúc rảnh hoặc lúc được đánh thức). */
  idleSince: number;
  seat: number | null;
  napSpot: number | null;
}

interface Conv {
  id: number;
  a: string;
  b: string;
  poi: number;
  script: readonly string[];
  line: number;
  arrived: Set<string>;
  /** Mốc câu thoại tiếp theo; `null` khi còn chờ đủ hai người. */
  nextAt: number | null;
  waitUntil: number;
}

export interface Sim {
  grid: Grid;
  rng: () => number;
  wanderMs: number;
  motion: boolean;
  started: boolean;
  actors: Map<string, Actor>;
  convs: Map<number, Conv>;
  seats: (string | null)[];
  nextConv: number;
  /** Số agent không có chỗ trên sàn. */
  overflow: number;
}

export interface SimAgent {
  id: string;
  mood: Mood;
  small: boolean;
}

export interface SimOptions {
  rng?: () => number;
  wanderMs?: number;
  motion?: boolean;
}

/** Bộ sinh số giả ngẫu nhiên có hạt (mulberry32). */
export function seeded(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function createSim(opts: SimOptions = {}): Sim {
  return {
    grid: buildGrid(),
    rng: opts.rng ?? Math.random,
    wanderMs: opts.wanderMs ?? WANDER_MS,
    motion: opts.motion ?? true,
    started: false,
    actors: new Map(),
    convs: new Map(),
    seats: SEATS.map(() => null),
    nextConv: 1,
    overflow: 0,
  };
}

/** Mood cần ngồi ở bàn. */
export function busy(mood: Mood): boolean {
  return mood === 'working' || mood === 'approval' || mood === 'error';
}

// ---------- đồng bộ với dữ liệu hub ----------

/** Thêm/bớt/cập nhật nhân vật theo danh sách agent (đã ưu tiên). Trả `true` khi có gì thay đổi cần vẽ lại. */
export function syncAgents(sim: Sim, agents: readonly SimAgent[], now: number): boolean {
  let changed = false;
  const present = new Set(agents.map((a) => a.id));
  for (const actor of [...sim.actors.values()]) {
    if (!present.has(actor.id)) {
      removeActor(sim, actor, now);
      changed = true;
    }
  }
  const first = !sim.started;
  sim.started = true;
  for (const a of agents) {
    const actor = sim.actors.get(a.id);
    if (actor === undefined) continue;
    if (actor.small !== a.small) {
      actor.small = a.small;
      changed = true;
    }
    if (actor.mood !== a.mood) {
      setMood(sim, actor, a.mood, now);
      changed = true;
    }
  }
  let overflow = 0;
  for (const a of agents) {
    if (sim.actors.has(a.id)) continue;
    if (sim.actors.size >= MAX_ON_FLOOR) {
      overflow++;
      continue;
    }
    addActor(sim, a, now, first);
    changed = true;
  }
  if (overflow !== sim.overflow) {
    sim.overflow = overflow;
    changed = true;
  }
  return changed;
}

function addActor(sim: Sim, a: SimAgent, now: number, initial: boolean): void {
  const free = sim.seats.indexOf(null);
  const seat = free >= 0 ? free : null;
  if (seat !== null) sim.seats[seat] = a.id;
  const actor: Actor = {
    id: a.id,
    mood: a.mood,
    small: a.small,
    x: DOOR.x,
    y: DOOR.y,
    fromX: DOOR.x,
    fromY: DOOR.y,
    legStart: now,
    legEnd: now,
    legMs: 0,
    path: [],
    speed: WALK_SPEED,
    facing: 1,
    pose: 'stand',
    task: null,
    bubble: null,
    bubbleUntil: null,
    idleSince: now,
    seat,
    napSpot: null,
  };
  sim.actors.set(a.id, actor);
  if (!sim.motion) {
    snap(sim, actor, now);
  } else if (initial) {
    // Mở văn phòng lần đầu: mọi người đã ở sẵn chỗ, không diễu hành từ cửa vào.
    if (busy(actor.mood)) {
      place(actor, deskTarget(sim, actor), now);
      actor.task = { k: 'desk' };
      arrive(sim, actor, now);
    } else {
      const poi = freeStrollPoi(sim, actor);
      const spot = POIS[poi]?.spots[0] ?? DOOR;
      place(actor, spot, now);
      actor.task = { k: 'stroll', poi: POIS[poi]?.id ?? 'plant', until: now + 800 + sim.rng() * 6000 };
      actor.facing = sim.rng() < 0.5 ? 1 : -1;
    }
  } else {
    actor.bubble = { text: 'Chào cả nhà!', kind: 'say' };
    actor.bubbleUntil = now + BUBBLE_MS;
    if (busy(actor.mood)) goDesk(sim, actor, now, true);
    else actor.task = null;
  }
}

function removeActor(sim: Sim, actor: Actor, now: number): void {
  leaveConv(sim, actor, now);
  if (actor.seat !== null) sim.seats[actor.seat] = null;
  sim.actors.delete(actor.id);
}

function setMood(sim: Sim, actor: Actor, mood: Mood, now: number): void {
  const was = actor.mood;
  actor.mood = mood;
  if (!sim.motion) {
    snap(sim, actor, now);
    return;
  }
  if (busy(mood)) {
    if (actor.task?.k === 'desk') {
      if (actor.pose !== 'walk') arrive(sim, actor, now);
      if (mood === 'error') say(actor, pick(ERROR_SIGH, sim.rng), now, BUBBLE_MS);
      return;
    }
    leaveConv(sim, actor, now);
    actor.napSpot = null;
    say(actor, pick(CALLED_TO_WORK, sim.rng), now, 1800);
    goDesk(sim, actor, now, true);
    return;
  }
  if (!busy(was)) return; // idle ↔ done: không đổi hành vi
  actor.idleSince = now;
  const atDesk = actor.task?.k === 'desk' && actor.pose !== 'walk';
  if (mood === 'done' && atDesk) {
    actor.task = { k: 'pause', until: now + CELEBRATE_MS };
    actor.pose = 'stand';
    say(actor, pick(DONE_CHEER, sim.rng), now, CELEBRATE_MS);
  } else {
    actor.task = { k: 'pause', until: now + 1200 };
    if (actor.pose !== 'walk') actor.pose = 'stand';
    if (sim.rng() < 0.6) say(actor, pick(BACK_TO_IDLE, sim.rng), now, BUBBLE_MS);
    else clearBubble(actor);
  }
}

/** Người dùng bấm "Giải lao tiếp": mọi agent rảnh được thêm một giờ giải lao, ai đang ngủ thì dậy. */
export function wakeAll(sim: Sim, now: number): boolean {
  if (!sim.motion) return false;
  let changed = false;
  for (const actor of sim.actors.values()) {
    if (busy(actor.mood)) continue;
    actor.idleSince = now;
    if (actor.task?.k === 'nap') {
      actor.napSpot = null;
      actor.pose = 'stand';
      actor.task = { k: 'pause', until: now + 300 + sim.rng() * 1500 };
      if (sim.rng() < 0.6) say(actor, pick(WAKE_UP, sim.rng), now, BUBBLE_MS);
      changed = true;
    }
  }
  return changed;
}

/** Bật/tắt chuyển động (trang ẩn, giảm chuyển động). Tắt: mọi người đứng ngay ở đích, không còn mốc nào. */
export function setMotion(sim: Sim, motion: boolean, now: number): boolean {
  if (sim.motion === motion) return false;
  sim.motion = motion;
  if (!motion) {
    sim.convs.clear();
    for (const actor of sim.actors.values()) snap(sim, actor, now);
  } else {
    for (const actor of sim.actors.values()) {
      if (!busy(actor.mood) && now - actor.idleSince < sim.wanderMs) {
        actor.task = { k: 'pause', until: now + sim.rng() * 2000 };
        actor.napSpot = null;
        actor.pose = 'stand';
      }
    }
  }
  return true;
}

// ---------- vòng thời gian ----------

/** Xử lý mọi mốc đã tới hạn ở thời điểm `now`. */
export function advance(sim: Sim, now: number): void {
  if (!sim.motion) return;
  for (let guard = 0; guard < 500; guard++) {
    let did = false;
    for (const conv of [...sim.convs.values()]) {
      if (conv.nextAt !== null && conv.nextAt <= now) {
        speakNext(sim, conv, now);
        did = true;
      } else if (conv.nextAt === null && conv.waitUntil <= now) {
        endConv(sim, conv, now, null);
        did = true;
      }
    }
    for (const actor of sim.actors.values()) {
      if (actor.bubbleUntil !== null && actor.bubbleUntil <= now) {
        clearBubble(actor);
        did = true;
      }
      if (actor.pose === 'walk' && actor.legEnd <= now) {
        if (actor.path.length > 0) startLeg(actor, now);
        else arrive(sim, actor, now);
        did = true;
      } else if (actor.pose !== 'walk' && actor.task !== null && 'until' in actor.task && actor.task.until !== null && actor.task.until <= now) {
        actor.task = null;
        did = true;
      }
      if (actor.task === null) {
        decide(sim, actor, now);
        did = true;
      }
    }
    if (!did) return;
  }
}

/** Mốc gần nhất cần gọi `advance`; `Infinity` khi không còn gì chuyển động (không đặt timer). */
export function nextWakeAt(sim: Sim): number {
  if (!sim.motion) return Infinity;
  let t = Infinity;
  for (const conv of sim.convs.values()) {
    if (conv.nextAt !== null) t = Math.min(t, conv.nextAt);
    else if (Number.isFinite(conv.waitUntil)) t = Math.min(t, conv.waitUntil);
  }
  for (const actor of sim.actors.values()) {
    if (actor.task === null) return -Infinity;
    if (actor.pose === 'walk') t = Math.min(t, actor.legEnd);
    if (actor.bubbleUntil !== null) t = Math.min(t, actor.bubbleUntil);
    // Đang đi thì hạn của việc chỉ được xét khi tới nơi (mốc hết đoạn đã tính ở trên).
    if (actor.pose !== 'walk' && 'until' in actor.task && actor.task.until !== null) t = Math.min(t, actor.task.until);
  }
  return t;
}

// ---------- quyết định ----------

function decide(sim: Sim, actor: Actor, now: number): void {
  if (busy(actor.mood)) {
    goDesk(sim, actor, now, false);
    return;
  }
  if (now - actor.idleSince >= sim.wanderMs) {
    goNap(sim, actor, now, false);
    return;
  }
  if (sim.rng() < 0.55) {
    const partner = findPartner(sim, actor, now);
    const poi = freeChatPoi(sim);
    if (partner !== null && poi !== null) {
      startConv(sim, actor, partner, poi, now);
      return;
    }
  }
  const poi = freeStrollPoi(sim, actor);
  const target = POIS[poi];
  if (target === undefined) {
    actor.task = { k: 'pause', until: now + 3000 };
    return;
  }
  const spot = target.spots[target.spots.length === 2 && sim.rng() < 0.5 ? 1 : 0] ?? target.spots[0] ?? DOOR;
  actor.task = { k: 'stroll', poi: target.id, until: null };
  goTo(sim, actor, spot, now, WALK_SPEED);
}

function wandering(sim: Sim, a: Actor, now: number): boolean {
  return !busy(a.mood) && (a.task === null || a.task.k === 'stroll' || a.task.k === 'pause') &&
    now - a.idleSince < sim.wanderMs - 20_000;
}

function findPartner(sim: Sim, actor: Actor, now: number): Actor | null {
  const candidates = [...sim.actors.values()].filter((a) => a.id !== actor.id && wandering(sim, a, now));
  if (candidates.length === 0) return null;
  return pick(candidates, sim.rng);
}

function freeChatPoi(sim: Sim): number | null {
  const used = new Set([...sim.convs.values()].map((c) => c.poi));
  const free = CHAT_POIS.map((_, i) => i).filter((i) => !used.has(i));
  return free.length === 0 ? null : pick(free, sim.rng);
}

function freeStrollPoi(sim: Sim, actor: Actor): number {
  const used = new Set<PoiId>();
  for (const a of sim.actors.values()) if (a.id !== actor.id && a.task?.k === 'stroll') used.add(a.task.poi);
  const chatUsed = new Set([...sim.convs.values()].map((c) => CHAT_POIS[c.poi]?.id));
  const current = actor.task?.k === 'stroll' ? actor.task.poi : null;
  const free = POIS.map((_, i) => i).filter((i) => {
    const id = POIS[i]?.id;
    return id !== undefined && !used.has(id) && !chatUsed.has(id) && id !== current;
  });
  return free.length === 0 ? Math.floor(sim.rng() * POIS.length) : pick(free, sim.rng);
}

// ---------- tán gẫu ----------

function startConv(sim: Sim, a: Actor, b: Actor, poi: number, now: number): void {
  const place = CHAT_POIS[poi];
  if (place === undefined) return;
  const conv: Conv = {
    id: sim.nextConv++,
    a: a.id,
    b: b.id,
    poi,
    script: pick(CHATS, sim.rng),
    line: 0,
    arrived: new Set(),
    nextAt: null,
    waitUntil: Infinity,
  };
  sim.convs.set(conv.id, conv);
  // Ai gần chỗ nào hơn thì đứng chỗ đó.
  const [s0, s1] = [place.spots[0] ?? DOOR, place.spots[1] ?? DOOR];
  const pa = posAt(a, now);
  const swap = distance(pa, s1) + distance(posAt(b, now), s0) < distance(pa, s0) + distance(posAt(b, now), s1);
  for (const [actor, spot] of [[a, swap ? s1 : s0], [b, swap ? s0 : s1]] as const) {
    actor.task = { k: 'chat', conv: conv.id };
    clearBubble(actor);
    goTo(sim, actor, spot, now, WALK_SPEED);
  }
  say(a, `Ê, ra ${place.label} tí không?`, now, 1800);
}

function speakNext(sim: Sim, conv: Conv, now: number): void {
  const a = sim.actors.get(conv.a);
  const b = sim.actors.get(conv.b);
  if (a === undefined || b === undefined || conv.line >= conv.script.length) {
    endConv(sim, conv, now, null);
    return;
  }
  const text = conv.script[conv.line] ?? '';
  const [speaker, listener] = conv.line % 2 === 0 ? [a, b] : [b, a];
  speaker.bubble = { text, kind: 'say' };
  speaker.bubbleUntil = null;
  clearBubble(listener);
  conv.line++;
  conv.nextAt = now + LINE_MS + Math.min(2000, text.length * 25);
}

function endConv(sim: Sim, conv: Conv, now: number, leaver: string | null): void {
  sim.convs.delete(conv.id);
  for (const id of [conv.a, conv.b]) {
    if (id === leaver) continue;
    const actor = sim.actors.get(id);
    if (actor === undefined || actor.task?.k !== 'chat' || actor.task.conv !== conv.id) continue;
    actor.task = { k: 'pause', until: now + 600 + sim.rng() * 1600 };
    clearBubble(actor);
    if (leaver !== null) say(actor, pick(PARTNER_LEFT, sim.rng), now, BUBBLE_MS);
  }
}

function leaveConv(sim: Sim, actor: Actor, now: number): void {
  if (actor.task?.k !== 'chat') return;
  const conv = sim.convs.get(actor.task.conv);
  if (conv !== undefined) endConv(sim, conv, now, actor.id);
  actor.task = null;
}

// ---------- đích đến ----------

function deskTarget(sim: Sim, actor: Actor): Point {
  if (actor.seat !== null) return SEATS[actor.seat]?.at ?? DOOR;
  const seatless = [...sim.actors.values()].filter((a) => a.seat === null).map((a) => a.id);
  const i = Math.max(0, seatless.indexOf(actor.id));
  return STANDING_SPOTS[i % STANDING_SPOTS.length] ?? DOOR;
}

function goDesk(sim: Sim, actor: Actor, now: number, hurry: boolean): void {
  if (actor.seat === null) {
    const free = sim.seats.indexOf(null);
    if (free >= 0) {
      sim.seats[free] = actor.id;
      actor.seat = free;
    }
  }
  actor.task = { k: 'desk' };
  actor.napSpot = null;
  goTo(sim, actor, deskTarget(sim, actor), now, hurry ? HURRY_SPEED : WALK_SPEED);
}

function napTarget(sim: Sim, actor: Actor): Point | null {
  if (actor.napSpot === null) {
    const taken = new Set([...sim.actors.values()].map((a) => a.napSpot).filter((s) => s !== null));
    const free = NAP_SPOTS.map((_, i) => i).filter((i) => !taken.has(i));
    if (free.length > 0) actor.napSpot = pick(free, sim.rng);
  }
  if (actor.napSpot !== null) return NAP_SPOTS[actor.napSpot] ?? null;
  return actor.seat !== null ? (SEATS[actor.seat]?.at ?? null) : null;
}

function goNap(sim: Sim, actor: Actor, now: number, instant: boolean): void {
  actor.task = { k: 'nap' };
  const target = napTarget(sim, actor);
  if (target === null) {
    // Không còn chỗ nào: đứng ngủ gật tại chỗ.
    place(actor, posAt(actor, now), now);
    arrive(sim, actor, now);
    return;
  }
  if (instant) {
    place(actor, target, now);
    arrive(sim, actor, now);
  } else {
    goTo(sim, actor, target, now, WALK_SPEED);
  }
}

/** Đặt thẳng vào đích (không chuyển động): dùng khi tắt chuyển động. */
function snap(sim: Sim, actor: Actor, now: number): void {
  if (actor.bubble?.kind !== 'alert') clearBubble(actor);
  if (busy(actor.mood)) {
    actor.task = { k: 'desk' };
    actor.napSpot = null;
    place(actor, deskTarget(sim, actor), now);
    arrive(sim, actor, now);
  } else {
    goNap(sim, actor, now, true);
  }
}

// ---------- di chuyển ----------

export function posAt(actor: Actor, now: number): Point {
  if (actor.legMs <= 0 || now >= actor.legEnd) return { x: actor.x, y: actor.y };
  const t = Math.max(0, (now - actor.legStart) / actor.legMs);
  return { x: actor.fromX + (actor.x - actor.fromX) * t, y: actor.fromY + (actor.y - actor.fromY) * t };
}

function place(actor: Actor, p: Point, now: number): void {
  actor.x = p.x;
  actor.y = p.y;
  actor.fromX = p.x;
  actor.fromY = p.y;
  actor.legStart = now;
  actor.legEnd = now;
  actor.legMs = 0;
  actor.path = [];
}

function goTo(sim: Sim, actor: Actor, target: Point, now: number, speed: number): void {
  const from = posAt(actor, now);
  actor.speed = speed;
  if (distance(from, target) < 1) {
    place(actor, target, now);
    arrive(sim, actor, now);
    return;
  }
  const path = findPath(sim.grid, from, target);
  if (path === null) {
    place(actor, target, now);
    arrive(sim, actor, now);
    return;
  }
  actor.path = path;
  startLeg(actor, now);
}

function startLeg(actor: Actor, now: number): void {
  const from = posAt(actor, now);
  const next = actor.path.shift();
  if (next === undefined) return;
  const ms = Math.round((distance(from, next) / actor.speed) * 1000);
  actor.fromX = from.x;
  actor.fromY = from.y;
  actor.x = next.x;
  actor.y = next.y;
  actor.legStart = now;
  actor.legMs = ms;
  actor.legEnd = now + ms;
  if (next.x < from.x - 0.5) actor.facing = -1;
  else if (next.x > from.x + 0.5) actor.facing = 1;
  actor.pose = 'walk';
}

function arrive(sim: Sim, actor: Actor, now: number): void {
  actor.path = [];
  const task = actor.task;
  switch (task?.k) {
    case 'desk':
      actor.pose = actor.mood === 'approval' ? 'raise' : 'sit';
      actor.facing = 1;
      if (actor.mood === 'approval') {
        actor.bubble = { text: '!', kind: 'alert' };
        actor.bubbleUntil = null;
      } else if (actor.bubble?.kind === 'alert') {
        clearBubble(actor);
      }
      return;
    case 'stroll': {
      actor.pose = 'stand';
      task.until = now + 4000 + sim.rng() * 5000;
      const lines = MUSINGS[task.poi];
      if (lines !== undefined && sim.rng() < 0.45) {
        actor.bubble = { text: pick(lines, sim.rng), kind: 'think' };
        actor.bubbleUntil = now + 2800;
      }
      return;
    }
    case 'chat': {
      actor.pose = 'stand';
      const conv = sim.convs.get(task.conv);
      if (conv === undefined) {
        actor.task = null;
        return;
      }
      conv.arrived.add(actor.id);
      if (conv.arrived.size === 1) {
        conv.waitUntil = now + CHAT_WAIT_MS;
        return;
      }
      const a = sim.actors.get(conv.a);
      const b = sim.actors.get(conv.b);
      if (a !== undefined && b !== undefined) {
        a.facing = b.x >= a.x ? 1 : -1;
        b.facing = a.x >= b.x ? 1 : -1;
      }
      conv.nextAt = now + 400;
      return;
    }
    case 'nap': {
      const lounging = actor.napSpot !== null || (actor.seat !== null && distance(actor, SEATS[actor.seat]?.at ?? DOOR) < 1);
      actor.pose = lounging ? 'sleep' : 'stand';
      clearBubble(actor);
      return;
    }
    case 'pause':
    case undefined:
      actor.pose = 'stand';
      return;
  }
}

function say(actor: Actor, text: string, now: number, ms: number): void {
  actor.bubble = { text, kind: 'say' };
  actor.bubbleUntil = now + ms;
}

function clearBubble(actor: Actor): void {
  actor.bubble = null;
  actor.bubbleUntil = null;
}

// ---------- cho bộ vẽ ----------

/** Người đang tán gẫu cùng `actor` (nếu có). */
export function partnerOf(sim: Sim, actor: Actor): string | null {
  if (actor.task?.k !== 'chat') return null;
  const conv = sim.convs.get(actor.task.conv);
  if (conv === undefined) return null;
  return conv.a === actor.id ? conv.b : conv.a;
}

/** Nhãn ngắn mô tả nhân vật đang làm gì trên sàn (cho trình đọc màn hình và chú thích). */
export function doing(sim: Sim, actor: Actor, nameOf: (id: string) => string): string {
  const task = actor.task;
  switch (task?.k) {
    case 'desk':
      return actor.pose === 'walk' ? 'đang về bàn' : actor.seat === null ? 'làm việc đứng' : 'ở bàn làm việc';
    case 'stroll': {
      const label = POIS.find((p) => p.id === task.poi)?.label ?? '';
      return actor.pose === 'walk' ? `đang đi tới ${label}` : `đi dạo ở ${label}`;
    }
    case 'chat': {
      const partner = partnerOf(sim, actor);
      return partner === null ? 'đi dạo' : `tán gẫu với ${nameOf(partner)}`;
    }
    case 'nap':
      return actor.pose === 'walk' ? 'đi tìm chỗ nghỉ' : 'ngủ gật';
    case 'pause':
    case undefined:
      return 'đứng nghỉ';
  }
}
