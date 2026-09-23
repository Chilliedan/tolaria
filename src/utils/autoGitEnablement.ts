interface AutoGitEnablement {
  /** Git automation as a whole (Git features on, not a detached note window). */
  automaticGitEnabled: boolean
  /** The user's stored AutoGit preference; null when they never set one. */
  autoGitSetting: boolean | null | undefined
  /** True when each write is committed for us (the web server does this). */
  serverCommitsWrites: boolean
}

/**
 * Whether the debounced AutoGit checkpoint (commit + push) should run.
 *
 * On desktop AutoGit is opt-in, so the stored preference is required. On the
 * web server the vault exists to sync and its settings are not persisted yet,
 * so an unset preference defaults to on — but an explicit opt-out is honoured
 * rather than overridden, which is what the settings switch promises.
 *
 * Note this governs pushing only: the web server commits every write as it
 * lands, independently of this and of the switch.
 */
export function isAutoGitCheckpointEnabled({
  automaticGitEnabled,
  autoGitSetting,
  serverCommitsWrites,
}: AutoGitEnablement): boolean {
  if (!automaticGitEnabled) return false
  return serverCommitsWrites ? autoGitSetting !== false : autoGitSetting === true
}
