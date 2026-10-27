// Cầu nối tới các lệnh `pet_*` phía Rust (crates/app/src-tauri/src/main.rs, pet.rs). Tách thành giao diện để test view
// không cần Tauri.
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { ModelInfo, PetSettings } from './types';

export interface PetApi {
  settings(): Promise<PetSettings>;
  /** Bytes của model đã chọn; lỗi khi chưa chọn. */
  model(): Promise<ArrayBuffer>;
  /** Hộp thoại chọn file `.vrm`; `null` khi người dùng hủy. */
  chooseModel(): Promise<ModelInfo | null>;
  removeModel(): Promise<void>;
  setVoice(on: boolean): Promise<void>;
  drag(): Promise<void>;
  hide(): Promise<void>;
  openQueue(): Promise<void>;
  openOffice(): Promise<void>;
  standup(hours: number): Promise<string>;
  ready(kind: 'vrm' | 'svg'): void;
}

const OFF: PetSettings = { enabled: false, x: null, y: null, model: null, voice: false };

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

export const tauriPetApi: PetApi = {
  settings: () => (isTauri() ? call<PetSettings>('pet_settings') : Promise.resolve(OFF)),
  model: () => call<ArrayBuffer>('pet_model'),
  chooseModel: () => call<ModelInfo | null>('pet_choose_model'),
  removeModel: () => call<void>('pet_remove_model'),
  setVoice: (on) => call<void>('pet_set_voice', { on }),
  drag: () => call<void>('pet_drag'),
  hide: () => call<void>('pet_hide'),
  openQueue: () => call<void>('pet_open_queue'),
  openOffice: () => call<void>('pet_open_office'),
  standup: (hours) => call<string>('standup', { hours }),
  ready: (kind) => {
    if (isTauri()) void invoke('pet_ready', { kind }).catch(() => undefined);
  },
};
