import { describe, it, expect } from 'vitest'
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
    ]) {
      expect(SERVER_COMMANDS.has(cmd)).toBe(true)
    }
  })
})
