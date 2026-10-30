// Nền và đồ đạc của văn phòng sống (SVG tĩnh). Màu riêng của cảnh nằm trong bộ vẽ SVG, không phải CSS
// (docs/UI_EXTENSION.md luật 5). Mỗi món đồ có "chân" (điểm neo đáy giữa) để xếp lớp theo trục y với nhân vật.
import type { ReactNode } from 'react';
import type { Mood } from '../protocol';
import {
  BOOKSHELF,
  COFFEE_TABLE,
  COOLER,
  COUNTER,
  FRIDGE,
  HALF_WALL,
  MEETING_TABLE,
  NAP_SPOTS,
  PANTRY_TABLE,
  PARTITION_DOORS,
  PARTITION_X,
  PLANTS,
  PRINTER,
  SEATS,
  SOFA,
  STAGE_H,
  STAGE_W,
  WALL_H,
  WHITEBOARD,
} from './layout';

/** Một món đặt trên sàn: neo ở (x, y) = đáy giữa, z = y. */
export function Sprite({ x, y, w, h, z, children, className }: {
  x: number;
  y: number;
  w: number;
  h: number;
  z?: number;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={className ? `sprite ${className}` : 'sprite'}
      style={{ transform: `translate(${x - w / 2}px, ${y - h}px)`, width: w, height: h, zIndex: Math.round(z ?? y) }}
      aria-hidden="true"
    >
      <svg viewBox={`0 0 ${w} ${h}`} width={w} height={h} overflow="visible">
        {children}
      </svg>
    </div>
  );
}

const GLOW: Record<Mood | 'off', string> = {
  working: '#8B7CF6',
  approval: '#FFB020',
  error: '#FF6B6B',
  done: '#34D1B5',
  idle: '#4A5163',
  off: '#3A3F4D',
};

// ---------- nền ----------

function sky(hour: number): [string, string, boolean] {
  if (hour < 5 || hour >= 20) return ['#0B1430', '#1E2A55', true];
  if (hour < 7) return ['#F6A86B', '#7FA6E0', false];
  if (hour >= 17) return ['#F08A5D', '#5B5FA8', false];
  return ['#7EC8F2', '#CDEBFF', false];
}

function Window({ x, w, hour }: { x: number; w: number; hour: number }) {
  const [top, bottom, night] = sky(hour);
  const id = `sky-${x}`;
  return (
    <g>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={top} />
          <stop offset="1" stopColor={bottom} />
        </linearGradient>
      </defs>
      <rect x={x - 4} y={14} width={w + 8} height={70} rx={4} fill="#3B3550" />
      <rect x={x} y={18} width={w} height={62} rx={2} fill={`url(#${id})`} />
      {night ? (
        <g fill="#FFF7D6">
          <circle cx={x + w * 0.2} cy={30} r={1.3} />
          <circle cx={x + w * 0.55} cy={26} r={1} />
          <circle cx={x + w * 0.8} cy={40} r={1.4} />
          <circle cx={x + w * 0.35} cy={48} r={0.9} />
          <path d={`M${x + w * 0.72} 30 a9 9 0 1 0 9 11 a7 7 0 1 1 -9 -11 Z`} />
        </g>
      ) : (
        <g fill="#FFFFFF" opacity={0.85}>
          <ellipse cx={x + w * 0.3} cy={40} rx={16} ry={6} />
          <ellipse cx={x + w * 0.38} cy={36} rx={10} ry={6} />
          <ellipse cx={x + w * 0.75} cy={58} rx={14} ry={5} />
        </g>
      )}
      {/* dãy nhà phía xa */}
      <path
        d={`M${x} 80 L${x} 66 L${x + 14} 66 L${x + 14} 58 L${x + 30} 58 L${x + 30} 70 L${x + 46} 70 L${x + 46} 54 L${x + 60} 54 L${x + 60} 68 L${x + w * 0.6} 68 L${x + w * 0.6} 60 L${x + w * 0.75} 60 L${x + w * 0.75} 72 L${x + w} 72 L${x + w} 80 Z`}
        fill={night ? '#141B36' : '#8BA3C7'}
        opacity={0.9}
      />
      {night && (
        <g fill="#FFD66B" opacity={0.9}>
          <rect x={x + 18} y={62} width={3} height={3} />
          <rect x={x + 50} y={58} width={3} height={3} />
          <rect x={x + w * 0.65} y={64} width={3} height={3} />
        </g>
      )}
      <rect x={x + w / 2 - 2} y={18} width={4} height={62} fill="#3B3550" />
      <rect x={x} y={47} width={w} height={3} fill="#3B3550" />
      <rect x={x - 8} y={82} width={w + 16} height={6} rx={2} fill="#4A4363" />
    </g>
  );
}

function Clock({ cx, cy, date }: { cx: number; cy: number; date: Date }) {
  const m = date.getMinutes();
  const h = (date.getHours() % 12) + m / 60;
  const hand = (deg: number, len: number) => {
    const r = ((deg - 90) * Math.PI) / 180;
    return `M${cx} ${cy} L${cx + Math.cos(r) * len} ${cy + Math.sin(r) * len}`;
  };
  return (
    <g>
      <circle cx={cx} cy={cy} r={17} fill="#F4F1FF" stroke="#3B3550" strokeWidth={3} />
      <path d={hand(h * 30, 8)} stroke="#241B4D" strokeWidth={2.5} strokeLinecap="round" />
      <path d={hand(m * 6, 12)} stroke="#241B4D" strokeWidth={1.8} strokeLinecap="round" />
      <circle cx={cx} cy={cy} r={1.8} fill="#FF7A59" />
    </g>
  );
}

/** Sàn, tường, cửa sổ, vách kính, thảm: nằm dưới mọi nhân vật. */
export function Backdrop({ date }: { date: Date }) {
  const hour = date.getHours();
  let top = WALL_H;
  const glass: [number, number][] = [];
  for (const [a, b] of PARTITION_DOORS) {
    glass.push([top, a]);
    top = b;
  }
  glass.push([top, STAGE_H]);
  return (
    <svg className="live__backdrop" viewBox={`0 0 ${STAGE_W} ${STAGE_H}`} width={STAGE_W} height={STAGE_H} aria-hidden="true">
      <defs>
        <pattern id="planks" width="80" height="24" patternUnits="userSpaceOnUse">
          <rect width="80" height="24" fill="#2B2A36" />
          <path d="M0 23.5 H80 M40 0 V12 M10 12 V24 M70 12 V24 M0 11.5 H80" stroke="#24232E" strokeWidth={1} />
        </pattern>
        <pattern id="tiles" width="28" height="28" patternUnits="userSpaceOnUse">
          <rect width="28" height="28" fill="#2A3138" />
          <rect width="14" height="14" fill="#2F373F" />
          <rect x="14" y="14" width="14" height="14" fill="#2F373F" />
        </pattern>
        <linearGradient id="wall" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#3A3352" />
          <stop offset="1" stopColor="#2D2842" />
        </linearGradient>
        <radialGradient id="lamp" cx="0.5" cy="0.5" r="0.5">
          <stop offset="0" stopColor="#FFE9B0" stopOpacity={0.12} />
          <stop offset="1" stopColor="#FFE9B0" stopOpacity={0} />
        </radialGradient>
      </defs>
      {/* sàn ba khu */}
      <rect x={0} y={WALL_H} width={PARTITION_X + 9} height={STAGE_H - WALL_H} fill="url(#planks)" />
      <rect x={PARTITION_X + 9} y={WALL_H} width={STAGE_W - PARTITION_X - 9} height={HALF_WALL.y + HALF_WALL.h - WALL_H} fill="#2C3A46" />
      <rect x={PARTITION_X + 9} y={HALF_WALL.y + HALF_WALL.h} width={STAGE_W - PARTITION_X - 9} height={STAGE_H - HALF_WALL.y - HALF_WALL.h} fill="url(#tiles)" />
      {/* thảm */}
      <ellipse cx={850} cy={222} rx={120} ry={52} fill="#3D4F73" />
      <ellipse cx={850} cy={222} rx={104} ry={42} fill="none" stroke="#5A6E96" strokeWidth={3} strokeDasharray="2 7" />
      <rect x={290} y={462} width={240} height={104} rx={14} fill="#33303F" />
      <rect x={612} y={590} width={76} height={18} rx={4} fill="#4B3B33" />
      {/* tường trên */}
      <rect x={0} y={0} width={STAGE_W} height={WALL_H} fill="url(#wall)" />
      <rect x={0} y={WALL_H - 6} width={STAGE_W} height={6} fill="#1F1B2E" />
      <Window x={140} w={150} hour={hour} />
      <Window x={380} w={140} hour={hour} />
      <Clock cx={590} cy={46} date={date} />
      <g>
        <rect x={26} y={26} width={86} height={34} rx={6} fill="#1C1830" />
        <text x={69} y={48} textAnchor="middle" fontSize={11} fontWeight={700} letterSpacing={2.5} fill="#8B7CF6">
          ARCTURUS
        </text>
      </g>
      {/* tranh trên sofa phòng nghỉ */}
      <rect x={790} y={20} width={120} height={58} rx={3} fill="#E9E4D6" stroke="#4A4363" strokeWidth={4} />
      <circle cx={830} cy={46} r={12} fill="#FF7A59" />
      <path d="M796 74 L836 42 L862 62 L880 48 L906 74 Z" fill="#34D1B5" />
      <rect x={945} y={30} width={30} height={40} rx={3} fill="#24203A" />
      <text x={960} y={56} textAnchor="middle" fontSize={16}>☕</text>
      {/* vách kính */}
      {glass.map(([a, b]) => (
        <g key={a}>
          <rect x={PARTITION_X} y={a} width={18} height={b - a} fill="#9EC9FF" opacity={0.14} />
          <rect x={PARTITION_X} y={a} width={3} height={b - a} fill="#6E7B96" />
          <rect x={PARTITION_X + 15} y={a} width={3} height={b - a} fill="#6E7B96" />
        </g>
      ))}
      {/* tường thấp phòng nghỉ / pantry */}
      <rect x={HALF_WALL.x} y={HALF_WALL.y} width={HALF_WALL.w} height={HALF_WALL.h} fill="#4A4363" />
      <rect x={HALF_WALL.x} y={HALF_WALL.y} width={HALF_WALL.w} height={5} fill="#5C547A" />
      {/* tường dưới, cửa ra vào */}
      <rect x={0} y={STAGE_H - 12} width={620} height={12} fill="#1F1B2E" />
      <rect x={680} y={STAGE_H - 12} width={STAGE_W - 680} height={12} fill="#1F1B2E" />
      <rect x={0} y={WALL_H} width={10} height={STAGE_H - WALL_H} fill="#1F1B2E" />
      <rect x={STAGE_W - 10} y={WALL_H} width={10} height={STAGE_H - WALL_H} fill="#1F1B2E" />
      {/* quầng đèn trần */}
      {[180, 480, 850].map((x) => <ellipse key={x} cx={x} cy={280} rx={190} ry={150} fill="url(#lamp)" />)}
    </svg>
  );
}

// ---------- đồ đạc ----------

export interface DeskState {
  /** Mood của người ngồi đang ở bàn, `null` khi bàn trống. */
  mood: Mood | null;
  typing: boolean;
}

function Desk({ i, state }: { i: number; state: DeskState }) {
  const seat = SEATS[i];
  if (seat?.desk === undefined) return null;
  const { x, y } = seat.desk;
  const glow = GLOW[state.mood ?? 'off'];
  return (
    <>
      <Sprite x={seat.at.x} y={seat.at.y - 1} w={40} h={48}>
        <rect x={6} y={2} width={28} height={30} rx={8} fill="#3B4256" />
        <rect x={9} y={5} width={22} height={24} rx={6} fill="#4B5470" />
        <rect x={18} y={32} width={4} height={10} fill="#2A2F3E" />
        <rect x={8} y={42} width={24} height={4} rx={2} fill="#2A2F3E" />
      </Sprite>
      <Sprite x={x} y={y + 22} w={124} h={70} className={state.typing ? 'desk-sprite desk-sprite--typing' : 'desk-sprite'}>
        {state.mood !== null && state.mood !== 'idle' && (
          <ellipse cx={62} cy={30} rx={46} ry={10} fill={glow} opacity={0.18} />
        )}
        <path d="M4 20 H120 L122 44 H2 Z" fill="#C99A6B" />
        <path d="M4 20 H120 L120.6 25 H3.4 Z" fill="#DDB083" />
        <rect x={2} y={44} width={120} height={14} rx={2} fill="#A87A4F" />
        <rect x={6} y={58} width={6} height={12} fill="#7A5638" />
        <rect x={112} y={58} width={6} height={12} fill="#7A5638" />
        {/* laptop: mặt sau nắp hướng về người xem, logo sáng theo trạng thái */}
        <path d="M44 28 H80 L84 33 H40 Z" fill="#9AA3B5" />
        <rect x={45} y={6} width={34} height={23} rx={3} fill="#2B2F3A" />
        <circle cx={62} cy={17.5} r={4.5} fill={glow} />
        {state.mood !== null && <circle cx={62} cy={17.5} r={8} fill={glow} opacity={0.25} />}
        {state.typing && (
          <g className="desk-sprite__hands" fill="#FFE6D8">
            <circle className="desk-sprite__hand" cx={48} cy={31} r={3.6} />
            <circle className="desk-sprite__hand desk-sprite__hand--r" cx={76} cy={31} r={3.6} />
          </g>
        )}
        {/* cốc và cây nhỏ */}
        <rect x={14} y={22} width={9} height={11} rx={2} fill={i % 2 === 0 ? '#FF7A59' : '#34D1B5'} />
        <path d="M23 25 q4 0 4 3 q0 3 -4 3" fill="none" stroke={i % 2 === 0 ? '#FF7A59' : '#34D1B5'} strokeWidth={1.6} />
        <rect x={100} y={26} width={10} height={8} rx={2} fill="#8A6A55" />
        <path d="M105 26 q-8 -10 -2 -14 q3 6 2 14 q2 -10 8 -12 q0 8 -8 12" fill="#5BBF7A" />
        {/* bảng tên: sáng khi có người */}
        <rect x={42} y={47} width={40} height={8} rx={2} fill="#F4F1FF" opacity={state.mood === null ? 0.25 : 0.9} />
      </Sprite>
    </>
  );
}

function Chair({ x, y }: { x: number; y: number }) {
  return (
    <Sprite x={x} y={y - 1} w={36} h={44}>
      <rect x={5} y={2} width={26} height={26} rx={8} fill="#3B4256" />
      <rect x={16} y={28} width={4} height={10} fill="#2A2F3E" />
      <rect x={7} y={38} width={22} height={4} rx={2} fill="#2A2F3E" />
    </Sprite>
  );
}

function MeetingTable() {
  const t = MEETING_TABLE;
  return (
    <>
      {SEATS.filter((s) => s.kind === 'hot').map((s) => <Chair key={s.at.x} x={s.at.x} y={s.at.y} />)}
      <Sprite x={t.x + t.w / 2} y={t.y + t.h} w={t.w + 10} h={66}>
        <rect x={0} y={14} width={t.w + 10} height={30} rx={14} fill="#B98A5E" />
        <rect x={4} y={16} width={t.w + 2} height={6} rx={3} fill="#D3A777" />
        <rect x={0} y={38} width={t.w + 10} height={10} rx={5} fill="#946A43" />
        <rect x={20} y={48} width={8} height={18} fill="#6E4E33" />
        <rect x={t.w - 18} y={48} width={8} height={18} fill="#6E4E33" />
        <text x={(t.w + 10) / 2} y={32} textAnchor="middle" fontSize={9} fill="#6E4E33" letterSpacing={2}>HOT DESK</text>
      </Sprite>
    </>
  );
}

function Sofa() {
  const s = SOFA;
  return (
    <Sprite x={s.x + s.w / 2} y={s.y + s.h} w={s.w + 10} h={66}>
      <rect x={10} y={4} width={s.w - 10} height={36} rx={12} fill="#7C5CBF" />
      <rect x={0} y={24} width={22} height={40} rx={10} fill="#6A4BAA" />
      <rect x={s.w - 12} y={24} width={22} height={40} rx={10} fill="#6A4BAA" />
      <rect x={18} y={34} width={s.w - 26} height={26} rx={8} fill="#8E70D2" />
      <path d={`M${s.w / 3 + 6} 36 V58 M${(2 * s.w) / 3 + 2} 36 V58`} stroke="#7C5CBF" strokeWidth={2} />
      <rect x={30} y={14} width={22} height={18} rx={6} fill="#FFB020" transform="rotate(-10 41 23)" />
    </Sprite>
  );
}

function CoffeeTable() {
  const t = COFFEE_TABLE;
  return (
    <Sprite x={t.x + t.w / 2} y={t.y + t.h} w={t.w + 8} h={44}>
      <rect x={0} y={8} width={t.w + 8} height={20} rx={8} fill="#5E4B3C" />
      <rect x={3} y={9} width={t.w + 2} height={6} rx={3} fill="#7A6250" />
      <rect x={8} y={28} width={5} height={16} fill="#43352B" />
      <rect x={t.w - 5} y={28} width={5} height={16} fill="#43352B" />
      <rect x={18} y={2} width={22} height={8} rx={1} fill="#4C8DF6" />
      <rect x={52} y={0} width={8} height={10} rx={2} fill="#F4F1FF" />
    </Sprite>
  );
}

function BeanBag({ x, y, color }: { x: number; y: number; color: string }) {
  return (
    <Sprite x={x} y={y + 6} w={56} h={34} z={y - 1}>
      <ellipse cx={28} cy={20} rx={27} ry={14} fill={color} />
      <ellipse cx={24} cy={14} rx={14} ry={6} fill="#FFFFFF" opacity={0.15} />
    </Sprite>
  );
}

function Counter({ steaming }: { steaming: boolean }) {
  const c = COUNTER;
  return (
    <Sprite x={c.x + c.w / 2} y={c.y + c.h} w={c.w + 6} h={92}>
      <rect x={0} y={40} width={c.w + 6} height={52} rx={3} fill="#E3E6EE" />
      <rect x={0} y={36} width={c.w + 6} height={8} rx={2} fill="#B9BFCC" />
      <path d={`M${(c.w + 6) / 2} 46 V88`} stroke="#B9BFCC" strokeWidth={2} />
      {/* máy pha cà phê */}
      <rect x={12} y={4} width={34} height={34} rx={4} fill="#2B2F3A" />
      <rect x={16} y={8} width={26} height={8} rx={2} fill="#4A5163" />
      <circle cx={36} cy={12} r={2} fill={steaming ? '#34D1B5' : '#FF6B6B'} />
      <rect x={24} y={24} width={10} height={10} rx={2} fill="#F4F1FF" />
      {steaming && (
        <g className="steam" fill="none" stroke="#F4F1FF" strokeWidth={1.6} strokeLinecap="round" opacity={0.7}>
          <path className="steam__puff" d="M27 20 q-3 -5 0 -9 q3 -4 0 -9" />
          <path className="steam__puff steam__puff--late" d="M32 20 q3 -5 0 -9 q-3 -4 0 -9" />
        </g>
      )}
      {/* lọ bánh, chậu hoa */}
      <rect x={60} y={20} width={16} height={18} rx={4} fill="#FFD27A" opacity={0.85} />
      <circle cx={92} cy={24} r={8} fill="#5BBF7A" />
      <rect x={87} y={28} width={10} height={10} rx={2} fill="#C46A4A" />
    </Sprite>
  );
}

function Cooler() {
  const c = COOLER;
  return (
    <Sprite x={c.x + c.w / 2} y={c.y + c.h} w={c.w + 4} h={78}>
      <rect x={6} y={0} width={c.w - 8} height={26} rx={8} fill="#7CC4F2" opacity={0.85} />
      <rect x={10} y={4} width={6} height={18} rx={3} fill="#FFFFFF" opacity={0.4} />
      <rect x={2} y={26} width={c.w} height={52} rx={4} fill="#E3E6EE" />
      <rect x={10} y={36} width={8} height={5} rx={1} fill="#4C8DF6" />
      <rect x={20} y={36} width={8} height={5} rx={1} fill="#FF6B6B" />
    </Sprite>
  );
}

function PantryTable() {
  const t = PANTRY_TABLE;
  return (
    <Sprite x={t.x + t.w / 2} y={t.y + t.h} w={t.w + 6} h={52}>
      <ellipse cx={(t.w + 6) / 2} cy={14} rx={(t.w + 6) / 2} ry={13} fill="#E9E4D6" />
      <ellipse cx={(t.w + 6) / 2} cy={12} rx={(t.w + 6) / 2 - 3} ry={10} fill="#F6F1E3" />
      <rect x={(t.w + 6) / 2 - 3} y={26} width={6} height={22} fill="#9AA3B5" />
      <ellipse cx={(t.w + 6) / 2} cy={49} rx={14} ry={3} fill="#9AA3B5" />
      <rect x={18} y={4} width={8} height={9} rx={2} fill="#FF7A59" />
      <ellipse cx={44} cy={10} rx={8} ry={3.5} fill="#FFD27A" />
    </Sprite>
  );
}

function Fridge() {
  const f = FRIDGE;
  return (
    <Sprite x={f.x + f.w / 2} y={f.y + f.h} w={f.w + 4} h={110}>
      <rect x={0} y={0} width={f.w + 4} height={110} rx={5} fill="#DCE2EC" />
      <path d={`M0 40 H${f.w + 4}`} stroke="#B9BFCC" strokeWidth={2} />
      <rect x={4} y={14} width={3} height={18} rx={1.5} fill="#9AA3B5" />
      <rect x={4} y={50} width={3} height={24} rx={1.5} fill="#9AA3B5" />
      <rect x={18} y={56} width={12} height={10} fill="#FFB020" transform="rotate(8 24 61)" />
      <circle cx={24} cy={20} r={4} fill="#FF6B6B" />
    </Sprite>
  );
}

function Whiteboard() {
  const b = WHITEBOARD;
  return (
    <Sprite x={b.x + b.w / 2} y={b.y + b.h} w={b.w + 4} h={92}>
      <rect x={10} y={66} width={4} height={26} fill="#6E7B96" />
      <rect x={b.w - 10} y={66} width={4} height={26} fill="#6E7B96" />
      <rect x={0} y={0} width={b.w + 4} height={68} rx={3} fill="#9AA3B5" />
      <rect x={4} y={4} width={b.w - 4} height={60} rx={2} fill="#FAFBFF" />
      <rect x={12} y={12} width={26} height={14} rx={2} fill="none" stroke="#4C8DF6" strokeWidth={1.5} />
      <rect x={58} y={12} width={26} height={14} rx={2} fill="none" stroke="#4C8DF6" strokeWidth={1.5} />
      <path d="M38 19 H58 M53 15 L58 19 L53 23" fill="none" stroke="#FF7A59" strokeWidth={1.5} />
      <rect x={34} y={38} width={30} height={14} rx={2} fill="none" stroke="#34D1B5" strokeWidth={1.5} />
      <path d="M71 26 V36 L64 44" fill="none" stroke="#FF7A59" strokeWidth={1.5} />
      {/* con mèo ai đó vẽ */}
      <path d="M96 46 l3 -6 l3 5 h4 l3 -5 l3 6 q2 10 -8 10 q-10 0 -8 -10" fill="none" stroke="#8B7CF6" strokeWidth={1.5} />
      <rect x={20} y={62} width={14} height={3} rx={1} fill="#FF6B6B" />
      <rect x={38} y={62} width={14} height={3} rx={1} fill="#4C8DF6" />
    </Sprite>
  );
}

const BOOK_COLORS = ['#FF7A59', '#4C8DF6', '#34D1B5', '#FFB020', '#8B7CF6', '#E9E4D6', '#FF6B6B'];

function Bookshelf() {
  const s = BOOKSHELF;
  return (
    <Sprite x={s.x + s.w / 2} y={s.y + s.h} w={s.w + 4} h={104}>
      <rect x={0} y={0} width={s.w + 4} height={104} rx={3} fill="#6B4E37" />
      {[0, 1, 2].map((row) => (
        <g key={row}>
          <rect x={5} y={6 + row * 32} width={s.w - 6} height={26} fill="#4A3526" />
          {Array.from({ length: 9 }, (_, i) => (
            <rect
              key={i}
              x={8 + i * 10.5}
              y={10 + row * 32 + ((i + row) % 3)}
              width={8}
              height={22 - ((i + row) % 3)}
              rx={1}
              fill={BOOK_COLORS[(i * 3 + row) % BOOK_COLORS.length]}
            />
          ))}
        </g>
      ))}
    </Sprite>
  );
}

function Printer() {
  const p = PRINTER;
  return (
    <Sprite x={p.x + p.w / 2} y={p.y + p.h} w={p.w + 4} h={58}>
      <rect x={2} y={34} width={p.w} height={24} rx={2} fill="#4A5163" />
      <rect x={0} y={14} width={p.w + 4} height={24} rx={4} fill="#DCE2EC" />
      <rect x={8} y={4} width={p.w - 12} height={12} fill="#FAFBFF" />
      <rect x={8} y={30} width={p.w - 12} height={3} fill="#2B2F3A" />
      <circle cx={p.w - 4} cy={22} r={2} fill="#34D1B5" />
    </Sprite>
  );
}

function Plant({ x, y, tall }: { x: number; y: number; tall: boolean }) {
  return (
    <Sprite x={x} y={y} w={40} h={tall ? 70 : 54}>
      <g fill="#4FAF6E">
        <path d={tall ? 'M20 46 Q2 30 8 4 Q18 22 20 46' : 'M20 32 Q4 22 8 4 Q18 16 20 32'} />
        <path d={tall ? 'M20 46 Q38 28 32 0 Q22 20 20 46' : 'M20 32 Q36 20 32 2 Q22 14 20 32'} />
      </g>
      <path d={tall ? 'M20 46 Q12 26 20 12 Q28 26 20 46' : 'M20 32 Q14 20 20 10 Q26 20 20 32'} fill="#6BCB8A" />
      <path d={`M9 ${tall ? 46 : 32} H31 L28 ${tall ? 70 : 54} H12 Z`} fill="#C46A4A" />
      <rect x={8} y={tall ? 44 : 30} width={24} height={5} rx={2} fill="#D98A68" />
    </Sprite>
  );
}

/** Mọi món đồ trên sàn. `desks[i]` là trạng thái bàn thứ i (theo `SEATS`). */
export function Furniture({ desks, steaming }: { desks: readonly DeskState[]; steaming: boolean }) {
  return (
    <>
      {SEATS.map((s, i) => (s.kind === 'desk' ? <Desk key={i} i={i} state={desks[i] ?? EMPTY_DESK} /> : null))}
      <MeetingTable />
      <Sofa />
      <CoffeeTable />
      <BeanBag x={NAP_SPOTS[3]?.x ?? 742} y={NAP_SPOTS[3]?.y ?? 318} color="#FF7A59" />
      <BeanBag x={NAP_SPOTS[4]?.x ?? 952} y={NAP_SPOTS[4]?.y ?? 318} color="#34D1B5" />
      <Counter steaming={steaming} />
      <Cooler />
      <PantryTable />
      <Fridge />
      <Whiteboard />
      <Bookshelf />
      <Printer />
      {PLANTS.map((p, i) => <Plant key={i} x={p.x} y={p.y} tall={i % 2 === 0} />)}
    </>
  );
}

const EMPTY_DESK: DeskState = { mood: null, typing: false };
