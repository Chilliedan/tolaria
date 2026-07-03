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
})
