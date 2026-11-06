import { describe, expect, it } from 'vitest';
import { DEFAULT_PALETTE } from '../components/Character';
import {
  BOTTOM_STYLES, fnv1a, hairStyle, HAIR_STYLES, lookFor, MOOD_FACE, NO_EXTRAS, partColors, partTransforms, PART_IDS, POSES, safePalette, SECRETARY_LOOK, SECRETARY_PALETTE, shapeKey, SHOE_COLORS,
  SIT_FORWARD, SKIN_TONES, TOP_STYLES, walkAngles, type HairStyle, type Look, type PartId, type PoseId, type TopStyle,
} from './figure';
import type { Mood } from '../protocol';

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

const WEIRD_IDS = ['', ' ', '\u0000', 'agt_\u202e', '😀', 'é', 'Đạt', '\ud800', '../../etc/passwd', '<script>', 'a'.repeat(10_000), '0', 'null', 'constructor', '__proto__'];

function valid(look: Look): void {
  expect(HAIR_STYLES).toContain(look.hair);
  expect(TOP_STYLES).toContain(look.top);
  expect(BOTTOM_STYLES).toContain(look.bottom);
  expect(Number.isInteger(look.skin) && look.skin >= 0 && look.skin < SKIN_TONES.length).toBe(true);
  expect(Number.isInteger(look.shoe) && look.shoe >= 0 && look.shoe < SHOE_COLORS.length).toBe(true);
  expect(typeof look.glasses).toBe('boolean');
  expect(typeof look.headphones).toBe('boolean');
  expect(typeof look.lanyard).toBe('boolean');
}

describe('look (kiểu dáng theo id)', () => {
  it('is_deterministic_and_valid_for_any_string_including_weird_and_unicode', () => {
    for (const id of WEIRD_IDS) {
      const a = lookFor(id);
      valid(a);
      expect(lookFor(id)).toEqual(a);
      expect(hairStyle(id)).toBe(a.hair);
    }
  });

  it('every_variant_appears_across_many_ids', () => {
    const ids = Array.from({ length: 600 }, (_, i) => `agt_${i}`);
    const looks = ids.map(lookFor);
    for (const h of HAIR_STYLES) expect(looks.some((l) => l.hair === h), h).toBe(true);
    for (const t of TOP_STYLES) expect(looks.some((l) => l.top === t), t).toBe(true);
    for (let k = 0; k < SKIN_TONES.length; k++) expect(looks.some((l) => l.skin === k), `skin ${k}`).toBe(true);
    for (let k = 0; k < SHOE_COLORS.length; k++) expect(looks.some((l) => l.shoe === k), `shoe ${k}`).toBe(true);
    for (const b of BOTTOM_STYLES) expect(looks.some((l) => l.bottom === b), b).toBe(true);
    const share = (f: (l: Look) => boolean) => looks.filter(f).length / looks.length;
    expect(share((l) => l.glasses)).toBeGreaterThan(0.15);
    expect(share((l) => l.glasses)).toBeLessThan(0.45);
    expect(share((l) => l.headphones)).toBeGreaterThan(0.1);
    expect(share((l) => l.lanyard)).toBeGreaterThan(0.3);
    expect(share((l) => l.lanyard)).toBeLessThan(0.6);
  });

  it('neighbouring_ids_do_not_look_alike', () => {
    const keys = new Set(Array.from({ length: 12 }, (_, i) => lookFor(`agt_${i}`)).map((l) => `${l.hair}.${l.top}.${l.skin}.${l.bottom}`));
    expect(keys.size).toBeGreaterThanOrEqual(9);
  });

  it('shape_key_separates_everything_that_changes_geometry', () => {
    const base = lookFor('x');
    expect(shapeKey({ ...base, hair: 'bun' })).not.toBe(shapeKey({ ...base, hair: 'long' }));
    expect(shapeKey({ ...base, top: 'tee' })).not.toBe(shapeKey({ ...base, top: 'hoodie' }));
    expect(shapeKey({ ...base, glasses: true })).not.toBe(shapeKey({ ...base, glasses: false }));
    expect(shapeKey({ ...base, skin: 0 })).toBe(shapeKey({ ...base, skin: 3 }));
  });

  it('secretary_look_is_a_smart_outfit', () => {
    valid(SECRETARY_LOOK);
    expect(SECRETARY_LOOK.top).toBe('blazer');
    expect(SECRETARY_LOOK.bottom).toBe('slacks');
  });
});

describe('figure variants', () => {
  const visible = (parts: ReturnType<typeof partTransforms>) => new Set(parts.filter((p) => p.scale.x !== 0).map((p) => p.part));
  const HAIR_PARTS: PartId[] = ['hairCap', 'fringe', 'hairBob', 'hairLong', 'hairCurly', 'hairPony', 'hairBun', 'beanie'];
  const MOODS: Mood[] = ['idle', 'working', 'approval', 'error', 'done'];

  it('every_hair_style_emits_each_part_once_and_shows_only_its_own_hair', () => {
    const expected: Record<HairStyle, PartId[]> = {
      short: ['hairCap', 'fringe'], bob: ['hairCap', 'fringe', 'hairBob'], bun: ['hairCap', 'fringe', 'hairBun'], long: ['hairCap', 'fringe', 'hairLong'],
      curly: ['hairCurly'], ponytail: ['hairCap', 'fringe', 'hairPony'], beanie: ['beanie'],
    };
    for (const h of HAIR_STYLES) {
      const parts = partTransforms(POSES.stand, h, { ...NO_EXTRAS, look: { ...lookFor('x'), hair: h } });
      expect(parts.map((p) => p.part).sort()).toEqual([...PART_IDS].sort());
      const shown = visible(parts);
      expect(HAIR_PARTS.filter((p) => shown.has(p)).sort(), h).toEqual([...expected[h]].sort());
    }
  });

  it('every_top_every_mood_every_pose_is_complete_and_finite', () => {
    for (const top of TOP_STYLES) {
      for (const mood of MOODS) {
        for (const pose of Object.keys(POSES) as Exclude<PoseId, 'walk'>[]) {
          const look: Look = { ...lookFor('y'), top, glasses: true, headphones: true, lanyard: true };
          const parts = partTransforms(POSES[pose], look.hair, { mood, secretary: false, folder: false, look });
          expect(parts).toHaveLength(PART_IDS.length);
          for (const p of parts) for (const v of [p.pos.x, p.pos.y, p.pos.z, p.rot.x, p.rot.y, p.rot.z, p.scale.x, p.scale.y, p.scale.z]) expect(Number.isFinite(v)).toBe(true);
        }
      }
    }
  });

  it('clothing_parts_follow_the_top_style', () => {
    const shown = (top: TopStyle) => visible(partTransforms(POSES.stand, 'short', { ...NO_EXTRAS, look: { ...lookFor('z'), top, lanyard: false } }));
    expect(shown('hoodie').has('hood')).toBe(true);
    expect(shown('hoodie').has('pocket')).toBe(true);
    expect(shown('shirt').has('collar')).toBe(true);
    expect(shown('shirt').has('belt')).toBe(true);
    expect(shown('blazer').has('collar')).toBe(true);
    expect(shown('sweater').has('hem')).toBe(true);
    expect(shown('tee').has('collar')).toBe(false);
    expect(shown('tee').has('cuffL')).toBe(false);
    expect(shown('shirt').has('cuffL')).toBe(true);
  });

  it('t_shirt_forearms_are_bare_skin_and_sleeves_keep_the_outfit_colour', () => {
    const fore = (top: TopStyle) => find(partTransforms(POSES.sit_type, 'short', { ...NO_EXTRAS, look: { ...lookFor('z'), top } }), 'foreL');
    expect(fore('tee').tint).toBe('skin');
    expect(fore('hoodie').tint).toBeUndefined();
  });

  it('accessories_follow_the_look_and_the_secretary_keeps_her_own_headset', () => {
    const base = lookFor('acc');
    const on = visible(partTransforms(POSES.stand, base.hair, { ...NO_EXTRAS, look: { ...base, glasses: true, headphones: true, lanyard: true } }));
    const off = visible(partTransforms(POSES.stand, base.hair, { ...NO_EXTRAS, look: { ...base, glasses: false, headphones: false, lanyard: false } }));
    for (const p of ['glasses', 'headphones', 'lanyard'] as const) {
      expect(on.has(p)).toBe(true);
      expect(off.has(p)).toBe(false);
    }
    const sec = visible(partTransforms(POSES.stand, 'bob', { mood: 'idle', secretary: true, folder: true, look: { ...SECRETARY_LOOK, headphones: true } }));
    expect(sec.has('headset')).toBe(true);
    expect(sec.has('headphones')).toBe(false);
  });

  it('mood_changes_brows_and_mouth', () => {
    const brow = (m: Mood) => find(partTransforms(POSES.stand, 'short', { ...NO_EXTRAS, mood: m }), 'browL');
    expect(MOOD_FACE.error.browTilt).toBeGreaterThan(0);
    expect(MOOD_FACE.working.browTilt).toBeLessThan(0);
    expect(brow('approval').pos.y).toBeGreaterThan(brow('working').pos.y);
    // mày trái và phải đối xứng qua mặt phẳng giữa
    const p = partTransforms(POSES.stand, 'short', { ...NO_EXTRAS, mood: 'error' });
    expect(find(p, 'browL').rot.z).toBeCloseTo(-find(p, 'browR').rot.z, 6);
    const mouthOf = (m: Mood) => visible(partTransforms(POSES.stand, 'short', { ...NO_EXTRAS, mood: m }));
    expect(mouthOf('done').has('smile')).toBe(true);
    expect(mouthOf('error').has('smile')).toBe(true);
    expect(mouthOf('working').has('smile')).toBe(false);
    expect(mouthOf('working').has('mouth')).toBe(true);
    expect(find(partTransforms(POSES.stand, 'short', { ...NO_EXTRAS, mood: 'error' }), 'smile').pos.y).toBeLessThan(find(partTransforms(POSES.stand, 'short', { ...NO_EXTRAS, mood: 'done' }), 'smile').pos.y);
    for (const m of MOODS) expect(['smile', 'flat', 'open', 'frown']).toContain(MOOD_FACE[m].mouth);
  });

  it('colours_follow_skin_trousers_and_shoes_of_the_look', () => {
    const a = partColors({ hair: '#112233', outfit: '#445566', acc: '#778899' }, '#FFB020', { ...lookFor('c1'), skin: 0, bottom: 'jeans', shoe: 0 });
    const b = partColors({ hair: '#112233', outfit: '#445566', acc: '#778899' }, '#FFB020', { ...lookFor('c1'), skin: 5, bottom: 'chino', shoe: 2 });
    expect(a.skin).toBe(SKIN_TONES[0]);
    expect(b.skin).toBe(SKIN_TONES[5]);
    expect(a.pants).not.toBe(b.pants);
    expect(a.shoe).not.toBe(b.shoe);
    expect(a.outfit).toBe(b.outfit);
    for (const v of Object.values(a)) expect(v).toMatch(/^#[0-9a-f]{6}$/i);
  });

  it('head_stays_round_and_the_figure_stays_1_5_m_tall', () => {
    const p = pose('stand');
    expect(find(p, 'head').pos.y + 0.19).toBeCloseTo(1.5, 1);
  });
});
