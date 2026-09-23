import { describe, expect, it } from 'vitest'
import { isAutoGitCheckpointEnabled } from './autoGitEnablement'

describe('isAutoGitCheckpointEnabled', () => {
  it('requires the setting on desktop, where AutoGit is opt-in', () => {
    const desktop = { automaticGitEnabled: true, serverCommitsWrites: false }

    expect(isAutoGitCheckpointEnabled({ ...desktop, autoGitSetting: true })).toBe(true)
    expect(isAutoGitCheckpointEnabled({ ...desktop, autoGitSetting: false })).toBe(false)
    expect(isAutoGitCheckpointEnabled({ ...desktop, autoGitSetting: null })).toBe(false)
  })

  it('honours an explicit opt-out on the web server', () => {
    // The switch used to be ignored here: pushes ran regardless of it.
    expect(isAutoGitCheckpointEnabled({
      automaticGitEnabled: true,
      autoGitSetting: false,
      serverCommitsWrites: true,
    })).toBe(false)
  })

  it('defaults to on when the web server has no stored preference', () => {
    // Web settings currently live in memory and start unset; defaulting to off
    // would silently stop syncing a vault whose whole purpose is to sync.
    expect(isAutoGitCheckpointEnabled({
      automaticGitEnabled: true,
      autoGitSetting: null,
      serverCommitsWrites: true,
    })).toBe(true)
  })

  it('stays off wherever Git automation itself is off', () => {
    expect(isAutoGitCheckpointEnabled({
      automaticGitEnabled: false,
      autoGitSetting: true,
      serverCommitsWrites: true,
    })).toBe(false)
  })
})
