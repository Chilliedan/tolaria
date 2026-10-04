import { act, fireEvent, render, screen } from '@testing-library/react'
import { useState, type ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { EditorProps } from './Editor'
import { LazyEditor } from './LazyEditor'

vi.mock('../lib/startupPerformance', () => ({
  markStartupPhase: vi.fn(),
  waitForStartupPhase: vi.fn(() => Promise.resolve()),
}))

const editorModule = vi.hoisted(() => {
  let resolve!: () => void
  const ready = new Promise<void>((next) => { resolve = next })
  return { ready, resolve }
})

// Mirrors the real Editor, which renders the AI workspace surface it is given
// beside the note content.
vi.mock('./Editor', async () => {
  await editorModule.ready
  return {
    Editor: ({ showAIChat, aiWorkspaceSurface }: { showAIChat?: boolean; aiWorkspaceSurface?: ReactNode }) => (
      <div>
        Loaded editor
        {showAIChat && aiWorkspaceSurface}
      </div>
    ),
  }
})

function DraftSurface() {
  const [draft, setDraft] = useState('')
  return <textarea aria-label="AI draft" value={draft} onChange={(event) => setDraft(event.target.value)} />
}

describe('LazyEditor AI workspace surface', () => {
  it('keeps the AI workspace state while the editor bundle finishes loading', async () => {
    const props = {
      activeTabPath: '/vault/note.md',
      showAIChat: true,
      aiWorkspaceSurface: <DraftSurface />,
    } as EditorProps
    render(<LazyEditor {...props} />)

    expect(screen.getByTestId('editor-module-loading')).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('AI draft'), { target: { value: 'typed before the editor loaded' } })

    await act(async () => { editorModule.resolve() })
    expect(await screen.findByText('Loaded editor')).toBeInTheDocument()

    expect(screen.getByLabelText('AI draft')).toHaveValue('typed before the editor loaded')
  })
})
