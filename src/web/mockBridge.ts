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
import { markWebServerBridge } from '../lib/webServerBridge'

/** Commands the web server implements; these go to the real HTTP transport. */
export const SERVER_COMMANDS = new Set<string>([
  'list_vault',
  'reload_vault',
  'list_vault_folders',
  'list_views',
  'get_note_content',
  'get_all_content',
  'reload_vault_entry',
  'search_vault',
  'save_note_content',
  'create_note',
  'create_note_content',
  'rename_note',
  'rename_note_filename',
  'move_note_to_folder',
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
  // git sync (Phase 5)
  'git_remote_status',
  'git_author_identity',
  'git_commit',
  'git_push',
  'git_pull',
  'git_resolve_conflict',
  'git_commit_conflict_resolution',
  // git reads + folder ops (routing sweep)
  'is_git_repo',
  'get_modified_files',
  'get_modified_files_with_stats',
  'get_file_diff',
  'get_file_diff_at_commit',
  'get_file_history',
  'get_last_commit_info',
  'get_vault_pulse',
  'git_file_url',
  'git_add_remote',
  'git_discard_file',
  'init_git_repo',
  'create_vault_folder',
  'delete_vault_folder',
  'rename_vault_folder',
])

/**
 * Commands that only make sense inside the native desktop shell (clipboard,
 * PDF export/print, window/menu chrome, the updater, the vault file watcher,
 * and AI streaming/session persistence). On web these must no-op rather than
 * fall through to the server, which does not implement them and would 501.
 */
export const DESKTOP_ONLY = new Set<string>([
  'copy_text_to_clipboard',
  'read_text_from_clipboard',
  'copy_image_to_vault',
  'save_image',
  'export_current_webview_pdf',
  'print_current_webview',
  'can_export_current_webview_pdf',
  'update_current_window_min_size',
  'perform_current_window_titlebar_double_click',
  'trigger_menu_command',
  'update_menu_state',
  'check_for_app_update',
  'download_and_install_app_update',
  'start_vault_watcher',
  'stop_vault_watcher',
  'get_process_memory_snapshot',
  'update_app_icon',
  'open_vault_file_external',
  'should_use_external_media_preview',
  'sync_vault_asset_scope_for_window',
  'get_agent_docs_path',
  'check_claude_cli',
  'get_ai_workspace_sessions',
  'save_ai_workspace_sessions',
  'save_ai_model_provider_api_key',
  'delete_ai_model_provider_api_key',
  'test_ai_model_provider',
  'stream_ai_agent',
  'stream_ai_model',
  'stream_claude_chat',
])

export function isTauri(): boolean {
  return false
}

// Mark this as the real web server bridge so code that must pick a server
// round-trip over an in-browser JS mock (frontmatter edits, cross-vault moves)
// can detect it without every mock-tauri test mock having to declare a flag.
markWebServerBridge()

export function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (DESKTOP_ONLY.has(cmd)) {
    return Promise.resolve(undefined as T)
  }
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
