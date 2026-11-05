// Cảnh "Tòa nhà Arcturus" (docs/OFFICE_BUILDING.md, ADR 0003): vỏ DOM nhẹ trong bundle chính. Phần three.js nạp muộn ở
// `three/BuildingCanvas`. Theo hợp đồng cảnh (components/scenes.tsx): chỉ nhận dữ liệu + callback, KHÔNG gửi thông điệp nào lên hub;
// `onSelect` chỉ được gọi sau thao tác của người dùng (bấm nút, bấm hình trong canvas), không bao giờ tự gọi.
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type ComponentType, type KeyboardEvent, type ReactNode } from 'react';
import { DeskGridScene, type OfficeSceneProps } from '../components/DeskGridScene';
import { LiveOfficeScene } from '../live/LiveOfficeScene';
import { SceneBoundary } from '../components/SceneBoundary';
import { MOOD_LABELS } from '../components/Character';
import { cleanText } from '../pet/brain';
import type { AgentView } from '../protocol';
import { hasWebGL } from '../webgl';
import './Building.css';
import { DEFAULT_VIEW, viewName, type View } from './camera';
import { CEO_FLOOR, FLOORS, roleShort, type FloorId } from './floors';
import type { BuildingCanvasProps, BuildingModel, PickHit } from './model';
import { timeModeAt } from './model';
import { summaryBubble } from './secretary';
import { assignSeats, EMPTY_SEATS, rosters as buildRosters, type FloorRoster, type SeatMap } from './seating';
import { useSecretary } from './useSecretary';

const LazyCanvas = lazy(() => import('./three/BuildingCanvas'));
const FOLLOW_KEY = 'arcturus.building.follow';

function loadFollow(): boolean {
  try {
    return window.localStorage.getItem(FOLLOW_KEY) === '1';
  } catch {
    return false;
  }
}
function saveFollow(on: boolean): void {
  try {
    window.localStorage.setItem(FOLLOW_KEY, on ? '1' : '0');
  } catch {
    // Không lưu được thì lần sau tắt (mặc định).
  }
}

/** Giữ nguyên tham chiếu mảng khi các phần tử không đổi (Office dựng mảng mới ở mỗi lần render). */
function useStableList<T>(next: readonly T[]): readonly T[] {
  const ref = useRef<readonly T[]>(next);
  if (ref.current.length !== next.length || ref.current.some((x, i) => x !== next[i])) ref.current = next;
  return ref.current;
}

export interface BuildingSceneProps extends OfficeSceneProps {
  /** Bộ vẽ 3D (mặc định nạp muộn); test thay bằng bản giả. */
  Canvas?: ComponentType<BuildingCanvasProps>;
  /** Có WebGL không (mặc định tự kiểm). */
  webgl?: boolean;
}

interface Tip {
  lines: string[];
  x: number;
  y: number;
}

const floorLabel = (f: FloorId) => (f === CEO_FLOOR ? 'Phòng CEO' : (FLOORS[f]?.name ?? ''));
const accentVar = (f: FloorId) => `var(--floor-${FLOORS[f]?.key ?? 'lobby'})`;

function shortCwd(cwd: string): string {
  return cleanText(cwd, 40);
}

export function BuildingScene(props: BuildingSceneProps) {
  const { webgl } = props;
  const canWebGL = useMemo(() => webgl ?? hasWebGL(), [webgl]);
  const [failed, setFailed] = useState<string | null>(null);
  if (!canWebGL || failed !== null) {
    return (
      <div className="live-scene">
        <p className="muted building__note" role="status">Máy này không dựng được cảnh 3D, đang hiện văn phòng 2D.</p>
        <SceneBoundary fallback={<DeskGridScene {...props} />}>
          <LiveOfficeScene {...props} />
        </SceneBoundary>
      </div>
    );
  }
  return <BuildingSceneInner {...props} onFail={setFailed} />;
}

function BuildingSceneInner({ desks, support, canSelect, onSelect, motion = true, brief, Canvas = LazyCanvas, onFail }: BuildingSceneProps & { onFail: (message: string) => void }) {
  const [view, setView] = useState<View>(DEFAULT_VIEW);
  const [follow, setFollow] = useState(loadFollow);
  const [ready, setReady] = useState(false);
  const [tip, setTip] = useState<Tip | null>(null);

  const agents = useStableList(useMemo(() => [...desks, ...support], [desks, support]));
  const seats = useRef<SeatMap>(EMPTY_SEATS);
  const rosters: readonly FloorRoster[] = useMemo(() => {
    seats.current = assignSeats(seats.current, agents);
    return buildRosters(seats.current, agents, brief, Date.now());
  }, [agents, brief]);

  const { mem, say } = useSecretary({ rosters, brief, canSelect, motion });

  // "Theo dõi thư ký": chuyến mới (rank 0–2) thì chuyển góc nhìn theo; về nhà thì về Phòng CEO.
  const lastTrip = useRef<string | null>(null);
  useEffect(() => {
    const s = mem.state;
    const id = s.k === 'moving' ? `m${s.startAt}` : s.k === 'briefing' ? `b${s.since}:${s.goal.key}` : s.k;
    if (lastTrip.current === id) return;
    const before = lastTrip.current;
    lastTrip.current = id;
    if (!follow) return;
    if (s.k === 'moving') setView(s.goal === null ? CEO_FLOOR : s.goal.rank <= 2 ? s.goal.floor : view);
    else if (s.k === 'briefing' && s.goal.rank <= 2) setView(s.goal.floor);
    else if (s.k === 'home' && before !== null && before !== 'home' && before !== 'asleep') setView(CEO_FLOOR);
  }, [mem.state, follow]);

  const toggleFollow = useCallback((on: boolean) => {
    setFollow(on);
    saveFollow(on);
  }, []);

  const selectable = useMemo(() => new Set(agents.filter(canSelect).map((a) => a.id)), [agents, canSelect]);
  const model: BuildingModel = useMemo(
    () => ({
      view,
      rosters,
      secretary: { mem, startAt: mem.state.k === 'moving' ? mem.state.startAt : 0 },
      selectable,
      pending: brief?.pending ?? new Map(),
      timeMode: timeModeAt(new Date()),
    }),
    [view, rosters, mem, selectable, brief],
  );

  const secFloor: FloorId = mem.state.k === 'briefing' ? mem.state.goal.floor : mem.state.k === 'moving' ? (mem.state.goal?.floor ?? CEO_FLOOR) : CEO_FLOOR;
  const byId = useMemo(() => new Map(agents.map((a) => [a.id, a] as const)), [agents]);

  // --- chọn tầng / chọn agent ---
  const goTo = useCallback((v: View) => {
    setView(v);
    setTip(null);
  }, []);

  const onPick = useCallback(
    (hit: PickHit) => {
      if (hit.k === 'floor') goTo(hit.floor);
      else if (hit.k === 'status-screen') goTo(hit.floor);
      else if (hit.k === 'secretary') say(summaryBubble(rosters, Date.now()));
      else {
        const a = byId.get(hit.agentId);
        if (a !== undefined && selectable.has(a.id)) onSelect(a.id);
      }
    },
    [goTo, say, rosters, byId, selectable, onSelect],
  );

  const onHover = useCallback(
    (hit: PickHit | null, x: number, y: number) => {
      if (hit === null) {
        setTip(null);
        return;
      }
      setTip({ lines: tooltipLines(hit, rosters, byId, selectable, mem.state.k), x, y });
    },
    [rosters, byId, selectable, mem.state.k],
  );

  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    const el = e.target as HTMLElement;
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable) return;
    const k = e.key.toLowerCase();
    if (k >= '0' && k <= '5' && k.length === 1) goTo(Number(k) as FloorId);
    else if (k === 'e') goTo('ext');
    else if (k === 'r') goTo(DEFAULT_VIEW);
    else if (k === 'f') toggleFollow(!follow);
  };

  // --- thẻ chi tiết dưới canvas ---
  const detail: readonly AgentView[] = useMemo(() => {
    if (typeof view === 'number' && view !== CEO_FLOOR) return rosters[view]?.figures.map((f) => f.agent) ?? [];
    return agents.filter((a) => a.mood === 'approval' || a.mood === 'error' || brief?.pending.has(a.id) === true || brief?.handoffs.has(a.id) === true);
  }, [view, rosters, agents, brief]);

  const bubble = mem.bubble;
  const secLine = mem.state.k === 'moving' ? (mem.state.goal === null ? 'Đang về Phòng CEO.' : `Đang đi tới tầng ${mem.state.goal.floor}.`) : mem.state.k === 'briefing' ? `Đang ở tầng ${mem.state.goal.floor}.` : mem.state.k === 'asleep' ? 'Đang gục đầu nghỉ ở Phòng CEO.' : 'Đang ở Phòng CEO, chưa có việc chờ.';

  return (
    <section className="building" tabIndex={0} aria-label={`Tòa nhà Arcturus, đang xem ${viewName(view) === 'ngoại cảnh' ? 'ngoại cảnh' : floorLabel(view as FloorId)}. Phím 0–5 chọn tầng, E ngoại cảnh.`} onKeyDown={onKeyDown}>
      <div className="building__bar" role="toolbar" aria-label="Điều khiển tòa nhà">
        <button type="button" aria-pressed={view === CEO_FLOOR} onClick={() => goTo(CEO_FLOOR)}>Phòng CEO (5)</button>
        <button type="button" aria-pressed={view === 'ext'} onClick={() => goTo('ext')}>Ngoại cảnh (E)</button>
        <button type="button" onClick={() => goTo(DEFAULT_VIEW)}>Đặt lại góc nhìn</button>
        <label className="building__follow">
          <input type="checkbox" checked={follow} onChange={(e) => toggleFollow(e.target.checked)} />
          Theo dõi thư ký
        </label>
      </div>

      <p className="building__sec" role="status" aria-live="polite" data-idle={bubble === null}>
        <b>Thư ký</b>
        {bubble === null ? <span>{secLine}</span> : <span>{bubble.text}</span>}
        {bubble?.actions.map((a) =>
          a.kind === 'select' ? (
            <button key={`s-${a.agentId}`} type="button" className="building__act" onClick={() => onSelect(a.agentId)}>{a.label}</button>
          ) : (
            <button key={`v-${String(a.view)}`} type="button" className="building__act" onClick={() => goTo(a.view)}>{a.label}</button>
          ),
        )}
        <button type="button" className="building__secbtn" onClick={() => say(summaryBubble(rosters, Date.now()))}>Tóm tắt cả tòa</button>
      </p>

      <div className="building__stage">
        <div className="building__view">
          <div className="building__canvas" aria-hidden="true">
            <SceneBoundary onError={(e) => onFail(e.message)} fallback={null}>
              <Suspense fallback={null}>
                <Canvas model={model} motion={motion} onHover={onHover} onPick={onPick} onReady={() => setReady(true)} onError={onFail} />
              </Suspense>
            </SceneBoundary>
          </div>
          {!ready && <p className="muted building__loading" role="status">Đang dựng tòa nhà…</p>}
          {tip !== null && (
            <div className="building__tip" style={{ left: tip.x + 12, top: tip.y + 12 }}>
              {tip.lines.map((l, i) => <p key={i}>{l}</p>)}
            </div>
          )}
        </div>
        <nav className="building__lifts" aria-label="Bảng thang máy">
          {([5, 4, 3, 2, 1, 0] as const).map((f) => (
            <LiftButton key={f} floor={f} roster={f === CEO_FLOOR ? undefined : rosters[f]} pressed={view === f} secretaryHere={secFloor === f} onClick={() => goTo(f)} />
          ))}
          <button type="button" className="building__lift" aria-pressed={view === 'ext'} onClick={() => goTo('ext')}>
            <b>Ngoại cảnh</b>
            <small>Cả tòa nhà</small>
          </button>
        </nav>
      </div>

      <DeskGridScene desks={detail} support={[]} canSelect={canSelect} onSelect={onSelect} />
    </section>
  );
}

function LiftButton({ floor, roster, pressed, secretaryHere, onClick }: { floor: FloorId; roster: FloorRoster | undefined; pressed: boolean; secretaryHere: boolean; onClick: () => void }): ReactNode {
  const def = FLOORS[floor];
  const style = { ['--accent' as string]: accentVar(floor) };
  return (
    <button type="button" className="building__lift" style={style} aria-pressed={pressed} onClick={onClick}>
      <b>{floor} · {def?.name}{secretaryHere && <span className="building__here" title="Thư ký đang ở đây"> ●</span>}</b>
      {floor === CEO_FLOOR ? (
        <small>thư ký · bàn CEO</small>
      ) : (
        <>
          <small>{def?.plugin ?? 'phiên chính · khách'}</small>
          <small>▶ {roster?.stats.busy ?? 0} · ! {roster?.stats.raised ?? 0}{(roster?.overflow ?? 0) > 0 ? ` · +${roster?.overflow}` : ''}</small>
        </>
      )}
    </button>
  );
}

function tooltipLines(hit: PickHit, rosters: readonly FloorRoster[], byId: ReadonlyMap<string, AgentView>, selectable: ReadonlySet<string>, secState: string): string[] {
  switch (hit.k) {
    case 'floor': {
      const def = FLOORS[hit.floor];
      if (hit.floor === CEO_FLOOR) return ['Tầng 5 · Phòng CEO · bàn CEO và thư ký'];
      const r = rosters[hit.floor];
      const seatsText = `${r?.stats.present ?? 0} có người`;
      return [`Tầng ${hit.floor} · ${def?.name ?? ''} · ${def?.plugin ?? 'phiên chính, khách'} · ${seatsText} · ${r?.stats.busy ?? 0} đang làm · ${r?.stats.raised ?? 0} giơ tay`];
    }
    case 'status-screen':
      return [`Tầng ${hit.floor} · ${FLOORS[hit.floor]?.name ?? ''}: bấm để xuống tầng`];
    case 'secretary':
      return [`Thư ký · ${secState === 'moving' ? 'đang đi' : secState === 'briefing' ? 'đang báo cáo' : secState === 'asleep' ? 'đang nghỉ' : 'ở bàn chỉ huy'}`, 'Bấm để nghe tóm tắt cả tòa'];
    case 'agent':
    case 'bang': {
      const a = byId.get(hit.agentId);
      if (a === undefined) return [];
      const fig = rosters.flatMap((r) => r.figures).find((f) => f.agent.id === a.id);
      const role = fig === undefined ? '' : (roleShort(fig.placement.floor, fig.placement.roleId) ?? cleanText(a.role, 40));
      const lines = [`${cleanText(a.name, 32)}${role === '' ? '' : ` · ${cleanText(role, 40)}`} · ${MOOD_LABELS[a.mood]}`];
      if (a.act_text !== '') lines.push(cleanText(a.act_text, 80));
      if (a.cwd !== '') lines.push(shortCwd(a.cwd));
      if (selectable.has(a.id)) lines.push('Bấm để mở thẻ duyệt');
      return lines;
    }
  }
}
