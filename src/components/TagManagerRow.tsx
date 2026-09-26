import { useState } from 'react'
import { DotsThree } from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { translate, type AppLocale } from '../lib/i18n'
import type { VaultEntry } from '../types'
import type { TagUsage } from '../utils/tagInventory'
import { TagPill } from './TagsDropdown'

export type TagRowEditMode = 'rename' | 'merge' | null

interface TagManagerRowProps {
  usage: TagUsage
  otherTags: string[]
  editMode: TagRowEditMode
  expanded: boolean
  notes: VaultEntry[]
  locale: AppLocale
  disabled: boolean
  onToggleNotes: () => void
  onStartEdit: (mode: Exclude<TagRowEditMode, null>) => void
  onCancelEdit: () => void
  onRename: (nextName: string) => void
  onMerge: (target: string) => void
  onDelete: () => void
  onOpenNote: (entry: VaultEntry) => void
}

function RenameEditor({ tag, locale, onRename, onCancel }: {
  tag: string; locale: AppLocale; onRename: (next: string) => void; onCancel: () => void
}) {
  const [value, setValue] = useState(tag)
  return (
    <form
      className="flex items-center gap-2 px-2 pb-2"
      onSubmit={(event) => { event.preventDefault(); onRename(value) }}
    >
      <Input
        autoFocus
        value={value}
        placeholder={translate(locale, 'tagManager.renamePlaceholder')}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => { if (event.key === 'Escape') { event.stopPropagation(); onCancel() } }}
        className="h-7"
      />
      <Button type="submit" size="sm">{translate(locale, 'tagManager.continue')}</Button>
      <Button type="button" variant="ghost" size="sm" onClick={onCancel}>{translate(locale, 'common.cancel')}</Button>
    </form>
  )
}

function MergeEditor({ otherTags, locale, onMerge, onCancel }: {
  otherTags: string[]; locale: AppLocale; onMerge: (target: string) => void; onCancel: () => void
}) {
  return (
    <div className="flex items-center gap-2 px-2 pb-2">
      <Select onValueChange={onMerge}>
        <SelectTrigger className="h-7 w-full" data-testid="tag-manager-merge-target">
          <SelectValue placeholder={translate(locale, 'tagManager.mergePlaceholder')} />
        </SelectTrigger>
        <SelectContent>
          {otherTags.map((tag) => (
            <SelectItem key={tag} value={tag}><TagPill tag={tag} /></SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button type="button" variant="ghost" size="sm" onClick={onCancel}>{translate(locale, 'common.cancel')}</Button>
    </div>
  )
}

function NotesList({ notes, onOpenNote }: { notes: VaultEntry[]; onOpenNote: (entry: VaultEntry) => void }) {
  return (
    <ul className="flex flex-col px-2 pb-2">
      {notes.map((entry) => (
        <li key={entry.path}>
          <Button variant="ghost" size="sm" className="h-6 w-full justify-start truncate" onClick={() => onOpenNote(entry)}>
            {entry.title}
          </Button>
        </li>
      ))}
    </ul>
  )
}

function RowMenu({ tag, expanded, canMerge, locale, disabled, onToggleNotes, onStartEdit, onDelete }: {
  tag: string; expanded: boolean; canMerge: boolean; locale: AppLocale; disabled: boolean
  onToggleNotes: () => void; onStartEdit: (mode: 'rename' | 'merge') => void; onDelete: () => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild disabled={disabled}>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          aria-label={translate(locale, 'tagManager.actions', { tag })}
          data-testid={`tag-manager-menu-${tag}`}
        >
          <DotsThree size={16} weight="bold" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={onToggleNotes}>
          {translate(locale, expanded ? 'tagManager.hideNotes' : 'tagManager.showNotes')}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => onStartEdit('rename')}>{translate(locale, 'tagManager.rename')}</DropdownMenuItem>
        <DropdownMenuItem disabled={!canMerge} onSelect={() => onStartEdit('merge')}>
          {translate(locale, 'tagManager.merge')}
        </DropdownMenuItem>
        <DropdownMenuItem variant="destructive" onSelect={onDelete}>{translate(locale, 'tagManager.delete')}</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function TagManagerRow(props: TagManagerRowProps) {
  const { usage, otherTags, editMode, expanded, notes, locale, disabled } = props
  return (
    <li className="rounded-md hover:bg-muted/50" data-testid={`tag-manager-row-${usage.tag}`}>
      <div className="flex items-center gap-2 px-2 py-1">
        <TagPill tag={usage.tag} className="min-w-0" />
        <span className="ml-auto text-xs text-muted-foreground">
          {translate(locale, 'tagManager.noteCount', { count: usage.count })}
        </span>
        <RowMenu
          tag={usage.tag}
          expanded={expanded}
          canMerge={otherTags.length > 0}
          locale={locale}
          disabled={disabled}
          onToggleNotes={props.onToggleNotes}
          onStartEdit={props.onStartEdit}
          onDelete={props.onDelete}
        />
      </div>
      {editMode === 'rename' && (
        <RenameEditor tag={usage.tag} locale={locale} onRename={props.onRename} onCancel={props.onCancelEdit} />
      )}
      {editMode === 'merge' && (
        <MergeEditor otherTags={otherTags} locale={locale} onMerge={props.onMerge} onCancel={props.onCancelEdit} />
      )}
      {expanded && <NotesList notes={notes} onOpenNote={props.onOpenNote} />}
    </li>
  )
}
