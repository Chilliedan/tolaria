import { act, render, screen } from '@testing-library/react'
import { useRef } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAiPanelFocus } from './useAiPanelFocus'

function FocusHarness({ hasMessages }: { hasMessages: boolean }) {
  const panelRef = useRef<HTMLElement | null>(null)
  const inputRef = useRef<HTMLDivElement | null>(null)
  useAiPanelFocus({ inputRef, panelRef, hasMessages, isActive: false, onClose: vi.fn() })

  return (
    <>
      <button type="button">Outside the workspace</button>
      <div data-ai-workspace-mode="side">
        <button type="button">New chat</button>
        <aside ref={panelRef} tabIndex={-1} data-testid="panel">
          <div ref={inputRef} contentEditable suppressContentEditableWarning data-testid="composer" />
        </aside>
      </div>
    </>
  )
}

describe('useAiPanelFocus', () => {
  beforeEach(() => { vi.useFakeTimers() })
  afterEach(() => { vi.useRealTimers() })

  it('leaves focus on a workspace control focused before the delayed focus pass', () => {
    render(<FocusHarness hasMessages />)
    expect(screen.getByTestId('panel')).toHaveFocus()

    const newChat = screen.getByRole('button', { name: 'New chat' })
    newChat.focus()
    act(() => { vi.runAllTimers() })

    expect(newChat).toHaveFocus()
  })

  it('still reclaims focus when it lands outside the workspace before the delayed pass', () => {
    render(<FocusHarness hasMessages={false} />)
    expect(screen.getByTestId('composer')).toHaveFocus()

    screen.getByRole('button', { name: 'Outside the workspace' }).focus()
    act(() => { vi.runAllTimers() })

    expect(screen.getByTestId('composer')).toHaveFocus()
  })
})
