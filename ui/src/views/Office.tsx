import { useEffect, useRef, useState } from 'react';
import { QueueList } from '../components/QueueList';
import { DEFAULT_SCENE, isSceneId, OFFICE_SCENES, SCENE_LABELS, type SceneId } from '../components/scenes';
import { SuggestionList } from '../components/SuggestionList';
import { TeamControls } from '../components/TeamControls';
import { approvalInput } from '../hub';
import { officeRoster } from '../office';
import { DEFAULT_MIN_REVIEW_MS, useApprovalQueue } from '../useApprovalQueue';
import { useHubConnection } from '../useHubConnection';
import { useMotionPolicy } from '../useMotionPolicy';
import type { LitePopupViewProps } from './LitePopup';
import './Office.css';

const SCENE_KEY = 'arcturus.office.scene';

/** Cảnh đã chọn lần trước (chỉ là tiện ích cho người xem: lỗi hoặc không có storage thì dùng mặc định). */
function loadScene(): SceneId {
  try {
    const saved = window.localStorage.getItem(SCENE_KEY);
    return isSceneId(saved) ? saved : DEFAULT_SCENE;
  } catch {
    return DEFAULT_SCENE;
  }
}

function saveScene(id: SceneId): void {
  try {
    window.localStorage.setItem(SCENE_KEY, id);
  } catch {
    // Không lưu được thì lần sau dùng mặc định.
  }
}

export function OfficeView({ state, send, loadFull, minReviewMs }: LitePopupViewProps) {
  const { now, selected, setSelectedId, gateOf, decide, decideGroup, setReviewed, notice, countdownOf, suggestions, answerSuggestion } =
    useApprovalQueue(state, send, minReviewMs);
  const [sceneId, setSceneId] = useState<SceneId>(loadScene);
  const Scene = OFFICE_SCENES[sceneId];
  const motion = useMotionPolicy();
  const { desks, support } = officeRoster(Object.values(state.agents), state.team.roster_max);
  const queueRef = useRef<HTMLElement>(null);
  const [hidden, setHidden] = useState(() => document.hidden);
  useEffect(() => {
    const visibility = () => setHidden(document.hidden);
    document.addEventListener('visibilitychange', visibility);
    return () => document.removeEventListener('visibilitychange', visibility);
  }, []);

  const selectAgent = (id: string) => {
    const request = state.queue.find((a) => a.agent_id === id);
    const handoff = state.handoffs.find((h) => h.agent_id === id);
    if (request) setSelectedId(request.request_id);
    // Compare data attributes directly: ids are data, never interpolated into a selector.
    const target = request
      ? [...(queueRef.current?.querySelectorAll<HTMLElement>('[data-request-id]') ?? [])]
        .find((el) => el.dataset.requestId === request.request_id)
      : [...(queueRef.current?.querySelectorAll<HTMLElement>('[data-handoff-id]') ?? [])]
        .find((el) => el.dataset.handoffId === handoff?.handoff_id);
    target?.scrollIntoView({ block: 'nearest', behavior: 'instant' });
    target?.focus({ preventScroll: true });
  };

  return (
    <main className="office" data-hidden={hidden}>
      <header className="office__head">
        <div>
          <p className="office__brand">ARCTURUS</p>
          <h1>Văn phòng</h1>
        </div>
        <div className="office__scenes" role="group" aria-label="Kiểu văn phòng">
          {(Object.keys(OFFICE_SCENES) as SceneId[]).map((id) => (
            <button
              key={id}
              type="button"
              aria-pressed={sceneId === id}
              onClick={() => {
                setSceneId(id);
                saveScene(id);
              }}
            >
              {SCENE_LABELS[id]}
            </button>
          ))}
        </div>
        <p className="muted" role="status">
          {Object.keys(state.agents).length} agent · {state.queue.length} yêu cầu đang chờ
        </p>
      </header>
      <TeamControls team={state.team} connected={state.connection === 'open' && state.synced} send={send} />
      {(state.connection !== 'open' || !state.synced) && (
        <p className="banner banner--error" role="alert">{state.error ?? 'Đang đồng bộ với hub…'}</p>
      )}
      {state.team.paused && (
        <p className="banner banner--paused">Đội đang tạm dừng: không có gì được tự duyệt.</p>
      )}
      {notice && <p className="banner banner--error" role="alert">{notice}</p>}
      {state.feed[0] && <p className="banner" role="status">{state.feed[0].text}</p>}
      <div className="office__layout">
        <Scene
          desks={desks}
          support={support}
          canSelect={(a) =>
            a.mood === 'approval' &&
            (state.queue.some((q) => q.agent_id === a.id) || state.handoffs.some((h) => h.agent_id === a.id))
          }
          onSelect={selectAgent}
          motion={motion}
        />
        <aside className="office__queue" ref={queueRef} aria-labelledby="queue-heading">
          <h2 id="queue-heading">Hàng đợi duyệt</h2>
          {state.queue.length === 0 && state.handoffs.length === 0 && (
            <p className="muted">Không có yêu cầu nào đang chờ.</p>
          )}
          <SuggestionList suggestions={suggestions} onAnswer={answerSuggestion} />
          <QueueList
            queue={state.queue}
            handoffs={state.handoffs}
            agents={state.agents}
            selectedId={selected?.request_id ?? null}
            now={now}
            gateOf={gateOf}
            countdownOf={countdownOf}
            onSelect={setSelectedId}
            onDecide={decide}
            onDecideGroup={decideGroup}
            loadFull={loadFull}
            onReviewed={(id) => setReviewed((r) => ({ ...r, [id]: true }))}
          />
        </aside>
      </div>
    </main>
  );
}

export function Office() {
  const { state, send } = useHubConnection();
  return <OfficeView state={state} send={send} loadFull={approvalInput} minReviewMs={DEFAULT_MIN_REVIEW_MS} />;
}
