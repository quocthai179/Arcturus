/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Chỉ khi phát triển UI ngoài app: `ws://127.0.0.1:7878/ws`. */
  readonly VITE_HUB_WS_URL?: string;
  /** Chỉ khi phát triển UI ngoài app: token UI của hub bản debug. */
  readonly VITE_HUB_TOKEN?: string;
}
