// Điểm thay thế duy nhất cho cách vẽ nhân vật (docs/UI_EXTENSION.md). Mọi nơi hiện nhân vật (thẻ duyệt, bàn văn phòng,
// danh sách đội) đều đi qua `AgentAvatar`; thêm kiểu vẽ mới (SVG chuyển động, Lottie, 3D VRM) = thêm một khóa vào
// `AVATAR_RENDERERS`, không phải sửa từng nơi dùng.
//
// Hợp đồng của bộ vẽ: chỉ nhận `AvatarRendererProps`, chỉ vẽ. Không gọi hub, không đọc store, không có logic nghiệp vụ;
// phải tôn trọng chính sách chuyển động (`useMotionPolicy`, thuộc tính `data-motion`) và ngân sách tài nguyên
// (docs/BUDGET.md): không vòng lặp chạy mãi khi rảnh, kiểu nặng phải nạp muộn (`React.lazy`) và tự giải phóng khi gỡ.
import type { ComponentType } from 'react';
import type { Mood, Palette } from '../protocol';
import { Character } from './Character';

export type AvatarStyle = 'svg';

export const DEFAULT_AVATAR_STYLE: AvatarStyle = 'svg';

export interface AvatarRendererProps {
  mood: Mood;
  /** `undefined` khi chưa biết agent (bộ vẽ dùng bảng màu mặc định). */
  palette: Palette | undefined;
  /** Chiều rộng (px). */
  size: number;
  /** Mô tả cho trình đọc màn hình (đã gồm tên và trạng thái). */
  label: string;
}

function SvgAvatar({ mood, palette, size, label }: AvatarRendererProps) {
  return (
    <Character
      mood={mood}
      width={size}
      label={label}
      {...(palette !== undefined ? { hair: palette.hair, outfit: palette.outfit, acc: palette.acc } : {})}
    />
  );
}

/** Bảng bộ vẽ. Có thể đổi/thêm khóa (test, tính năng sau này); giao diện chỉ biết `AvatarRendererProps`. */
export const AVATAR_RENDERERS: Record<AvatarStyle, ComponentType<AvatarRendererProps>> = {
  svg: SvgAvatar,
};

export interface AgentAvatarProps extends AvatarRendererProps {
  /** Mặc định `DEFAULT_AVATAR_STYLE`; sau này lấy từ tùy chọn của người dùng. */
  style?: AvatarStyle | undefined;
}

export function AgentAvatar({ style = DEFAULT_AVATAR_STYLE, ...props }: AgentAvatarProps) {
  const Renderer = AVATAR_RENDERERS[style];
  return <Renderer {...props} />;
}
