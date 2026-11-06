import { describe, expect, it } from 'vitest';
import { FLOORS, FLOOR_COUNT, MATCH_BARE_ROLE, PLUGIN_FLOORS, parseAgentType, placeAgent, roleShort, type Placement } from './floors';
import type { AgentKind } from '../protocol';

const P = (floor: number, zone: Placement['zone'], roleId: string | null = null): Placement => ({ floor: floor as Placement['floor'], zone, roleId });
const place = (kind: AgentKind, role: string) => placeAgent({ kind, role });

describe('placeAgent table (docs §3.3)', () => {
  const rows: [AgentKind, string, Placement][] = [
    ['subagent', 'dev-squad:developer', P(1, 'role', 'developer')],
    ['subagent', 'dev-squad:architect', P(1, 'role', 'architect')],
    ['subagent', 'dev-squad:new-role', P(1, 'extra')],
    ['subagent', 'tester-squad:api-tester', P(2, 'role', 'api-tester')],
    ['subagent', 'lab:analyst', P(3, 'role', 'analyst')],
    ['subagent', 'general-purpose', P(0, 'general')],
    ['subagent', 'Explore', P(0, 'general')],
    ['subagent', 'developer', P(1, 'role', 'developer')],
    ['subagent', 'evil:analyst', P(0, 'guest')],
    ['subagent', 'evil:../x', P(0, 'guest')],
    ['subagent', 'ceo:secretary', P(0, 'guest')],
    ['subagent', 'ceo', P(0, 'general')],
    ['subagent', '5', P(0, 'general')],
    ['subagent', 'DEV-SQUAD:developer', P(0, 'guest')],
    ['subagent', 'dev-squad:developer​', P(0, 'guest')],
    ['subagent', '__proto__:x', P(0, 'guest')],
    ['subagent', 'constructor', P(0, 'general')],
    ['subagent', 'a:b:c', P(0, 'guest')],
    ['subagent', 'x'.repeat(200), P(0, 'guest')],
    ['subagent', '', P(0, 'general')],
    ['subagent', 'trợ lý', P(0, 'general')],
    ['session', 'backend', P(0, 'session')],
    ['teammate', 'lab:scout', P(3, 'role', 'scout')],
  ];
  for (const [kind, role, want] of rows) {
    it(`${kind} ${JSON.stringify(role.slice(0, 30))}`, () => {
      expect(place(kind, role)).toEqual(want);
    });
  }

  it('bare_role_matching_is_on', () => {
    expect(MATCH_BARE_ROLE).toBe(true);
  });
});

describe('never floor 5', () => {
  it('hostile_roles_never_reach_the_ceo_floor', () => {
    const roles = ['__proto__', 'constructor', 'ceo', 'ceo:secretary', 'CEO', '5', 'a:b:c', ':', '\u202etalc:x', 'dev-squad:\u202ex', 'x'.repeat(129), 'dev-squad:' + 'x'.repeat(120), 'Thư ký trưởng', 'toString', 'hasOwnProperty:x'];
    for (const kind of ['session', 'subagent', 'teammate'] as const) {
      for (const role of roles) expect(place(kind, role).floor, `${kind} ${role.slice(0, 12)}`).not.toBe(5);
    }
  });
});

describe('invariants', () => {
  it('floor_table_shape', () => {
    expect(FLOORS.length).toBe(FLOOR_COUNT);
    expect(FLOORS[5]?.plugin).toBeNull();
    expect(FLOORS[5]?.roles).toEqual([]);
    const ids = FLOORS.flatMap((f) => f.roles.map((r) => r.id));
    expect(new Set(ids).size).toBe(ids.length);
    expect(FLOORS.map((f) => f.roles.length)).toEqual([0, 15, 11, 7, 10, 0]);
    for (const [plugin, f] of PLUGIN_FLOORS) expect(FLOORS[f]?.plugin).toBe(plugin);
  });

  it('short_labels_from_the_allow_list', () => {
    expect(roleShort(1, 'qa-engineer')).toBe('qa');
    expect(roleShort(1, 'developer')).toBe('developer');
    expect(roleShort(1, 'nope')).toBeNull();
    expect(roleShort(2, 'ux-accessibility-tester')).toBe('ux-a11y');
  });

  it('parseAgentType_rejects_bad_shapes', () => {
    expect(parseAgentType('a:b')).toEqual({ plugin: 'a', name: 'b' });
    expect(parseAgentType('a')).toEqual({ plugin: null, name: 'a' });
    for (const bad of ['', 'a:', ':a', 'a b', 'a/b', 'a.b', 'a\\b', 'é', 'a:b:c', '-a']) expect(parseAgentType(bad), bad).toBeNull();
  });

  it('fuzz_never_leaves_floors_0_to_4_or_the_allow_list', () => {
    const alphabet = ['a', 'b', 'Z', '9', ':', ':', '/', '.', '-', '_', ' ', '​', '‮', 'é', '\u0000', 'dev-squad', 'lab', 'developer', 'ceo', '__proto__'];
    let seed = 12345;
    const rnd = () => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
    const allowed = new Set(FLOORS.flatMap((f) => f.roles.map((r) => r.id)));
    for (let i = 0; i < 1000; i++) {
      const n = Math.floor(rnd() * (i % 50 === 0 ? 300 : 8));
      let s = '';
      for (let j = 0; j < n; j++) s += alphabet[Math.floor(rnd() * alphabet.length)];
      for (const kind of ['subagent', 'teammate'] as const) {
        const r = place(kind, s);
        expect([0, 1, 2, 3, 4]).toContain(r.floor);
        if (r.roleId !== null) expect(allowed.has(r.roleId)).toBe(true);
      }
    }
  });
});
