import { describe, expect, it, vi } from 'vitest'
import { buildNoteCommands } from './noteCommands'

function baseConfig(overrides: Record<string, unknown> = {}) {
  return {
    hasActiveNote: false,
    activeTabPath: null,
    isArchived: false,
    onCreateNote: vi.fn(),
    onSave: vi.fn(),
    onPastePlainText: vi.fn(),
    onDeleteNote: vi.fn(),
    onArchiveNote: vi.fn(),
    onUnarchiveNote: vi.fn(),
    ...overrides,
  }
}

describe('buildNoteCommands manage tags', () => {
  it('exposes a Manage Tags command that opens the tag manager', () => {
    const onManageTags = vi.fn()
    const commands = buildNoteCommands(baseConfig({ onManageTags }))
    const command = commands.find((item) => item.id === 'manage-tags')
    expect(command).toMatchObject({ label: 'Manage Tags', group: 'Note', enabled: true })
    command?.execute()
    expect(onManageTags).toHaveBeenCalled()
  })

  it('disables Manage Tags when no handler is wired', () => {
    const commands = buildNoteCommands(baseConfig({ onManageTags: undefined }))
    expect(commands.find((item) => item.id === 'manage-tags')?.enabled).toBe(false)
  })
})
