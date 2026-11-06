// Xếp agent vào tầng của tòa nhà (docs/OFFICE_BUILDING.md §3). Danh sách trắng cố định: không chuỗi nào từ agent sinh ra tầng,
// khu hay vai trò mới, và KHÔNG có đường nào tới tầng 5 (Phòng CEO). Kết quả chỉ dùng để chọn chỗ vẽ.
import type { AgentView } from '../protocol';

export type FloorId = 0 | 1 | 2 | 3 | 4 | 5;
/** Tầng agent có thể ở: tầng 5 không nằm trong kiểu trả về của `placeAgent`. */
export type AgentFloorId = 0 | 1 | 2 | 3 | 4;
export const CEO_FLOOR = 5 as const;
export const FLOOR_COUNT = 6;
export type FloorKey = 'lobby' | 'dev' | 'test' | 'rnd' | 'media' | 'ceo';

export interface RoleDef {
  id: string;
  short: string;
}
export interface FloorDef {
  id: FloorId;
  key: FloorKey;
  name: string;
  plugin: string | null;
  accent: string;
  blurb: string;
  roles: readonly RoleDef[];
}

function roles(spec: string): RoleDef[] {
  return spec.split(' ').map((entry) => {
    const [id = '', short] = entry.split('=');
    return { id, short: short ?? id };
  });
}

export const FLOORS: readonly FloorDef[] = [
  { id: 0, key: 'lobby', name: 'Sảnh', plugin: null, accent: '#9AA3B5', blurb: 'Phiên chính, agent chung, khách', roles: [] },
  {
    id: 1, key: 'dev', name: 'Dev', plugin: 'dev-squad', accent: '#8B7CF6', blurb: 'Đội phát triển phần mềm',
    roles: roles('orchestrator business-analyst=ba project-manager=pm architect developer qa-engineer=qa code-reviewer=reviewer debugger devops-engineer=devops security-auditor=security performance-engineer=perf database-engineer=database sre-incident-responder=sre dependency-upgrader=upgrader tech-writer'),
  },
  {
    id: 2, key: 'test', name: 'Test', plugin: 'tester-squad', accent: '#34D1B5', blurb: 'Đội kiểm thử',
    roles: roles('test-lead test-analyst=analyst test-designer=designer exploratory-tester=exploratory functional-tester=functional api-tester=api ux-accessibility-tester=ux-a11y nonfunctional-tester=non-func automation-engineer=automation bug-triager=triager test-reporter=reporter'),
  },
  {
    id: 3, key: 'rnd', name: 'R&D', plugin: 'lab', accent: '#FFD27A', blurb: 'Nghiên cứu và thử nghiệm',
    roles: roles('scout reader ideator experimenter analyst skeptic writer'),
  },
  {
    id: 4, key: 'media', name: 'Media', plugin: 'demo-builder', accent: '#FF7A59', blurb: 'Dựng demo và truyền thông',
    roles: roles('demo-director=director demo-strategist=strategist scriptwriter flow-engineer=flow screen-recorder=recorder voice-producer=voice video-editor=editor localizer demo-qa launch-copywriter=copywriter'),
  },
  { id: 5, key: 'ceo', name: 'Phòng CEO', plugin: null, accent: '#FFB020', blurb: 'Thư ký trưởng và bàn CEO', roles: [] },
];

export const PLUGIN_FLOORS: ReadonlyMap<string, AgentFloorId> = new Map<string, AgentFloorId>([
  ['dev-squad', 1],
  ['tester-squad', 2],
  ['lab', 3],
  ['demo-builder', 4],
]);

export const ROLE_MAX = 128;
export const SEGMENT = /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/;
/** Khớp `DEFAULT_SUBAGENT_ROLE` ở crates/hub-core/src/persona.rs. */
export const HUB_DEFAULT_SUBAGENT_ROLE = 'trợ lý';
/** Agent có tên vai trò trần (không plugin) trùng đúng một vai trò thì ngồi tầng của vai trò đó (Q2: bật). */
export const MATCH_BARE_ROLE = true;

export type Zone = 'session' | 'general' | 'guest' | 'role' | 'extra';
export interface Placement {
  floor: AgentFloorId;
  zone: Zone;
  /** Luôn là id trong danh sách trắng (hoặc `null`). */
  roleId: string | null;
}

const BARE_ROLE_FLOORS: ReadonlyMap<string, AgentFloorId> = new Map(
  FLOORS.flatMap((f) => (f.id >= 1 && f.id <= 4 ? f.roles.map((r): [string, AgentFloorId] => [r.id, f.id as AgentFloorId]) : [])),
);

/** Nhãn ngắn của vai trò trong danh sách trắng (hiện trên bàn); `null` nếu không thuộc danh sách. */
export function roleShort(floor: FloorId, roleId: string | null): string | null {
  if (roleId === null) return null;
  return FLOORS[floor]?.roles.find((r) => r.id === roleId)?.short ?? null;
}

/** Tách `plugin:agent` hoặc `agent`; `null` nếu quá dài, nhiều hơn 2 đoạn, hoặc đoạn sai dạng. Không chuẩn hóa, so chính xác. */
export function parseAgentType(raw: string): { plugin: string | null; name: string } | null {
  if (raw.length > ROLE_MAX) return null;
  const parts = raw.split(':');
  if (parts.length > 2 || !parts.every((p) => SEGMENT.test(p))) return null;
  const [first = '', second] = parts;
  return second === undefined ? { plugin: null, name: first } : { plugin: first, name: second };
}

const GUEST: Placement = { floor: 0, zone: 'guest', roleId: null };
const GENERAL: Placement = { floor: 0, zone: 'general', roleId: null };

export function placeAgent(a: Pick<AgentView, 'kind' | 'role'>): Placement {
  if (a.kind === 'session') return { floor: 0, zone: 'session', roleId: null };
  if (a.role === '' || a.role === HUB_DEFAULT_SUBAGENT_ROLE) return GENERAL;
  const parsed = parseAgentType(a.role);
  if (parsed === null) return GUEST;
  if (parsed.plugin !== null) {
    const f = PLUGIN_FLOORS.get(parsed.plugin);
    if (f === undefined) return GUEST;
    const known = FLOORS[f]?.roles.some((r) => r.id === parsed.name) === true;
    return known ? { floor: f, zone: 'role', roleId: parsed.name } : { floor: f, zone: 'extra', roleId: null };
  }
  if (parsed.name === 'general-purpose') return GENERAL;
  if (MATCH_BARE_ROLE) {
    const f = BARE_ROLE_FLOORS.get(parsed.name);
    if (f !== undefined) return { floor: f, zone: 'role', roleId: parsed.name };
  }
  return GENERAL;
}
