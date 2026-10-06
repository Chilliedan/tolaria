import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { VaultEntry } from '../types'
import { useVaultLoader } from './useVaultLoader'

function entryAt(path: string, title: string): VaultEntry {
  return {
    path, filename: path.split('/').pop() ?? 'note.md', title,
    isA: 'Note', aliases: [], belongsTo: [], relatedTo: [], status: null, archived: false,
    modifiedAt: 1700000000, createdAt: 1700000000, fileSize: 100, snippet: '', wordCount: 0,
    relationships: { Topics: ['[[a]]'] }, icon: null, color: null, order: null, template: null, sort: null,
    outgoingLinks: ['a'], sidebarLabel: null, view: null, visible: null, organized: false, favorite: false,
    favoriteIndex: null, listPropertiesDisplay: [], properties: { Tags: ['x'] }, hasH1: false,
  }
}

let vaultEntries: VaultEntry[] = []

// Every vault load returns brand-new objects, as IPC deserialisation does.
const backendInvokeFn = vi.fn((cmd: string) => {
  if (cmd === 'list_vault' || cmd === 'reload_vault') return Promise.resolve(structuredClone(vaultEntries))
  if (cmd === 'list_vault_folders' || cmd === 'list_views' || cmd === 'get_modified_files') return Promise.resolve([])
  return Promise.resolve(null)
})

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

vi.mock('../mock-tauri', () => ({
  isTauri: () => false,
  mockInvoke: (cmd: string) => backendInvokeFn(cmd),
}))

async function renderLoadedVault() {
  const hook = renderHook(() => useVaultLoader('/vault'))
  await waitFor(() => expect(hook.result.current.entries).toHaveLength(vaultEntries.length))
  return hook
}

describe('useVaultLoader entry identity across refreshes', () => {
  beforeEach(() => {
    vaultEntries = [entryAt('/vault/a.md', 'A'), entryAt('/vault/b.md', 'B')]
    window.history.replaceState({}, '', '/')
  })

  it('keeps the entries array when a reload returns identical data', async () => {
    const { result } = await renderLoadedVault()
    const before = result.current.entries

    await act(async () => { await result.current.reloadVault() })

    expect(result.current.entries).toBe(before)
  })

  it('keeps unchanged entry objects when a reload changes one note', async () => {
    const { result } = await renderLoadedVault()
    const [a, b] = result.current.entries
    vaultEntries = [entryAt('/vault/a.md', 'A'), entryAt('/vault/b.md', 'B renamed')]

    await act(async () => { await result.current.reloadVault() })

    expect(result.current.entries[0]).toBe(a)
    expect(result.current.entries[1]).not.toBe(b)
    expect(result.current.entries[1].title).toBe('B renamed')
  })
})
