import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { VaultEntry } from '../types'
import type { ActionHistoryController } from './useActionHistory'
import { useEntryActions } from './useEntryActions'

const NOTE_PATH = '/vault/note/test.md'

const makeEntry = (overrides: Partial<VaultEntry> = {}): VaultEntry => ({
  path: NOTE_PATH,
  filename: 'test.md',
  title: 'Test Note',
  isA: 'Note',
  aliases: [],
  belongsTo: [],
  relatedTo: [],
  status: null,
  archived: false,
  modifiedAt: 1700000000,
  createdAt: 1700000000,
  fileSize: 100,
  snippet: '',
  wordCount: 0,
  relationships: {},
  icon: null,
  color: null,
  order: null,
  sidebarLabel: null,
  template: null,
  sort: null,
  view: null,
  visible: null,
  organized: false,
  favorite: false,
  favoriteIndex: null,
  listPropertiesDisplay: [],
  outgoingLinks: [],
  properties: {},
  hasH1: false,
  ...overrides,
})

describe('useEntryActions localized feedback', () => {
  const updateEntry = vi.fn()
  const handleUpdateFrontmatter = vi.fn().mockResolvedValue(undefined)
  const handleDeleteProperty = vi.fn().mockResolvedValue(undefined)
  const setToastMessage = vi.fn()
  const record = vi.fn()
  const actionHistory: ActionHistoryController = {
    canRedo: false,
    canUndo: false,
    redoLabel: null,
    undoLabel: null,
    record,
    recordAction: vi.fn(),
    isReplaying: vi.fn(() => false),
    undo: vi.fn(async () => false),
    redo: vi.fn(async () => false),
    withoutRecording: vi.fn(async (run) => await run()),
  }

  beforeEach(() => {
    vi.clearAllMocks()
    handleUpdateFrontmatter.mockResolvedValue(undefined)
    vi.spyOn(console, 'error').mockImplementation(() => {})
  })

  function setup(entries: VaultEntry[]) {
    return renderHook(() =>
      useEntryActions({
        entries,
        updateEntry,
        handleUpdateFrontmatter,
        handleDeleteProperty,
        setToastMessage,
        createTypeEntry: vi.fn(),
        actionHistory,
        locale: 'de-DE',
      }),
    )
  }

  it('shows the archive toast and records the undo label in the app language', async () => {
    const { result } = setup([makeEntry()])

    await act(async () => { await result.current.handleArchiveNote(NOTE_PATH) })

    expect(setToastMessage).toHaveBeenCalledWith('Notiz archiviert')
    expect(record).toHaveBeenCalledWith(expect.objectContaining({ label: 'Notiz archivieren' }))
  })

  it('shows the rollback toast in the app language when organizing fails', async () => {
    handleUpdateFrontmatter.mockRejectedValueOnce(new Error('disk full'))
    const { result } = setup([makeEntry()])

    await act(async () => { await result.current.handleToggleOrganized(NOTE_PATH) })

    expect(setToastMessage).toHaveBeenCalledWith('Konnte nicht als organisiert markiert werden – rückgängig gemacht')
  })

  it('shows the favorites reorder failure in the app language', async () => {
    handleUpdateFrontmatter.mockRejectedValueOnce(new Error('disk full'))
    const { result } = setup([
      makeEntry({ path: '/vault/a.md', favorite: true, favoriteIndex: 0 }),
      makeEntry({ path: '/vault/b.md', favorite: true, favoriteIndex: 1 }),
    ])

    await act(async () => { await result.current.handleReorderFavorites(['/vault/b.md', '/vault/a.md']) })

    expect(setToastMessage).toHaveBeenCalledWith('Favoriten konnten nicht neu sortiert werden – rückgängig gemacht')
  })
})
