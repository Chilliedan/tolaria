import type { VaultEntry } from '../types'

/**
 * Structural sharing for vault list refreshes.
 *
 * A refreshed vault list (`list_vault`, `reload_vault`, snapshot reconciliation)
 * arrives as brand-new objects even when nothing changed. This keeps the
 * previous object for every entry whose data is structurally equal, so
 * identity-keyed consumers (memoised selectors, React props) only see the
 * entries that really changed. The refreshed list's order is preserved. When
 * every entry is reused in the same order, the previous array is returned.
 *
 * Equality is a full structural compare of the entry's plain data, not a
 * timestamp check: workspace identity and other derived fields can change
 * without the file changing.
 */
export function reuseUnchangedEntries(previous: readonly VaultEntry[], next: VaultEntry[]): VaultEntry[] {
  if (previous === next || previous.length === 0) return next

  const lookup = lazyKeyIndex(previous)
  const result = next.map((entry, index) => reusableEntry({ entry, index, previous, lookup }))

  if (sameItems(result, previous)) return previous as VaultEntry[]
  return sameItems(result, next) ? next : result
}

interface ReusableEntryOptions {
  entry: VaultEntry
  index: number
  previous: readonly VaultEntry[]
  lookup: (key: string) => VaultEntry | undefined
}

function reusableEntry({ entry, index, previous, lookup }: ReusableEntryOptions): VaultEntry {
  const samePosition = previous.at(index)
  if (samePosition === entry) return entry

  const key = entryKey(entry)
  if (samePosition && entryKey(samePosition) === key) return plainValuesEqual(samePosition, entry) ? samePosition : entry

  const candidate = lookup(key)
  return candidate && plainValuesEqual(candidate, entry) ? candidate : entry
}

function entryKey(entry: VaultEntry): string {
  return `${entry.workspace?.path ?? ''}\n${entry.path}`
}

/** Builds the key index only once an entry has moved, been added or been removed. */
function lazyKeyIndex(entries: readonly VaultEntry[]): (key: string) => VaultEntry | undefined {
  let byKey: Map<string, VaultEntry> | null = null
  return (key) => {
    byKey ??= indexByKey(entries)
    return byKey.get(key)
  }
}

function indexByKey(entries: readonly VaultEntry[]): Map<string, VaultEntry> {
  const byKey = new Map<string, VaultEntry>()
  for (const entry of entries) {
    const key = entryKey(entry)
    if (!byKey.has(key)) byKey.set(key, entry)
  }
  return byKey
}

function sameItems(left: readonly VaultEntry[], right: readonly VaultEntry[]): boolean {
  return left.length === right.length && left.every((entry, index) => entry === right.at(index))
}

type PlainRecord = Record<string, unknown>

function isPlainRecord(value: unknown): value is PlainRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false
  const prototype = Object.getPrototypeOf(value)
  return prototype === Object.prototype || prototype === null
}

function plainValuesEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true
  if (Array.isArray(left)) return Array.isArray(right) && arraysEqual(left, right)
  return isPlainRecord(left) && isPlainRecord(right) && recordsEqual(left, right)
}

function arraysEqual(left: readonly unknown[], right: readonly unknown[]): boolean {
  return left.length === right.length && left.every((value, index) => plainValuesEqual(value, right.at(index)))
}

function recordsEqual(left: PlainRecord, right: PlainRecord): boolean {
  const leftKeys = Object.keys(left)
  if (leftKeys.length !== Object.keys(right).length) return false
  return leftKeys.every((key) => (
    Object.hasOwn(right, key)
    && plainValuesEqual(Reflect.get(left, key), Reflect.get(right, key))
  ))
}
