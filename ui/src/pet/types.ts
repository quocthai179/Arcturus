// Kiểu dùng chung của trợ lý pet (docs/PLAN.md §7). Không phụ thuộc three.js: file này nằm trong bundle chính.

/** Trạng thái hiển thị của pet (suy từ trạng thái đội ở `brain.ts`). */
export type PetMood = 'idle' | 'working' | 'approval' | 'critical' | 'done' | 'error' | 'paused' | 'sleep';

export const PET_MOODS: readonly PetMood[] = ['idle', 'working', 'approval', 'critical', 'done', 'error', 'paused', 'sleep'];

/** Thông tin model đọc từ meta VRM (khớp `ModelInfo` ở crates/app/src-tauri/src/pet.rs). Chuỗi là dữ liệu không tin cậy. */
export interface ModelInfo {
  file_name: string;
  bytes: number;
  vrm_version: string;
  name: string;
  authors: string[];
  license: string;
  allow_redistribution: boolean | null;
  commercial_usage: string;
}

/** Tùy chọn của pet (khớp `Settings` ở pet.rs). */
export interface PetSettings {
  enabled: boolean;
  x: number | null;
  y: number | null;
  model: ModelInfo | null;
  voice: boolean;
  chatter: Chatter;
  size: PetSize;
}

/** Mức nhắc (PLAN §7.11): `normal` như cũ, `urgent` chỉ câu ưu tiên cao, `quiet` không nói chủ động. Khớp `Chatter` ở pet.rs. */
export type Chatter = 'normal' | 'urgent' | 'quiet';
export const CHATTERS: readonly Chatter[] = ['normal', 'urgent', 'quiet'];
export const CHATTER_LABELS: Record<Chatter, string> = { normal: 'Bình thường', urgent: 'Chỉ việc khẩn', quiet: 'Im lặng' };

/** Kích cỡ cửa sổ pet (PLAN §7.11). Khớp `PetSize` ở pet.rs. */
export type PetSize = 'small' | 'normal' | 'large';
export const PET_SIZES: readonly PetSize[] = ['small', 'normal', 'large'];
export const SIZE_LABELS: Record<PetSize, string> = { small: 'Nhỏ', normal: 'Vừa', large: 'Lớn' };
