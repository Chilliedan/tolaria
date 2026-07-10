import { describe, it, expect, vi } from 'vitest'
// Importing the bridge runs its module-load `markWebServerBridge()`. Mock the
// module to a no-op so that side effect can't leak the "web server" flag into
// other test files running in the same worker.
vi.mock('../lib/webServerBridge', () => ({
  isWebServerBridge: () => false,
  markWebServerBridge: () => {},
}))
import { SERVER_COMMANDS } from './mockBridge'

describe('mockBridge SERVER_COMMANDS', () => {
  it('routes git sync commands to the server (Phase 5)', () => {
    const gitCommands = [
      'git_remote_status',
      'git_author_identity',
      'git_commit',
      'git_push',
      'git_pull',
      'git_resolve_conflict',
      'git_commit_conflict_resolution',
    ]
    for (const cmd of gitCommands) {
      expect(SERVER_COMMANDS.has(cmd)).toBe(true)
    }
  })

  it('routes git read + folder commands to the server', () => {
    for (const cmd of [
      'is_git_repo', 'get_modified_files', 'get_modified_files_with_stats',
      'get_file_diff', 'get_file_diff_at_commit', 'get_file_history',
      'get_last_commit_info', 'get_vault_pulse', 'git_file_url',
      'git_add_remote', 'git_discard_file', 'init_git_repo',
      'create_vault_folder', 'delete_vault_folder', 'rename_vault_folder',
      'move_note_to_folder', 'list_views', 'save_view_cmd', 'delete_view_cmd',
    ]) {
      expect(SERVER_COMMANDS.has(cmd)).toBe(true)
    }
  })

  it('no-ops desktop-only commands without hitting the server', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    const { mockInvoke } = await import('./mockBridge')
    for (const cmd of ['copy_text_to_clipboard', 'update_current_window_min_size', 'stream_ai_model', 'check_for_app_update']) {
      await expect(mockInvoke(cmd, {})).resolves.toBeUndefined()
    }
    expect(fetchMock).not.toHaveBeenCalled()
  })
})
