import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { makeEntry } from '../test-utils/noteListTestUtils'
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

  it('warns when a rename targets an existing tag', () => {
    renderDialog()
    openRowMenu('live')
    fireEvent.click(screen.getByText('Rename…'))
    fireEvent.change(screen.getByPlaceholderText('New tag name'), { target: { value: 'jazz' } })
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    expect(screen.getByText('“jazz” already exists. Merge “live” into it in 1 notes?')).toBeInTheDocument()
  })

  it('cancelling a confirmation writes nothing', () => {
    const { updateFrontmatter } = renderDialog()
    openRowMenu('live')
    fireEvent.click(screen.getByText('Delete…'))
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(updateFrontmatter).not.toHaveBeenCalled()
    expect(screen.queryByText('Remove “live” from 1 notes?')).not.toBeInTheDocument()
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
})
