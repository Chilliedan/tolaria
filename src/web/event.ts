/** Web stubs for @tauri-apps/api/event. Phase 2 has no live events; listen is a no-op. */
export type UnlistenFn = () => void
export async function listen(): Promise<UnlistenFn> {
  return () => {}
}
export async function once(): Promise<UnlistenFn> {
  return () => {}
}
export async function emit(): Promise<void> {}
