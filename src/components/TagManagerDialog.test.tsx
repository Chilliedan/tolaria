import { act, fireEvent, render, renderHook, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { makeEntry } from '../test-utils/noteListTestUtils'
import { useTagManagerDialogState } from '../hooks/useTagManagerDialogState'
import { TagManagerDialog } from './TagManagerDialog'

vi.mock('../lib/telemetry', () => ({ trackEvent: vi.fn() }))

const entries = [
  makeEntry({ path: '/a.md', title: 'Alpha', properties: { tags: ['blues', 'live'] } }),
  makeEntry({ path: '/b.md', title: 'Beta', properties: { tags: ['blues', 'jazz'] } }),
]

function renderDialog(updateFrontmatter = vi.fn().mockResolvedValue(undefined), onOpenNote = vi.fn()) {
  const onClose = vi.fn()
  render(
    <TagManagerDialog
      open
      onClose={onClose}
      entries={entries}
      locale="en"
      onUpdateFrontmatter={updateFrontmatter}
      onOpenNote={onOpenNote}
    />,
  )
  return { updateFrontmatter, onOpenNote, onClose }
}

function openRowMenu(tag: string) {
  fireEvent.pointerDown(screen.getByTestId(`tag-manager-menu-${tag}`), { button: 0, ctrlKey: false })
}

describe('TagManagerDialog', () => {
  beforeEach(() => vi.clearAllMocks())

  it('lists tags with counts, most used first', () => {
    renderDialog()
    const rows = screen.getAllByTestId(/^tag-manager-row-/)
    expect(rows.map((row) => row.dataset.testid)).toEqual([
      'tag-manager-row-blues', 'tag-manager-row-jazz', 'tag-manager-row-live',
    ])
    expect(screen.getByTestId('tag-manager-row-blues')).toHaveTextContent('2 notes')
    expect(within(screen.getByTestId('tag-manager-row-jazz')).getByText('1 note')).toBeInTheDocument()
  })

  it('filters tags case-insensitively', () => {
    renderDialog()
    fireEvent.change(screen.getByPlaceholderText('Filter tags…'), { target: { value: 'JA' } })
    expect(screen.getAllByTestId(/^tag-manager-row-/)).toHaveLength(1)
  })

  it('shows notes for a tag and opens one', () => {
    const { onOpenNote, onClose } = renderDialog()
    openRowMenu('jazz')
    fireEvent.click(screen.getByText('Show notes'))
    fireEvent.click(screen.getByRole('button', { name: 'Beta' }))
    expect(onOpenNote).toHaveBeenCalledWith(entries[1])
    expect(onClose).toHaveBeenCalled()
  })

  it('renames after confirmation and reports success', async () => {
    const { updateFrontmatter } = renderDialog()
    openRowMenu('blues')
    fireEvent.click(screen.getByText('Rename…'))
    fireEvent.change(screen.getByPlaceholderText('New tag name'), { target: { value: 'soul' } })
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))

    expect(screen.getByText('Rename “blues” to “soul” in 2 notes?')).toBeInTheDocument()
    expect(updateFrontmatter).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(screen.getByText('Updated 2 notes.')).toBeInTheDocument())
    expect(updateFrontmatter).toHaveBeenCalledWith('/a.md', 'tags', ['soul', 'live'], { silent: true })
  })

  it('focuses the rename input once the row menu has closed', async () => {
    renderDialog()
    openRowMenu('blues')
    fireEvent.click(screen.getByText('Rename…'))

    const input = screen.getByPlaceholderText('New tag name')
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument())
    await waitFor(() => expect(document.activeElement).toBe(input))
  })

  it('focuses the merge target picker once the row menu has closed', async () => {
    renderDialog()
    openRowMenu('blues')
    fireEvent.click(screen.getByText('Merge into…'))

    const trigger = screen.getByTestId('tag-manager-merge-target')
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument())
    await waitFor(() => expect(document.activeElement).toBe(trigger))
  })

  it('warns when a rename targets an existing tag', () => {
    renderDialog()
    openRowMenu('live')
    fireEvent.click(screen.getByText('Rename…'))
    fireEvent.change(screen.getByPlaceholderText('New tag name'), { target: { value: 'jazz' } })
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    expect(screen.getByText('“jazz” already exists. Merge “live” into it in 1 note?')).toBeInTheDocument()
  })

  it('uses singular copy when a rename touches one note', async () => {
    renderDialog()
    openRowMenu('live')
    fireEvent.click(screen.getByText('Rename…'))
    fireEvent.change(screen.getByPlaceholderText('New tag name'), { target: { value: 'gig' } })
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    expect(screen.getByText('Rename “live” to “gig” in 1 note?')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(screen.getByText('Updated 1 note.')).toBeInTheDocument())
  })

  it('uses singular copy when the only affected note fails', async () => {
    renderDialog(vi.fn().mockRejectedValue(new Error('locked')))
    openRowMenu('jazz')
    fireEvent.click(screen.getByText('Delete…'))
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(screen.getByText('Changed 0 of 1 note; 1 failed:')).toBeInTheDocument())
  })

  it('cancelling a confirmation writes nothing', () => {
    const { updateFrontmatter } = renderDialog()
    openRowMenu('live')
    fireEvent.click(screen.getByText('Delete…'))
    expect(screen.getByText('Remove “live” from 1 note?')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(updateFrontmatter).not.toHaveBeenCalled()
    expect(screen.queryByText('Remove “live” from 1 note?')).not.toBeInTheDocument()
  })

  it('reports partial failures with note titles', async () => {
    const updateFrontmatter = vi.fn()
      .mockRejectedValueOnce(new Error('locked'))
      .mockResolvedValueOnce(undefined)
    renderDialog(updateFrontmatter)
    openRowMenu('blues')
    fireEvent.click(screen.getByText('Delete…'))
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(screen.getByText('Changed 1 of 2 notes; 1 failed:')).toBeInTheDocument())
    expect(screen.getByTestId('tag-manager-failed-list')).toHaveTextContent('Alpha')
  })

  it('shows an empty state when the vault has no tags', () => {
    render(
      <TagManagerDialog open onClose={vi.fn()} entries={[]} locale="en" onUpdateFrontmatter={vi.fn()} onOpenNote={vi.fn()} />,
    )
    expect(screen.getByText('No tags in this vault yet.')).toBeInTheDocument()
  })

  it('cannot be closed or navigated away from while an apply is in progress', async () => {
    let finishFirstWrite: () => void = () => {}
    const updateFrontmatter = vi.fn()
      .mockImplementationOnce(() => new Promise<void>((resolve) => { finishFirstWrite = resolve }))
      .mockResolvedValue(undefined)
    const { onClose, onOpenNote } = renderDialog(updateFrontmatter)
    openRowMenu('blues')
    fireEvent.click(screen.getByText('Show notes'))
    openRowMenu('blues')
    fireEvent.click(screen.getByText('Delete…'))
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(screen.getByText('Updating 0 of 2 notes…')).toBeInTheDocument())

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' })
    fireEvent.pointerDown(document.body)
    expect(screen.queryByRole('button', { name: 'Close' })).not.toBeInTheDocument()
    const noteButton = screen.getByRole('button', { name: 'Alpha' })
    expect(noteButton).toBeDisabled()
    fireEvent.click(noteButton)
    expect(screen.getByTestId('tag-manager-menu-jazz')).toBeDisabled()

    expect(onClose).not.toHaveBeenCalled()
    expect(onOpenNote).not.toHaveBeenCalled()
    expect(screen.getByText('Updating 0 of 2 notes…')).toBeInTheDocument()

    await act(async () => { finishFirstWrite() })
    await waitFor(() => expect(screen.getByText('Updated 2 notes.')).toBeInTheDocument())
    expect(screen.getByRole('button', { name: 'Close' })).toBeInTheDocument()
  })

  it('cancels the inline rename editor on Escape instead of closing the dialog', () => {
    const { updateFrontmatter, onClose } = renderDialog()
    openRowMenu('blues')
    fireEvent.click(screen.getByText('Rename…'))
    expect(screen.getByPlaceholderText('New tag name')).toBeInTheDocument()

    fireEvent.keyDown(screen.getByPlaceholderText('New tag name'), { key: 'Escape' })

    expect(screen.queryByPlaceholderText('New tag name')).not.toBeInTheDocument()
    expect(onClose).not.toHaveBeenCalled()
    expect(updateFrontmatter).not.toHaveBeenCalled()
    expect(screen.getByTestId('tag-manager-row-blues')).toBeInTheDocument()
  })
})

describe('useTagManagerDialogState property pinning', () => {
  it('pins a pending action to the property selected when it was proposed, and clears it when the property changes', async () => {
    const multiPropertyEntries = [
      makeEntry({ path: '/a.md', title: 'Alpha', properties: { tags: ['urgent'], categories: ['urgent'] } }),
    ]
    const updateFrontmatter = vi.fn().mockResolvedValue(undefined)
    const { result } = renderHook(() =>
      useTagManagerDialogState({ entries: multiPropertyEntries, locale: 'en', onUpdateFrontmatter: updateFrontmatter }),
    )

    expect(result.current.property).toBe('tags')
    act(() => result.current.propose({ kind: 'delete', tag: 'urgent' }))
    expect(result.current.pending?.property).toBe('tags')

    // Switching the selected property must clear any pending confirmation, in-progress row
    // edit, expanded notes, and the filter — never leave a confirmation dangling against a
    // property the user has since navigated away from.
    act(() => result.current.setSelectedProperty('categories'))
    expect(result.current.property).toBe('categories')
    expect(result.current.pending).toBeNull()
    expect(result.current.rowEdit).toBeNull()
    expect(result.current.expandedTag).toBeNull()
    expect(result.current.filter).toBe('')

    // Proposing again now pins to the newly selected property, and applying writes only that
    // property — never the one that happened to be selected earlier.
    act(() => result.current.propose({ kind: 'delete', tag: 'urgent' }))
    expect(result.current.pending?.property).toBe('categories')
    await act(async () => { await result.current.applyPending() })

    expect(updateFrontmatter).toHaveBeenCalledTimes(1)
    expect(updateFrontmatter).toHaveBeenCalledWith('/a.md', 'categories', [], { silent: true })
  })
})
