// WebSocket giả cho test (docs/PLAN.md §5): ghi lại mọi URL và frame gửi đi, test tự phát sự kiện mở/nhận/đóng.
import type { SocketLike } from '../ws';

export class FakeSocket implements SocketLike {
  static all: FakeSocket[] = [];

  readyState = 0;
  sent: string[] = [];
  closed = false;
  onopen: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onclose: ((ev: CloseEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;

  constructor(public readonly url: string) {
    FakeSocket.all.push(this);
  }

  static factory = (url: string): FakeSocket => new FakeSocket(url);

  static reset(): void {
    FakeSocket.all = [];
  }

  static last(): FakeSocket {
    const s = FakeSocket.all[FakeSocket.all.length - 1];
    if (s === undefined) throw new Error('chưa có FakeSocket nào');
    return s;
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.closed = true;
    this.readyState = 3;
  }

  open(): void {
    this.readyState = 1;
    this.onopen?.(new Event('open'));
  }

  receive(msg: unknown): void {
    this.onmessage?.(new MessageEvent('message', { data: typeof msg === 'string' ? msg : JSON.stringify(msg) }));
  }

  /** Hub đóng kết nối (hoặc mất mạng). */
  drop(): void {
    this.readyState = 3;
    this.onclose?.(new CloseEvent('close', { code: 1006 }));
  }

  sentJson(): unknown[] {
    return this.sent.map((s) => JSON.parse(s) as unknown);
  }
}
