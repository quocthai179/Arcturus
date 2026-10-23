// Client WebSocket tới hub: lấy thông tin kết nối (token có thể đổi khi hub khởi động lại), tự kết nối lại theo backoff,
// không polling khi đang mở. Không bao giờ ghi token ra log hay thông báo lỗi.
import { parseServerMsg, type ClientMsg, type ServerMsg } from './protocol';
import type { ConnectionStatus } from './store';

export interface SocketLike {
  readonly readyState: number;
  onopen: ((ev: Event) => void) | null;
  onmessage: ((ev: MessageEvent) => void) | null;
  onclose: ((ev: CloseEvent) => void) | null;
  onerror: ((ev: Event) => void) | null;
  send(data: string): void;
  close(code?: number, reason?: string): void;
}

export type SocketFactory = (url: string) => SocketLike;

export interface ConnectionInfo {
  /** `ws://127.0.0.1:<cổng>/ws`, không kèm token. */
  wsUrl: string;
  token: string;
}

export interface HubClientOptions {
  connection: () => Promise<ConnectionInfo>;
  onMessage: (msg: ServerMsg) => void;
  onStatus: (status: ConnectionStatus, error: string | null) => void;
  factory?: SocketFactory;
  /** Thời gian chờ trước mỗi lần kết nối lại (ms); hết mảng thì dùng phần tử cuối. */
  backoff?: readonly number[];
}

const OPEN = 1;
const DEFAULT_BACKOFF = [500, 1000, 2000, 5000, 10_000, 30_000] as const;

export class HubClient {
  private socket: SocketLike | null = null;
  private stopped = false;
  private attempt = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private readonly factory: SocketFactory;
  private readonly backoff: readonly number[];

  constructor(private readonly opts: HubClientOptions) {
    this.factory = opts.factory ?? ((url) => new WebSocket(url));
    this.backoff = opts.backoff ?? DEFAULT_BACKOFF;
  }

  start(): void {
    this.stopped = false;
    void this.connect();
  }

  stop(): void {
    this.stopped = true;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    const s = this.socket;
    this.socket = null;
    s?.close(1000, 'client đóng');
  }

  /** Gửi một lệnh; `false` khi chưa kết nối (người dùng bấm lại sau). */
  send(msg: ClientMsg): boolean {
    const s = this.socket;
    if (s === null || s.readyState !== OPEN) return false;
    s.send(JSON.stringify(msg));
    return true;
  }

  private async connect(): Promise<void> {
    if (this.stopped) return;
    this.opts.onStatus('connecting', null);
    let info: ConnectionInfo;
    try {
      info = await this.opts.connection();
    } catch (e) {
      this.opts.onStatus('closed', e instanceof Error ? e.message : 'Không lấy được thông tin kết nối hub');
      this.retry();
      return;
    }
    if (this.stopped) return;
    const url = `${info.wsUrl}?token=${encodeURIComponent(info.token)}`;
    let s: SocketLike;
    try {
      s = this.factory(url);
    } catch {
      this.opts.onStatus('closed', 'Không mở được kết nối tới hub');
      this.retry();
      return;
    }
    this.socket = s;
    s.onopen = () => {
      if (this.socket !== s) return;
      this.attempt = 0;
      this.opts.onStatus('open', null);
    };
    s.onmessage = (ev) => {
      if (this.socket !== s || typeof ev.data !== 'string') return;
      const msg = parseServerMsg(ev.data);
      if (msg !== null) this.opts.onMessage(msg);
    };
    s.onerror = () => {
      // `onclose` luôn tới sau; không làm gì ở đây để không kết nối lại hai lần.
    };
    s.onclose = () => {
      if (this.socket !== s) return;
      this.socket = null;
      if (this.stopped) return;
      this.opts.onStatus('closed', 'Mất kết nối hub, đang thử lại…');
      this.retry();
    };
  }

  private retry(): void {
    if (this.stopped) return;
    const delay = this.backoff[Math.min(this.attempt, this.backoff.length - 1)] ?? 30_000;
    this.attempt += 1;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.connect();
    }, delay);
  }
}
