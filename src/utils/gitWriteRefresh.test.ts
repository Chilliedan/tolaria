import { describe, expect, it, vi } from 'vitest'
import { refreshGitSurfacesAfterWrite } from './gitWriteRefresh'

function refreshers() {
  return {
    refreshModifiedFiles: vi.fn(async () => {}),
    refreshRemoteStatuses: vi.fn(async () => {}),
  }
}

describe('refreshGitSurfacesAfterWrite', () => {
  it('refreshes only the working tree when the client owns committing', async () => {
    // Desktop: a save leaves the file dirty, so the modified-file list is the
    // signal AutoGit needs. Polling the remote on every keystroke-driven save
    // would add git calls for nothing.
    const { refreshModifiedFiles, refreshRemoteStatuses } = refreshers()

    await refreshGitSurfacesAfterWrite({
      serverCommitsWrites: false,
      refreshModifiedFiles,
      refreshRemoteStatuses,
    })

    expect(refreshModifiedFiles).toHaveBeenCalledOnce()
    expect(refreshRemoteStatuses).not.toHaveBeenCalled()
  })

  it('also refreshes remote status when the server commits each write', async () => {
    // Web: the server commits the save immediately, so the working tree is
    // already clean and "ahead of remote" is the only remaining signal that
    // there is anything to push.
    const { refreshModifiedFiles, refreshRemoteStatuses } = refreshers()

    await refreshGitSurfacesAfterWrite({
      serverCommitsWrites: true,
      refreshModifiedFiles,
      refreshRemoteStatuses,
    })

    expect(refreshModifiedFiles).toHaveBeenCalledOnce()
    expect(refreshRemoteStatuses).toHaveBeenCalledOnce()
  })

  it('still refreshes remote status when listing modified files fails', async () => {
    const refreshRemoteStatuses = vi.fn(async () => {})

    await expect(refreshGitSurfacesAfterWrite({
      serverCommitsWrites: true,
      refreshModifiedFiles: async () => { throw new Error('offline') },
      refreshRemoteStatuses,
    })).rejects.toThrow('offline')

    expect(refreshRemoteStatuses).toHaveBeenCalledOnce()
  })
})
