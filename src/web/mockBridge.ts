/**
 * Web-build replacement for `src/mock-tauri`.
 *
 * The app uses the pattern `isTauri() ? invoke : mockInvoke` (and
 * `if (!isTauri()) mockInvoke(...)`) so that, outside Tauri, data operations
 * fall back to an in-memory mock. In the real web deployment that fallback must
 * talk to the server, not the mock — otherwise writes (create/rename/delete)
 * only touch in-memory state and vanish on refresh.
 *
 * So here `mockInvoke` delegates to the real HTTP transport, and the mock
 * mutators become no-ops. `isTauri()` stays `false` on purpose: the many
 * `if (isTauri()) <tauri-plugin-call>` guards must remain off in the browser
 * (those plugins are not shimmed and would throw), while `!isTauri()` browser
 * branches now reach the server through `mockInvoke`.
 */
import { invoke } from './transport'

export function isTauri(): boolean {
  return false
}

export function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args)
}

// Mock-only state mutators — the real server is the source of truth on web.
export function addMockEntry(...args: unknown[]): void {
  void args
}
export function updateMockContent(...args: unknown[]): void {
  void args
}
export function trackMockChange(...args: unknown[]): void {
  void args
}
