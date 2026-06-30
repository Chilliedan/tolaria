import { defineConfig, mergeConfig } from 'vite'
import base from './vite.config'
import { resolve } from 'node:path'

// Web build: same app, Tauri APIs aliased to HTTP/no-op shims, output to dist/.
export default mergeConfig(base, defineConfig({
  resolve: {
    alias: {
      '@tauri-apps/api/core': resolve(__dirname, 'src/web/transport.ts'),
      '@tauri-apps/api/event': resolve(__dirname, 'src/web/event.ts'),
      '@tauri-apps/api/window': resolve(__dirname, 'src/web/window.ts'),
      '@tauri-apps/api/webview': resolve(__dirname, 'src/web/window.ts'),
    },
  },
  build: { outDir: 'dist' },
}))
