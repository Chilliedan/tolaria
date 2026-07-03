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

/** Tracks the last known content hash per note path, keyed by vault-relative
 *  path. Populated on `get_note_content` reads and updated on successful
 *  saves; used to supply `baseHash` for optimistic-concurrency checks. */
const noteVersions = new Map<string, string>()

/** SHA-256 hex digest of `text`, used to fingerprint note content for the
 *  optimistic-concurrency `baseHash` handshake with the server.
 *
 *  Returns `null` when Web Crypto's SubtleCrypto is unavailable. `crypto.subtle`
 *  only exists in secure contexts (HTTPS or localhost); over plain HTTP it is
 *  `undefined`. In that case we skip version tracking so note load/save keep
 *  working — optimistic concurrency degrades to best-effort (no `baseHash` is
 *  sent, so the server does not run the stale-write check). Deploy behind TLS
 *  to get full conflict protection. */
async function sha256Hex(text: string): Promise<string | null> {
  const subtle = typeof crypto !== 'undefined' ? crypto.subtle : undefined
  if (!subtle) return null
  const buf = await subtle.digest('SHA-256', new TextEncoder().encode(text))
  return Array.from(new Uint8Array(buf)).map((b) => b.toString(16).padStart(2, '0')).join('')
}

/** Reads the double-submit CSRF token from the `tolaria_csrf` cookie. */
function csrfToken(): string {
  if (typeof document === 'undefined') return ''
  const m = document.cookie.match(/(?:^|;\s*)tolaria_csrf=([^;]+)/)
  return m ? m[1] : ''
}

export async function invoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const path = typeof args?.path === 'string' ? (args.path as string) : undefined
  let body: Record<string, unknown> = { ...(args ?? {}) }
  if (command === 'save_note_content' && path && noteVersions.has(path)) {
    body = { ...body, baseHash: noteVersions.get(path) }
  }
  const res = await fetch(`/api/cmd/${command}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'X-CSRF-Token': csrfToken() },
    body: JSON.stringify(body),
  })
  if (res.status === 501) {
    // Command not implemented on web (read-only phase) — degrade gracefully.
    return undefined as T
  }
  if (res.status === 401) {
    // Session expired or missing — bounce to login instead of throwing.
    if (typeof window !== 'undefined') window.location.assign('/login')
    return undefined as T
  }
  if (res.status === 409) {
    // Optimistic-concurrency conflict: someone else changed the note since
    // our baseHash was recorded. Discard the stale edit and reload with the
    // server's current version rather than risk silently clobbering it.
    if (typeof window !== 'undefined') {
      window.alert('This note changed on the server. Reloading the latest version — your last change was not saved.')
      window.location.reload()
    }
    return undefined as T
  }
  if (!res.ok) {
    const detail = await res.json().catch(() => ({ error: res.statusText }))
    throw new Error(typeof (detail as { error?: string })?.error === 'string' ? (detail as { error: string }).error : `invoke ${command} failed`)
  }
  const data = (await res.json()) as T
  if (command === 'get_note_content' && path && typeof data === 'string') {
    const version = await sha256Hex(data)
    if (version) noteVersions.set(path, version)
  }
  if (command === 'save_note_content' && path) {
    const version = await sha256Hex(String((args as { content?: unknown } | undefined)?.content ?? ''))
    if (version) noteVersions.set(path, version)
  }
  return data
}
