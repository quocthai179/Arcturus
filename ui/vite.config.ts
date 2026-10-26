import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// Cổng cố định: crates/app/src-tauri/tauri.conf.json trỏ devUrl vào đây.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true, host: '127.0.0.1' },
  build: { outDir: 'dist', target: 'es2022' },
  test: { environment: 'jsdom', setupFiles: ['src/test/setup.ts'], css: { include: [/\.css\?raw/] } },
});
