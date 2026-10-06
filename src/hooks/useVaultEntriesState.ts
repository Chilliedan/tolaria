import { useCallback, useState, type Dispatch, type SetStateAction } from 'react'
import type { VaultEntry } from '../types'
import { reuseUnchangedEntries } from '../utils/reuseUnchangedEntries'

/**
 * The vault entry list state. Every update (startup snapshot, background
 * reconciliation, full reloads, workspace loads, single-entry patches) goes
 * through `reuseUnchangedEntries`, so unchanged entries keep their object
 * identity and an identical refresh keeps the previous array.
 */
export function useVaultEntriesState(): [VaultEntry[], Dispatch<SetStateAction<VaultEntry[]>>] {
  const [entries, setRawEntries] = useState<VaultEntry[]>([])
  const setEntries = useCallback((action: SetStateAction<VaultEntry[]>) => {
    setRawEntries((previous) => reuseUnchangedEntries(
      previous,
      typeof action === 'function' ? action(previous) : action,
    ))
  }, [])
  return [entries, setEntries]
}
