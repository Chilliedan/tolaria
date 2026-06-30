/**
 * Web transport: replaces Tauri's `invoke` for the browser build.
 * Forwards each command to the server's generic RPC endpoint. Unsupported
 * (501) commands resolve to `undefined` so the read-only app still renders.
 *
 * Also provides no-op stubs for other `@tauri-apps/api/core` exports
 * (convertFileSrc, Channel) used by the app source.
 */

/**
 * On web there is no local file serving; return the path unchanged.
 * Components that use convertFileSrc should gracefully handle the case
 * where the URL cannot load (404) — the same degradation as 501 on invoke.
 */
export function convertFileSrc(filePath: string): string {
  return filePath
}

/** Stub for the Tauri Channel class used by the updater flow. On web the
 *  updater is disabled, so Channel is never meaningfully exercised. */
export class Channel<T = unknown> {
  id = 0
  private _onmessage: (response: T) => void = () => {}

  constructor(onmessage?: (response: T) => void) {
    if (onmessage) this._onmessage = onmessage
  }

  set onmessage(handler: (response: T) => void) {
    this._onmessage = handler
  }

  get onmessage(): (response: T) => void {
    return this._onmessage
  }

  toJSON(): string {
    return `__CHANNEL__:${this.id}`
  }
}

export async function invoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const res = await fetch(`/api/cmd/${command}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(args ?? {}),
  })
  if (res.status === 501) {
    // Command not implemented on web (read-only phase) — degrade gracefully.
    return undefined as T
  }
  if (!res.ok) {
    const detail = await res.json().catch(() => ({ error: res.statusText }))
    throw new Error(typeof detail?.error === 'string' ? detail.error : `invoke ${command} failed`)
  }
  return (await res.json()) as T
}
