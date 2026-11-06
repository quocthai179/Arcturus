import { describe, expect, it } from 'vitest';
import { DEFAULT_PALETTE } from '../components/Character';
import { fnv1a, hairStyle, NO_EXTRAS, partColors, partTransforms, PART_IDS, POSES, safePalette, SECRETARY_PALETTE, SIT_FORWARD, walkAngles, type PartId, type PoseId } from './figure';

const find = (parts: ReturnType<typeof partTransforms>, id: PartId) => {
  const p = parts.find((t) => t.part === id);
  if (p === undefined) throw new Error(id);
  return p;
};
const pose = (id: Exclude<PoseId, 'walk'>, extras = NO_EXTRAS) => partTransforms(POSES[id], 'short', extras);

describe('figure FK', () => {
  it('every_part_is_emitted_exactly_once', () => {
    const parts = pose('stand');
    expect(parts.map((p) => p.part).sort()).toEqual([...PART_IDS].sort());
  });

  it('raise_puts_the_right_hand_above_the_head', () => {
    const p = pose('raise');
    expect(find(p, 'handR').pos.y).toBeGreaterThan(find(p, 'head').pos.y + 0.165);
    expect(find(p, 'handL').pos.y).toBeLessThan(find(p, 'head').pos.y);
  });

  it('sitting_hips_are_at_the_seat_and_typing_hands_are_above_the_desk_in_front_of_the_torso', () => {
    for (const id of ['sit_type', 'sit_read', 'sit_think', 'sit_error'] as const) {
      expect(POSES[id].pelvisY).toBeCloseTo(0.45, 2);
    }
    const p = pose('sit_type');
    for (const h of ['handL', 'handR'] as const) {
      const hand = find(p, h).pos;
      expect(hand.y).toBeGreaterThanOrEqual(0.75);
      expect(hand.y).toBeLessThanOrEqual(0.9);
      expect(hand.z).toBeLessThan(find(p, 'torso').pos.z - 0.2);
      // sau khi kéo về bàn, tay nằm trên mặt bàn (mép bàn cách ghế 0,6 m)
      expect(hand.z - SIT_FORWARD).toBeLessThan(-0.55);
    }
  });

  it('stand_feet_touch_the_ground', () => {
    const p = pose('stand');
    for (const s of ['shoeL', 'shoeR'] as const) expect(find(p, s).pos.y - 0.035).toBeCloseTo(0, 1);
    expect(find(p, 'head').pos.y + 0.17).toBeCloseTo(1.5, 1);
  });

  it('seated_shoes_rest_near_the_floor', () => {
    const p = pose('sit_type');
    expect(find(p, 'shoeL').pos.y).toBeLessThan(0.2);
    expect(find(p, 'shoeL').pos.z).toBeLessThan(-0.3);
  });

  it('walk_is_periodic_and_legs_are_in_antiphase', () => {
    const a0 = walkAngles(0);
    const a1 = walkAngles(1);
    expect(a1.legL.pitch).toBeCloseTo(a0.legL.pitch, 9);
    expect(a1.legR.bend).toBeCloseTo(a0.legR.bend, 9);
    for (const t of [0.1, 0.25, 0.6, 0.9]) {
      const w = walkAngles(t);
      expect(w.legL.pitch).toBeCloseTo(-w.legR.pitch, 9);
      expect(w.armL.pitch).toBeCloseTo(-w.legL.pitch, 9);
      expect(w.legL.bend).toBeGreaterThanOrEqual(10);
      expect(w.legR.bend).toBeLessThanOrEqual(30);
    }
  });

  it('mood_drives_mouth_and_cheeks', () => {
    const happy = partTransforms(POSES.stand, 'short', { ...NO_EXTRAS, mood: 'done' });
    expect(find(happy, 'cheekL').scale.x).toBe(1);
    const calm = partTransforms(POSES.stand, 'short', NO_EXTRAS);
    expect(find(calm, 'cheekL').scale.x).toBe(0);
  });

  it('hair_style_parts_follow_the_style', () => {
    expect(find(partTransforms(POSES.stand, 'bob', NO_EXTRAS), 'hairBob').scale.x).toBe(1);
    expect(find(partTransforms(POSES.stand, 'short', NO_EXTRAS), 'hairBob').scale.x).toBe(0);
    expect(find(partTransforms(POSES.stand, 'bun', NO_EXTRAS), 'hairBun').scale.x).toBe(1);
  });

  it('secretary_has_antenna_headset_and_folder_only_when_asked', () => {
    const s = partTransforms(POSES.stand, 'bob', { mood: 'idle', secretary: true, folder: true });
    for (const id of ['antStem', 'antBall', 'headset', 'folder'] as const) expect(find(s, id).scale.x).toBe(1);
    expect(find(s, 'antBall').pos.y).toBeGreaterThan(find(s, 'head').pos.y + 0.4);
    const n = pose('stand');
    for (const id of ['antStem', 'antBall', 'headset', 'folder'] as const) expect(find(n, id).scale.x).toBe(0);
  });
});

describe('style helpers', () => {
  it('hair_style_is_stable_per_id', () => {
    expect(hairStyle('agt_a')).toBe(hairStyle('agt_a'));
    expect(new Set(['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'].map(hairStyle)).size).toBeGreaterThan(1);
    expect(fnv1a('')).toBe(0x811c9dc5);
  });

  it('bad_palettes_fall_back_to_the_default', () => {
    expect(safePalette({ hair: 'url(x)', outfit: 'red', acc: '#12345' })).toEqual(DEFAULT_PALETTE);
    expect(safePalette({ hair: '#112233', outfit: '#abcdef', acc: '#ABCDEF' })).toEqual({ hair: '#112233', outfit: '#abcdef', acc: '#ABCDEF' });
    expect(SECRETARY_PALETTE.outfit).toBe('#8B7CF6');
    expect(partColors(SECRETARY_PALETTE, 'not-a-color').badge).toBe(SECRETARY_PALETTE.acc);
  });
});
