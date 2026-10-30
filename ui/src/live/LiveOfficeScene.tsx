// Cảnh "văn phòng sống" (docs/OFFICE_LIVE.md): agent rảnh đi dạo, tán gẫu; có việc thì chạy về bàn. Theo hợp đồng cảnh
// (components/scenes.tsx): chỉ nhận dữ liệu + callback, không đọc store, không gửi lệnh. Dưới sàn vẫn có lưới thẻ chi tiết
// (`DeskGridScene`) để đọc đủ thông tin và dùng bàn phím.
import { useEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { MOOD_LABELS } from '../components/Character';
import { DeskGridScene, type OfficeSceneProps } from '../components/DeskGridScene';
import type { AgentView } from '../protocol';
import { Backdrop, Furniture, type DeskState } from './Furniture';
import { SEATS, STAGE_H, STAGE_W } from './layout';
import { busy, doing, type Actor, type Sim } from './sim';
import { useLiveSim } from './useLiveSim';
import { Walker } from './Walker';
import './LiveOffice.css';

/** Tỉ lệ co sàn 1000 × 620 theo bề rộng khung (ResizeObserver; không có thì giữ 1). */
function useStageScale(ref: RefObject<HTMLDivElement | null>): number {
  const [k, setK] = useState(1);
  useEffect(() => {
    const el = ref.current;
    if (el === null || typeof ResizeObserver === 'undefined') return undefined;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width ?? 0;
      if (w > 0) setK(Math.min(1.5, w / STAGE_W));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return k;
}

function deskStates(sim: Sim): DeskState[] {
  return sim.seats.map((id) => {
    const a = id === null ? undefined : sim.actors.get(id);
    const seated = a !== undefined && a.task?.k === 'desk' && a.pose !== 'walk';
    return {
      mood: seated ? a.mood : null,
      typing: seated && a.mood === 'working',
    };
  });
}

function summary(sim: Sim): string {
  let work = 0;
  let chat = 0;
  let walk = 0;
  let nap = 0;
  for (const a of sim.actors.values()) {
    if (busy(a.mood)) work++;
    else if (a.task?.k === 'chat') chat++;
    else if (a.task?.k === 'nap' && a.pose !== 'walk') nap++;
    else walk++;
  }
  const parts = [
    work > 0 ? `${work} đang làm` : null,
    chat > 0 ? `${chat} đang tán gẫu` : null,
    walk > 0 ? `${walk} đi dạo` : null,
    nap > 0 ? `${nap} ngủ gật` : null,
  ].filter((p) => p !== null);
  return parts.length === 0 ? 'Văn phòng trống' : parts.join(' · ');
}

export function LiveOfficeScene(props: OfficeSceneProps) {
  const { desks, support, canSelect, onSelect, motion = true } = props;
  const agents: AgentView[] = [...desks, ...support];
  const byId = new Map(agents.map((a) => [a.id, a]));
  const { sim, wake } = useLiveSim(agents, motion);
  const viewport = useRef<HTMLDivElement>(null);
  const k = useStageScale(viewport);
  const actors = [...sim.actors.values()];
  const napping = motion && actors.some((a) => a.task?.k === 'nap' && !busy(a.mood));
  const steaming = actors.some((a) => a.task?.k === 'stroll' && a.task.poi === 'coffee' && a.pose !== 'walk') ||
    [...sim.convs.values()].some((c) => c.poi === 0);
  const nameOf = (id: string) => byId.get(id)?.name ?? '…';
  // Office vẽ lại mỗi 250 ms khi có yêu cầu chờ (đồng hồ đếm ngược): nền và đồ đạc chỉ dựng lại khi thật sự đổi.
  const minute = Math.floor(Date.now() / 60_000);
  const backdrop = useMemo(() => <Backdrop date={new Date(minute * 60_000)} />, [minute]);
  const desksNow = deskStates(sim);
  const deskKey = desksNow.map((d) => `${d.mood ?? '-'}${d.typing ? 1 : 0}`).join(',');
  // `deskKey` tóm đủ nội dung của `desksNow`.
  const furniture = useMemo(() => <Furniture desks={desksNow} steaming={steaming} />, [deskKey, steaming]);

  return (
    <div className="live-scene">
      <section className="live" aria-labelledby="live-heading">
        <div className="live__bar">
          <h2 id="live-heading">Sàn văn phòng</h2>
          <p className="muted small" role="status">{summary(sim)}</p>
          {napping && (
            <button type="button" className="live__wake" onClick={wake}>
              Giải lao tiếp ☕
            </button>
          )}
        </div>
        <div className="live__viewport" ref={viewport} style={{ height: STAGE_H * k }}>
          <div className="live__stage" style={{ transform: `scale(${k})`, width: STAGE_W, height: STAGE_H }}>
            {backdrop}
            {furniture}
            {actors.map((a) => {
              const agent = byId.get(a.id);
              return agent === undefined ? null : (
                <ActorSprite
                  key={a.id}
                  actor={a}
                  agent={agent}
                  doingText={doing(sim, a, nameOf)}
                  onSelect={canSelect(agent) ? () => onSelect(agent.id) : undefined}
                />
              );
            })}
            {sim.overflow > 0 && <p className="live__overflow">+{sim.overflow} agent khác (xem thẻ bên dưới)</p>}
            {actors.length === 0 && (
              <p className="live__empty">Đang chờ agent đầu tiên bước vào từ cửa…</p>
            )}
          </div>
        </div>
      </section>
      <DeskGridScene {...props} />
    </div>
  );
}

function ActorSprite({ actor: a, agent, doingText, onSelect }: {
  actor: Actor;
  agent: AgentView;
  doingText: string;
  onSelect: (() => void) | undefined;
}) {
  const label = `${agent.name}: ${MOOD_LABELS[agent.mood]}, ${doingText}`;
  const position = {
    transform: `translate(${a.x}px, ${a.y}px)`,
    transitionDuration: `${a.legMs}ms`,
  };
  // Ngồi ở bàn/hot desk thì thẻ tên nằm trên mặt bàn (bàn che chân và chỗ thẻ tên thường nằm).
  const seat = a.seat === null ? undefined : SEATS[a.seat];
  const seated = a.pose !== 'walk' && seat !== undefined && a.x === seat.at.x && a.y === seat.at.y;
  return (
    <>
      <div
        className="actor"
        data-agent-id={a.id}
        data-pose={a.pose}
        data-small={a.small}
        style={{ ...position, zIndex: Math.round(a.y) }}
      >
        <div className="actor__body" data-facing={a.facing}>
          <Walker mood={agent.mood} pose={a.pose} palette={agent.palette} label={label} />
        </div>
      </div>
      {/* Lớp phủ: bong bóng và tên luôn nằm trên đồ đạc, đi cùng nhân vật. */}
      <div className="actor actor--overlay" data-pose={a.pose} data-seated={seated} style={position}>
        {a.bubble !== null && a.bubble.kind === 'alert' ? (
          onSelect ? (
            <button
              type="button"
              className="bubble bubble--alert"
              onClick={onSelect}
              aria-label={`${agent.name} đang giơ tay: mở thẻ duyệt`}
            >
              !
            </button>
          ) : (
            <span className="bubble bubble--alert" aria-hidden="true">!</span>
          )
        ) : a.bubble !== null ? (
          <span key={a.bubble.text} className={`bubble bubble--${a.bubble.kind}`}>{a.bubble.text}</span>
        ) : null}
        <span className="actor__name" aria-hidden="true">{agent.name}</span>
      </div>
    </>
  );
}
