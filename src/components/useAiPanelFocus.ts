import { useCallback, useEffect } from 'react'

interface UseAiPanelFocusArgs {
  inputRef: React.RefObject<HTMLDivElement | null>
  panelRef: React.RefObject<HTMLElement | null>
  hasMessages: boolean
  isActive: boolean
  onClose: () => void
  enabled?: boolean
}

function focusPreferredElement(
  panelRef: React.RefObject<HTMLElement | null>,
  inputRef: React.RefObject<HTMLDivElement | null>,
  shouldFocusPanel: boolean,
) {
  if (panelRef.current?.contains(document.activeElement)) return

  if (shouldFocusPanel) {
    panelRef.current?.focus()
    return
  }

  inputRef.current?.focus()
}

/** Both AI workspace layouts mark their root with this attribute. */
const AI_WORKSPACE_SELECTOR = '[data-ai-workspace-mode]'

/**
 * The delayed focus pass re-claims focus lost while the panel mounts, but it
 * must not pull focus back off a control in the surrounding AI workspace (such
 * as "New chat" in the side header) that took it after the first pass.
 */
function focusMovedToWorkspaceControl(panelRef: React.RefObject<HTMLElement | null>): boolean {
  const panel = panelRef.current
  const active = document.activeElement
  if (!panel || !active || panel.contains(active)) return false
  return Boolean(panel.closest(AI_WORKSPACE_SELECTOR)?.contains(active))
}

function shouldHandleEscape(
  event: KeyboardEvent,
  panelRef: React.RefObject<HTMLElement | null>,
): boolean {
  return event.key === 'Escape' && !!panelRef.current?.contains(document.activeElement)
}

export function useAiPanelFocus({
  inputRef,
  panelRef,
  hasMessages,
  isActive,
  onClose,
  enabled = true,
}: UseAiPanelFocusArgs) {
  const shouldFocusPanel = hasMessages || isActive

  useEffect(() => {
    if (!enabled) return

    const timer = setTimeout(() => {
      if (focusMovedToWorkspaceControl(panelRef)) return
      focusPreferredElement(panelRef, inputRef, shouldFocusPanel)
    }, 0)
    return () => clearTimeout(timer)
  }, [enabled, inputRef, panelRef, shouldFocusPanel])

  useEffect(() => {
    if (!enabled) return
    focusPreferredElement(panelRef, inputRef, shouldFocusPanel)
  }, [enabled, inputRef, panelRef, shouldFocusPanel])

  const handleEscape = useCallback((event: KeyboardEvent) => {
    if (!shouldHandleEscape(event, panelRef)) return

    event.preventDefault()
    onClose()
  }, [onClose, panelRef])

  useEffect(() => {
    if (!enabled) return
    window.addEventListener('keydown', handleEscape)
    return () => window.removeEventListener('keydown', handleEscape)
  }, [enabled, handleEscape])
}
