// Tầng 1 của pet (docs/PLAN.md §7.4): lệnh có cấu trúc. Danh sách cố định; mỗi lệnh ánh xạ thẳng tới một thông điệp hoặc thao
// tác có sẵn; ngoài danh sách thì từ chối, không đoán. Không có lệnh nào duyệt, duyệt gộp, mở batch hay trả lời đề xuất tin
// cậy: kiểu `PetClientMsg` chỉ cho phép ba thông điệp điều khiển đội.
import type { ClientMsg } from '../protocol';

/** Thông điệp pet được phép gửi lên hub. */
export type PetClientMsg = Extract<ClientMsg, { t: 'team_pause' } | { t: 'team_resume' } | { t: 'focus_set' }>;

export type PetAction =
  | { kind: 'send'; msg: PetClientMsg; done: string }
  | { kind: 'open_queue' }
  | { kind: 'open_office' }
  | { kind: 'who_idle' }
  | { kind: 'standup' }
  | { kind: 'sleep' }
  | { kind: 'wake' }
  | { kind: 'hide' }
  | { kind: 'help' };

export interface CommandSpec {
  /** Cách gõ chính (hiện trong trợ giúp). */
  label: string;
  /** Các cách gõ khác (so sau khi bỏ dấu, chữ thường). */
  aliases: readonly string[];
  action: PetAction;
}

export const COMMANDS: readonly CommandSpec[] = [
  {
    label: 'tạm dừng đội',
    aliases: ['tam dung', 'tam dung doi', 'tam dung ca doi', 'dung doi', 'pause'],
    action: { kind: 'send', msg: { t: 'team_pause' }, done: 'Đã tạm dừng cả đội.' },
  },
  {
    label: 'tiếp tục',
    aliases: ['tiep tuc', 'tiep tuc doi', 'chay tiep', 'resume'],
    action: { kind: 'send', msg: { t: 'team_resume' }, done: 'Đội làm việc tiếp.' },
  },
  {
    label: 'bật giờ tập trung',
    aliases: ['gio tap trung', 'bat gio tap trung', 'tap trung', 'focus'],
    action: { kind: 'send', msg: { t: 'focus_set', on: true }, done: 'Đã bật giờ tập trung: em chỉ báo yêu cầu rủi ro cao.' },
  },
  {
    label: 'tắt giờ tập trung',
    aliases: ['tat gio tap trung', 'het tap trung', 'unfocus'],
    action: { kind: 'send', msg: { t: 'focus_set', on: false }, done: 'Đã tắt giờ tập trung.' },
  },
  {
    label: 'tới yêu cầu cũ nhất',
    aliases: ['toi yeu cau cu nhat', 'yeu cau cu nhat', 'hang doi', 'mo hang doi', 'duyet', 'queue'],
    action: { kind: 'open_queue' },
  },
  { label: 'mở văn phòng', aliases: ['mo van phong', 'van phong', 'office'], action: { kind: 'open_office' } },
  { label: 'ai đang rảnh', aliases: ['ai dang ranh', 'ai ranh', 'ranh', 'idle'], action: { kind: 'who_idle' } },
  { label: 'tóm tắt hôm nay', aliases: ['tom tat', 'tom tat hom nay', 'standup', 'bao cao'], action: { kind: 'standup' } },
  { label: 'ngủ', aliases: ['ngu', 'ngu di', 'sleep'], action: { kind: 'sleep' } },
  { label: 'dậy', aliases: ['day', 'thuc day', 'wake'], action: { kind: 'wake' } },
  { label: 'ẩn', aliases: ['an', 'an di', 'hide'], action: { kind: 'hide' } },
  { label: 'trợ giúp', aliases: ['tro giup', 'giup', 'help', '?'], action: { kind: 'help' } },
];

/** Chữ thường, bỏ dấu tiếng Việt (đ → d), gộp khoảng trắng, bỏ dấu câu ở hai đầu. */
export function normalizeCommand(s: string): string {
  return s
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .replace(/[đĐ]/g, 'd')
    .toLowerCase()
    .replace(/\s+/g, ' ')
    .replace(/^[\s.!,;:]+|[\s.!,;:]+$/g, '')
    .trim();
}

export type ParseResult = { ok: true; action: PetAction } | { ok: false; reason: string };

export function parseCommand(input: string): ParseResult {
  const n = normalizeCommand(input);
  if (n === '') return { ok: false, reason: 'Anh gõ lệnh giúp em nhé.' };
  const hit = COMMANDS.find((c) => c.aliases.includes(n) || normalizeCommand(c.label) === n);
  if (hit === undefined) return { ok: false, reason: `Em chưa hiểu lệnh này. Gõ "trợ giúp" để xem các lệnh.` };
  return { ok: true, action: hit.action };
}

export function helpText(): string {
  return `Em hiểu: ${COMMANDS.map((c) => c.label).join(', ')}.`;
}
