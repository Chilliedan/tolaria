import { fireEvent, render, screen, within } from '@testing-library/react'
import { useRef } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AiWorkspace } from './AiWorkspace'
import { createAiAgentAvailability, createMissingAiAgentsStatus } from '../lib/aiAgents'
import type { VaultEntry } from '../types'

vi.mock('./useAiPanelController', () => ({
  useAiPanelController: () => ({
    agent: {
      messages: [],
      status: 'idle',
      sendMessage: vi.fn(),
      clearConversation: vi.fn(),
      addLocalMarker: vi.fn(),
    },
    input: '',
    setInput: vi.fn(),
    linkedEntries: [],
    hasContext: false,
    isActive: false,
    permissionMode: 'safe',
    handleSend: vi.fn(),
    handleNavigateWikilink: vi.fn(),
    handlePermissionModeChange: vi.fn(),
    handleNewChat: vi.fn(),
  }),
}))

// Counts how often the notes list a session's panel receives changes identity.
// React's dev performance tracks diff changed props, so swapping a 9000-note
// list for `undefined` on every chat switch is expensive in development.
vi.mock('./AiPanel', () => ({
  AiPanelView: ({ entries }: { entries?: VaultEntry[] }) => {
    const lastEntries = useRef(entries)
    const changes = useRef(0)
    if (lastEntries.current !== entries) {
      changes.current += 1
      lastEntries.current = entries
    }
    return <div data-testid="ai-panel-view" data-entries-changes={changes.current} />
  },
}))

const entries: VaultEntry[] = [{
  aliases: [], archived: false, belongsTo: [], color: null, createdAt: 1700000000, favorite: false,
  favoriteIndex: null, fileSize: 100, filename: 'active.md', hasH1: false, icon: null, isA: 'Note',
  listPropertiesDisplay: [], modifiedAt: 1700000000, order: null, organized: false, outgoingLinks: [],
  path: '/tmp/vault/active.md', properties: {}, relatedTo: [], relationships: {}, sidebarLabel: null,
  snippet: '', sort: null, status: null, template: null, title: 'Active', view: null, visible: null, wordCount: 0,
}]

function entriesChanges(conversationId: string): string | null {
  return within(screen.getByTestId(`ai-workspace-session-${conversationId}`))
    .getByTestId('ai-panel-view')
    .getAttribute('data-entries-changes')
}

describe('AiWorkspace inactive sessions', () => {
  beforeEach(() => { localStorage.clear() })

  it('keeps the notes list identity stable for sessions when the active chat changes', () => {
    render(
      <AiWorkspace
        open
        mode="side"
        aiAgentsStatus={{ ...createMissingAiAgentsStatus(), claude_code: createAiAgentAvailability('installed', '1.0.0') }}
        aiModelProviders={[]}
        activeEntry={entries[0]}
        entries={entries}
        conversationSettings={[
          { id: 'first-chat', title: 'First Chat', target_id: null, archived: false },
          { id: 'second-chat', title: 'Second Chat', target_id: null, archived: false },
        ]}
        vaultPath="/tmp/vault"
        onClose={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Second Chat' }))
    expect(screen.getByTestId('ai-workspace-session-first-chat')).toHaveClass('hidden')
    fireEvent.click(screen.getByRole('button', { name: 'First Chat' }))
    expect(screen.getByTestId('ai-workspace-session-second-chat')).toHaveClass('hidden')

    expect(entriesChanges('first-chat')).toBe('0')
    expect(entriesChanges('second-chat')).toBe('0')
  })
})
