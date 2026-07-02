import { defineConfig, mergeConfig } from 'vite'
import base from './vite.config'
import { resolve } from 'node:path'

// Redirect any import of the in-memory mock (`src/mock-tauri`, imported by the
// app via relative paths) to the web bridge, so the browser's `mockInvoke`
// fallback talks to the real server instead of fixtures. A resolveId hook is
// used rather than a string alias because the imports are relative.
function mockBridgePlugin() {
  const bridge = resolve(__dirname, 'src/web/mockBridge.ts')
  return {
    name: 'tolaria-web-mock-bridge',
    enforce: 'pre' as const,
    resolveId(source: string) {
      return /(^|\/)mock-tauri(\/index)?(\.ts)?$/.test(source) ? bridge : null
    },
  }
}

// Web build: same app, Tauri APIs aliased to HTTP/no-op shims, output to dist/.
export default mergeConfig(base, defineConfig({
  plugins: [mockBridgePlugin()],
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
