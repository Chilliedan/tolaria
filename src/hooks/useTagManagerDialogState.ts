import { useMemo, useState } from 'react'
import { translate, type AppLocale } from '../lib/i18n'
import type { VaultEntry } from '../types'
import type { TagUsage } from '../utils/tagInventory'
import { type TagRewriteOp } from '../utils/tagRewrite'
import type { TagRowEditMode } from '../components/TagManagerRow'
import { useTagManager, type TagManagerApplyResult, type TagManagerProgress, type UpdateFrontmatter } from './useTagManager'

export interface PendingAction {
  op: TagRewriteOp
  property: string
  message: string
}

export interface RowEdit {
  tag: string
  mode: Exclude<TagRowEditMode, null>
}

export interface EscapeGuard {
  active: boolean
  cancel: () => void
}

export const NO_ESCAPE_GUARD: EscapeGuard = { active: false, cancel: () => {} }

function confirmMessage(op: TagRewriteOp, count: number, intoExisting: boolean, locale: AppLocale): string {
  if (op.kind === 'delete') return translate(locale, 'tagManager.confirm.delete', { tag: op.tag, count })
  if (op.kind === 'rename') return translate(locale, 'tagManager.confirm.rename', { from: op.from, to: op.to, count })
  const key = intoExisting ? 'tagManager.confirm.renameIntoExisting' : 'tagManager.confirm.merge'
  return translate(locale, key, { from: op.sources.join(', '), to: op.target, count })
}

function filterUsages(usages: TagUsage[], filter: string): TagUsage[] {
  const needle = filter.trim().toLowerCase()
  return needle ? usages.filter((usage) => usage.tag.toLowerCase().includes(needle)) : usages
}

export interface UseTagManagerDialogStateOptions {
  entries: VaultEntry[]
  locale: AppLocale
  onUpdateFrontmatter: UpdateFrontmatter
}

export interface TagManagerDialogState {
  property: string | null
  usages: TagUsage[]
  visible: TagUsage[]
  filter: string
  setFilter: (filter: string) => void
  setSelectedProperty: (property: string) => void
  expandedTag: string | null
  rowEdit: RowEdit | null
  pending: PendingAction | null
  result: TagManagerApplyResult | null
  progress: TagManagerProgress | null
  busy: boolean
  entriesByPath: Map<string, VaultEntry>
  properties: string[]
  toggleNotes: (tag: string) => void
  startEdit: (tag: string, mode: Exclude<TagRowEditMode, null>) => void
  cancelEdit: () => void
  cancelPending: () => void
  applyPending: () => Promise<void>
  propose: (op: TagRewriteOp, intoExisting?: boolean) => void
}

export function useTagManagerDialogState(
  { entries, locale, onUpdateFrontmatter }: UseTagManagerDialogStateOptions,
): TagManagerDialogState {
  const manager = useTagManager({ entries, updateFrontmatter: onUpdateFrontmatter })
  const [selectedProperty, setSelectedPropertyState] = useState<string | null>(null)
  const [filter, setFilter] = useState('')
  const [expandedTag, setExpandedTag] = useState<string | null>(null)
  const [rowEdit, setRowEdit] = useState<RowEdit | null>(null)
  const [pending, setPending] = useState<PendingAction | null>(null)
  const [result, setResult] = useState<TagManagerApplyResult | null>(null)

  const property = selectedProperty && manager.properties.includes(selectedProperty)
    ? selectedProperty
    : manager.properties[0] ?? null
  const usages = useMemo(() => (property ? manager.inventory.get(property) ?? [] : []), [manager.inventory, property])
  const visible = filterUsages(usages, filter)
  const entriesByPath = useMemo(() => new Map(entries.map((entry) => [entry.path, entry])), [entries])
  const busy = manager.progress !== null

  const propose = (op: TagRewriteOp, intoExisting = false) => {
    if (!property) return
    setRowEdit(null)
    setResult(null)
    const count = manager.countAffected(property, op)
    setPending({ op, property, message: confirmMessage(op, count, intoExisting, locale) })
  }

  const applyPending = async () => {
    if (!pending) return
    const { op, property: pendingProperty } = pending
    setPending(null)
    setResult(await manager.apply(pendingProperty, op))
  }

  const changeProperty = (nextProperty: string) => {
    setSelectedPropertyState(nextProperty)
    setFilter('')
    setExpandedTag(null)
    setRowEdit(null)
    setPending(null)
    setResult(null)
  }

  return {
    property,
    usages,
    visible,
    filter,
    setFilter,
    setSelectedProperty: changeProperty,
    expandedTag,
    rowEdit,
    pending,
    result,
    progress: manager.progress,
    busy,
    entriesByPath,
    properties: manager.properties,
    toggleNotes: (tag) => setExpandedTag((current) => (current === tag ? null : tag)),
    startEdit: (tag, mode) => setRowEdit({ tag, mode }),
    cancelEdit: () => setRowEdit(null),
    cancelPending: () => setPending(null),
    applyPending,
    propose,
  }
}
