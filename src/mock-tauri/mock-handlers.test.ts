import { describe, expect, it } from 'vitest'
import { mockHandlers } from './mock-handlers'

describe('mockHandlers git remote state', () => {
  it('keeps starter vaults local-only until a remote is added', () => {
    const vaultPath = '/Users/mock/Documents/Getting Started Test'

    expect(mockHandlers.create_getting_started_vault({ targetPath: vaultPath })).toBe(vaultPath)
    expect(mockHandlers.git_remote_status({ vaultPath }).hasRemote).toBe(false)

    expect(
      mockHandlers.git_add_remote({
        request: {
          vaultPath,
          remoteUrl: 'https://example.com/starter.git',
        },
      }).status,
    ).toBe('connected')

    expect(mockHandlers.git_remote_status({ vaultPath }).hasRemote).toBe(true)
  })

  it('starts empty vaults without a remote and keeps cloned vaults remote-backed', () => {
    const emptyVaultPath = '/Users/mock/Documents/Local Vault'
    const clonedVaultPath = '/Users/mock/Documents/Cloned Vault'

    expect(mockHandlers.create_empty_vault({ targetPath: emptyVaultPath })).toBe(emptyVaultPath)
    expect(mockHandlers.git_remote_status({ vaultPath: emptyVaultPath }).hasRemote).toBe(false)

    expect(mockHandlers.clone_repo({ url: 'https://example.com/repo.git', localPath: clonedVaultPath })).toContain(clonedVaultPath)
    expect(mockHandlers.git_remote_status({ vaultPath: clonedVaultPath }).hasRemote).toBe(true)
  })
})

describe('mockHandlers vault listing', () => {
  // The mock filesystem lives under /Users/mock. A registered vault outside it (e.g. the
  // dev-injected real demo-vault path for Getting Started) has no mock files, so listing it
  // must not return a second copy of the mock notes under another workspace.
  it('lists the mock notes for vaults in the mock filesystem', () => {
    const entries = mockHandlers.list_vault({ path: '/Users/mock/demo-vault-v2' })
    expect(entries.length).toBeGreaterThan(0)
    expect(mockHandlers.reload_vault({ path: '/Users/mock/Documents/Getting Started' })).toBe(entries)
  })

  it('lists nothing for a vault outside the mock filesystem', () => {
    expect(mockHandlers.list_vault({ path: '/Users/daniel/Projects/tolaria/demo-vault-v2' })).toEqual([])
    expect(mockHandlers.reload_vault({ path: '/Volumes/Notes/vault' })).toEqual([])
  })

  it('keeps listing the mock notes when no path is given', () => {
    expect(mockHandlers.list_vault({}).length).toBeGreaterThan(0)
  })
})
