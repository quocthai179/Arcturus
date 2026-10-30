// Nhân vật SVG có tham số (docs/PLAN.md T11, phụ lục A). Mỗi mood là một nhánh render riêng (không bật/tắt bằng opacity),
// nên ở mỗi thời điểm DOM chỉ có đúng phần mắt, miệng, phụ kiện của mood hiện tại.
import { useState } from 'react';
import type { Mood } from '../protocol';
import './Character.css';

export type { Mood };

export interface CharacterProps {
  mood: Mood;
  /** Chiều rộng (px); chiều cao luôn bằng width × 1,2. */
  width?: number;
  hair?: string;
  outfit?: string;
  acc?: string;
  /** Ghi đè `aria-label` mặc định theo mood. */
  label?: string;
  /** Bóng dưới chân (mặc định có). Văn phòng sống tự vẽ bóng dưới chân nhân vật có chân. */
  shadow?: boolean;
}

export const MOOD_LABELS: Record<Mood, string> = {
  idle: 'Đang rảnh',
  working: 'Đang làm việc',
  approval: 'Giơ tay xin duyệt',
  error: 'Gặp lỗi',
  done: 'Vừa xong việc',
};

/** Bảng màu mặc định đầu tiên của phụ lục B (persona không cấu hình). */
export const DEFAULT_PALETTE = { hair: '#2E2A5C', outfit: '#8B7CF6', acc: '#FF7A59' } as const;

const INK = '#241B4D';
const SKIN = '#FFE6D8';
const MOUTH = '#B4445E';
const SPARK = '#FFB020';

const HEX = /^#[0-9a-fA-F]{6}$/;

/**
 * Chỉ nhận `#RRGGBB` (hub đã kiểm màu persona ở config.rs). Giá trị khác (rỗng, `url(...)`, tên màu lạ) dùng màu mặc
 * định: màu đi thẳng vào thuộc tính SVG nên không nhận chuỗi tùy ý.
 */
function color(value: string | undefined, fallback: string): string {
  return value !== undefined && HEX.test(value) ? value : fallback;
}

/** Mood có hiệu ứng ngắn khi vừa chuyển sang (≤600 ms, một lần, tắt khi prefers-reduced-motion). */
function pops(mood: Mood): boolean {
  return mood === 'approval' || mood === 'done';
}

export function Character({ mood, width = 120, hair, outfit, acc, label, shadow = true }: CharacterProps) {
  const H = color(hair, DEFAULT_PALETTE.hair);
  const O = color(outfit, DEFAULT_PALETTE.outfit);
  const A = color(acc, DEFAULT_PALETTE.acc);

  // Đếm số lần vừa chuyển sang approval/done (điều chỉnh state ngay trong render khi prop đổi, theo cách React khuyên
  // dùng thay cho effect). Lần render đầu không có hiệu ứng.
  const [prevMood, setPrevMood] = useState<Mood>(mood);
  const [popCount, setPopCount] = useState(0);
  if (mood !== prevMood) {
    setPrevMood(mood);
    if (pops(mood)) {
      setPopCount((n) => n + 1);
    }
  }
  const popping = popCount > 0 && pops(mood);

  return (
    <svg
      className="character"
      viewBox="0 0 200 240"
      width={width}
      height={width * 1.2}
      role="img"
      aria-label={label ?? MOOD_LABELS[mood]}
      focusable="false"
      data-mood={mood}
    >
      {shadow && <ellipse cx={100} cy={231} rx={52} ry={7} fill="#000" opacity={0.25} />}
      <path
        data-part="hair-back"
        d="M36 104 Q32 38 100 34 Q168 38 164 104 L160 160 Q150 176 140 162 L140 122 L60 122 L60 162 Q50 176 40 160 Z"
        fill={H}
      />
      <path data-part="outfit" d="M52 232 Q50 166 100 160 Q150 166 148 232 Z" fill={O} />
      <path
        d="M84 163 L100 182 L116 163"
        fill="none"
        stroke="#F4F1FF"
        strokeWidth={4}
        strokeLinejoin="round"
        opacity={0.85}
      />
      <rect x={90} y={146} width={20} height={18} rx={6} fill="#F7D3C1" />
      <ellipse cx={100} cy={104} rx={56} ry={52} fill={SKIN} />
      <path
        data-part="hair-front"
        d="M42 100 Q44 44 100 42 Q156 44 158 100 Q146 78 128 80 Q122 64 106 76 Q94 60 80 76 Q62 70 56 88 Q48 90 42 100 Z"
        fill={H}
      />
      <path d="M100 44 Q104 22 122 24 Q108 30 106 46 Z" fill={H} />
      <g data-part="headphones">
        <path d="M40 104 Q38 30 100 28 Q162 30 160 104" fill="none" stroke={A} strokeWidth={8} strokeLinecap="round" />
        <rect x={28} y={92} width={18} height={34} rx={9} fill={A} />
        <rect x={154} y={92} width={18} height={34} rx={9} fill={A} />
      </g>
      <ellipse cx={70} cy={122} rx={9} ry={5} fill="#FF8FA8" opacity={0.55} />
      <ellipse cx={130} cy={122} rx={9} ry={5} fill="#FF8FA8" opacity={0.55} />
      <Eyes mood={mood} />
      <Mouth mood={mood} />
      {/* `key` đổi mỗi lần chuyển sang approval/done: nhóm được dựng lại nên hiệu ứng CSS chạy đúng một lần. */}
      <g key={popCount} className={popping ? 'character__pop' : undefined} data-part="accessory">
        <Accessory mood={mood} outfit={O} />
      </g>
    </svg>
  );
}

function Eyes({ mood }: { mood: Mood }) {
  switch (mood) {
    case 'approval':
      return (
        <g data-part="eyes-approval">
          <ellipse cx={78} cy={106} rx={7} ry={10} fill={INK} />
          <ellipse cx={122} cy={106} rx={7} ry={10} fill={INK} />
          <circle cx={80.5} cy={102} r={3} fill="#fff" />
          <circle cx={124.5} cy={102} r={3} fill="#fff" />
        </g>
      );
    case 'done':
      return (
        <g data-part="eyes-done" fill="none" stroke={INK} strokeWidth={4} strokeLinecap="round">
          <path d="M70 108 Q78 98 86 108" />
          <path d="M114 108 Q122 98 130 108" />
        </g>
      );
    case 'working':
      return (
        <g data-part="eyes-working">
          <ellipse cx={78} cy={110} rx={7} ry={6} fill={INK} />
          <ellipse cx={122} cy={110} rx={7} ry={6} fill={INK} />
          <path d="M69 101 L87 103 M113 103 L131 101" fill="none" stroke={INK} strokeWidth={3} strokeLinecap="round" />
        </g>
      );
    case 'idle':
      return (
        <g data-part="eyes-idle" fill="none" stroke={INK} strokeWidth={3.5} strokeLinecap="round">
          <path d="M70 106 Q78 112 86 106" />
          <path d="M114 106 Q122 112 130 106" />
        </g>
      );
    case 'error':
      return (
        <g
          data-part="eyes-error"
          fill="none"
          stroke={INK}
          strokeWidth={4}
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M71 100 L85 107 L71 114" />
          <path d="M129 100 L115 107 L129 114" />
        </g>
      );
  }
}

function Mouth({ mood }: { mood: Mood }) {
  switch (mood) {
    case 'idle':
    case 'working':
      return (
        <path
          data-part="mouth-smile"
          d="M93 130 Q100 136 107 130"
          fill="none"
          stroke={MOUTH}
          strokeWidth={3}
          strokeLinecap="round"
        />
      );
    case 'done':
      return <path data-part="mouth-open" d="M91 128 L109 128 Q107 141 100 141 Q93 141 91 128 Z" fill={MOUTH} />;
    case 'approval':
      return <ellipse data-part="mouth-o" cx={100} cy={132} rx={4.5} ry={5.5} fill={MOUTH} />;
    case 'error':
      return (
        <path
          data-part="mouth-wavy"
          d="M90 133 Q95 128 100 133 Q105 138 110 133"
          fill="none"
          stroke={MOUTH}
          strokeWidth={3}
          strokeLinecap="round"
        />
      );
  }
}

function Accessory({ mood, outfit }: { mood: Mood; outfit: string }) {
  switch (mood) {
    case 'approval':
      return (
        <>
          <g data-part="raised-hand">
            <path d="M144 192 Q160 172 164 148" fill="none" stroke={outfit} strokeWidth={16} strokeLinecap="round" />
            <circle cx={165} cy={140} r={11} fill={SKIN} />
          </g>
          <g data-part="alert-badge">
            <circle cx={178} cy={42} r={16} fill={SPARK} />
            <path d="M178 32 L178 45" stroke={INK} strokeWidth={4} strokeLinecap="round" />
            <circle cx={178} cy={52} r={2.5} fill={INK} />
          </g>
        </>
      );
    case 'done':
      return (
        <g data-part="sparkles" fill={SPARK}>
          <path d="M22 50 L26 60 L36 64 L26 68 L22 78 L18 68 L8 64 L18 60 Z" />
          <path d="M178 28 L181 36 L189 39 L181 42 L178 50 L175 42 L167 39 L175 36 Z" />
        </g>
      );
    case 'working':
      return (
        <g data-part="typing-bubble">
          <rect x={146} y={18} width={46} height={26} rx={13} fill="#fff" />
          <circle cx={159} cy={31} r={3.5} fill={outfit} />
          <circle cx={169} cy={31} r={3.5} fill={outfit} />
          <circle cx={179} cy={31} r={3.5} fill={outfit} />
        </g>
      );
    case 'idle':
      return (
        <path
          data-part="sleep-z"
          d="M160 30 L174 30 L160 44 L174 44"
          fill="none"
          stroke="#C3C8D4"
          strokeWidth={3}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      );
    case 'error':
      return <path data-part="sweat-drop" d="M152 68 Q159 79 152 86 Q145 79 152 68 Z" fill="#7CC4F2" />;
  }
}
