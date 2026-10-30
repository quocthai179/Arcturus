// Cầu nối tới các lệnh `setup_*` phía Rust (crates/app/src-tauri/src/setup.rs). Kiểu viết tay khớp các struct `Serialize`
// ở đó. Tách thành giao diện để test view không cần Tauri.
import { invoke, isTauri } from '@tauri-apps/api/core';

export type SetupAction = 'install' | 'uninstall';

export interface SetupStatus {
  path: string;
  exists: boolean;
  /** Đã cài và đúng bản hiện tại của app. */
  up_to_date: boolean;
  arc_hooks: number;
  problems: string[];
  /** settings.json hỏng hoặc không đọc được: không cài/gỡ được. */
  error: string | null;
  hub_port: number;
}

export interface DiffLine {
  op: 'ctx' | 'add' | 'del' | 'skip';
  text: string;
}

export interface SetupPreview {
  action: SetupAction;
  path: string;
  changed: boolean;
  /** Token đã được che (`<token …abcd>`). */
  diff: DiffLine[];
  digest: string;
}

export interface SetupApplied {
  changed: boolean;
  backup: string | null;
}

export type StepId = 'settings' | 'hub' | 'hook' | 'ui';
export type StepState = 'ok' | 'warn' | 'fail' | 'skip' | 'running';

export interface SelfTestStep {
  id: StepId;
  state: StepState;
  detail: string;
}

export interface SelfTestResult {
  steps: SelfTestStep[];
  /** Agent mà giao diện phải thấy xuất hiện (chỉ có khi hub đã nhận sự kiện thử). */
  agent_id: string | null;
  nonce: string | null;
}

export interface SetupApi {
  status(): Promise<SetupStatus>;
  preview(action: SetupAction): Promise<SetupPreview>;
  /** Ghi đúng thay đổi đã xem trước; file đổi sau lúc xem trước thì lỗi. */
  apply(action: SetupAction, digest: string): Promise<SetupApplied>;
  selfTest(): Promise<SelfTestResult>;
  /** Kết thúc phiên giả của tự kiểm tra. */
  cleanup(nonce: string): Promise<void>;
}

function errorText(e: unknown): Error {
  return e instanceof Error ? e : new Error(typeof e === 'string' ? e : 'Lỗi không rõ');
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) throw new Error('Chỉ chạy được trong app Arcturus');
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw errorText(e);
  }
}

export const tauriSetupApi: SetupApi = {
  status: () => call<SetupStatus>('setup_status'),
  preview: (action) => call<SetupPreview>('setup_preview', { action }),
  apply: (action, digest) => call<SetupApplied>('setup_apply', { action, digest }),
  selfTest: () => call<SelfTestResult>('setup_selftest'),
  cleanup: (nonce) => call<void>('setup_selftest_cleanup', { nonce }),
};
