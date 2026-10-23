import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { Character, DEFAULT_PALETTE, MOOD_LABELS, type Mood } from './Character';

afterEach(cleanup);

const MOODS: readonly Mood[] = ['idle', 'working', 'approval', 'error', 'done'];

/** Phần đặc trưng của từng mood (phụ lục A): mắt, miệng, phụ kiện. */
const PARTS: Record<Mood, readonly string[]> = {
  idle: ['eyes-idle', 'mouth-smile', 'sleep-z'],
  working: ['eyes-working', 'mouth-smile', 'typing-bubble'],
  approval: ['eyes-approval', 'mouth-o', 'raised-hand', 'alert-badge'],
  error: ['eyes-error', 'mouth-wavy', 'sweat-drop'],
  done: ['eyes-done', 'mouth-open', 'sparkles'],
};

const ALL_MOOD_PARTS = [...new Set(Object.values(PARTS).flat())];

function svgOf(container: HTMLElement): SVGSVGElement {
  const svg = container.querySelector('svg');
  if (svg === null) {
    throw new Error('không thấy <svg>');
  }
  return svg;
}

function part(container: HTMLElement, name: string): Element | null {
  return container.querySelector(`[data-part="${name}"]`);
}

describe('Character', () => {
  it.each(MOODS)('renders_only_parts_of_mood_%s', (mood) => {
    const { container } = render(<Character mood={mood} />);
    for (const name of ALL_MOOD_PARTS) {
      const expected = PARTS[mood].includes(name);
      // Phần của mood khác không nằm trong DOM (không ẩn bằng opacity).
      expect(part(container, name) !== null, `${mood}: ${name}`).toBe(expected);
    }
    // Phần chung luôn có.
    for (const name of ['hair-back', 'hair-front', 'outfit', 'headphones']) {
      expect(part(container, name), name).not.toBeNull();
    }
    expect(svgOf(container).getAttribute('data-mood')).toBe(mood);
  });

  it.each(MOODS)('default_aria_label_for_%s', (mood) => {
    render(<Character mood={mood} />);
    expect(screen.getByRole('img', { name: MOOD_LABELS[mood] })).toBeTruthy();
  });

  it('aria_labels_match_spec', () => {
    expect(MOOD_LABELS).toEqual({
      idle: 'Đang rảnh',
      working: 'Đang làm việc',
      approval: 'Giơ tay xin duyệt',
      error: 'Gặp lỗi',
      done: 'Vừa xong việc',
    });
  });

  it('label_prop_overrides_aria_label', () => {
    render(<Character mood="approval" label="Khoa đang giơ tay" />);
    expect(screen.getByRole('img', { name: 'Khoa đang giơ tay' })).toBeTruthy();
  });

  it('applies_palette_props', () => {
    const { container } = render(
      <Character mood="approval" hair="#1F1F24" outfit="#4C8DF6" acc="#9AA3B5" />,
    );
    expect(part(container, 'hair-back')?.getAttribute('fill')).toBe('#1F1F24');
    expect(part(container, 'hair-front')?.getAttribute('fill')).toBe('#1F1F24');
    expect(part(container, 'outfit')?.getAttribute('fill')).toBe('#4C8DF6');
    const band = part(container, 'headphones')?.querySelector('path');
    expect(band?.getAttribute('stroke')).toBe('#9AA3B5');
    // Tay giơ lên cùng màu áo.
    expect(part(container, 'raised-hand')?.querySelector('path')?.getAttribute('stroke')).toBe('#4C8DF6');
  });

  it('typing_dots_use_outfit_color', () => {
    const { container } = render(<Character mood="working" outfit="#2BB5A0" />);
    const dots = part(container, 'typing-bubble')?.querySelectorAll('circle') ?? [];
    expect(dots.length).toBe(3);
    for (const d of dots) {
      expect(d.getAttribute('fill')).toBe('#2BB5A0');
    }
  });

  it('invalid_colors_fall_back_to_default_palette', () => {
    const { container } = render(
      <Character mood="idle" hair="url(#x)" outfit="red" acc="#12345" />,
    );
    expect(part(container, 'hair-back')?.getAttribute('fill')).toBe(DEFAULT_PALETTE.hair);
    expect(part(container, 'outfit')?.getAttribute('fill')).toBe(DEFAULT_PALETTE.outfit);
    expect(part(container, 'headphones')?.querySelector('path')?.getAttribute('stroke')).toBe(
      DEFAULT_PALETTE.acc,
    );
  });

  it('height_is_width_times_1_2', () => {
    const { container, rerender } = render(<Character mood="idle" width={50} />);
    expect(svgOf(container).getAttribute('width')).toBe('50');
    expect(svgOf(container).getAttribute('height')).toBe('60');
    expect(svgOf(container).getAttribute('viewBox')).toBe('0 0 200 240');
    rerender(<Character mood="idle" />);
    expect(svgOf(container).getAttribute('height')).toBe('144');
  });

  it('pop_effect_only_when_switching_to_approval_or_done', () => {
    const popped = (c: HTMLElement) => c.querySelector('.character__pop') !== null;
    // Lần đầu hiện thẻ: không có hiệu ứng, kể cả khi đã ở approval.
    const { container, rerender } = render(<Character mood="approval" />);
    expect(popped(container)).toBe(false);
    rerender(<Character mood="working" />);
    expect(popped(container)).toBe(false);
    rerender(<Character mood="approval" />);
    expect(popped(container)).toBe(true);
    rerender(<Character mood="done" />);
    expect(popped(container)).toBe(true);
    rerender(<Character mood="idle" />);
    expect(popped(container)).toBe(false);
    // Render lại cùng mood (ví dụ đổi màu) không chạy lại hiệu ứng mới: nhóm giữ nguyên.
    rerender(<Character mood="done" />);
    const group = container.querySelector('.character__pop');
    rerender(<Character mood="done" hair="#5A3A22" />);
    expect(container.querySelector('.character__pop')).toBe(group);
  });
});
