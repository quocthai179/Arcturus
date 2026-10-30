// Cửa sổ cài đặt (`?view=setup`): cài/gỡ hook của Arcturus vào settings.json của Claude Code sau khi xem trước thay đổi,
// rồi tự kiểm tra cả đường đi hook → hub → giao diện. Phần ghi file và gửi sự kiện thử nằm ở phía Rust (setup.rs); view
// này chỉ hiển thị và kiểm bước cuối (nó chính là bên nhận sự kiện qua WebSocket).
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  tauriSetupApi,
  type SelfTestStep,
  type SetupAction,
  type SetupApi,
  type SetupPreview,
  type SetupStatus,
  type StepId,
} from '../setup/setupApi';
import type { HubState } from '../store';
import { useHubConnection } from '../useHubConnection';
import './Setup.css';

/** Chờ agent của sự kiện thử hiện trong giao diện tối đa chừng này. */
export const UI_WAIT_MS = 4000;

const STEP_LABELS: Record<StepId, string> = {
  settings: 'Hook trong settings.json của Claude Code',
  hub: 'Hub đang chạy',
  hook: 'Hook tới được hub (gửi một sự kiện thử)',
  ui: 'Giao diện thấy sự kiện',
};

const STATE_ICON: Record<SelfTestStep['state'], string> = {
  ok: '✓',
  warn: '!',
  fail: '✗',
  skip: '–',
  running: '…',
};

const STATE_TEXT: Record<SelfTestStep['state'], string> = {
  ok: 'Đạt',
  warn: 'Cần chú ý',
  fail: 'Hỏng',
  skip: 'Bỏ qua',
  running: 'Đang kiểm',
};

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

interface Pending {
  agentId: string;
  nonce: string;
}

export interface SetupViewProps {
  state: HubState;
  api: SetupApi;
}

export function SetupView({ state, api }: SetupViewProps) {
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<SetupPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const [steps, setSteps] = useState<SelfTestStep[] | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const cleaned = useRef<string | null>(null);

  const reload = useCallback(async () => {
    // Không xóa `error` ở đây: đọc lại trạng thái sau khi ghi lỗi không được che mất lỗi đó.
    try {
      setStatus(await api.status());
    } catch (e) {
      setError(message(e));
    }
  }, [api]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const showPreview = async (action: SetupAction) => {
    setBusy(true);
    setDone(null);
    try {
      setPreview(await api.preview(action));
      setError(null);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };

  const applyPreview = async () => {
    if (!preview) return;
    setBusy(true);
    try {
      const r = await api.apply(preview.action, preview.digest);
      const what = preview.action === 'install' ? 'Đã cài hook' : 'Đã gỡ hook';
      setDone(
        r.changed
          ? `${what}.${r.backup ? ` Bản sao lưu: ${r.backup}` : ''}`
          : 'Không có gì thay đổi.',
      );
      setPreview(null);
      setSteps(null);
      setError(null);
    } catch (e) {
      setError(message(e));
      setPreview(null);
    } finally {
      setBusy(false);
      await reload();
    }
  };

  const finishUi = useCallback(
    (step: SelfTestStep, nonce: string) => {
      setSteps((s) => (s ? s.map((x) => (x.id === 'ui' ? step : x)) : s));
      setPending(null);
      if (cleaned.current !== nonce) {
        cleaned.current = nonce;
        api.cleanup(nonce).catch(() => undefined);
      }
    },
    [api],
  );

  const runSelfTest = async () => {
    setBusy(true);
    setPending(null);
    setSteps([
      ...(['settings', 'hub', 'hook'] as const).map((id) => ({ id, state: 'running' as const, detail: '' })),
      { id: 'ui', state: 'skip', detail: '' },
    ]);
    try {
      const r = await api.selfTest();
      const ui: SelfTestStep = r.agent_id
        ? { id: 'ui', state: 'running', detail: 'Đang chờ sự kiện thử hiện trong giao diện…' }
        : { id: 'ui', state: 'skip', detail: 'Hub chưa nhận được sự kiện thử.' };
      setSteps([...r.steps, ui]);
      if (r.agent_id && r.nonce) setPending({ agentId: r.agent_id, nonce: r.nonce });
    } catch (e) {
      setSteps(null);
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };

  // Bước cuối: agent của sự kiện thử phải tới giao diện qua WebSocket.
  const seen = pending !== null && state.agents[pending.agentId] !== undefined;
  useEffect(() => {
    if (!pending) return;
    if (seen) {
      finishUi({ id: 'ui', state: 'ok', detail: 'Giao diện nhận được sự kiện thử qua WebSocket.' }, pending.nonce);
      return;
    }
    const t = window.setTimeout(() => {
      const conn = state.connection === 'open' ? 'đang mở' : `ở trạng thái "${state.connection}"`;
      finishUi(
        {
          id: 'ui',
          state: 'fail',
          detail: `Hub nhận sự kiện nhưng giao diện không thấy sau ${UI_WAIT_MS / 1000} giây (kết nối WebSocket ${conn}). Thử Thoát app rồi mở lại.`,
        },
        pending.nonce,
      );
    }, UI_WAIT_MS);
    return () => window.clearTimeout(t);
  }, [pending, seen, state.connection, finishUi]);

  const allOk = steps !== null && steps.every((s) => s.state === 'ok' || s.state === 'warn');
  const installed = status !== null && status.arc_hooks > 0;

  return (
    <main className="setup">
      <header className="setup__head">
        <p className="setup__brand">ARCTURUS</p>
        <h1>Nối Claude Code với Arcturus</h1>
        <p className="muted">
          Ba bước: cài hook vào settings.json, mở một phiên <code>claude</code> mới, rồi tự kiểm tra. Không cần đặt biến
          môi trường hay chạy script: app tự tạo token cho hook.
        </p>
      </header>

      {error && (
        <p className="setup__error" role="alert">
          {error}
        </p>
      )}

      <section className="setup__card" aria-labelledby="setup-install">
        <h2 id="setup-install">1. Hook</h2>
        {status === null && !error && <p className="muted">Đang đọc settings.json…</p>}
        {status && (
          <>
            <p className="setup__path">
              <span className="muted">File:</span> <code>{status.path}</code>
              {!status.exists && <span className="muted"> (chưa có, sẽ được tạo)</span>}
            </p>
            {status.error ? (
              <p className="setup__state setup__state--fail">{status.error}</p>
            ) : status.up_to_date ? (
              <p className="setup__state setup__state--ok">
                Đã cài {status.arc_hooks} hook, trỏ tới hub ở cổng {status.hub_port}.
              </p>
            ) : (
              <ul className="setup__problems" aria-label="Vấn đề">
                {status.problems.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            )}
            {!preview && (
              <div className="setup__actions">
                <button
                  type="button"
                  className="setup__primary"
                  disabled={busy || status.error !== null}
                  onClick={() => void showPreview('install')}
                >
                  {installed ? 'Xem trước khi cài lại' : 'Xem trước khi cài'}
                </button>
                <button
                  type="button"
                  disabled={busy || status.error !== null || !installed}
                  onClick={() => void showPreview('uninstall')}
                >
                  Gỡ hook…
                </button>
              </div>
            )}
          </>
        )}

        {preview && (
          <div className="setup__preview">
            {preview.changed ? (
              <>
                <p>
                  {preview.action === 'install' ? 'Thay đổi sẽ ghi vào' : 'Phần sẽ gỡ khỏi'} <code>{preview.path}</code>.
                  Chỉ khóa <code>hooks</code> thay đổi; file cũ được sao lưu bên cạnh trước khi ghi.
                </p>
                <pre className="setup__diff" aria-label="Thay đổi">
                  {preview.diff.map((l, i) => (
                    <span key={i} className={`diff diff--${l.op}`}>
                      {l.op === 'add' ? '+ ' : l.op === 'del' ? '- ' : '  '}
                      {l.text}
                      {'\n'}
                    </span>
                  ))}
                </pre>
                <div className="setup__actions">
                  <button type="button" className="setup__primary" disabled={busy} onClick={() => void applyPreview()}>
                    {preview.action === 'install' ? 'Đồng ý cài' : 'Đồng ý gỡ'}
                  </button>
                  <button type="button" disabled={busy} onClick={() => setPreview(null)}>
                    Hủy
                  </button>
                </div>
              </>
            ) : (
              <>
                <p>Không có gì thay đổi: {preview.action === 'install' ? 'hook đã đúng bản hiện tại.' : 'không có hook nào để gỡ.'}</p>
                <div className="setup__actions">
                  <button type="button" onClick={() => setPreview(null)}>
                    Đóng
                  </button>
                </div>
              </>
            )}
          </div>
        )}
        {done && (
          <p className="setup__done" role="status">
            {done}
          </p>
        )}
      </section>

      <section className="setup__card" aria-labelledby="setup-session">
        <h2 id="setup-session">2. Mở phiên claude mới</h2>
        <p className="muted">
          Claude Code đọc hook lúc phiên bắt đầu: phiên đang chạy vẫn giữ cấu hình cũ. Hub tắt hay lỗi thì Claude Code hỏi
          trong terminal như bình thường; Arcturus không bao giờ tự cho phép thay bạn.
        </p>
      </section>

      <section className="setup__card" aria-labelledby="setup-check">
        <h2 id="setup-check">3. Tự kiểm tra</h2>
        <p className="muted">
          App gửi một sự kiện thử qua đúng URL và token của hook đã cài, rồi chờ nó hiện ở đây. Không tạo yêu cầu duyệt
          nào; agent thử tự biến mất sau khi kiểm.
        </p>
        <div className="setup__actions">
          <button type="button" className="setup__primary" disabled={busy || pending !== null} onClick={() => void runSelfTest()}>
            {steps ? 'Kiểm tra lại' : 'Chạy tự kiểm tra'}
          </button>
        </div>
        {steps && (
          <ol className="setup__steps" aria-label="Kết quả tự kiểm tra">
            {steps.map((s) => (
              <li key={s.id} className={`step step--${s.state}`} data-step={s.id}>
                <span className="step__icon" aria-hidden="true">
                  {STATE_ICON[s.state]}
                </span>
                <span className="step__body">
                  <strong>{STEP_LABELS[s.id]}</strong>
                  <span className="step__state">{STATE_TEXT[s.state]}</span>
                  {s.detail && <span className="step__detail">{s.detail}</span>}
                </span>
              </li>
            ))}
          </ol>
        )}
        {allOk && pending === null && (
          <p className="setup__done" role="status">
            Mọi thứ đã nối. Thử bảo Claude chạy <code>touch dogfood-marker.txt</code> trong một thư mục thử: thẻ duyệt sẽ
            hiện ở popup.
          </p>
        )}
      </section>
    </main>
  );
}

export function Setup() {
  const { state } = useHubConnection();
  return <SetupView state={state} api={tauriSetupApi} />;
}
