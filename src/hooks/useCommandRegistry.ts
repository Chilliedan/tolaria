import { useMemo } from 'react'
import type { AiAgentId, AiAgentsStatus } from '../lib/aiAgents'
import type { AppLocale, UiLanguagePreference } from '../lib/i18n'
import type { ThemeMode } from '../lib/themeMode'
import type { VaultAiGuidanceStatus } from '../lib/vaultAiGuidance'
import type { NoteWidthMode, SidebarSelection, VaultEntry } from '../types'
import type { NoteListFilter } from '../utils/noteListHelpers'
import type { ViewMode } from './useViewMode'
import { buildNavigationCommands } from './commands/navigationCommands'
import { buildNoteCommands } from './commands/noteCommands'
import { buildGitCommands } from './commands/gitCommands'
import { buildViewCommands } from './commands/viewCommands'
import { buildSettingsCommands } from './commands/settingsCommands'
import { buildAiAgentCommands } from './commands/aiAgentCommands'
import { buildTypeCommands } from './commands/typeCommands'
import { buildFilterCommands } from './commands/filterCommands'
import { localizeCommandActions } from './commands/localizeCommands'
import { extractVaultTypes } from '../utils/vaultTypes'
import type { GitRepositoryOption } from '../utils/gitRepositories'
import type { ImmediateCreateOptions } from './useNoteCreation'
import type { RichEditorBlockTypeDefinition } from '../utils/richEditorBlockTypes'

// Re-export types and helpers for backward compatibility
export type { CommandAction, CommandGroup } from './commands/types'
export { groupSortKey } from './commands/types'
export { pluralizeType, buildTypeCommands } from './commands/typeCommands'
export { extractVaultTypes } from '../utils/vaultTypes'
export { buildViewCommands } from './commands/viewCommands'

interface CommandRegistryConfig {
  activeTabPath: string | null
  entries: VaultEntry[]
  modifiedCount: number
  activeNoteHasIcon?: boolean
  mcpStatus?: string
  onInstallMcp?: () => void
  aiFeaturesEnabled?: boolean
  aiAgentsStatus?: AiAgentsStatus
  vaultAiGuidanceStatus?: VaultAiGuidanceStatus
  onOpenAiAgents?: () => void
  onRestoreVaultAiGuidance?: () => void
  onSetDefaultAiAgent?: (agent: AiAgentId) => void
  selectedAiAgent?: AiAgentId
  onCycleDefaultAiAgent?: () => void
  selectedAiAgentLabel?: string
  onReloadVault?: () => void
  onRepairVault?: () => void
  onSetNoteIcon?: () => void
  onRemoveNoteIcon?: () => void
  locale?: AppLocale
  systemLocale?: AppLocale
  selectedUiLanguage?: UiLanguagePreference
  onSetUiLanguage?: (language: UiLanguagePreference) => void
  onSetThemeMode?: (mode: ThemeMode) => void
  onChangeNoteType?: () => void
  onMoveNoteToFolder?: () => void
  canMoveNoteToFolder?: boolean
  onTurnCurrentBlockInto?: (target: RichEditorBlockTypeDefinition) => void
  onOpenInNewWindow?: () => void
  onRevealActiveFile?: (path: string) => void
  onCopyActiveFilePath?: (path: string) => void
  onCopyActiveDeepLink?: (path: string) => void
  onOpenActiveFileExternal?: (path: string) => void
  onExportNoteAsPdf?: () => void
  onToggleFavorite?: (path: string) => void
  onToggleOrganized?: (path: string) => void
  onCustomizeNoteListColumns?: () => void
  canCustomizeNoteListColumns?: boolean
  noteListColumnsLabel?: string
  onRestoreDeletedNote?: () => void
  canRestoreDeletedNote?: boolean
  onQuickOpen: () => void
  onCreateNote: (type?: string, options?: ImmediateCreateOptions) => void
  onCreateNoteOfType: (type: string) => void
  onSave: () => void
  onUndo?: () => void
  onRedo?: () => void
  canUndo?: boolean
  canRedo?: boolean
  undoLabel?: string | null
  redoLabel?: string | null
  onPastePlainText: () => void
  onOpenSettings: () => void
  onOpenFeedback?: () => void
  onOpenVault?: () => void
  onCreateEmptyVault?: () => void
  onAddRemote?: () => void
  canAddRemote?: boolean
  gitFeaturesEnabled?: boolean
  isGitVault?: boolean
  gitRepositories?: GitRepositoryOption[]
  onInitializeGit?: () => void
  onCreateType?: () => void
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
  onFindInNote?: () => void
  onReplaceInNote?: () => void
  noteWidth?: NoteWidthMode
  defaultNoteWidth?: NoteWidthMode
  onSetNoteWidth?: (mode: NoteWidthMode) => void
  onSetDefaultNoteWidth?: (mode: NoteWidthMode) => void
  onToggleAIChat?: () => void
  onToggleTableOfContents?: () => void
  activeNoteModified: boolean
  onCheckForUpdates?: () => void
  onZoomIn: () => void
  onZoomOut: () => void
  onZoomReset: () => void
  zoomLevel: number
  onSelect: (sel: SidebarSelection) => void
  onRenameFolder?: () => void
  onDeleteFolder?: () => void
  onRevealSelectedFolder?: () => void
  onCopySelectedFolderPath?: () => void
  showInbox?: boolean
  onGoBack?: () => void
  onGoForward?: () => void
  canGoBack?: boolean
  canGoForward?: boolean
  onRemoveActiveVault?: () => void
  onRestoreGettingStarted?: () => void
  isGettingStartedHidden?: boolean
  vaultCount?: number
  selection?: SidebarSelection
  noteListFilter?: NoteListFilter
  onSetNoteListFilter?: (filter: NoteListFilter) => void
}

function currentFolderCreateOptions(selection: SidebarSelection | undefined): ImmediateCreateOptions | undefined {
  if (selection?.kind !== 'folder') return undefined
  return {
    creationPath: 'folder_command_palette',
    folderPath: selection.path,
    vaultPath: selection.rootPath,
  }
}

function defaultNoteListColumnsLabel(selection: SidebarSelection | undefined): string {
  const showsAllNotes = selection?.kind === 'filter' && selection.filter === 'all'
  return showsAllNotes ? 'Customize All Notes columns' : 'Customize Inbox columns'
}

interface CommandRegistryDerivedState {
  hasActiveNote: boolean
  activeEntry: VaultEntry | undefined
  isArchived: boolean
  isFavorite: boolean
  isSectionGroup: boolean
  folderCreateOptions: ImmediateCreateOptions | undefined
  noteListColumnsLabel: string
  vaultTypes: ReturnType<typeof extractVaultTypes>
}

function useCommandRegistryDerivedState(config: CommandRegistryConfig): CommandRegistryDerivedState {
  const { activeTabPath, entries, selection } = config
  const hasActiveNote = activeTabPath !== null

  const activeEntry = useMemo(
    () => (hasActiveNote ? entries.find(e => e.path === activeTabPath) : undefined),
    [entries, activeTabPath, hasActiveNote],
  )
  const isArchived = activeEntry?.archived ?? false
  const isFavorite = activeEntry?.favorite ?? false
  const isSectionGroup = selection?.kind === 'sectionGroup'
  const folderCreateOptions = useMemo(() => currentFolderCreateOptions(selection), [selection])
  const noteListColumnsLabel = config.noteListColumnsLabel ?? defaultNoteListColumnsLabel(selection)
  const vaultTypes = useMemo(() => extractVaultTypes(entries), [entries])

  return { hasActiveNote, activeEntry, isArchived, isFavorite, isSectionGroup, folderCreateOptions, noteListColumnsLabel, vaultTypes }
}

function useNavigationGroupCommands(config: CommandRegistryConfig) {
  const {
    onQuickOpen, onSelect, selection, onRenameFolder, onDeleteFolder,
    onRevealSelectedFolder, onCopySelectedFolderPath, showInbox,
    onGoBack, onGoForward, canGoBack, canGoForward,
  } = config

  return useMemo(() => buildNavigationCommands({
    onQuickOpen, onSelect, selection, onRenameFolder, onDeleteFolder,
    onRevealSelectedFolder, onCopySelectedFolderPath, showInbox,
    onGoBack, onGoForward, canGoBack, canGoForward,
  }), [
    onQuickOpen, onSelect, selection, onRenameFolder, onDeleteFolder,
    onRevealSelectedFolder, onCopySelectedFolderPath, showInbox,
    onGoBack, onGoForward, canGoBack, canGoForward,
  ])
}

function useNoteGroupCommands(config: CommandRegistryConfig, derived: CommandRegistryDerivedState) {
  const {
    activeTabPath, locale, onCreateNote, onCreateType, onSave,
    onUndo, onRedo, canUndo, canRedo, undoLabel, redoLabel,
    onFindInNote, onReplaceInNote, onPastePlainText,
    onDeleteNote, onArchiveNote, onUnarchiveNote,
    onChangeNoteType, onMoveNoteToFolder, canMoveNoteToFolder, onTurnCurrentBlockInto,
    onSetNoteIcon, onRemoveNoteIcon, activeNoteHasIcon, onOpenInNewWindow,
    onRevealActiveFile, onCopyActiveFilePath, onOpenActiveFileExternal,
    onCopyActiveDeepLink, onExportNoteAsPdf, onToggleFavorite, onToggleOrganized,
    onRestoreDeletedNote, canRestoreDeletedNote,
  } = config
  const { hasActiveNote, activeEntry, isArchived, isFavorite, folderCreateOptions } = derived

  return useMemo(() => buildNoteCommands({
    hasActiveNote, activeTabPath, activeFileKind: activeEntry?.fileKind ?? 'markdown', isArchived, locale,
    currentFolderCreateOptions: folderCreateOptions, onCreateNote, onCreateType, onSave,
    onUndo, onRedo, canUndo, canRedo, undoLabel, redoLabel,
    onFindInNote, onReplaceInNote, onPastePlainText,
    onDeleteNote, onArchiveNote, onUnarchiveNote,
    onChangeNoteType, onMoveNoteToFolder, canMoveNoteToFolder,
    onTurnCurrentBlockInto,
    onSetNoteIcon, onRemoveNoteIcon, activeNoteHasIcon, onOpenInNewWindow,
    onRevealActiveFile, onCopyActiveFilePath, onOpenActiveFileExternal,
    onCopyActiveDeepLink, onExportNoteAsPdf,
    onToggleFavorite, isFavorite,
    onToggleOrganized, isOrganized: activeEntry?.organized ?? false,
    onRestoreDeletedNote, canRestoreDeletedNote,
  }), [
    hasActiveNote, activeTabPath, activeEntry?.fileKind, isArchived, locale,
    folderCreateOptions, onCreateNote, onCreateType, onSave, onUndo, onRedo, canUndo, canRedo, undoLabel, redoLabel,
    onFindInNote, onReplaceInNote, onPastePlainText, onDeleteNote, onArchiveNote, onUnarchiveNote,
    onChangeNoteType, onMoveNoteToFolder, canMoveNoteToFolder, onTurnCurrentBlockInto,
    onSetNoteIcon, onRemoveNoteIcon, activeNoteHasIcon, onOpenInNewWindow,
    onRevealActiveFile, onCopyActiveFilePath, onOpenActiveFileExternal,
    onCopyActiveDeepLink, onExportNoteAsPdf,
    onToggleFavorite, isFavorite,
    onToggleOrganized, activeEntry?.organized, onRestoreDeletedNote, canRestoreDeletedNote,
  ])
}

function useGitGroupCommands(config: CommandRegistryConfig) {
  const {
    modifiedCount, gitFeaturesEnabled, isGitVault, gitRepositories,
    onCommitPush, onGenerateCommitMessage, onInitializeGit, onPull, onPullRepository, onResolveConflicts, onSelect,
  } = config

  return useMemo(() => buildGitCommands({
    modifiedCount,
    gitFeaturesEnabled,
    isGitVault,
    repositories: gitRepositories,
    canAddRemote: config.canAddRemote ?? false,
    onAddRemote: config.onAddRemote,
    onCommitPush,
    onGenerateCommitMessage,
    onInitializeGit,
    onPull,
    onPullRepository,
    onResolveConflicts,
    onSelect,
  }), [
    modifiedCount, gitFeaturesEnabled, isGitVault, gitRepositories, config.canAddRemote, config.onAddRemote,
    onCommitPush, onGenerateCommitMessage, onInitializeGit, onPull, onPullRepository, onResolveConflicts, onSelect,
  ])
}

function useViewGroupCommands(config: CommandRegistryConfig, hasActiveNote: boolean, noteListColumnsLabel: string) {
  const {
    aiFeaturesEnabled, activeNoteModified, onSetViewMode, onToggleInspector,
    onToggleDiff, onToggleRawEditor, noteWidth, defaultNoteWidth, onSetNoteWidth, onSetDefaultNoteWidth,
    onToggleAIChat, onToggleTableOfContents, zoomLevel, onZoomIn, onZoomOut, onZoomReset,
    onCustomizeNoteListColumns, canCustomizeNoteListColumns,
    selectedViewName, onMoveSelectedViewUp, onMoveSelectedViewDown, canMoveSelectedViewUp, canMoveSelectedViewDown,
  } = config

  return useMemo(() => buildViewCommands({
    aiFeaturesEnabled,
    hasActiveNote, activeNoteModified, onSetViewMode, onToggleInspector,
    onToggleDiff, onToggleRawEditor, noteWidth, defaultNoteWidth, onSetNoteWidth, onSetDefaultNoteWidth, onToggleAIChat, onToggleTableOfContents, zoomLevel, onZoomIn, onZoomOut, onZoomReset,
    onCustomizeNoteListColumns, canCustomizeNoteListColumns, noteListColumnsLabel,
    selectedViewName, onMoveSelectedViewUp, onMoveSelectedViewDown, canMoveSelectedViewUp, canMoveSelectedViewDown,
  }), [
    aiFeaturesEnabled,
    hasActiveNote, activeNoteModified, onSetViewMode, onToggleInspector,
    onToggleDiff, onToggleRawEditor, noteWidth, defaultNoteWidth, onSetNoteWidth, onSetDefaultNoteWidth, onToggleAIChat, onToggleTableOfContents,
    zoomLevel, onZoomIn, onZoomOut, onZoomReset,
    onCustomizeNoteListColumns, canCustomizeNoteListColumns, noteListColumnsLabel,
    selectedViewName, onMoveSelectedViewUp, onMoveSelectedViewDown, canMoveSelectedViewUp, canMoveSelectedViewDown,
  ])
}

function useSettingsGroupCommands(config: CommandRegistryConfig) {
  const {
    mcpStatus, vaultCount, isGettingStartedHidden,
    onOpenSettings, onOpenFeedback, onOpenVault, onCreateEmptyVault, onRemoveActiveVault, onRestoreGettingStarted,
    onCheckForUpdates, onInstallMcp, onReloadVault, onRepairVault,
    locale, systemLocale, selectedUiLanguage, onSetUiLanguage, onSetThemeMode,
  } = config

  return useMemo(() => buildSettingsCommands({
    mcpStatus, vaultCount, isGettingStartedHidden,
    onOpenSettings, onOpenFeedback, onOpenVault, onCreateEmptyVault, onRemoveActiveVault, onRestoreGettingStarted,
    onCheckForUpdates, onInstallMcp, onReloadVault, onRepairVault,
    locale, systemLocale, selectedUiLanguage, onSetUiLanguage, onSetThemeMode,
  }), [
    mcpStatus, vaultCount, isGettingStartedHidden, onOpenSettings, onOpenFeedback,
    onOpenVault, onCreateEmptyVault, onRemoveActiveVault, onRestoreGettingStarted,
    onCheckForUpdates, onInstallMcp, onReloadVault, onRepairVault,
    locale, systemLocale, selectedUiLanguage, onSetUiLanguage, onSetThemeMode,
  ])
}

function useAiGroupCommands(config: CommandRegistryConfig) {
  const {
    aiFeaturesEnabled, aiAgentsStatus, vaultAiGuidanceStatus, selectedAiAgent, selectedAiAgentLabel,
    onOpenAiAgents, onRestoreVaultAiGuidance, onSetDefaultAiAgent, onCycleDefaultAiAgent,
  } = config

  return useMemo(() => buildAiAgentCommands({
    aiFeaturesEnabled,
    aiAgentsStatus,
    vaultAiGuidanceStatus,
    selectedAiAgent,
    selectedAiAgentLabel,
    onOpenAiAgents,
    onRestoreVaultAiGuidance,
    onSetDefaultAiAgent,
    onCycleDefaultAiAgent,
  }), [
    aiFeaturesEnabled,
    aiAgentsStatus, vaultAiGuidanceStatus, selectedAiAgent, selectedAiAgentLabel,
    onOpenAiAgents, onRestoreVaultAiGuidance, onSetDefaultAiAgent, onCycleDefaultAiAgent,
  ])
}

function useTypeAndFilterGroupCommands(config: CommandRegistryConfig, derived: CommandRegistryDerivedState) {
  const { onCreateNoteOfType, onSelect, noteListFilter, onSetNoteListFilter } = config
  const { vaultTypes, isSectionGroup } = derived

  const typeCommands = useMemo(
    () => buildTypeCommands(vaultTypes, onCreateNoteOfType, onSelect),
    [vaultTypes, onCreateNoteOfType, onSelect],
  )
  const filterCommands = useMemo(
    () => buildFilterCommands({ isSectionGroup, noteListFilter, onSetNoteListFilter }),
    [isSectionGroup, noteListFilter, onSetNoteListFilter],
  )

  return { typeCommands, filterCommands }
}

export function useCommandRegistry(config: CommandRegistryConfig): import('./commands/types').CommandAction[] {
  const derived = useCommandRegistryDerivedState(config)
  const navigationCommands = useNavigationGroupCommands(config)
  const noteCommands = useNoteGroupCommands(config, derived)
  const gitCommands = useGitGroupCommands(config)
  const viewCommands = useViewGroupCommands(config, derived.hasActiveNote, derived.noteListColumnsLabel)
  const settingsCommands = useSettingsGroupCommands(config)
  const aiCommands = useAiGroupCommands(config)
  const { typeCommands, filterCommands } = useTypeAndFilterGroupCommands(config, derived)

  const commands = useMemo(() => [
    ...navigationCommands,
    ...noteCommands,
    ...gitCommands,
    ...viewCommands,
    ...settingsCommands,
    ...aiCommands,
    ...typeCommands,
    ...filterCommands,
  ], [
    navigationCommands, noteCommands, gitCommands, viewCommands,
    settingsCommands, aiCommands, typeCommands, filterCommands,
  ])

  return useMemo(() => localizeCommandActions(commands, config.locale), [commands, config.locale])
}
