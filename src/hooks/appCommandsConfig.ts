import type { useCommandRegistry } from './useCommandRegistry'
import type { AppCommandsConfig } from './useAppCommands'
import { requestAddRemote } from '../utils/addRemoteEvents'

/**
 * Config shape and builder functions that assemble a `useCommandRegistry` config
 * from the larger `AppCommandsConfig`. Split out of `useAppCommands.ts` to keep
 * that file's line count manageable; these are pure, side-effect-free mappers.
 */
export type CommandRegistryConfig = Parameters<typeof useCommandRegistry>[0]

type CommandRegistrySelectionState = Pick<
  CommandRegistryConfig,
  | 'activeNoteModified'
  | 'onZoomIn'
  | 'onZoomOut'
  | 'onZoomReset'
  | 'zoomLevel'
  | 'onSelect'
  | 'onRenameFolder'
  | 'onDeleteFolder'
  | 'onRevealSelectedFolder'
  | 'onCopySelectedFolderPath'
  | 'showInbox'
  | 'onGoBack'
  | 'onGoForward'
  | 'canGoBack'
  | 'canGoForward'
  | 'selection'
>
type CommandRegistryCoreActions = Pick<
  CommandRegistryConfig,
  | 'activeTabPath'
  | 'entries'
  | 'modifiedCount'
  | 'onQuickOpen'
  | 'onCreateNote'
  | 'onCreateNoteOfType'
  | 'onSave'
  | 'onUndo'
  | 'onRedo'
  | 'canUndo'
  | 'canRedo'
  | 'undoLabel'
  | 'redoLabel'
  | 'onFindInNote'
  | 'onReplaceInNote'
  | 'onPastePlainText'
  | 'onTurnCurrentBlockInto'
  | 'onOpenSettings'
  | 'onOpenFeedback'
  | 'onDeleteNote'
  | 'onArchiveNote'
  | 'onUnarchiveNote'
  | 'onCommitPush'
  | 'onGenerateCommitMessage'
  | 'onPull'
  | 'onPullRepository'
  | 'onResolveConflicts'
  | 'onSetViewMode'
  | 'onToggleInspector'
  | 'onToggleDiff'
  | 'onToggleRawEditor'
  | 'selectedViewName'
  | 'onMoveSelectedViewUp'
  | 'onMoveSelectedViewDown'
  | 'canMoveSelectedViewUp'
  | 'canMoveSelectedViewDown'
  | 'noteWidth'
  | 'defaultNoteWidth'
  | 'onSetNoteWidth'
  | 'onSetDefaultNoteWidth'
  | 'onToggleAIChat'
  | 'onToggleTableOfContents'
>
type CommandRegistryVaultActions = Pick<
  CommandRegistryConfig,
  | 'onOpenVault'
  | 'onCreateEmptyVault'
  | 'onAddRemote'
  | 'canAddRemote'
  | 'gitFeaturesEnabled'
  | 'isGitVault'
  | 'gitRepositories'
  | 'onInitializeGit'
  | 'onCheckForUpdates'
  | 'onCreateType'
  | 'locale'
  | 'systemLocale'
  | 'selectedUiLanguage'
  | 'onSetUiLanguage'
  | 'onSetThemeMode'
  | 'onRemoveActiveVault'
  | 'onRestoreGettingStarted'
  | 'isGettingStartedHidden'
  | 'vaultCount'
  | 'onReloadVault'
  | 'onRepairVault'
  | 'onOpenInNewWindow'
  | 'onRevealActiveFile'
  | 'onCopyActiveFilePath'
  | 'onCopyActiveDeepLink'
  | 'onOpenActiveFileExternal'
  | 'onRestoreDeletedNote'
  | 'canRestoreDeletedNote'
>
type CommandRegistryAiActions = Pick<
  CommandRegistryConfig,
  | 'aiFeaturesEnabled'
  | 'mcpStatus'
  | 'onInstallMcp'
  | 'aiAgentsStatus'
  | 'vaultAiGuidanceStatus'
  | 'onOpenAiAgents'
  | 'onRestoreVaultAiGuidance'
  | 'onSetDefaultAiAgent'
  | 'selectedAiAgent'
  | 'onCycleDefaultAiAgent'
  | 'selectedAiAgentLabel'
>
type CommandRegistryNoteActions = Pick<
  CommandRegistryConfig,
  | 'onSetNoteIcon'
  | 'onRemoveNoteIcon'
  | 'onChangeNoteType'
  | 'onMoveNoteToFolder'
  | 'canMoveNoteToFolder'
  | 'onTurnCurrentBlockInto'
  | 'activeNoteHasIcon'
  | 'noteListFilter'
  | 'onSetNoteListFilter'
  | 'onToggleFavorite'
  | 'onToggleOrganized'
  | 'onCustomizeNoteListColumns'
  | 'canCustomizeNoteListColumns'
  | 'noteListColumnsLabel'
  | 'onExportNoteAsPdf'
>

export function aiFeaturesAreEnabled(config: Pick<AppCommandsConfig, 'aiFeaturesEnabled'>): boolean {
  return config.aiFeaturesEnabled !== false
}

export function enabledAiChatToggle(config: Pick<AppCommandsConfig, 'aiFeaturesEnabled' | 'onToggleAIChat'>): (() => void) | undefined {
  return aiFeaturesAreEnabled(config) ? config.onToggleAIChat : undefined
}

function createCommandRegistrySelectionConfig(
  config: AppCommandsConfig,
): CommandRegistrySelectionState {
  return {
    activeNoteModified: config.activeNoteModified,
    onZoomIn: config.onZoomIn,
    onZoomOut: config.onZoomOut,
    onZoomReset: config.onZoomReset,
    zoomLevel: config.zoomLevel,
    onSelect: config.onSelect,
    onRenameFolder: config.onRenameFolder,
    onDeleteFolder: config.onDeleteFolder,
    onRevealSelectedFolder: config.onRevealSelectedFolder,
    onCopySelectedFolderPath: config.onCopySelectedFolderPath,
    showInbox: config.showInbox,
    onGoBack: config.onGoBack,
    onGoForward: config.onGoForward,
    canGoBack: config.canGoBack,
    canGoForward: config.canGoForward,
    selection: config.selection,
  }
}

function createCommandRegistryCoreConfig(
  config: AppCommandsConfig,
): CommandRegistryCoreActions {
  return {
    activeTabPath: config.activeTabPath,
    entries: config.entries,
    modifiedCount: config.modifiedCount,
    onQuickOpen: config.onQuickOpen,
    onCreateNote: config.onCreateNote,
    onCreateNoteOfType: config.onCreateNoteOfType,
    onSave: config.onSave,
    onUndo: config.onUndo,
    onRedo: config.onRedo,
    canUndo: config.canUndo,
    canRedo: config.canRedo,
    undoLabel: config.undoLabel,
    redoLabel: config.redoLabel,
    onOpenSettings: config.onOpenSettings,
    onOpenFeedback: config.onOpenFeedback,
    onDeleteNote: config.onDeleteNote,
    onArchiveNote: config.onArchiveNote,
    onUnarchiveNote: config.onUnarchiveNote,
    onCommitPush: config.onCommitPush,
    onGenerateCommitMessage: config.onGenerateCommitMessage,
    onPull: config.onPull,
    onPullRepository: config.onPullRepository,
    onResolveConflicts: config.onResolveConflicts,
    onSetViewMode: config.onSetViewMode,
    onToggleInspector: config.onToggleInspector,
    onToggleDiff: config.onToggleDiff,
    onToggleRawEditor: config.onToggleRawEditor,
    selectedViewName: config.selectedViewName,
    onMoveSelectedViewUp: config.onMoveSelectedViewUp,
    onMoveSelectedViewDown: config.onMoveSelectedViewDown,
    canMoveSelectedViewUp: config.canMoveSelectedViewUp,
    canMoveSelectedViewDown: config.canMoveSelectedViewDown,
    onFindInNote: config.onFindInNote,
    onReplaceInNote: config.onReplaceInNote,
    onPastePlainText: config.onPastePlainText,
    onTurnCurrentBlockInto: config.onTurnCurrentBlockInto,
    noteWidth: config.noteWidth,
    defaultNoteWidth: config.defaultNoteWidth,
    onSetNoteWidth: config.onSetNoteWidth,
    onSetDefaultNoteWidth: config.onSetDefaultNoteWidth,
    onToggleAIChat: enabledAiChatToggle(config),
    onToggleTableOfContents: config.onToggleTableOfContents,
  }
}

function createCommandRegistryVaultConfig(
  config: AppCommandsConfig,
): CommandRegistryVaultActions {
  return {
    onOpenVault: config.onOpenVault,
    onCreateEmptyVault: config.onCreateEmptyVault,
    onAddRemote: config.onAddRemote ?? requestAddRemote,
    canAddRemote: config.canAddRemote ?? true,
    gitFeaturesEnabled: config.gitFeaturesEnabled,
    isGitVault: config.isGitVault,
    gitRepositories: config.gitRepositories,
    onInitializeGit: config.onInitializeGit,
    onCheckForUpdates: config.onCheckForUpdates,
    onCreateType: config.onCreateType,
    locale: config.locale,
    systemLocale: config.systemLocale,
    selectedUiLanguage: config.selectedUiLanguage,
    onSetUiLanguage: config.onSetUiLanguage,
    onSetThemeMode: config.onSetThemeMode,
    onRemoveActiveVault: config.onRemoveActiveVault,
    onRestoreGettingStarted: config.onRestoreGettingStarted,
    isGettingStartedHidden: config.isGettingStartedHidden,
    vaultCount: config.vaultCount,
    onReloadVault: config.onReloadVault,
    onRepairVault: config.onRepairVault,
    onOpenInNewWindow: config.onOpenInNewWindow,
    onRevealActiveFile: config.onRevealActiveFile,
    onCopyActiveFilePath: config.onCopyActiveFilePath,
    onCopyActiveDeepLink: config.onCopyActiveDeepLink,
    onOpenActiveFileExternal: config.onOpenActiveFileExternal,
    onRestoreDeletedNote: config.onRestoreDeletedNote,
    canRestoreDeletedNote: config.canRestoreDeletedNote,
  }
}

function createCommandRegistryAiConfig(
  config: AppCommandsConfig,
): CommandRegistryAiActions {
  const aiFeaturesEnabled = aiFeaturesAreEnabled(config)
  const sharedConfig = {
    aiFeaturesEnabled,
    mcpStatus: config.mcpStatus,
    onInstallMcp: config.onInstallMcp,
  }

  if (!aiFeaturesEnabled) return sharedConfig

  return {
    ...sharedConfig,
    aiAgentsStatus: config.aiAgentsStatus,
    vaultAiGuidanceStatus: config.vaultAiGuidanceStatus,
    onOpenAiAgents: config.onOpenAiAgents,
    onRestoreVaultAiGuidance: config.onRestoreVaultAiGuidance,
    onSetDefaultAiAgent: config.onSetDefaultAiAgent,
    selectedAiAgent: config.selectedAiAgent,
    onCycleDefaultAiAgent: config.onCycleDefaultAiAgent,
    selectedAiAgentLabel: config.selectedAiAgentLabel,
  }
}

function createCommandRegistryNoteConfig(
  config: AppCommandsConfig,
): CommandRegistryNoteActions {
  return {
    onSetNoteIcon: config.onSetNoteIcon,
    onRemoveNoteIcon: config.onRemoveNoteIcon,
    onChangeNoteType: config.onChangeNoteType,
    onMoveNoteToFolder: config.onMoveNoteToFolder,
    canMoveNoteToFolder: config.canMoveNoteToFolder,
    activeNoteHasIcon: config.activeNoteHasIcon,
    noteListFilter: config.noteListFilter,
    onSetNoteListFilter: config.onSetNoteListFilter,
    onToggleFavorite: config.onToggleFavorite,
    onToggleOrganized: config.onToggleOrganized,
    onCustomizeNoteListColumns: config.onCustomizeNoteListColumns,
    canCustomizeNoteListColumns: config.canCustomizeNoteListColumns,
    noteListColumnsLabel: config.noteListColumnsLabel,
    onExportNoteAsPdf: config.onExportNoteAsPdf,
  }
}

export function createCommandRegistryConfig(config: AppCommandsConfig): CommandRegistryConfig {
  return {
    ...createCommandRegistryCoreConfig(config),
    ...createCommandRegistrySelectionConfig(config),
    ...createCommandRegistryVaultConfig(config),
    ...createCommandRegistryAiConfig(config),
    ...createCommandRegistryNoteConfig(config),
  }
}
