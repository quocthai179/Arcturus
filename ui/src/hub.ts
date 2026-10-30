// Cầu nối tới phía Rust của app (crates/app, T12): thông tin kết nối hub và bản đầy đủ của `tool_input`.
//
// Token UI chỉ đi qua `invoke` (không nằm trong URL của trang, không có file token khi app tự khởi động hub, F6 B1).
// `GET /approvals/{id}/input` không có CORS nên phải gọi từ phía Rust (docs/PROTOCOL.md "HTTP cho UI").
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { ConnectionInfo } from './ws';

interface HubInfo {
  ws_url: string;
  token: string;
}

export async function connectionInfo(): Promise<ConnectionInfo> {
  if (isTauri()) {
    const info = await invoke<HubInfo>('hub_info');
    return { wsUrl: info.ws_url, token: info.token };
  }
  // Chạy UI riêng trong trình duyệt khi phát triển (`pnpm -C ui dev` với hub bản debug): đặt hai biến trong `ui/.env.local`.
  const wsUrl = import.meta.env.VITE_HUB_WS_URL;
  const token = import.meta.env.VITE_HUB_TOKEN;
  // Không dùng trong test: `ui/.env.local` của dev không được làm test nối vào hub đang chạy trên máy.
  if (import.meta.env.DEV && import.meta.env.MODE !== 'test' && wsUrl !== undefined && token !== undefined) {
    return { wsUrl, token };
  }
  throw new Error('Chỉ chạy được trong app Arcturus');
}

/** `tool_input` đầy đủ của yêu cầu đang chờ, dạng JSON đã định dạng (hiển thị như văn bản thuần). */
export async function approvalInput(requestId: string): Promise<string> {
  if (!isTauri()) {
    throw new Error('Xem bản đầy đủ chỉ có trong app Arcturus');
  }
  return invoke<string>('approval_input', { requestId });
}

/** Báo app rằng popup đã dựng xong khung đầu tiên (đo T16 "popup mở ≤300 ms"). Ngoài app hoặc lỗi thì bỏ qua. */
export function reportPopupReady(): void {
  if (!isTauri()) {
    return;
  }
  invoke('popup_ready').catch(() => undefined);
}
