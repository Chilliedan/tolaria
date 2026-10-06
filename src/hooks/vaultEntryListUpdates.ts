import type { VaultEntry } from '../types'
import { normalizeVaultEntry } from '../utils/vaultMetadataNormalization'

export function removeEntryByPath(entries: VaultEntry[], path: string): VaultEntry[] {
  const nextEntries = entries.filter((entry) => entry.path !== path)
  return nextEntries.length === entries.length ? entries : nextEntries
}

export function removeEntriesByPath(entries: VaultEntry[], paths: string[]): VaultEntry[] {
  if (paths.length === 0) return entries

  const pathSet = new Set(paths)
  const nextEntries = entries.filter((entry) => !pathSet.has(entry.path))
  return nextEntries.length === entries.length ? entries : nextEntries
}

export function replaceEntryByPath(
  entries: VaultEntry[],
  oldPath: string,
  patch: Partial<VaultEntry> & { path: string },
): VaultEntry[] {
  const entryIndex = entries.findIndex((entry) => entry.path === oldPath)
  if (entryIndex < 0) return entries

  return entries.map((entry, index) => (
    index === entryIndex ? normalizeVaultEntry({ ...entry, ...patch }, '', index) : entry
  ))
}

