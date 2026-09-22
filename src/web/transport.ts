/**
 * Web transport: replaces Tauri's `invoke` for the browser build.
 * Forwards each command to the server's generic RPC endpoint. Unsupported
 * (501) commands resolve to `undefined` so the read-only app still renders.
 *
 * Also provides no-op stubs for other `@tauri-apps/api/core` exports
 * (convertFileSrc, Channel) used by the app source.
 */

/**
 * Map a vault file path to the server's authenticated asset route, mirroring
 * Tauri's `asset://localhost/<encoded path>` shape (the prefix is recognised
 * by `src/utils/vaultAttachments.ts`). The server serves image types from
 * inside the vault only; anything else 404s, like a missing asset.
 */
export function convertFileSrc(filePath: string, protocol = 'asset'): string {
  if (protocol !== 'asset') return filePath
  return `/api/asset/${encodeURIComponent(filePath)}`
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
  const prefix = 'tolaria_csrf='
  const cookie = document.cookie.split(';').map((part) => part.trim()).find((part) => part.startsWith(prefix))
  return cookie ? cookie.slice(prefix.length) : ''
}

type InvokeArgs = Record<string, unknown> | undefined

function notePathArg(args: InvokeArgs): string | undefined {
  return typeof args?.path === 'string' ? args.path : undefined
}

/** Attach the last known `baseHash` to saves so the server can reject stale writes. */
function requestBody(command: string, args: InvokeArgs): Record<string, unknown> {
  const body: Record<string, unknown> = { ...(args ?? {}) }
  const path = notePathArg(args)
  const baseHash = command === 'save_note_content' && path ? noteVersions.get(path) : undefined
  return baseHash ? { ...body, baseHash } : body
}

function reloadAfterConflict(): void {
  // Optimistic-concurrency conflict: someone else changed the note since our
  // baseHash was recorded. Discard the stale edit and reload with the server's
  // current version rather than risk silently clobbering it.
  window.alert('This note changed on the server. Reloading the latest version — your last change was not saved.')
  window.location.reload()
}

/** Statuses that degrade to `undefined` instead of throwing. */
const DEGRADED_STATUS_ACTIONS: Record<number, () => void> = {
  // Command not implemented on web — degrade gracefully.
  501: () => {},
  // Session expired or missing — bounce to login instead of throwing.
  401: () => window.location.assign('/login'),
  409: reloadAfterConflict,
}

function handleDegradedStatus(status: number): boolean {
  const action = DEGRADED_STATUS_ACTIONS[status]
  if (!action) return false
  if (typeof window !== 'undefined') action()
  return true
}

type ErrorBody = { error?: unknown }

async function errorMessage(res: Response, command: string): Promise<string> {
  const detail = (await res.json().catch(() => ({ error: res.statusText }))) as ErrorBody
  return typeof detail.error === 'string' ? detail.error : `invoke ${command} failed`
}

/** Remember the content hash of notes we read or wrote, for the next save's `baseHash`. */
async function recordNoteVersion(command: string, args: InvokeArgs, data: unknown): Promise<void> {
  const path = notePathArg(args)
  if (!path) return
  let content: string | undefined
  if (command === 'get_note_content' && typeof data === 'string') content = data
  if (command === 'save_note_content') content = String(args?.content ?? '')
  if (content === undefined) return
  const version = await sha256Hex(content)
  if (version) noteVersions.set(path, version)
}

export async function invoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const res = await fetch(`/api/cmd/${command}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'X-CSRF-Token': csrfToken() },
    body: JSON.stringify(requestBody(command, args)),
  })
  if (handleDegradedStatus(res.status)) return undefined as T
  if (!res.ok) throw new Error(await errorMessage(res, command))
  const data = (await res.json()) as T
  await recordNoteVersion(command, args, data)
  return data
}
