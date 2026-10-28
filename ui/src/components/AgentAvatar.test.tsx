import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { AgentAvatar, AVATAR_RENDERERS, DEFAULT_AVATAR_STYLE, type AvatarRendererProps } from './AgentAvatar';

afterEach(cleanup);

const PALETTE = { hair: '#112233', outfit: '#445566', acc: '#778899' };

describe('AgentAvatar', () => {
  it('default_renderer_draws_the_svg_character_with_label_and_size', () => {
    render(<AgentAvatar mood="working" palette={PALETTE} size={88} label="Khoa: Đang làm việc" />);
    const img = screen.getByRole('img', { name: 'Khoa: Đang làm việc' });
    expect(img.getAttribute('width')).toBe('88');
    expect(DEFAULT_AVATAR_STYLE).toBe('svg');
  });

  it('missing_palette_falls_back_to_default_colors_without_crashing', () => {
    render(<AgentAvatar mood="idle" palette={undefined} size={40} label="Ai đó" />);
    expect(screen.getByRole('img', { name: 'Ai đó' })).toBeTruthy();
  });

  it('renderer_registry_is_the_single_swap_point_and_gets_only_the_contract_props', () => {
    // Bộ vẽ mới (ảnh động, 3D) chỉ cần thêm một khóa vào AVATAR_RENDERERS; giao diện không đổi.
    const seen: AvatarRendererProps[] = [];
    const original = AVATAR_RENDERERS.svg;
    AVATAR_RENDERERS.svg = (p) => {
      seen.push(p);
      return <span data-testid="custom" />;
    };
    try {
      render(<AgentAvatar mood="approval" palette={PALETTE} size={28} label="X" />);
      expect(screen.getByTestId('custom')).toBeTruthy();
      expect(seen).toEqual([{ mood: 'approval', palette: PALETTE, size: 28, label: 'X' }]);
    } finally {
      AVATAR_RENDERERS.svg = original;
    }
  });
});
