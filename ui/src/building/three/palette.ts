// Bảng màu "văn phòng startup ấm" (art pass 2026-11-06): gỗ sồi sáng, trắng kem, màu pastel dịu; không xám bê tông.
// Không import module nào khác của cảnh nên cả `world.ts` lẫn `interiors.ts` dùng chung được.
import { Color } from 'three';

export const PAL = {
  oak: '#D9B98C',
  oakLight: '#E8D2AD',
  oakDark: '#B8946A',
  walnut: '#7B5A43',
  espresso: '#4E362A',
  cream: '#F5EEE3',
  white: '#FBF8F3',
  plaster: '#EFE6D8',
  charcoal: '#3D3A40',
  ink: '#27242B',
  steel: '#8C8580',
  brass: '#C9A35B',
  terracotta: '#C9795A',
  sage: '#8FB59A',
  mustard: '#E3B55F',
  dustyBlue: '#7C9CBF',
  blush: '#E5A5A0',
  oat: '#DCCFBE',
  teal: '#5FA9A0',
  plum: '#9C7BA6',
  warmLight: '#FFE2A8',
} as const;

/** Vải bọc sofa/ghế/đệm: pastel dịu. */
export const FABRICS: readonly string[] = [PAL.terracotta, PAL.sage, PAL.mustard, PAL.dustyBlue, PAL.blush, PAL.oat, PAL.teal, PAL.plum];
export const LEAVES: readonly string[] = ['#3F8F63', '#58A878', '#2F7A52', '#6DB57F', '#4C9A6A'];
export const POTS: readonly string[] = [PAL.white, PAL.terracotta, PAL.charcoal, PAL.oat, PAL.sage];
/** Sách, gáy tranh: màu dịu có độ tương phản vừa phải. */
export const BOOKS: readonly string[] = ['#C9795A', '#E3B55F', '#7C9CBF', '#8FB59A', '#E5A5A0', '#F5EEE3', '#5F6B8A', '#B5651D', '#9C7BA6'];

export function mix(a: string, b: string, t: number): string {
  const c = new Color(a).lerp(new Color(b), t);
  return `#${c.getHexString()}`;
}

/** Màu nhấn của tầng, làm dịu để làm tường nhấn/thảm (không neon). */
export function pastel(accent: string, t = 0.6): string {
  return mix(accent, PAL.cream, t);
}
