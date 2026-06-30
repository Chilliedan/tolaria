/** Web stubs for @tauri-apps/api/window and /webview. No native window on web. */
export function getCurrentWindow() {
  return {
    listen: async () => () => {},
    onCloseRequested: async () => () => {},
    setTitle: async () => {},
    show: async () => {},
    hide: async () => {},
  }
}
export function getCurrentWebview() {
  return { listen: async () => () => {} }
}
