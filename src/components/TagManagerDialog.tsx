import { useMemo, useState } from 'react'
import {
  Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  useTagManager, type TagManagerApplyResult, type TagManagerProgress, type UpdateFrontmatter,
} from '../hooks/useTagManager'
import { translate, type AppLocale } from '../lib/i18n'
import type { VaultEntry } from '../types'
import type { TagUsage } from '../utils/tagInventory'
import { resolveRenameOp, type TagRewriteOp } from '../utils/tagRewrite'
import { TagManagerRow, type TagRowEditMode } from './TagManagerRow'
import { TagManagerConfirm, TagManagerProgressLine, TagManagerResult } from './TagManagerStatus'

interface TagManagerDialogProps {
  open: boolean
  onClose: () => void
  entries: VaultEntry[]
  locale: AppLocale
  onUpdateFrontmatter: UpdateFrontmatter
  onOpenNote: (entry: VaultEntry) => void
}

interface PendingAction {
  op: TagRewriteOp
  message: string
}

interface RowEdit {
  tag: string
  mode: Exclude<TagRowEditMode, null>
}

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

function PropertyPicker({ properties, value, locale, onChange }: {
  properties: string[]; value: string; locale: AppLocale; onChange: (property: string) => void
}) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger className="h-8 w-40" aria-label={translate(locale, 'tagManager.property')}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {properties.map((property) => <SelectItem key={property} value={property}>{property}</SelectItem>)}
      </SelectContent>
    </Select>
  )
}

interface TagManagerDialogState {
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

function useTagManagerDialogState(
  { entries, locale, onUpdateFrontmatter }: Omit<TagManagerDialogProps, 'open' | 'onClose' | 'onOpenNote'>,
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
    setPending({ op, message: confirmMessage(op, count, intoExisting, locale) })
  }

  const applyPending = async () => {
    if (!property || !pending) return
    const op = pending.op
    setPending(null)
    setResult(await manager.apply(property, op))
  }

  return {
    property,
    usages,
    visible,
    filter,
    setFilter,
    setSelectedProperty: setSelectedPropertyState,
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

function TagManagerStatusArea({ state, locale }: { state: TagManagerDialogState; locale: AppLocale }) {
  return (
    <>
      {state.pending && (
        <TagManagerConfirm
          message={state.pending.message}
          destructive={state.pending.op.kind === 'delete'}
          locale={locale}
          onCancel={state.cancelPending}
          onApply={() => void state.applyPending()}
        />
      )}
      {state.progress && <TagManagerProgressLine progress={state.progress} locale={locale} />}
      {state.result && (
        <TagManagerResult
          result={state.result}
          titleForPath={(path) => state.entriesByPath.get(path)?.title ?? path}
          locale={locale}
        />
      )}
    </>
  )
}

function TagManagerRowsList({ state, locale, onOpenNote }: {
  state: TagManagerDialogState; locale: AppLocale; onOpenNote: (entry: VaultEntry) => void
}) {
  const { usages, visible, entriesByPath } = state
  if (visible.length === 0) {
    return <p className="py-4 text-center text-sm text-muted-foreground">{translate(locale, 'tagManager.noMatches')}</p>
  }
  return (
    <ul className="flex min-h-0 flex-col overflow-y-auto">
      {visible.map((usage) => (
        <TagManagerRow
          key={usage.tag}
          usage={usage}
          otherTags={usages.map((other) => other.tag).filter((tag) => tag !== usage.tag)}
          editMode={state.rowEdit?.tag === usage.tag ? state.rowEdit.mode : null}
          expanded={state.expandedTag === usage.tag}
          notes={usage.paths.flatMap((path) => entriesByPath.get(path) ?? [])}
          locale={locale}
          disabled={state.busy}
          onToggleNotes={() => state.toggleNotes(usage.tag)}
          onStartEdit={(mode) => state.startEdit(usage.tag, mode)}
          onCancelEdit={state.cancelEdit}
          onRename={(next) => {
            const op = resolveRenameOp(usage.tag, next, usages.map((other) => other.tag))
            if (op) state.propose(op, op.kind === 'merge')
            else state.cancelEdit()
          }}
          onMerge={(target) => state.propose({ kind: 'merge', sources: [usage.tag], target })}
          onDelete={() => state.propose({ kind: 'delete', tag: usage.tag })}
          onOpenNote={onOpenNote}
        />
      ))}
    </ul>
  )
}

function TagManagerContent({ entries, locale, onUpdateFrontmatter, onOpenNote, onClose }: Omit<TagManagerDialogProps, 'open'>) {
  const state = useTagManagerDialogState({ entries, locale, onUpdateFrontmatter })
  const { property } = state

  const openNote = (entry: VaultEntry) => {
    onOpenNote(entry)
    onClose()
  }

  if (!property) return <p className="py-6 text-center text-sm text-muted-foreground">{translate(locale, 'tagManager.empty')}</p>

  return (
    <div className="flex min-h-0 flex-col gap-3">
      <div className="flex items-center gap-2">
        <PropertyPicker properties={state.properties} value={property} locale={locale} onChange={state.setSelectedProperty} />
        <Input
          value={state.filter}
          placeholder={translate(locale, 'tagManager.filterPlaceholder')}
          onChange={(event) => state.setFilter(event.target.value)}
          className="h-8"
        />
      </div>
      <TagManagerStatusArea state={state} locale={locale} />
      <TagManagerRowsList state={state} locale={locale} onOpenNote={openNote} />
    </div>
  )
}

export function TagManagerDialog({ open, onClose, ...contentProps }: TagManagerDialogProps) {
  return (
    <Dialog open={open} onOpenChange={(isOpen) => { if (!isOpen) onClose() }}>
      <DialogContent className="flex max-h-[80vh] flex-col sm:max-w-[520px]">
        <DialogHeader>
          <DialogTitle>{translate(contentProps.locale, 'tagManager.title')}</DialogTitle>
          <DialogDescription>{translate(contentProps.locale, 'tagManager.description')}</DialogDescription>
        </DialogHeader>
        {open && <TagManagerContent {...contentProps} onClose={onClose} />}
      </DialogContent>
    </Dialog>
  )
}
