import { useCallback, useRef } from 'react'
import type { AiAgentId, AiAgentsStatus } from '../lib/aiAgents'
import type { AppLocale, UiLanguagePreference } from '../lib/i18n'
import type { ThemeMode } from '../lib/themeMode'
import type { VaultAiGuidanceStatus } from '../lib/vaultAiGuidance'
import { useAppKeyboard } from './useAppKeyboard'
import { useCommandRegistry } from './useCommandRegistry'
import type { CommandAction } from './useCommandRegistry'
import { createCommandRegistryConfig, enabledAiChatToggle } from './appCommandsConfig'
import { useKeyboardNavigation } from './useKeyboardNavigation'
import { useMenuEvents } from './useMenuEvents'
import type { NoteWidthMode, SidebarSelection, SidebarFilter, VaultEntry } from '../types'
import { requestAddRemote } from '../utils/addRemoteEvents'
import type { NoteListFilter } from '../utils/noteListHelpers'
import type { ViewMode } from './useViewMode'
import type { ImmediateCreateOptions } from './useNoteCreation'
import type { NoteListMultiSelectionCommands } from '../components/note-list/multiSelectionCommands'
import type { GitRepositoryOption } from '../utils/gitRepositories'
import type { RichEditorBlockTypeDefinition } from '../utils/richEditorBlockTypes'

export interface AppCommandsConfig {
  activeTabPath: string | null
  activeTabPathRef: React.MutableRefObject<string | null>
  entries: VaultEntry[]
  visibleNotesRef: React.RefObject<VaultEntry[]>
  multiSelectionCommandRef: React.MutableRefObject<NoteListMultiSelectionCommands | null>
  modifiedCount: number
  selection: SidebarSelection
  onQuickOpen: () => void
  onCommandPalette: () => void
  onSearch: () => void
  onFindInNote?: () => void
  onUndo?: () => void
  onRedo?: () => void
  canUndo?: boolean
  canRedo?: boolean
  undoLabel?: string | null
  redoLabel?: string | null
  onReplaceInNote?: () => void
  onPastePlainText: () => void
  onCreateNote: (type?: string, options?: ImmediateCreateOptions) => void
  onCreateNoteOfType: (type: string) => void
  onSave: () => void
  onOpenSettings: () => void
  onOpenFeedback?: () => void
  onDeleteNote: (path: string) => void
  onArchiveNote: (path: string) => void
  onUnarchiveNote: (path: string) => void
  onCommitPush: () => void
  onGenerateCommitMessage?: () => void
  onPull?: () => void
  onPullRepository?: (path: string) => void
  onResolveConflicts?: () => void
  onSetViewMode: (mode: ViewMode) => void
  onToggleInspector: () => void
  onToggleDiff?: () => void
  onToggleRawEditor?: () => void
  selectedViewName?: string
  onMoveSelectedViewUp?: () => void
  onMoveSelectedViewDown?: () => void
  canMoveSelectedViewUp?: boolean
  canMoveSelectedViewDown?: boolean
  noteWidth?: NoteWidthMode
  defaultNoteWidth?: NoteWidthMode
  onSetNoteWidth?: (mode: NoteWidthMode) => void
  onSetDefaultNoteWidth?: (mode: NoteWidthMode) => void
  activeNoteModified: boolean
  onZoomIn: () => void
  onZoomOut: () => void
  onZoomReset: () => void
  zoomLevel: number
  onSelect: (sel: SidebarSelection) => void
  onRenameFolder?: () => void
  onDeleteFolder?: () => void
  showInbox?: boolean
  onReplaceActiveTab: (entry: VaultEntry) => void
  onSelectNote: (entry: VaultEntry) => void
  onGoBack?: () => void
  onGoForward?: () => void
  canGoBack?: boolean
  canGoForward?: boolean
  onOpenVault?: () => void
  onCreateEmptyVault?: () => void
  onAddRemote?: () => void
  canAddRemote?: boolean
  gitFeaturesEnabled?: boolean
  isGitVault?: boolean
  gitRepositories?: GitRepositoryOption[]
  onInitializeGit?: () => void
  onCreateType?: () => void
  onManageTags?: () => void
  aiFeaturesEnabled?: boolean
  onToggleAIChat?: () => void
  onToggleTableOfContents?: () => void
  onCheckForUpdates?: () => void
  onRemoveActiveVault?: () => void
  onRestoreGettingStarted?: () => void
  isGettingStartedHidden?: boolean
  vaultCount?: number
  locale?: AppLocale
  systemLocale?: AppLocale
  selectedUiLanguage?: UiLanguagePreference
  onSetUiLanguage?: (language: UiLanguagePreference) => void
  onSetThemeMode?: (mode: ThemeMode) => void
  mcpStatus?: string
  onInstallMcp?: () => void
  aiAgentsStatus?: AiAgentsStatus
  vaultAiGuidanceStatus?: VaultAiGuidanceStatus
  onOpenAiAgents?: () => void
  onRestoreVaultAiGuidance?: () => void
  onSetDefaultAiAgent?: (agent: AiAgentId) => void
  selectedAiAgent?: AiAgentId
  onCycleDefaultAiAgent?: () => void
  selectedAiAgentLabel?: string
  claudeCodeStatus?: string
  claudeCodeVersion?: string
  onReloadVault?: () => void
  onRepairVault?: () => void
  onSetNoteIcon?: () => void
  onRemoveNoteIcon?: () => void
  onChangeNoteType?: () => void
  onMoveNoteToFolder?: () => void
  canMoveNoteToFolder?: boolean
  onTurnCurrentBlockInto?: (target: RichEditorBlockTypeDefinition) => void
  activeNoteHasIcon?: boolean
  noteListFilter?: NoteListFilter
  onSetNoteListFilter?: (filter: NoteListFilter) => void
  onOpenInNewWindow?: () => void
  onRevealActiveFile?: (path: string) => void
  onCopyActiveFilePath?: (path: string) => void
  onCopyActiveDeepLink?: (path: string) => void
  onOpenActiveFileExternal?: (path: string) => void
  onExportNoteAsPdf?: () => void
  onRevealSelectedFolder?: () => void
  onCopySelectedFolderPath?: () => void
  onToggleFavorite?: (path: string) => void
  onToggleOrganized?: (path: string) => void
  onCustomizeNoteListColumns?: () => void
  canCustomizeNoteListColumns?: boolean
  noteListColumnsLabel?: string
  onRestoreDeletedNote?: () => void
  canRestoreDeletedNote?: boolean
}

function createKeyboardActions(
  config: AppCommandsConfig,
): Omit<Parameters<typeof useAppKeyboard>[0], 'onArchiveNote'> {
  return {
    onQuickOpen: config.onQuickOpen,
    onCommandPalette: config.onCommandPalette,
    onSearch: config.onSearch,
    onFindInNote: config.onFindInNote,
    onReplaceInNote: config.onReplaceInNote,
    onPastePlainText: config.onPastePlainText,
    onCreateNote: config.onCreateNote,
    onSave: config.onSave,
    onUndo: config.onUndo,
    onRedo: config.onRedo,
    canUndo: config.canUndo,
    canRedo: config.canRedo,
    onOpenSettings: config.onOpenSettings,
    onDeleteNote: config.onDeleteNote,
    onSetViewMode: config.onSetViewMode,
    onZoomIn: config.onZoomIn,
    onZoomOut: config.onZoomOut,
    onZoomReset: config.onZoomReset,
    onGoBack: config.onGoBack,
    onGoForward: config.onGoForward,
    onToggleAIChat: enabledAiChatToggle(config),
    onToggleTableOfContents: config.onToggleTableOfContents,
    onToggleRawEditor: config.onToggleRawEditor,
    onToggleInspector: config.onToggleInspector,
    onToggleFavorite: config.onToggleFavorite,
    onToggleOrganized: config.onToggleOrganized,
    onOpenInNewWindow: config.onOpenInNewWindow,
    activeTabPathRef: config.activeTabPathRef,
    multiSelectionCommandRef: config.multiSelectionCommandRef,
  }
}

function createMenuEventHandlers(
  config: AppCommandsConfig,
  selectFilter: (filter: SidebarFilter) => void,
  viewChanges: () => void,
): Omit<Parameters<typeof useMenuEvents>[0], 'onArchiveNote'> {
  return {
    ...createMenuEventActionHandlers(config, selectFilter),
    ...createMenuEventVaultHandlers(config, viewChanges),
    ...createMenuEventState(config),
  }
}

function createMenuEventActionHandlers(
  config: AppCommandsConfig,
  selectFilter: (filter: SidebarFilter) => void,
): Pick<
  Omit<Parameters<typeof useMenuEvents>[0], 'onArchiveNote'>,
  | 'onSetViewMode'
  | 'onCreateNote'
  | 'onCreateType'
  | 'onQuickOpen'
  | 'onSave'
  | 'onOpenSettings'
  | 'onToggleInspector'
  | 'onCommandPalette'
  | 'onZoomIn'
  | 'onZoomOut'
  | 'onZoomReset'
  | 'onDeleteNote'
  | 'onFindInNote'
  | 'onUndo'
  | 'onRedo'
  | 'onReplaceInNote'
  | 'onPastePlainText'
  | 'onSearch'
  | 'onToggleRawEditor'
  | 'onToggleDiff'
  | 'onToggleAIChat'
  | 'onToggleTableOfContents'
  | 'onExportNoteAsPdf'
  | 'onToggleOrganized'
  | 'onGoBack'
  | 'onGoForward'
  | 'onCheckForUpdates'
  | 'onSelectFilter'
> {
  return {
    onSetViewMode: config.onSetViewMode,
    onCreateNote: config.onCreateNote,
    onCreateType: config.onCreateType,
    onQuickOpen: config.onQuickOpen,
    onSave: config.onSave,
    onOpenSettings: config.onOpenSettings,
    onToggleInspector: config.onToggleInspector,
    onCommandPalette: config.onCommandPalette,
    onZoomIn: config.onZoomIn,
    onZoomOut: config.onZoomOut,
    onZoomReset: config.onZoomReset,
    onDeleteNote: config.onDeleteNote,
    onFindInNote: config.onFindInNote,
    onUndo: config.onUndo,
    onRedo: config.onRedo,
    onReplaceInNote: config.onReplaceInNote,
    onPastePlainText: config.onPastePlainText,
    onSearch: config.onSearch,
    onToggleRawEditor: config.onToggleRawEditor,
    onToggleDiff: config.onToggleDiff,
    onToggleAIChat: enabledAiChatToggle(config),
    onToggleTableOfContents: config.onToggleTableOfContents,
    onExportNoteAsPdf: config.onExportNoteAsPdf,
    onToggleOrganized: config.onToggleOrganized,
    onGoBack: config.onGoBack,
    onGoForward: config.onGoForward,
    onCheckForUpdates: config.onCheckForUpdates,
    onSelectFilter: selectFilter,
  }
}

function createMenuEventVaultHandlers(
  config: AppCommandsConfig,
  viewChanges: () => void,
): Pick<
  Omit<Parameters<typeof useMenuEvents>[0], 'onArchiveNote'>,
  | 'onOpenVault'
  | 'onRemoveActiveVault'
  | 'onRestoreGettingStarted'
  | 'onAddRemote'
  | 'onCommitPush'
  | 'onPull'
  | 'onResolveConflicts'
  | 'onViewChanges'
  | 'onInstallMcp'
  | 'onReloadVault'
  | 'onRepairVault'
  | 'onOpenInNewWindow'
  | 'onRestoreDeletedNote'
> {
  return {
    onOpenVault: config.onOpenVault,
    onRemoveActiveVault: config.onRemoveActiveVault,
    onRestoreGettingStarted: config.onRestoreGettingStarted,
    onAddRemote: config.onAddRemote ?? requestAddRemote,
    onCommitPush: config.onCommitPush,
    onPull: config.onPull,
    onResolveConflicts: config.onResolveConflicts,
    onViewChanges: viewChanges,
    onInstallMcp: config.onInstallMcp,
    onReloadVault: config.onReloadVault,
    onRepairVault: config.onRepairVault,
    onOpenInNewWindow: config.onOpenInNewWindow,
    onRestoreDeletedNote: config.onRestoreDeletedNote,
  }
}

function createMenuEventState(
  config: AppCommandsConfig,
): Pick<
  Omit<Parameters<typeof useMenuEvents>[0], 'onArchiveNote'>,
  | 'activeTabPathRef'
  | 'multiSelectionCommandRef'
  | 'activeTabPath'
  | 'modifiedCount'
  | 'hasRestorableDeletedNote'
  | 'hasNoRemote'
> {
  return {
    activeTabPathRef: config.activeTabPathRef,
    multiSelectionCommandRef: config.multiSelectionCommandRef,
    activeTabPath: config.activeTabPath,
    modifiedCount: config.modifiedCount,
    hasRestorableDeletedNote: config.canRestoreDeletedNote,
    hasNoRemote: config.canAddRemote ?? true,
  }
}

/** Sets up keyboard shortcuts, command registry, menu events, and keyboard navigation. */
export function useAppCommands(config: AppCommandsConfig): CommandAction[] {
  const entriesRef = useRef(config.entries)
  // eslint-disable-next-line react-hooks/refs
  entriesRef.current = config.entries

  const toggleArchive = useCallback((path: string) => {
    const entry = entriesRef.current.find(e => e.path === path)
    ;(entry?.archived ? config.onUnarchiveNote : config.onArchiveNote)(path)
  }, [config.onArchiveNote, config.onUnarchiveNote])


  const { onSelect } = config

  const selectFilter = useCallback((filter: SidebarFilter) => {
    const safeFilter = !config.showInbox && filter === 'inbox' ? 'all' : filter
    onSelect({ kind: 'filter', filter: safeFilter })
  }, [config.showInbox, onSelect])

  const viewChanges = useCallback(() => {
    onSelect({ kind: 'filter', filter: 'changes' })
  }, [onSelect])

  const keyboardActions = createKeyboardActions(config)
  const menuEventHandlers = createMenuEventHandlers(config, selectFilter, viewChanges)

  useAppKeyboard({ ...keyboardActions, onArchiveNote: toggleArchive })

  useMenuEvents({ ...menuEventHandlers, onArchiveNote: toggleArchive })

  const commands = useCommandRegistry(createCommandRegistryConfig(config))

  useKeyboardNavigation({
    activeTabPath: config.activeTabPath,
    visibleNotesRef: config.visibleNotesRef,
    onReplaceActiveTab: config.onReplaceActiveTab,
    onSelectNote: config.onSelectNote,
  })

  return commands
}
