import { describe, expect, it, vi } from 'vitest';
import { Batch, makeMaterials, Registry } from './kit';

describe('Registry', () => {
  it('disposeAll_disposes_every_item_once_and_empties_the_set', () => {
    const reg = new Registry();
    const a = { dispose: vi.fn() };
    const b = { dispose: vi.fn() };
    reg.add(a);
    reg.add(b);
    reg.add(a);
    expect(reg.size).toBe(2);
    reg.disposeAll();
    expect(a.dispose).toHaveBeenCalledTimes(1);
    expect(b.dispose).toHaveBeenCalledTimes(1);
    expect(reg.size).toBe(0);
    reg.disposeAll();
    expect(a.dispose).toHaveBeenCalledTimes(1);
  });

  it('batch_merges_into_one_mesh_per_kind_and_registers_geometry', () => {
    const reg = new Registry();
    const mats = makeMaterials(reg);
    const before = reg.size;
    const group = new Batch().box(1, 1, 1, 0, 0, 0, '#ffffff').box(1, 1, 1, 2, 0, 0, '#000000').box(1, 1, 1, 0, 2, 0, '#ff0000', { kind: 'glow' }).build(reg, mats);
    expect(group.children).toHaveLength(2);
    expect(reg.size).toBe(before + 2);
    reg.disposeAll();
  });
});
