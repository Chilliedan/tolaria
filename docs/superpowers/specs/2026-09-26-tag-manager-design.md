# Tag Manager — Design

Date: 2026-09-26
Status: Approved design, pending implementation plan

## Problem

Tags in Tolaria are ordinary frontmatter list properties (`tags: [blues, chicago]`). Users can add, remove, and colour tags one note at a time through `TagsDropdown`, and filter by tag in saved Views, but there is no way to see every tag in the vault or to rename, merge, or delete a tag across all notes. Doing so today means editing Markdown files by hand.

## Goals

- List every tag in the vault with its usage count, grouped by property.
- Rename a tag across every note that uses it.
- Merge one tag into another across every note.
- Delete a tag from every note that uses it.
- Keep tag colours consistent through these operations.

## Non-goals

- A persistent sidebar tag browser or a new note-list selection kind.
- Case-insensitive matching or automatic normalisation of tag spelling.
- Per-note preview/untick before applying (the confirmation shows a count only).
- Automatic Git commits; Git history is the undo, as for any other edit.
- A new Rust/Tauri command (see Approach).

## Scope of "tags"

Every property whose display mode resolves to `tags` counts: the default keys matched by `TAGS_KEY_PATTERNS` in `src/utils/propertyTypes.ts` (`tags`, `keywords`, `categories`, `labels`) plus any custom property explicitly set to Tags mode in `property_display_modes`. Tags are **grouped per property**: `blues` in `tags` and `blues` in `genre` are distinct entries, and operations act on one property at a time.

Matching is exact and case-sensitive, consistent with saved-View `contains` filtering over scalar arrays (`src/utils/viewFilterArrayFields.ts`).

## Approach

The frontend computes the inventory and the per-note rewrite plan from vault entries already held in memory, then applies the plan by calling the existing `update_frontmatter` command once per affected note through the same `updateFrontmatter` path the Properties panel uses (`src/hooks/frontmatterOps.ts`).

Rejected alternative: a new `rewrite_tag_values` Rust command in `tolaria-core`. It would be faster on very large vaults but requires new desktop and `tolaria-server` routing, docs, and duplicates YAML handling that `update_frontmatter` already owns. Per-note calls are adequate for the tens-to-hundreds of notes a single tag typically touches, and the web client gets the feature with no extra backend work.

## User experience

**Entry point:** a "Manage Tags" command in the command palette (registered via `useCommandRegistry`) opens `TagManagerDialog`, a shadcn `Dialog`.

**Dialog layout:**

- A shadcn `Select` chooses the property. It lists every tags-mode property that has at least one value in the vault and defaults to `tags` when present, otherwise the first alphabetically. If no tags-mode property has any value, the dialog shows an empty state.
- A shadcn `Input` filters the tag list by substring (case-insensitive for filtering only).
- Each row shows the tag as a `TagPill` (existing colour), its note count, and a row menu with: **Show notes**, **Rename…**, **Merge into…**, **Delete…**.
- Rows are sorted by count descending, then tag name ascending.

**Show notes:** expands the row in place to list the titles of notes using that tag. Clicking a title opens the note and closes the dialog.

**Rename:** the row turns into an inline shadcn `Input` prefilled with the tag. On submit:
- Empty or unchanged value → no-op.
- New value that does not exist in this property → rename.
- New value that already exists in this property → treated as a merge into the existing tag; the confirmation text says so.

**Merge into:** opens a picker reusing the tag option list/combobox pattern from `TagsDropdown`, listing the other tags of the same property. Choosing a target proceeds to confirmation.

**Delete:** proceeds to confirmation, styled like `ConfirmDeleteDialog`.

**Confirmation:** every write action shows "This will change N notes" (with rename/merge/delete-specific wording) and requires explicit confirmation. While applying, the dialog shows progress and disables actions.

**Result:** on full success the list refreshes. On partial failure the dialog shows "Changed X of N notes; Y failed" and lists the failed note titles. Already-written notes are not rolled back.

## Units

### `src/utils/tagInventory.ts` (pure)

`buildTagInventory(entries, displayModes) → TagInventory`

- `TagInventory = Map<property, TagUsage[]>`, `TagUsage = { tag: string; count: number; paths: string[] }`.
- Reads each entry's frontmatter for every tags-mode property. Arrays contribute each string element; a single string scalar contributes one tag. Non-string elements are ignored. A tag repeated within one note counts once for that note.
- Uses the same display-mode resolution as the Properties panel so the set of properties matches what users see.

### `src/utils/tagRewrite.ts` (pure)

`planTagRewrite(entries, property, op) → TagRewritePlan`

- `op` is `{ kind: 'rename'; from; to } | { kind: 'merge'; sources: string[]; target } | { kind: 'delete'; tag }`.
- Returns `[{ path, nextValues: string[] }]` for notes whose values actually change; unaffected notes are excluded.
- Preserves original tag order. Rename/merge replace the source in place; if the target is already present, the replaced occurrence is dropped so each tag appears once.
- A scalar-string value is rewritten as a list.
- Deleting the last tag yields `nextValues: []` (the property is kept as an empty list, not removed).

### `src/hooks/useTagManager.ts`

- Derives the inventory from current vault entries (memoised).
- `apply(property, op)`: builds the plan, writes each note sequentially via `updateFrontmatter`, collects `{ path, error }` failures, reports progress, then refreshes affected entries.
- Tag colours (`tag_colors` via `src/utils/tagStyles.ts`): on rename, the source colour moves to the new name unless the target already has one; on merge, the target keeps its colour and source colours are dropped; on delete, the colour is dropped. Colour updates happen only if at least one note write succeeded.
- Emits PostHog events (below).

### `src/components/TagManagerDialog.tsx`

Presentation only: property select, filter input, rows, row menu, inline rename, merge picker, confirmation, progress, and result states. Receives data and callbacks from `useTagManager`. Split into small subcomponents if needed to keep CodeScene at 10.0.

## Error handling

- Individual note write failures are collected, not thrown; the operation continues with remaining notes.
- A plan with zero affected notes shows a no-op message and makes no writes.
- If the vault entries change while the dialog is open, the inventory recomputes; an in-flight apply uses the plan computed at confirmation time.

## Analytics

- `tag_manager_opened` — no properties.
- `tag_manager_action` — `{ action: 'rename' | 'merge' | 'delete', notes_changed: number, failed: number }`.

No tag names, property names, or note content are sent. Event names are added to the `ProductAnalyticsEventName` union in `src/lib/telemetry.ts`.

## Localization

All copy goes in `src/lib/locales/en.json` under a `tagManager.*` namespace and is translated directly into every catalog in `src/lib/locales/` (fork rule; no Lara run). `pnpm l10n:validate` must pass.

## Testing

- Unit: `tagInventory` (arrays, scalars, duplicates in one note, custom tags-mode property, non-tags properties ignored).
- Unit: `tagRewrite` (rename, rename-into-existing = merge, multi-source merge with dedupe, delete, delete-last, order preservation, unaffected notes excluded).
- Hook: `useTagManager` apply with partial failure and colour migration rules.
- Component: `TagManagerDialog` filter, confirm flow, partial-failure result.
- Playwright: `tests/smoke/tag-manager.spec.ts` renaming a tag in `demo-vault-v2` via the command palette (not tagged `@smoke`).

## Docs

Update `docs/ARCHITECTURE.md` and `docs/ABSTRACTIONS.md` with the new hook, component, and pure utilities. No ADR: no new dependency, storage strategy, or core abstraction.
