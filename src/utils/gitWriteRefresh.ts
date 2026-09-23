interface GitWriteRefreshOptions {
  /** True when each vault write is committed for us (the web server does this). */
  serverCommitsWrites: boolean
  refreshModifiedFiles: () => Promise<void>
  refreshRemoteStatuses: () => Promise<void>
}

/**
 * Refresh the Git surfaces AutoGit reads after a vault write.
 *
 * AutoGit checkpoints only when it can see pending work: either a dirty
 * working tree or a repository ahead of its remote. On desktop the write
 * leaves the file dirty, so refreshing the modified-file list is enough.
 *
 * On the web server the write is committed as it lands, so the working tree
 * is clean the moment the save returns and the only remaining signal is the
 * repository being ahead. That status is otherwise refreshed just on mount
 * and after explicit commit flows, so without this AutoGit would never see
 * the work and would silently skip every checkpoint.
 */
export async function refreshGitSurfacesAfterWrite({
  serverCommitsWrites,
  refreshModifiedFiles,
  refreshRemoteStatuses,
}: GitWriteRefreshOptions): Promise<void> {
  const remoteStatus = serverCommitsWrites ? refreshRemoteStatuses() : Promise.resolve()
  try {
    await refreshModifiedFiles()
  } finally {
    await remoteStatus
  }
}
