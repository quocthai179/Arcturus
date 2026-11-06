// Xếp chỗ và tư thế (docs/OFFICE_BUILDING.md §5). Thuần, ổn định: agent đã có chỗ hợp lệ thì giữ chỗ (giống live/sim.ts), bàn
// không đổi khi mood đổi. Không có timer: tư thế ngủ được tính lại ở lần dựng kế tiếp.
import type { AgentId, AgentView } from '../protocol';
import type { SceneBrief } from '../sceneBrief';
import type { PoseId } from './figure';
import { FLOORS, placeAgent, type AgentFloorId, type Placement } from './floors';
import { deskCount, GUEST_SOFA_SPOTS, HOT_DESKS, LOBBY_SESSION_DESKS, LOUNGE_SOFA_SPOTS, LOUNGE_SPOT_COUNT, STANDING_SPOT_COUNT } from './layout';

export type SpotKind = 'desk' | 'lounge' | 'standing';
export interface Spot {
  floor: AgentFloorId;
  kind: SpotKind;
  index: number;
}
export interface SeatMap {
  readonly byAgent: ReadonlyMap<AgentId, Spot>;
  /** Thứ tự xuất hiện. */
  readonly order: readonly AgentId[];
}
export const MAX_FIGURES_PER_FLOOR = 24;
/** = `SLEEP_AFTER_MS` của brain. */
export const IDLE_SLEEP_MS = 10 * 60_000;
export const EMPTY_SEATS: SeatMap = { byAgent: new Map(), order: [] };

export interface Figure {
  agent: AgentView;
  placement: Placement;
  spot: Spot;
  pose: PoseId;
}
export interface FloorRoster {
  floor: AgentFloorId;
  figures: readonly Figure[];
  /** Agent không có chỗ vẽ ("+N"). */
  overflow: number;
  stats: { present: number; busy: number; raised: number; errors: number };
}

const AGENT_FLOORS: readonly AgentFloorId[] = [0, 1, 2, 3, 4];

function range(from: number, count: number): number[] {
  return Array.from({ length: count }, (_, i) => from + i);
}

/** Chỉ số bàn mà vùng của `placement` được phép dùng (theo thứ tự ưu tiên). */
function deskChoices(p: Placement): number[] {
  const f = p.floor;
  if (f === 0) {
    if (p.zone === 'session') return range(0, LOBBY_SESSION_DESKS);
    if (p.zone === 'general') return range(LOBBY_SESSION_DESKS, deskCount(0) - LOBBY_SESSION_DESKS);
    return [];
  }
  const roles = FLOORS[f]?.roles ?? [];
  const hot = range(roles.length, HOT_DESKS);
  if (p.zone === 'role') {
    const own = roles.findIndex((r) => r.id === p.roleId);
    return own >= 0 ? [own, ...hot] : hot;
  }
  return hot;
}

function key(s: Spot): string {
  return `${s.floor}:${s.kind}:${s.index}`;
}

/** Spot cũ còn hợp lệ cho `placement` không (cùng tầng, đúng vùng, trong giới hạn). */
function stillValid(s: Spot, p: Placement): boolean {
  if (s.floor !== p.floor) return false;
  if (s.kind === 'standing') return s.index >= 0 && s.index < STANDING_SPOT_COUNT;
  if (s.kind === 'lounge') return p.zone === 'guest' && s.index >= 0 && s.index < GUEST_SOFA_SPOTS;
  return deskChoices(p).includes(s.index);
}

export function assignSeats(prev: SeatMap, agents: readonly AgentView[]): SeatMap {
  const sorted = [...agents].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  const present = new Set(sorted.map((a) => a.id));
  const used = new Set<string>();
  const byAgent = new Map<AgentId, Spot>();
  const fresh: AgentView[] = [];
  for (const a of sorted) {
    const old = prev.byAgent.get(a.id);
    if (old !== undefined && stillValid(old, placeAgent(a)) && !used.has(key(old))) {
      byAgent.set(a.id, old);
      used.add(key(old));
    } else {
      fresh.push(a); // mới, chỗ cũ không còn hợp lệ, hoặc từng tràn (thử lại)
    }
  }
  for (const a of fresh) {
    const p = placeAgent(a);
    const candidates: Spot[] = [
      ...deskChoices(p).map((index): Spot => ({ floor: p.floor, kind: 'desk', index })),
      ...(p.zone === 'guest' ? range(0, GUEST_SOFA_SPOTS).map((index): Spot => ({ floor: p.floor, kind: 'lounge', index })) : []),
      ...range(0, STANDING_SPOT_COUNT).map((index): Spot => ({ floor: p.floor, kind: 'standing', index })),
    ];
    const spot = candidates.find((c) => !used.has(key(c)));
    if (spot !== undefined) {
      byAgent.set(a.id, spot);
      used.add(key(spot));
    }
  }
  const order = [...prev.order.filter((id) => present.has(id)), ...sorted.map((a) => a.id).filter((id) => !prev.order.includes(id))];
  return { byAgent, order };
}

/** Tư thế của agent tại `spot` (`now` chỉ để tính ngủ; không timer). */
export function poseFor(a: AgentView, spot: Spot, now: number): PoseId {
  const asleep = now - Date.parse(a.last_event_at) >= IDLE_SLEEP_MS;
  const sofa = spot.kind === 'lounge' && spot.index < LOUNGE_SOFA_SPOTS;
  if (spot.kind === 'desk') {
    switch (a.mood) {
      case 'working':
        if (a.activity === 'typing' || a.activity === 'running') return 'sit_type';
        return a.activity === 'reading' ? 'sit_read' : 'sit_think';
      case 'approval': return 'raise';
      case 'error': return 'sit_error';
      case 'done': return 'cheer';
      case 'idle': return asleep ? 'sleep' : 'sit_think';
    }
  }
  switch (a.mood) {
    case 'approval': return 'raise';
    case 'done': return 'cheer';
    case 'idle': return asleep ? 'sleep' : sofa ? 'lounge_sit' : 'stand';
    default: return 'stand';
  }
}

export function rosters(seats: SeatMap, agents: readonly AgentView[], brief: SceneBrief | undefined, now: number): FloorRoster[] {
  const byId = new Map(agents.map((a) => [a.id, a] as const));
  return AGENT_FLOORS.map((floor) => {
    const here = seats.order.map((id) => byId.get(id)).filter((a): a is AgentView => a !== undefined && placeAgent(a).floor === floor);
    // Chỗ nghỉ còn trống (khách đang chiếm sofa của họ).
    const taken = new Set<number>();
    for (const a of here) {
      const s = seats.byAgent.get(a.id);
      if (s?.kind === 'lounge' && s.floor === floor) taken.add(s.index);
    }
    const figures: Figure[] = [];
    let overflow = 0;
    for (const agent of here) {
      const home = seats.byAgent.get(agent.id);
      if (home === undefined || home.floor !== floor) {
        overflow++;
        continue;
      }
      let spot = home;
      if (home.kind === 'desk' && agent.mood === 'idle') {
        const free = range(0, LOUNGE_SPOT_COUNT).find((i) => !taken.has(i));
        if (free !== undefined) {
          taken.add(free);
          spot = { floor, kind: 'lounge', index: free };
        }
      }
      figures.push({ agent, placement: placeAgent(agent), spot, pose: poseFor(agent, spot, now) });
    }
    const shown = figures.slice(0, MAX_FIGURES_PER_FLOOR);
    overflow += figures.length - shown.length;
    let busy = 0;
    let raised = 0;
    let errors = 0;
    for (const f of shown) {
      if (f.agent.mood === 'working') busy++;
      if (f.agent.mood === 'approval' || brief?.pending.has(f.agent.id) === true) raised++;
      if (f.agent.mood === 'error') errors++;
    }
    return { floor, figures: shown, overflow, stats: { present: shown.length, busy, raised, errors } };
  });
}
