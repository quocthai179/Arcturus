// Nhân vật có chân cho văn phòng sống: dùng lại `Character` (mặt theo mood, màu persona) và thêm chân, bóng, tư thế.
// Chỉ vẽ. Chuyển động (bước chân, nhún, gõ phím) là CSS gắn theo `data-pose`, tự tắt theo chính sách chuyển động.
import { Character } from '../components/Character';
import type { Mood, Palette } from '../protocol';
import type { Pose } from './sim';

export const WALKER_W = 44;

const HEX = /^#[0-9a-fA-F]{6}$/;
const PANTS = '#2E3445';
const SHOE = '#1A1D26';

export interface WalkerProps {
  mood: Mood;
  pose: Pose;
  palette: Palette | undefined;
  label: string;
}

export function Walker({ mood, pose, palette, label }: WalkerProps) {
  const pants = palette !== undefined && HEX.test(palette.hair) ? palette.hair : PANTS;
  return (
    <div className="walker" data-pose={pose}>
      <svg className="walker__shadow" viewBox="0 0 44 8" width={WALKER_W} height={8} aria-hidden="true">
        <ellipse cx={22} cy={4} rx={16} ry={3.5} fill="#000" opacity={0.28} />
      </svg>
      <svg className="walker__legs" viewBox="0 0 44 16" width={WALKER_W} height={16} aria-hidden="true">
        <g className="walker__leg walker__leg--l">
          <rect x={14} y={0} width={6} height={12} rx={2.5} fill={pants} />
          <rect x={12.5} y={10.5} width={8.5} height={4} rx={2} fill={SHOE} />
        </g>
        <g className="walker__leg walker__leg--r">
          <rect x={24} y={0} width={6} height={12} rx={2.5} fill={pants} />
          <rect x={23} y={10.5} width={8.5} height={4} rx={2} fill={SHOE} />
        </g>
      </svg>
      <div className="walker__top">
        <Character
          mood={mood}
          width={WALKER_W}
          label={label}
          shadow={false}
          {...(palette !== undefined ? { hair: palette.hair, outfit: palette.outfit, acc: palette.acc } : {})}
        />
      </div>
    </div>
  );
}
