/**
 * Web-build replacement for `src/mock-tauri`.
 *
 * The app uses `isTauri() ? invoke : mockInvoke` (and `if (!isTauri())
 * mockInvoke(...)`) so that, outside Tauri, operations fall back to
 * `mockInvoke`. In the real web deployment that fallback must:
 *   - route commands the server ACTUALLY implements to the HTTP transport, so
 *     writes (create/rename/delete/frontmatter) and vault reads hit the server
 *     and persist; and
 *   - fall back to the in-memory mock for everything else (AI, git, clipboard,
 *     window/menu, PDF, …) so those unimplemented commands return plausible
 *     data instead of `undefined`, which the app would otherwise `.map()` and
 *     crash on at render time.
 *
 * `isTauri()` stays `false`: the many `if (isTauri()) <tauri-plugin-call>`
 * guards must remain off in the browser (those plugins are not shimmed and
 * would throw). The `addMock*` mutators are no-ops — the server is the source
 * of truth for the commands it owns.
 */
import { invoke } from './transport'
import { mockHandlers } from '../mock-tauri/mock-handlers'

/** Commands the web server implements; these go to the real HTTP transport. */
const SERVER_COMMANDS = new Set<string>([
  'list_vault',
  'reload_vault',
  'list_vault_folders',
  'get_note_content',
  'get_all_content',
  'reload_vault_entry',
  'search_vault',
  'save_note_content',
  'create_note',
  'create_note_content',
  'rename_note',
  'rename_note_filename',
  'delete_note',
  'update_frontmatter',
  'delete_frontmatter_property',
  // Vault registry — the server reports its single real vault so the app builds
  // note paths under it (not the mock's default path).
  'load_vault_list',
  'get_last_vault_path',
  'set_last_vault_path',
  'save_vault_list',
  'check_vault_exists',
])

export function isTauri(): boolean {
  return false
}

export function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (SERVER_COMMANDS.has(cmd)) {
    return invoke<T>(cmd, args)
  }
  const handler = mockHandlers[cmd]
  if (handler) {
    return Promise.resolve(handler(args) as T)
  }
  // Neither server-implemented nor mocked: let the server answer (501 → undefined).
  return invoke<T>(cmd, args)
}

// Mock-only state mutators — the server owns the commands it implements.
export function addMockEntry(...args: unknown[]): void {
  void args
}
export function updateMockContent(...args: unknown[]): void {
  void args
}
export function trackMockChange(...args: unknown[]): void {
  void args
}
