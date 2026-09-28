import { useEffect, useRef, useState } from 'react'
import {
  Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  useTagManagerDialogState, NO_ESCAPE_GUARD,
  type EscapeGuard, type PendingAction, type RowEdit, type TagManagerDialogState,
} from '../hooks/useTagManagerDialogState'
import type { UpdateFrontmatter } from '../hooks/useTagManager'
import { translate, type AppLocale } from '../lib/i18n'
import type { VaultEntry } from '../types'
import { resolveRenameOp } from '../utils/tagRewrite'
import { TagManagerRow } from './TagManagerRow'
import { TagManagerConfirm, TagManagerProgressLine, TagManagerResult } from './TagManagerStatus'

interface TagManagerDialogProps {
  open: boolean
  onClose: () => void
  entries: VaultEntry[]
  locale: AppLocale
  onUpdateFrontmatter: UpdateFrontmatter
  onOpenNote: (entry: VaultEntry) => void
}

function PropertyPicker({ properties, value, locale, disabled, onChange }: {
  properties: string[]; value: string; locale: AppLocale; disabled: boolean; onChange: (property: string) => void
}) {
  return (
    <Select value={value} onValueChange={onChange} disabled={disabled}>
      <SelectTrigger className="h-8 w-40" aria-label={translate(locale, 'tagManager.property')}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {properties.map((property) => <SelectItem key={property} value={property}>{property}</SelectItem>)}
      </SelectContent>
    </Select>
  )
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

function useEscapeGuardSync(
  escapeGuardRef: { current: EscapeGuard },
  rowEdit: RowEdit | null,
  pending: PendingAction | null,
  cancelEdit: () => void,
  cancelPending: () => void,
): void {
  useEffect(() => {
    if (rowEdit) escapeGuardRef.current = { active: true, cancel: cancelEdit }
    else if (pending) escapeGuardRef.current = { active: true, cancel: cancelPending }
    else escapeGuardRef.current = NO_ESCAPE_GUARD
  }, [escapeGuardRef, rowEdit, pending, cancelEdit, cancelPending])
}

// Report whether an apply is running so the dialog shell can refuse to close mid-write.
function useBusySync(busy: boolean, onBusyChange: (busy: boolean) => void): void {
  useEffect(() => {
    onBusyChange(busy)
    return () => onBusyChange(false)
  }, [busy, onBusyChange])
}

interface TagManagerContentProps extends Omit<TagManagerDialogProps, 'open'> {
  escapeGuardRef: { current: EscapeGuard }
  onBusyChange: (busy: boolean) => void
}

function TagManagerContent({
  entries, locale, onUpdateFrontmatter, onOpenNote, onClose, escapeGuardRef, onBusyChange,
}: TagManagerContentProps) {
  const state = useTagManagerDialogState({ entries, locale, onUpdateFrontmatter })
  const { property } = state
  useEscapeGuardSync(escapeGuardRef, state.rowEdit, state.pending, state.cancelEdit, state.cancelPending)
  useBusySync(state.busy, onBusyChange)

  const openNote = (entry: VaultEntry) => {
    onOpenNote(entry)
    onClose()
  }

  if (!property) return <p className="py-6 text-center text-sm text-muted-foreground">{translate(locale, 'tagManager.empty')}</p>

  return (
    <div className="flex min-h-0 flex-col gap-3">
      <div className="flex items-center gap-2">
        <PropertyPicker
          properties={state.properties}
          value={property}
          locale={locale}
          disabled={state.busy}
          onChange={state.setSelectedProperty}
        />
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
  const escapeGuardRef = useRef<EscapeGuard>(NO_ESCAPE_GUARD)
  // While an apply is writing notes the dialog must stay open: closing it would drop the
  // progress and result, and a reopened dialog could plan a second apply from stale entries.
  const [busy, setBusy] = useState(false)

  const handleEscapeKeyDown = (event: KeyboardEvent) => {
    if (!busy && !escapeGuardRef.current.active) return
    event.preventDefault()
    if (!busy) escapeGuardRef.current.cancel()
  }

  return (
    <Dialog open={open} onOpenChange={(isOpen) => { if (!isOpen && !busy) onClose() }}>
      <DialogContent
        className="flex max-h-[80vh] flex-col sm:max-w-[520px]"
        showCloseButton={!busy}
        onEscapeKeyDown={handleEscapeKeyDown}
        onInteractOutside={(event) => { if (busy) event.preventDefault() }}
      >
        <DialogHeader>
          <DialogTitle>{translate(contentProps.locale, 'tagManager.title')}</DialogTitle>
          <DialogDescription>{translate(contentProps.locale, 'tagManager.description')}</DialogDescription>
        </DialogHeader>
        {open && (
          <TagManagerContent {...contentProps} onClose={onClose} escapeGuardRef={escapeGuardRef} onBusyChange={setBusy} />
        )}
      </DialogContent>
    </Dialog>
  )
}
