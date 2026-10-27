// Cửa sổ trợ lý pet (docs/PLAN.md §7, docs/adr/0001-pet-v1.md). Là client của hub như popup:
// - Không tự duyệt, không đếm ngược phủ quyết (`useApprovalQueue({ countdown: false })`), không duyệt gộp/batch/đề xuất tin
//   cậy. Đồng ý từ bong bóng chỉ khi lệnh hiện ĐẦY ĐỦ trong bong bóng, qua `allowGate` với mốc thời gian tính từ lúc bong bóng
//   hiện. Critical, lệnh dài, lệnh đã cắt: chỉ có "Mở hàng đợi" (và Từ chối, trừ Critical).
// - Lời nói là văn bản mẫu (`brain.ts`); lệnh gõ chỉ trong danh sách cố định (`commands.ts`).
// - Không có model hoặc không có WebGL: nhân vật SVG 2D. Đóng cửa sổ thì app hủy cả webview.
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type ComponentType } from 'react';
import { allowGate, RISK_LABELS, SUSPICION_LABELS } from '../components/ApprovalCard';
import { Character } from '../components/Character';
import { initialMemory, think, SLEEP_AFTER_MS, type BrainMemory, type BrainOutput } from '../pet/brain';
import { helpText, parseCommand, type PetAction, type PetClientMsg } from '../pet/commands';
import { tauriPetApi, type PetApi } from '../pet/petApi';
import type { PetSceneProps } from '../pet/scene/PetScene';
import type { ModelInfo, PetMood, PetSettings } from '../pet/types';
import { useVoice, voiceAvailable } from '../pet/useVoice';
import type { ApprovalView, ClientMsg, Mood } from '../protocol';
import type { HubState } from '../store';
import { splitSpans } from '../text';
import { DEFAULT_MIN_REVIEW_MS, useApprovalQueue } from '../useApprovalQueue';
import { useHubConnection } from '../useHubConnection';
import { useMotionPolicy } from '../useMotionPolicy';
import { useNow } from '../useNow';
import './Pet.css';

const LazyScene = lazy(() => import('../pet/scene/PetScene'));

/** Lệnh dài hơn chừng này không hiện trong bong bóng (không cắt: phải mở hàng đợi, quy tắc bất biến 3). */
export const BUBBLE_MAX_CHARS = 280;
export const BUBBLE_MAX_LINES = 6;
const SAY_MS = 8_000;
const PET_COOLDOWN_MS = 20_000;

export function fitsBubble(a: ApprovalView): boolean {
  return !a.truncated && Array.from(a.command_normalized).length <= BUBBLE_MAX_CHARS && a.command_normalized.split('\n').length <= BUBBLE_MAX_LINES;
}

function hasWebGL(): boolean {
  try {
    const c = document.createElement('canvas');
    return (c.getContext('webgl2') ?? c.getContext('webgl')) !== null;
  } catch {
    return false;
  }
}

/** Tâm trạng pet → mood của nhân vật SVG 2D. */
export function svgMood(m: PetMood): Mood {
  if (m === 'critical') return 'approval';
  if (m === 'paused' || m === 'sleep') return 'idle';
  return m;
}

function modelSummary(m: ModelInfo): string {
  const by = m.authors.length > 0 ? m.authors.join(', ') : 'không rõ tác giả';
  const redist = m.allow_redistribution === null ? 'không ghi' : m.allow_redistribution ? 'cho phép' : 'không cho phép';
  return `Model: ${m.name || m.file_name} (VRM ${m.vrm_version}, ${(m.bytes / 1_048_576).toFixed(1)} MB) · ${by} · giấy phép: ${m.license || 'không ghi'} · thương mại: ${m.commercial_usage || 'không ghi'} · phân phối lại: ${redist}.`;
}

interface Say {
  text: string;
  until: number;
  long?: boolean;
}

export interface PetViewProps {
  state: HubState;
  send: (msg: ClientMsg) => boolean;
  api: PetApi;
  minReviewMs: number;
  /** Bộ vẽ 3D (mặc định nạp muộn); test thay bằng bản giả. */
  Scene?: ComponentType<PetSceneProps>;
  /** Có WebGL không (mặc định tự kiểm). */
  webgl?: boolean;
}

export function PetView({ state, send, api, minReviewMs, Scene = LazyScene, webgl }: PetViewProps) {
  const motion = useMotionPolicy();
  const connected = state.connection === 'open' && state.synced;
  const { decide } = useApprovalQueue(state, send, minReviewMs, { countdown: false });
  const sendPet = useCallback((m: PetClientMsg) => send(m), [send]);

  // --- tùy chọn và model ---
  const [settings, setSettings] = useState<PetSettings | null>(null);
  const [model, setModel] = useState<ArrayBuffer | null>(null);
  const [sceneError, setSceneError] = useState<string | null>(null);
  const canWebGL = useMemo(() => webgl ?? hasWebGL(), [webgl]);
  const loadModel = useCallback(async () => {
    setSceneError(null);
    try {
      const s = await api.settings();
      setSettings(s);
      setModel(s.model !== null ? await api.model() : null);
    } catch (e) {
      setModel(null);
      setSceneError(e instanceof Error ? e.message : 'Không đọc được model');
    }
  }, [api]);
  useEffect(() => {
    void loadModel();
  }, [loadModel]);
  const use3d = canWebGL && model !== null && sceneError === null;
  const reported = useRef(false);
  useEffect(() => {
    if (settings !== null && !use3d && !reported.current) {
      reported.current = true;
      api.ready('svg');
    }
  }, [settings, use3d, api]);

  // --- tầng 0 ---
  const [say, setSay] = useState<Say | null>(null);
  const voice = useVoice(settings?.voice === true);
  const memory = useRef<BrainMemory>(initialMemory(Date.now()));
  const [brain, setBrain] = useState<BrainOutput>(() => think(state, memory.current, Date.now()));
  const [tick, setTick] = useState(0);
  const [napping, setNapping] = useState(false);
  const speak = voice.speak;
  useEffect(() => {
    const out = think(state, memory.current, Date.now());
    memory.current = out.memory;
    setBrain(out);
    if (out.speech !== null) {
      setNapping(false);
      setSay({ text: out.speech.text, until: Date.now() + SAY_MS });
      speak(out.speech.text);
    }
  }, [state, tick, speak]);
  // Một hẹn giờ duy nhất tới lúc ngủ (theo sự kiện, không polling).
  useEffect(() => {
    if (brain.mood === 'sleep') return undefined;
    const wait = memory.current.lastChangeAt + SLEEP_AFTER_MS - Date.now() + 50;
    const t = setTimeout(() => setTick((n) => n + 1), Math.max(1000, wait));
    return () => clearTimeout(t);
  }, [brain]);
  const mood: PetMood = napping && brain.approval === null ? 'sleep' : brain.mood;

  // Ẩn câu nói khi hết giờ (một hẹn giờ cho câu hiện tại).
  useEffect(() => {
    if (say === null) return undefined;
    const t = setTimeout(() => setSay(null), Math.max(0, say.until - Date.now()));
    return () => clearTimeout(t);
  }, [say]);

  // --- bong bóng duyệt ---
  const approval = brain.approval;
  const [shownAt, setShownAt] = useState<Record<string, number>>({});
  useEffect(() => {
    if (approval === null) return;
    setShownAt((prev) => (prev[approval.request_id] !== undefined ? prev : { ...prev, [approval.request_id]: Date.now() }));
  }, [approval]);
  const now = useNow(approval !== null);
  const [dismissed, setDismissed] = useState<string | null>(null);

  // --- menu, hộp lệnh ---
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [cmdOpen, setCmdOpen] = useState(false);
  const [cmd, setCmd] = useState('');
  const body = useRef<HTMLDivElement>(null);

  const status = useCallback((): string => {
    const team = Object.values(state.agents).filter((a) => a.parent_id === null);
    if (!connected) return 'Em chưa kết nối được hub.';
    const working = team.filter((a) => a.mood === 'working' || a.mood === 'approval').length;
    const idle = team.filter((a) => a.mood === 'idle' || a.mood === 'done').length;
    const q = brain.visibleCount;
    const parts = [`${working} agent đang làm`, `${idle} rảnh`];
    if (q > 0) parts.push(`${q} yêu cầu chờ duyệt`);
    if (state.team.paused) parts.push('đội đang tạm dừng');
    if (state.team.focus) parts.push('đang giờ tập trung');
    return `${parts.join(', ')}.`;
  }, [state, connected, brain.visibleCount]);

  const tell = useCallback(
    (text: string, ms = SAY_MS, long = false) => {
      setSay({ text, until: Date.now() + ms, long });
      if (!long) speak(text);
    },
    [speak],
  );

  const run = useCallback(
    async (a: PetAction) => {
      setMenu(null);
      switch (a.kind) {
        case 'send':
          if (!connected) return tell('Chưa kết nối hub, chưa gửi được.');
          tell(sendPet(a.msg) ? a.done : 'Chưa kết nối hub, chưa gửi được.');
          return;
        case 'open_queue':
          return void api.openQueue().catch((e: unknown) => tell(e instanceof Error ? e.message : 'Không mở được hàng đợi'));
        case 'open_office':
          return void api.openOffice().catch(() => tell('Không mở được văn phòng'));
        case 'who_idle': {
          const idle = Object.values(state.agents).filter((x) => x.parent_id === null && (x.mood === 'idle' || x.mood === 'done'));
          return tell(idle.length === 0 ? 'Không ai rảnh cả.' : `Đang rảnh: ${idle.map((x) => x.name).join(', ')}.`);
        }
        case 'standup':
          tell('Em đang tóm tắt…', 4000);
          try {
            const text = await api.standup(24);
            tell(text, 45_000, true);
            speak('Đây là tóm tắt hôm nay.');
          } catch (e) {
            tell(e instanceof Error ? e.message : 'Không lấy được tóm tắt');
          }
          return;
        case 'sleep':
          setNapping(true);
          return tell('Em ngủ một chút nhé. Có yêu cầu duyệt là em dậy.', 3000);
        case 'wake':
          setNapping(false);
          return tell(status());
        case 'hide':
          return void api.hide();
        case 'help':
          return tell(helpText(), 15_000);
      }
    },
    [api, connected, sendPet, state.agents, status, tell, speak],
  );

  const submitCmd = () => {
    const r = parseCommand(cmd);
    setCmd('');
    if (!r.ok) return tell(r.reason);
    setCmdOpen(false);
    void run(r.action);
  };

  const chooseModel = async () => {
    setMenu(null);
    try {
      const info = await api.chooseModel();
      if (info === null) return;
      tell(`Đã nạp ${info.name || info.file_name}. ${info.allow_redistribution === false ? 'Model này không cho phân phối lại.' : ''}`);
      await loadModel();
    } catch (e) {
      tell(e instanceof Error ? e.message : 'Không nạp được model', 12_000);
    }
  };
  const removeModel = async () => {
    setMenu(null);
    await api.removeModel().catch(() => undefined);
    await loadModel();
    tell('Em dùng hình 2D nhé.');
  };
  const toggleVoice = async () => {
    setMenu(null);
    const on = !(settings?.voice ?? false);
    await api.setVoice(on).catch(() => undefined);
    setSettings((s) => (s === null ? s : { ...s, voice: on }));
    tell(on ? 'Em sẽ đọc to lời nhắc.' : 'Em tắt giọng rồi.', 3000);
  };

  // --- tương tác chuột/bàn phím trên nhân vật ---
  const press = useRef<{ x: number; y: number } | null>(null);
  const clickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [pointer, setPointer] = useState<{ x: number; y: number } | null>(null);
  const [reaction, setReaction] = useState(0);
  const rub = useRef({ dist: 0, since: 0, last: 0 });

  const primary = () => {
    if (approval !== null) void run({ kind: 'open_queue' });
    else {
      setNapping(false);
      tell(status());
    }
  };
  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    press.current = { x: e.clientX, y: e.clientY };
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const r = body.current?.getBoundingClientRect();
    if (r !== undefined && r.width > 0) {
      const x = ((e.clientX - r.left) / r.width) * 2 - 1;
      const y = -(((e.clientY - r.top) / r.height) * 2 - 1);
      setPointer((p) => (p !== null && Math.abs(p.x - x) < 0.05 && Math.abs(p.y - y) < 0.05 ? p : { x, y }));
      // Vuốt ve: rê qua lại trên đầu (phần trên của khung).
      const t = Date.now();
      const st = rub.current;
      if (y > 0.35 && press.current === null) {
        if (t - st.since > 1500) {
          st.since = t;
          st.dist = 0;
        }
        st.dist += Math.hypot(e.movementX, e.movementY);
        if (st.dist > 220 && t - st.last > PET_COOLDOWN_MS) {
          st.last = t;
          st.dist = 0;
          setReaction((n) => n + 1);
          setNapping(false);
          tell('Hihi, nhột quá!', 2500);
        }
      }
    }
    const p = press.current;
    if (p !== null && (e.buttons & 1) === 1 && Math.hypot(e.clientX - p.x, e.clientY - p.y) > 4) {
      press.current = null;
      void api.drag().catch(() => undefined);
    }
  };
  const onPointerUp = (e: React.PointerEvent) => {
    if (e.button !== 0 || press.current === null) return;
    press.current = null;
    if (clickTimer.current !== null) clearTimeout(clickTimer.current);
    clickTimer.current = setTimeout(() => {
      clickTimer.current = null;
      primary();
    }, 260);
  };
  const onDoubleClick = () => {
    if (clickTimer.current !== null) clearTimeout(clickTimer.current);
    clickTimer.current = null;
    void run({ kind: 'open_office' });
  };
  useEffect(() => () => {
    if (clickTimer.current !== null) clearTimeout(clickTimer.current);
  }, []);
  const openMenuAt = (x: number, y: number) => setMenu({ x: Math.min(x, 180), y: Math.min(y, 300) });
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      primary();
    } else if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
      e.preventDefault();
      openMenuAt(40, 200);
    } else if (e.key === '/') {
      e.preventDefault();
      setCmdOpen(true);
    }
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      setMenu(null);
      setCmdOpen(false);
      setSay(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  // --- hiển thị ---
  const showApproval = approval !== null && dismissed !== approval.request_id;
  const team = state.team;
  const menuItems: { label: string; act: () => void }[] = [
    { label: 'Mở hàng đợi duyệt', act: () => void run({ kind: 'open_queue' }) },
    { label: 'Mở văn phòng', act: () => void run({ kind: 'open_office' }) },
    {
      label: team.paused ? 'Tiếp tục cả đội' : 'Tạm dừng cả đội',
      act: () => void run({ kind: 'send', msg: team.paused ? { t: 'team_resume' } : { t: 'team_pause' }, done: team.paused ? 'Đội làm việc tiếp.' : 'Đã tạm dừng cả đội.' }),
    },
    {
      label: team.focus ? 'Tắt giờ tập trung' : 'Bật giờ tập trung',
      act: () => void run({ kind: 'send', msg: { t: 'focus_set', on: !team.focus }, done: team.focus ? 'Đã tắt giờ tập trung.' : 'Đã bật giờ tập trung.' }),
    },
    { label: 'Tóm tắt hôm nay', act: () => void run({ kind: 'standup' }) },
    { label: 'Gõ lệnh…', act: () => (setMenu(null), setCmdOpen(true)) },
    { label: mood === 'sleep' ? 'Thức dậy' : 'Ngủ', act: () => void run({ kind: mood === 'sleep' ? 'wake' : 'sleep' }) },
    { label: 'Chọn model…', act: () => void chooseModel() },
    ...(settings?.model != null
      ? [
          { label: 'Thông tin model', act: () => (setMenu(null), tell(modelSummary(settings.model as ModelInfo), 15_000)) },
          { label: 'Bỏ model (dùng 2D)', act: () => void removeModel() },
        ]
      : []),
    ...(voiceAvailable() ? [{ label: settings?.voice === true ? 'Tắt giọng nói' : 'Bật giọng nói', act: () => void toggleVoice() }] : []),
    { label: 'Ẩn trợ lý', act: () => void run({ kind: 'hide' }) },
  ];

  const sceneProps: PetSceneProps = {
    model: model ?? new ArrayBuffer(0),
    mood,
    speaking: voice.speaking,
    motion,
    pointer,
    reaction,
    onReady: () => api.ready('vrm'),
    onError: (m) => setSceneError(m),
  };
  const fallback = (
    <div className="pet__svg">
      <Character mood={svgMood(mood)} width={190} label={`Trợ lý: ${mood}`} />
    </div>
  );

  return (
    <main className="pet" onContextMenu={(e) => (e.preventDefault(), openMenuAt(e.clientX, e.clientY))}>
      <section className="pet__bubbles" aria-live="polite">
        {showApproval && approval !== null && (
          <ApprovalBubble
            a={approval}
            more={brain.visibleCount - 1}
            gate={allowGate(approval, {
              connected,
              shownAt: shownAt[approval.request_id],
              now: Math.max(now, Date.now()),
              minReviewMs,
              reviewed: false,
            })}
            onAllow={() => decide(approval.request_id, 'allow')}
            onDeny={() => decide(approval.request_id, 'deny')}
            onOpen={() => void run({ kind: 'open_queue' })}
            onClose={() => setDismissed(approval.request_id)}
          />
        )}
        {say !== null && (
          <p className={`pet__say${say.long === true ? ' pet__say--long' : ''}`} role="status" onClick={() => setSay(null)}>
            {say.text}
          </p>
        )}
        {sceneError !== null && settings?.model != null && <p className="pet__say pet__say--muted">Không vẽ được model 3D ({sceneError}). Em dùng hình 2D.</p>}
      </section>

      <div
        ref={body}
        className="pet__body"
        tabIndex={0}
        role="button"
        aria-label={`Trợ lý Arcturus. ${status()} Enter: ${approval !== null ? 'mở hàng đợi' : 'nghe trạng thái'}. Shift+F10: menu. /: gõ lệnh.`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={() => {
          setPointer(null);
          press.current = null;
        }}
        onDoubleClick={onDoubleClick}
        onKeyDown={onKeyDown}
      >
        {brain.visibleCount > 0 && <span className="pet__badge">{brain.visibleCount}</span>}
        {use3d ? (
          <Suspense fallback={fallback}>
            <Scene {...sceneProps} />
          </Suspense>
        ) : (
          fallback
        )}
        {settings !== null && settings.model === null && <p className="pet__hint muted small">Chuột phải → Chọn model… để dùng nhân vật 3D</p>}
      </div>

      {cmdOpen && (
        <form
          className="pet__cmd"
          onSubmit={(e) => {
            e.preventDefault();
            submitCmd();
          }}
        >
          <input
            autoFocus
            aria-label="Lệnh cho trợ lý"
            placeholder='Ví dụ: "tạm dừng đội", "tóm tắt hôm nay"'
            value={cmd}
            onChange={(e) => setCmd(e.target.value)}
          />
        </form>
      )}

      {menu !== null && (
        <ul className="pet__menu" role="menu" style={{ left: menu.x, top: menu.y }}>
          {menuItems.map((m) => (
            <li key={m.label} role="none">
              <button type="button" role="menuitem" onClick={m.act}>
                {m.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}

interface ApprovalBubbleProps {
  a: ApprovalView;
  more: number;
  gate: { ok: boolean; reason: string | null };
  onAllow: () => void;
  onDeny: () => void;
  onOpen: () => void;
  onClose: () => void;
}

function ApprovalBubble({ a, more, gate, onAllow, onDeny, onOpen, onClose }: ApprovalBubbleProps) {
  const critical = a.risk === 'critical';
  const full = fitsBubble(a);
  // Đồng ý chỉ khi lệnh hiện đầy đủ ở đây và không phải vùng khóa.
  const canAllow = full && !critical;
  return (
    <article className={`pet__card risk-${a.risk}`} aria-label={`Yêu cầu của ${a.agent_name}: ${a.tool}`}>
      <header className="pet__card-head">
        <strong>{a.agent_name}</strong> <span className="muted">{a.tool}</span>
        <span className={`risk-badge risk-badge--${a.risk}`}>{RISK_LABELS[a.risk]}</span>
        <button type="button" className="pet__x" aria-label="Ẩn bong bóng" onClick={onClose}>
          ×
        </button>
      </header>
      {full ? (
        <pre className="command pet__cmdline" aria-label="Lệnh cần duyệt">
          {splitSpans(a.command_normalized, a.danger_spans).map((p, i) => (p.danger ? <mark key={i} className="danger">{p.text}</mark> : <span key={i}>{p.text}</span>))}
        </pre>
      ) : (
        <p className="muted small">
          {a.truncated ? 'Lệnh đã bị cắt bớt' : `Lệnh dài ${Array.from(a.command_normalized).length} ký tự`}: mở hàng đợi để xem đầy đủ trước khi duyệt.
        </p>
      )}
      {a.suspicions.length > 0 && <p className="small pet__warn">{a.suspicions.map((s) => SUSPICION_LABELS[s]).join(' · ')}</p>}
      {critical && <p className="small pet__warn">Vùng khóa: chỉ duyệt trong hàng đợi sau khi xem kỹ.</p>}
      <div className="pet__actions">
        {canAllow && (
          <button type="button" className="btn btn--allow" disabled={!gate.ok} title={gate.reason ?? undefined} onClick={onAllow}>
            Đồng ý
          </button>
        )}
        {!critical && (
          <button type="button" className="btn" onClick={onDeny}>
            Từ chối
          </button>
        )}
        <button type="button" className="btn" onClick={onOpen}>
          Mở hàng đợi
        </button>
      </div>
      {canAllow && !gate.ok && gate.reason !== null && <p className="muted small">{gate.reason}</p>}
      {more > 0 && <p className="muted small">+{more} yêu cầu khác đang chờ</p>}
    </article>
  );
}

export function Pet({ minReviewMs = DEFAULT_MIN_REVIEW_MS }: { minReviewMs?: number }) {
  const { state, send } = useHubConnection();
  useEffect(() => {
    document.documentElement.dataset.view = 'pet';
  }, []);
  return <PetView state={state} send={send} api={tauriPetApi} minReviewMs={minReviewMs} />;
}
