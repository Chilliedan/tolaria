# Tag Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Manage Tags" command-palette dialog that lists every tag per tags-mode property with note counts and can rename, merge, and delete a tag across the whole vault.

**Architecture:** Pure utilities compute the tag inventory (`tagInventory.ts`), the per-note rewrite plan (`tagRewrite.ts`), and tag-colour changes (`tagColorMigration.ts`) from in-memory `VaultEntry` data. A `useTagManager` hook applies a plan by calling the existing `handleUpdateFrontmatter` once per affected note with `{ silent: true }` (no toast, no undo-history entry, errors thrown so they can be collected). `TagManagerDialog` renders the UI and is opened by a new `manage-tags` command wired the same way as `create-type`.

**Tech Stack:** React 19 + TypeScript, shadcn/ui (`Dialog`, `Select`, `Input`, `Button`, `DropdownMenu`), Phosphor icons, Vitest + Testing Library, Playwright.

**Spec:** `docs/superpowers/specs/2026-09-26-tag-manager-design.md`

## Global Constraints

- Tags are grouped **per property**; matching is exact and case-sensitive. The filter input alone is case-insensitive.
- A property is a tags property when `getEffectiveDisplayMode(key, value, overrides) === 'tags'` (`src/utils/propertyTypes.ts`).
- Rows sort by count descending, then tag ascending (`localeCompare`).
- Deleting a note's last tag writes `[]`; the property is never removed.
- Writes go only through `handleUpdateFrontmatter(path, property, nextValues, { silent: true })`. No new Tauri/server command, no ADR.
- Every write action requires confirmation that states the number of notes that will change. No rollback on partial failure.
- All UI copy lives in `src/lib/locales/en.json` under `tagManager.*` (plus `command.note.manageTags`) and is hand-translated into all 20 other catalogs in `src/lib/locales/` (fork rule; do not run `pnpm l10n:translate`). `pnpm l10n:validate` must pass.
- PostHog: `tag_manager_opened` (no properties) and `tag_manager_action` with `{ action, notes_changed, failed }`. Never send tag names, property names, paths, or note content. (`ProductAnalyticsEventName` is `string` in `src/lib/telemetry.ts`, so no union needs updating; this corrects the spec.)
- Use shadcn/ui controls only; no raw `<input>`, `<select>`, or `<button>`.
- Every new code file must score CodeScene 10.0 and have zero Codacy findings. Every touched existing file must improve or stay at 10.0 (AGENTS.md). Capture baselines before editing (Task 0).
- Current branch is `web-client` (fork `Chilliedan/tolaria`). Commit there. Confirm the push target with the repository owner before pushing.

### Refinements to the spec discovered while planning

1. **Tag colours are global, not per-property.** `tag_colors` is keyed by tag name only (`src/utils/tagStyles.ts`). So a source tag's colour is dropped only if the tag is no longer used in *any* property and every write succeeded. The target gets the source's colour only if the target has none.
2. **Merge picker uses a shadcn `Select`** of the other tags in the same property, each rendered as a `TagPill`, rather than re-embedding the `TagsDropdown` combobox. This follows the AGENTS.md control table.
3. **Confirmation is an inline panel inside the dialog** (destructive `Button` for delete), not a nested dialog.

---

## File Structure

| File | Status | Responsibility |
|---|---|---|
| `src/utils/tagInventory.ts` | Create | Pure: collect tags per tags-mode property with counts and paths |
| `src/utils/tagRewrite.ts` | Create | Pure: op types, rename→merge resolution, per-note rewrite plan |
| `src/utils/tagColorMigration.ts` | Create | Pure: colour changes implied by an op |
| `src/hooks/useTagManager.ts` | Create | Inventory memo, apply loop, progress, failures, colours, analytics |
| `src/components/TagManagerDialog.tsx` | Create | Dialog shell and content state machine |
| `src/components/TagManagerRow.tsx` | Create | One tag row: pill, count, menu, inline rename, merge select, notes list |
| `src/components/TagManagerStatus.tsx` | Create | Confirm panel, progress line, result summary |
| `src/hooks/useDialogs.ts` | Modify | `showTagManager` / `openTagManager` / `closeTagManager` |
| `src/hooks/commands/noteCommands.ts` | Modify | `manage-tags` command |
| `src/hooks/commands/localizeCommands.ts` | Modify | Map `manage-tags` → `command.note.manageTags` |
| `src/hooks/useCommandRegistry.ts` | Modify | `onManageTags` config passthrough |
| `src/hooks/useAppCommands.ts` | Modify | `onManageTags` config passthrough |
| `src/App.tsx` | Modify | Wire command, render dialog |
| `src/lib/locales/*.json` | Modify | New keys in all 21 catalogs |
| `tests/smoke/tag-manager.spec.ts` | Create | End-to-end rename on a fixture vault copy |
| `docs/ARCHITECTURE.md`, `docs/ABSTRACTIONS.md` | Modify | Document the new units |

---

### Task 0: Baselines (no code)

- [ ] **Step 1: Project-level CodeScene gate.** Compare the project's Hotspot and Average Code Health with `.codescene-thresholds` (CodeScene MCP, or the API via `CODESCENE_PAT` + `CODESCENE_PROJECT_ID`). If either is below its floor, stop and refactor first per AGENTS.md.
- [ ] **Step 2: File-level CodeScene baselines.** Record the score for each existing file to be modified: `src/hooks/useDialogs.ts`, `src/hooks/commands/noteCommands.ts`, `src/hooks/commands/localizeCommands.ts`, `src/hooks/useCommandRegistry.ts`, `src/hooks/useAppCommands.ts`, `src/App.tsx`.
- [ ] **Step 3: Codacy analyzer parity and baselines.** Run `codacy_list_repository_tools` and record the roster. Run `mcp__codacy__codacy_cli_analyze` (or `.codacy/cli.sh analyze <path> --format sarif`) on each file from Step 2 and record every finding. Also record dashboard findings for those files via `codacy_list_files` + `codacy_get_file_issues`.
- [ ] **Step 4: Save the evidence** in `/private/tmp/.../scratchpad/tag-manager-baselines.md` (not committed). It feeds the final release record.

---

### Task 1: Tag inventory

**Files:**
- Create: `src/utils/tagInventory.ts`
- Test: `src/utils/tagInventory.test.ts`

**Interfaces:**
- Consumes: `VaultEntry`, `VaultPropertyValue` from `src/types.ts`; `getEffectiveDisplayMode`, `PropertyDisplayMode` from `src/utils/propertyTypes.ts`; `FrontmatterValue` from `src/components/Inspector.tsx`.
- Produces:
  - `interface TagUsage { tag: string; count: number; paths: string[] }`
  - `type TagInventory = Map<string, TagUsage[]>` (property → sorted usages)
  - `function tagValuesOf(value: VaultPropertyValue | undefined): string[]`
  - `function buildTagInventory(entries: readonly VaultEntry[], overrides: Record<string, PropertyDisplayMode>): TagInventory`
  - `function listTagProperties(inventory: TagInventory): string[]`
  - `function isTagUsedOutside(inventory: TagInventory, tag: string, property: string): boolean`

- [ ] **Step 1: Write the failing tests**

```ts
// src/utils/tagInventory.test.ts
import { describe, expect, it } from 'vitest'
import { makeEntry } from '../test-utils/noteListTestUtils'
import { buildTagInventory, isTagUsedOutside, listTagProperties, tagValuesOf } from './tagInventory'

describe('tagValuesOf', () => {
  it('returns string elements of an array, once each, skipping empty and non-strings', () => {
    expect(tagValuesOf(['blues', 'jazz', 'blues', '', 3, true])).toEqual(['blues', 'jazz'])
  })

  it('treats a single string as one tag', () => {
    expect(tagValuesOf('blues')).toEqual(['blues'])
  })

  it('returns nothing for null, numbers, booleans, and missing values', () => {
    expect(tagValuesOf(null)).toEqual([])
    expect(tagValuesOf(4)).toEqual([])
    expect(tagValuesOf(false)).toEqual([])
    expect(tagValuesOf(undefined)).toEqual([])
  })
})

describe('buildTagInventory', () => {
  const entries = [
    makeEntry({ path: '/a.md', properties: { tags: ['blues', 'chicago'], genre: ['blues'] } }),
    makeEntry({ path: '/b.md', properties: { tags: ['blues'], categories: 'music' } }),
    makeEntry({ path: '/c.md', properties: { tags: ['chicago', 'chicago'], owner: 'Ann' } }),
  ]

  it('groups tags by tags-mode property with counts sorted by count desc then name', () => {
    const inventory = buildTagInventory(entries, {})
    expect(inventory.get('tags')).toEqual([
      { tag: 'blues', count: 2, paths: ['/a.md', '/b.md'] },
      { tag: 'chicago', count: 2, paths: ['/a.md', '/c.md'] },
    ])
    expect(inventory.get('categories')).toEqual([{ tag: 'music', count: 1, paths: ['/b.md'] }])
  })

  it('ignores properties that are not in tags mode', () => {
    const inventory = buildTagInventory(entries, {})
    expect(inventory.has('genre')).toBe(false)
    expect(inventory.has('owner')).toBe(false)
  })

  it('includes custom properties switched to tags mode', () => {
    const inventory = buildTagInventory(entries, { genre: 'tags' })
    expect(inventory.get('genre')).toEqual([{ tag: 'blues', count: 1, paths: ['/a.md'] }])
  })

  it('lists properties with tags first, then alphabetically', () => {
    const inventory = buildTagInventory(entries, { genre: 'tags' })
    expect(listTagProperties(inventory)).toEqual(['tags', 'categories', 'genre'])
  })

  it('reports whether a tag is used in another property', () => {
    const inventory = buildTagInventory(entries, { genre: 'tags' })
    expect(isTagUsedOutside(inventory, 'blues', 'tags')).toBe(true)
    expect(isTagUsedOutside(inventory, 'chicago', 'tags')).toBe(false)
  })
})
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run src/utils/tagInventory.test.ts`
Expected: FAIL, "Failed to resolve import ./tagInventory".

- [ ] **Step 3: Implement**

```ts
// src/utils/tagInventory.ts
import type { FrontmatterValue } from '../components/Inspector'
import type { VaultEntry, VaultPropertyValue } from '../types'
import { getEffectiveDisplayMode, type PropertyDisplayMode } from './propertyTypes'

export interface TagUsage {
  tag: string
  count: number
  paths: string[]
}

export type TagInventory = Map<string, TagUsage[]>

type DisplayModeOverrides = Record<string, PropertyDisplayMode>
type PathsByTag = Map<string, string[]>

const PRIMARY_TAG_PROPERTY = 'tags'

export function tagValuesOf(value: VaultPropertyValue | undefined): string[] {
  if (typeof value === 'string') return value === '' ? [] : [value]
  if (!Array.isArray(value)) return []
  const tags = value.filter((item): item is string => typeof item === 'string' && item !== '')
  return [...new Set(tags)]
}

function isTagsProperty(key: string, value: VaultPropertyValue, overrides: DisplayModeOverrides): boolean {
  return getEffectiveDisplayMode(key, value as FrontmatterValue, overrides) === 'tags'
}

function addUsage(collected: Map<string, PathsByTag>, property: string, tag: string, path: string): void {
  const byTag = collected.get(property) ?? new Map<string, string[]>()
  byTag.set(tag, [...(byTag.get(tag) ?? []), path])
  collected.set(property, byTag)
}

function compareUsage(a: TagUsage, b: TagUsage): number {
  return b.count - a.count || a.tag.localeCompare(b.tag)
}

function toSortedUsages(byTag: PathsByTag): TagUsage[] {
  return [...byTag.entries()]
    .map(([tag, paths]) => ({ tag, count: paths.length, paths }))
    .sort(compareUsage)
}

export function buildTagInventory(
  entries: readonly VaultEntry[],
  overrides: DisplayModeOverrides,
): TagInventory {
  const collected = new Map<string, PathsByTag>()
  for (const entry of entries) {
    for (const [property, value] of Object.entries(entry.properties ?? {})) {
      if (!isTagsProperty(property, value, overrides)) continue
      for (const tag of tagValuesOf(value)) addUsage(collected, property, tag, entry.path)
    }
  }
  return new Map([...collected.entries()].map(([property, byTag]) => [property, toSortedUsages(byTag)]))
}

export function listTagProperties(inventory: TagInventory): string[] {
  return [...inventory.keys()].sort((a, b) => {
    if (a === PRIMARY_TAG_PROPERTY) return -1
    if (b === PRIMARY_TAG_PROPERTY) return 1
    return a.localeCompare(b)
  })
}

export function isTagUsedOutside(inventory: TagInventory, tag: string, property: string): boolean {
  return [...inventory.entries()].some(
    ([otherProperty, usages]) => otherProperty !== property && usages.some((usage) => usage.tag === tag),
  )
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/utils/tagInventory.test.ts`
Expected: PASS (8 tests).

- [ ] **Step 5: Gates and commit.** Run CodeScene file review (must be 10.0) and a Codacy local scan on both new files (zero findings), then:

```bash
git add src/utils/tagInventory.ts src/utils/tagInventory.test.ts
git commit -m "feat: add tag inventory utility for tag manager"
```

---

### Task 2: Tag rewrite plan

**Files:**
- Create: `src/utils/tagRewrite.ts`
- Test: `src/utils/tagRewrite.test.ts`

**Interfaces:**
- Consumes: `VaultEntry`, `VaultPropertyValue` from `src/types.ts`.
- Produces:
  - `type TagRewriteOp = { kind: 'rename'; from: string; to: string } | { kind: 'merge'; sources: string[]; target: string } | { kind: 'delete'; tag: string }`
  - `interface TagRewriteStep { path: string; nextValues: string[] }`
  - `function planTagRewrite(entries: readonly VaultEntry[], property: string, op: TagRewriteOp): TagRewriteStep[]`
  - `function resolveRenameOp(from: string, rawTo: string, existingTags: readonly string[]): TagRewriteOp | null` (returns `null` for an empty or unchanged name, and a `merge` op when the new name already exists)
  - `function sourceTagsOf(op: TagRewriteOp): string[]`

- [ ] **Step 1: Write the failing tests**

```ts
// src/utils/tagRewrite.test.ts
import { describe, expect, it } from 'vitest'
import { makeEntry } from '../test-utils/noteListTestUtils'
import { planTagRewrite, resolveRenameOp, sourceTagsOf } from './tagRewrite'

const entries = [
  makeEntry({ path: '/a.md', properties: { tags: ['blues', 'chicago', 'live'] } }),
  makeEntry({ path: '/b.md', properties: { tags: ['jazz', 'blues'] } }),
  makeEntry({ path: '/c.md', properties: { tags: 'blues' } }),
  makeEntry({ path: '/d.md', properties: { tags: ['folk'] } }),
  makeEntry({ path: '/e.md', properties: { genre: ['blues'] } }),
]

describe('planTagRewrite', () => {
  it('renames in place, preserving order, and skips unaffected notes and other properties', () => {
    expect(planTagRewrite(entries, 'tags', { kind: 'rename', from: 'blues', to: 'soul' })).toEqual([
      { path: '/a.md', nextValues: ['soul', 'chicago', 'live'] },
      { path: '/b.md', nextValues: ['jazz', 'soul'] },
      { path: '/c.md', nextValues: ['soul'] },
    ])
  })

  it('merges several sources into a target without duplicating it', () => {
    expect(planTagRewrite(entries, 'tags', { kind: 'merge', sources: ['blues', 'live'], target: 'jazz' })).toEqual([
      { path: '/a.md', nextValues: ['jazz', 'chicago'] },
      { path: '/b.md', nextValues: ['jazz'] },
      { path: '/c.md', nextValues: ['jazz'] },
    ])
  })

  it('deletes a tag and leaves an empty list when it was the last one', () => {
    expect(planTagRewrite(entries, 'tags', { kind: 'delete', tag: 'blues' })).toEqual([
      { path: '/a.md', nextValues: ['chicago', 'live'] },
      { path: '/b.md', nextValues: ['jazz'] },
      { path: '/c.md', nextValues: [] },
    ])
  })

  it('keeps non-string list elements as strings when rewriting', () => {
    const mixed = [makeEntry({ path: '/m.md', properties: { tags: [2024, 'blues'] } })]
    expect(planTagRewrite(mixed, 'tags', { kind: 'delete', tag: 'blues' })).toEqual([
      { path: '/m.md', nextValues: ['2024'] },
    ])
  })

  it('returns no steps when no note uses the tag', () => {
    expect(planTagRewrite(entries, 'tags', { kind: 'delete', tag: 'missing' })).toEqual([])
  })
})

describe('resolveRenameOp', () => {
  const existing = ['blues', 'jazz']

  it('returns a rename for a new trimmed name', () => {
    expect(resolveRenameOp('blues', '  soul ', existing)).toEqual({ kind: 'rename', from: 'blues', to: 'soul' })
  })

  it('returns a merge when the new name already exists', () => {
    expect(resolveRenameOp('blues', 'jazz', existing)).toEqual({ kind: 'merge', sources: ['blues'], target: 'jazz' })
  })

  it('returns null for empty or unchanged names', () => {
    expect(resolveRenameOp('blues', '   ', existing)).toBeNull()
    expect(resolveRenameOp('blues', 'blues', existing)).toBeNull()
  })
})

describe('sourceTagsOf', () => {
  it('lists the tags an op removes', () => {
    expect(sourceTagsOf({ kind: 'rename', from: 'a', to: 'b' })).toEqual(['a'])
    expect(sourceTagsOf({ kind: 'merge', sources: ['a', 'c'], target: 'b' })).toEqual(['a', 'c'])
    expect(sourceTagsOf({ kind: 'delete', tag: 'a' })).toEqual(['a'])
  })
})
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run src/utils/tagRewrite.test.ts`
Expected: FAIL, "Failed to resolve import ./tagRewrite".

- [ ] **Step 3: Implement**

```ts
// src/utils/tagRewrite.ts
import type { VaultEntry, VaultPropertyValue } from '../types'

export type TagRewriteOp =
  | { kind: 'rename'; from: string; to: string }
  | { kind: 'merge'; sources: string[]; target: string }
  | { kind: 'delete'; tag: string }

export interface TagRewriteStep {
  path: string
  nextValues: string[]
}

export function sourceTagsOf(op: TagRewriteOp): string[] {
  if (op.kind === 'rename') return [op.from]
  if (op.kind === 'merge') return [...op.sources]
  return [op.tag]
}

function targetTagOf(op: TagRewriteOp): string | null {
  if (op.kind === 'rename') return op.to
  if (op.kind === 'merge') return op.target
  return null
}

function listValuesOf(value: VaultPropertyValue | undefined): string[] {
  if (typeof value === 'string') return [value]
  if (!Array.isArray(value)) return []
  return value.map(String)
}

function rewriteValues(values: readonly string[], sources: ReadonlySet<string>, target: string | null): string[] {
  const result: string[] = []
  for (const value of values) {
    const next = sources.has(value) ? target : value
    if (next !== null && !result.includes(next)) result.push(next)
  }
  return result
}

export function planTagRewrite(
  entries: readonly VaultEntry[],
  property: string,
  op: TagRewriteOp,
): TagRewriteStep[] {
  const sources = new Set(sourceTagsOf(op))
  const target = targetTagOf(op)
  const steps: TagRewriteStep[] = []
  for (const entry of entries) {
    const values = listValuesOf(entry.properties?.[property])
    if (!values.some((value) => sources.has(value))) continue
    steps.push({ path: entry.path, nextValues: rewriteValues(values, sources, target) })
  }
  return steps
}

export function resolveRenameOp(
  from: string,
  rawTo: string,
  existingTags: readonly string[],
): TagRewriteOp | null {
  const to = rawTo.trim()
  if (to === '' || to === from) return null
  if (existingTags.includes(to)) return { kind: 'merge', sources: [from], target: to }
  return { kind: 'rename', from, to }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/utils/tagRewrite.test.ts`
Expected: PASS (9 tests).

- [ ] **Step 5: Gates and commit.** CodeScene 10.0 and zero Codacy findings on both files, then:

```bash
git add src/utils/tagRewrite.ts src/utils/tagRewrite.test.ts
git commit -m "feat: plan vault-wide tag rename, merge, and delete rewrites"
```

---

### Task 3: Tag colour migration

**Files:**
- Create: `src/utils/tagColorMigration.ts`
- Test: `src/utils/tagColorMigration.test.ts`

**Interfaces:**
- Consumes: `TagRewriteOp`, `sourceTagsOf` from Task 2.
- Produces:
  - `interface TagColorChange { tag: string; colorKey: string | null }`
  - `interface TagColorContext { getColorKey: (tag: string) => string | null; isTagStillUsed: (tag: string) => boolean }`
  - `function planTagColorChanges(op: TagRewriteOp, context: TagColorContext): TagColorChange[]`

- [ ] **Step 1: Write the failing tests**

```ts
// src/utils/tagColorMigration.test.ts
import { describe, expect, it } from 'vitest'
import { planTagColorChanges } from './tagColorMigration'

function context(colors: Record<string, string>, stillUsed: string[] = []) {
  return {
    getColorKey: (tag: string) => colors[tag] ?? null,
    isTagStillUsed: (tag: string) => stillUsed.includes(tag),
  }
}

describe('planTagColorChanges', () => {
  it('moves a renamed tag colour to the new name', () => {
    expect(planTagColorChanges({ kind: 'rename', from: 'blues', to: 'soul' }, context({ blues: 'red' }))).toEqual([
      { tag: 'soul', colorKey: 'red' },
      { tag: 'blues', colorKey: null },
    ])
  })

  it('keeps the target colour on merge and drops the source colours', () => {
    const op = { kind: 'merge' as const, sources: ['blues', 'live'], target: 'jazz' }
    expect(planTagColorChanges(op, context({ blues: 'red', live: 'green', jazz: 'blue' }))).toEqual([
      { tag: 'blues', colorKey: null },
      { tag: 'live', colorKey: null },
    ])
  })

  it('gives an uncoloured merge target the first source colour', () => {
    const op = { kind: 'merge' as const, sources: ['folk', 'blues'], target: 'jazz' }
    expect(planTagColorChanges(op, context({ blues: 'red' }))).toEqual([
      { tag: 'jazz', colorKey: 'red' },
      { tag: 'blues', colorKey: null },
    ])
  })

  it('keeps a source colour while the tag is still used elsewhere', () => {
    expect(planTagColorChanges({ kind: 'delete', tag: 'blues' }, context({ blues: 'red' }, ['blues']))).toEqual([])
  })

  it('drops a deleted tag colour and ignores uncoloured tags', () => {
    expect(planTagColorChanges({ kind: 'delete', tag: 'blues' }, context({ blues: 'red' }))).toEqual([
      { tag: 'blues', colorKey: null },
    ])
    expect(planTagColorChanges({ kind: 'delete', tag: 'folk' }, context({}))).toEqual([])
  })
})
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run src/utils/tagColorMigration.test.ts`
Expected: FAIL, "Failed to resolve import ./tagColorMigration".

- [ ] **Step 3: Implement**

```ts
// src/utils/tagColorMigration.ts
import { sourceTagsOf, type TagRewriteOp } from './tagRewrite'

export interface TagColorChange {
  tag: string
  colorKey: string | null
}

export interface TagColorContext {
  getColorKey: (tag: string) => string | null
  isTagStillUsed: (tag: string) => boolean
}

function targetColorChange(op: TagRewriteOp, context: TagColorContext): TagColorChange[] {
  if (op.kind === 'delete') return []
  const target = op.kind === 'rename' ? op.to : op.target
  if (context.getColorKey(target) !== null) return []
  const inherited = sourceTagsOf(op).map(context.getColorKey).find((key) => key !== null)
  return inherited ? [{ tag: target, colorKey: inherited }] : []
}

function sourceColorRemovals(op: TagRewriteOp, context: TagColorContext): TagColorChange[] {
  return sourceTagsOf(op)
    .filter((tag) => context.getColorKey(tag) !== null && !context.isTagStillUsed(tag))
    .map((tag) => ({ tag, colorKey: null }))
}

export function planTagColorChanges(op: TagRewriteOp, context: TagColorContext): TagColorChange[] {
  return [...targetColorChange(op, context), ...sourceColorRemovals(op, context)]
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/utils/tagColorMigration.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 5: Gates and commit.**

```bash
git add src/utils/tagColorMigration.ts src/utils/tagColorMigration.test.ts
git commit -m "feat: migrate tag colours when tags are renamed, merged, or deleted"
```

---

### Task 4: `useTagManager` hook

**Files:**
- Create: `src/hooks/useTagManager.ts`
- Test: `src/hooks/useTagManager.test.ts`

**Interfaces:**
- Consumes: Tasks 1–3; `loadDisplayModeOverrides` from `src/utils/propertyTypes.ts`; `getTagColorKey`, `setTagColor` from `src/utils/tagStyles.ts`; `trackEvent` from `src/lib/telemetry.ts`; `FrontmatterValue` from `src/components/Inspector.tsx`; `FrontmatterOpOptions` from `src/hooks/frontmatterOps.ts`.
- Produces:
  - `type UpdateFrontmatter = (path: string, key: string, value: FrontmatterValue, options?: FrontmatterOpOptions) => Promise<void>`
  - `interface TagManagerApplyResult { total: number; changed: number; failedPaths: string[] }`
  - `interface TagManagerProgress { done: number; total: number }`
  - `function useTagManager(options: { entries: readonly VaultEntry[]; updateFrontmatter: UpdateFrontmatter }): { inventory: TagInventory; properties: string[]; progress: TagManagerProgress | null; countAffected: (property: string, op: TagRewriteOp) => number; apply: (property: string, op: TagRewriteOp) => Promise<TagManagerApplyResult> }`

- [ ] **Step 1: Write the failing tests**

```ts
// src/hooks/useTagManager.test.ts
import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { makeEntry } from '../test-utils/noteListTestUtils'
import { useTagManager } from './useTagManager'

const { trackEventMock, setTagColorMock, colors } = vi.hoisted(() => ({
  trackEventMock: vi.fn(),
  setTagColorMock: vi.fn(),
  colors: {} as Record<string, string>,
}))

vi.mock('../lib/telemetry', () => ({ trackEvent: trackEventMock }))
vi.mock('../utils/tagStyles', () => ({
  getTagColorKey: (tag: string) => colors[tag] ?? null,
  setTagColor: setTagColorMock,
}))
vi.mock('../utils/propertyTypes', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../utils/propertyTypes')>()),
  loadDisplayModeOverrides: () => ({}),
}))

const entries = [
  makeEntry({ path: '/a.md', properties: { tags: ['blues', 'live'] } }),
  makeEntry({ path: '/b.md', properties: { tags: ['blues'] } }),
]

describe('useTagManager', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    for (const key of Object.keys(colors)) Reflect.deleteProperty(colors, key)
  })

  it('exposes the inventory and affected-note counts', () => {
    const { result } = renderHook(() => useTagManager({ entries, updateFrontmatter: vi.fn() }))
    expect(result.current.properties).toEqual(['tags'])
    expect(result.current.inventory.get('tags')?.[0]).toEqual({ tag: 'blues', count: 2, paths: ['/a.md', '/b.md'] })
    expect(result.current.countAffected('tags', { kind: 'delete', tag: 'live' })).toBe(1)
  })

  it('writes each affected note silently, migrates colours, and tracks the action', async () => {
    colors.blues = 'red'
    const updateFrontmatter = vi.fn().mockResolvedValue(undefined)
    const { result } = renderHook(() => useTagManager({ entries, updateFrontmatter }))

    let outcome
    await act(async () => {
      outcome = await result.current.apply('tags', { kind: 'rename', from: 'blues', to: 'soul' })
    })

    expect(updateFrontmatter).toHaveBeenCalledWith('/a.md', 'tags', ['soul', 'live'], { silent: true })
    expect(updateFrontmatter).toHaveBeenCalledWith('/b.md', 'tags', ['soul'], { silent: true })
    expect(outcome).toEqual({ total: 2, changed: 2, failedPaths: [] })
    expect(setTagColorMock).toHaveBeenCalledWith('soul', 'red')
    expect(setTagColorMock).toHaveBeenCalledWith('blues', null)
    expect(trackEventMock).toHaveBeenCalledWith('tag_manager_action', { action: 'rename', notes_changed: 2, failed: 0 })
    expect(result.current.progress).toBeNull()
  })

  it('collects failures, keeps going, and keeps the source colour', async () => {
    colors.blues = 'red'
    const updateFrontmatter = vi.fn()
      .mockRejectedValueOnce(new Error('disk full'))
      .mockResolvedValueOnce(undefined)
    const { result } = renderHook(() => useTagManager({ entries, updateFrontmatter }))

    let outcome
    await act(async () => {
      outcome = await result.current.apply('tags', { kind: 'delete', tag: 'blues' })
    })

    expect(outcome).toEqual({ total: 2, changed: 1, failedPaths: ['/a.md'] })
    expect(setTagColorMock).not.toHaveBeenCalled()
    expect(trackEventMock).toHaveBeenCalledWith('tag_manager_action', { action: 'delete', notes_changed: 1, failed: 1 })
  })

  it('does nothing and tracks nothing when no note is affected', async () => {
    const updateFrontmatter = vi.fn()
    const { result } = renderHook(() => useTagManager({ entries, updateFrontmatter }))

    let outcome
    await act(async () => {
      outcome = await result.current.apply('tags', { kind: 'delete', tag: 'missing' })
    })

    expect(outcome).toEqual({ total: 0, changed: 0, failedPaths: [] })
    expect(updateFrontmatter).not.toHaveBeenCalled()
    expect(trackEventMock).not.toHaveBeenCalled()
  })
})
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run src/hooks/useTagManager.test.ts`
Expected: FAIL, "Failed to resolve import ./useTagManager".

- [ ] **Step 3: Implement**

```ts
// src/hooks/useTagManager.ts
import { useCallback, useMemo, useState } from 'react'
import type { FrontmatterValue } from '../components/Inspector'
import { trackEvent } from '../lib/telemetry'
import type { VaultEntry } from '../types'
import { loadDisplayModeOverrides } from '../utils/propertyTypes'
import { planTagColorChanges } from '../utils/tagColorMigration'
import { buildTagInventory, isTagUsedOutside, listTagProperties, type TagInventory } from '../utils/tagInventory'
import { planTagRewrite, type TagRewriteOp, type TagRewriteStep } from '../utils/tagRewrite'
import { getTagColorKey, setTagColor } from '../utils/tagStyles'
import type { FrontmatterOpOptions } from './frontmatterOps'

export type UpdateFrontmatter = (
  path: string,
  key: string,
  value: FrontmatterValue,
  options?: FrontmatterOpOptions,
) => Promise<void>

export interface TagManagerApplyResult {
  total: number
  changed: number
  failedPaths: string[]
}

export interface TagManagerProgress {
  done: number
  total: number
}

interface UseTagManagerOptions {
  entries: readonly VaultEntry[]
  updateFrontmatter: UpdateFrontmatter
}

async function writeSteps(
  steps: readonly TagRewriteStep[],
  property: string,
  updateFrontmatter: UpdateFrontmatter,
  onProgress: (done: number) => void,
): Promise<string[]> {
  const failedPaths: string[] = []
  for (const [index, step] of steps.entries()) {
    try {
      await updateFrontmatter(step.path, property, step.nextValues, { silent: true })
    } catch {
      failedPaths.push(step.path)
    }
    onProgress(index + 1)
  }
  return failedPaths
}

function migrateColors(op: TagRewriteOp, inventory: TagInventory, property: string, hadFailures: boolean): void {
  const changes = planTagColorChanges(op, {
    getColorKey: getTagColorKey,
    isTagStillUsed: (tag) => hadFailures || isTagUsedOutside(inventory, tag, property),
  })
  for (const change of changes) setTagColor(change.tag, change.colorKey)
}

export function useTagManager({ entries, updateFrontmatter }: UseTagManagerOptions) {
  const [progress, setProgress] = useState<TagManagerProgress | null>(null)
  const inventory = useMemo(() => buildTagInventory(entries, loadDisplayModeOverrides()), [entries])
  const properties = useMemo(() => listTagProperties(inventory), [inventory])

  const countAffected = useCallback(
    (property: string, op: TagRewriteOp) => planTagRewrite(entries, property, op).length,
    [entries],
  )

  const apply = useCallback(async (property: string, op: TagRewriteOp): Promise<TagManagerApplyResult> => {
    const steps = planTagRewrite(entries, property, op)
    if (steps.length === 0) return { total: 0, changed: 0, failedPaths: [] }
    setProgress({ done: 0, total: steps.length })
    const failedPaths = await writeSteps(steps, property, updateFrontmatter, (done) => {
      setProgress({ done, total: steps.length })
    })
    const changed = steps.length - failedPaths.length
    if (changed > 0) migrateColors(op, inventory, property, failedPaths.length > 0)
    trackEvent('tag_manager_action', { action: op.kind, notes_changed: changed, failed: failedPaths.length })
    setProgress(null)
    return { total: steps.length, changed, failedPaths }
  }, [entries, inventory, updateFrontmatter])

  return { inventory, properties, progress, countAffected, apply }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/hooks/useTagManager.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Gates and commit.**

```bash
git add src/hooks/useTagManager.ts src/hooks/useTagManager.test.ts
git commit -m "feat: add useTagManager hook to apply vault-wide tag rewrites"
```

---

### Task 5: Tag manager dialog UI + English copy

**Files:**
- Modify: `src/lib/locales/en.json` (append keys at the end, before the closing `}`)
- Create: `src/components/TagManagerStatus.tsx`
- Create: `src/components/TagManagerRow.tsx`
- Create: `src/components/TagManagerDialog.tsx`
- Test: `src/components/TagManagerDialog.test.tsx`

**Interfaces:**
- Consumes: `useTagManager`, `UpdateFrontmatter`, `TagManagerApplyResult`, `TagManagerProgress` (Task 4); `TagUsage` (Task 1); `TagRewriteOp`, `resolveRenameOp` (Task 2); `TagPill` from `src/components/TagsDropdown.tsx`; `translate`, `AppLocale`, `TranslationKey` from `src/lib/i18n.ts`.
- Produces: `function TagManagerDialog(props: { open: boolean; onClose: () => void; entries: VaultEntry[]; locale: AppLocale; onUpdateFrontmatter: UpdateFrontmatter; onOpenNote: (entry: VaultEntry) => void }): JSX.Element`

- [ ] **Step 1: Add English copy.** Append to `src/lib/locales/en.json` (keep existing order; add a comma after the current last entry):

```json
  "command.note.manageTags": "Manage Tags",
  "tagManager.title": "Manage Tags",
  "tagManager.description": "Rename, merge, or delete tags across every note in the vault.",
  "tagManager.property": "Property",
  "tagManager.filterPlaceholder": "Filter tags…",
  "tagManager.empty": "No tags in this vault yet.",
  "tagManager.noMatches": "No tags match this filter.",
  "tagManager.noteCount": "{count} notes",
  "tagManager.actions": "Actions for {tag}",
  "tagManager.showNotes": "Show notes",
  "tagManager.hideNotes": "Hide notes",
  "tagManager.rename": "Rename…",
  "tagManager.merge": "Merge into…",
  "tagManager.delete": "Delete…",
  "tagManager.renamePlaceholder": "New tag name",
  "tagManager.mergePlaceholder": "Choose a tag",
  "tagManager.continue": "Continue",
  "tagManager.confirm.rename": "Rename “{from}” to “{to}” in {count} notes?",
  "tagManager.confirm.merge": "Merge “{from}” into “{to}” in {count} notes?",
  "tagManager.confirm.renameIntoExisting": "“{to}” already exists. Merge “{from}” into it in {count} notes?",
  "tagManager.confirm.delete": "Remove “{tag}” from {count} notes?",
  "tagManager.apply": "Apply",
  "tagManager.applying": "Updating {done} of {total} notes…",
  "tagManager.result.success": "Updated {count} notes.",
  "tagManager.result.partial": "Changed {changed} of {total} notes; {failed} failed:",
  "tagManager.result.noop": "No notes needed changing."
```

- [ ] **Step 2: Write the failing component tests**

```tsx
// src/components/TagManagerDialog.test.tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { makeEntry } from '../test-utils/noteListTestUtils'
import { TagManagerDialog } from './TagManagerDialog'

vi.mock('../lib/telemetry', () => ({ trackEvent: vi.fn() }))

const entries = [
  makeEntry({ path: '/a.md', title: 'Alpha', properties: { tags: ['blues', 'live'] } }),
  makeEntry({ path: '/b.md', title: 'Beta', properties: { tags: ['blues', 'jazz'] } }),
]

function renderDialog(updateFrontmatter = vi.fn().mockResolvedValue(undefined), onOpenNote = vi.fn()) {
  const onClose = vi.fn()
  render(
    <TagManagerDialog
      open
      onClose={onClose}
      entries={entries}
      locale="en"
      onUpdateFrontmatter={updateFrontmatter}
      onOpenNote={onOpenNote}
    />,
  )
  return { updateFrontmatter, onOpenNote, onClose }
}

function openRowMenu(tag: string) {
  fireEvent.pointerDown(screen.getByTestId(`tag-manager-menu-${tag}`), { button: 0, ctrlKey: false })
}

describe('TagManagerDialog', () => {
  beforeEach(() => vi.clearAllMocks())

  it('lists tags with counts, most used first', () => {
    renderDialog()
    const rows = screen.getAllByTestId(/^tag-manager-row-/)
    expect(rows.map((row) => row.dataset.testid)).toEqual([
      'tag-manager-row-blues', 'tag-manager-row-jazz', 'tag-manager-row-live',
    ])
    expect(screen.getByTestId('tag-manager-row-blues')).toHaveTextContent('2 notes')
  })

  it('filters tags case-insensitively', () => {
    renderDialog()
    fireEvent.change(screen.getByPlaceholderText('Filter tags…'), { target: { value: 'JA' } })
    expect(screen.getAllByTestId(/^tag-manager-row-/)).toHaveLength(1)
  })

  it('shows notes for a tag and opens one', () => {
    const { onOpenNote, onClose } = renderDialog()
    openRowMenu('jazz')
    fireEvent.click(screen.getByText('Show notes'))
    fireEvent.click(screen.getByRole('button', { name: 'Beta' }))
    expect(onOpenNote).toHaveBeenCalledWith(entries[1])
    expect(onClose).toHaveBeenCalled()
  })

  it('renames after confirmation and reports success', async () => {
    const { updateFrontmatter } = renderDialog()
    openRowMenu('blues')
    fireEvent.click(screen.getByText('Rename…'))
    fireEvent.change(screen.getByPlaceholderText('New tag name'), { target: { value: 'soul' } })
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))

    expect(screen.getByText('Rename “blues” to “soul” in 2 notes?')).toBeInTheDocument()
    expect(updateFrontmatter).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(screen.getByText('Updated 2 notes.')).toBeInTheDocument())
    expect(updateFrontmatter).toHaveBeenCalledWith('/a.md', 'tags', ['soul', 'live'], { silent: true })
  })

  it('warns when a rename targets an existing tag', () => {
    renderDialog()
    openRowMenu('live')
    fireEvent.click(screen.getByText('Rename…'))
    fireEvent.change(screen.getByPlaceholderText('New tag name'), { target: { value: 'jazz' } })
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    expect(screen.getByText('“jazz” already exists. Merge “live” into it in 1 notes?')).toBeInTheDocument()
  })

  it('cancelling a confirmation writes nothing', () => {
    const { updateFrontmatter } = renderDialog()
    openRowMenu('live')
    fireEvent.click(screen.getByText('Delete…'))
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(updateFrontmatter).not.toHaveBeenCalled()
    expect(screen.queryByText('Remove “live” from 1 notes?')).not.toBeInTheDocument()
  })

  it('reports partial failures with note titles', async () => {
    const updateFrontmatter = vi.fn()
      .mockRejectedValueOnce(new Error('locked'))
      .mockResolvedValueOnce(undefined)
    renderDialog(updateFrontmatter)
    openRowMenu('blues')
    fireEvent.click(screen.getByText('Delete…'))
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(screen.getByText('Changed 1 of 2 notes; 1 failed:')).toBeInTheDocument())
    expect(screen.getByTestId('tag-manager-failed-list')).toHaveTextContent('Alpha')
  })

  it('shows an empty state when the vault has no tags', () => {
    render(
      <TagManagerDialog open onClose={vi.fn()} entries={[]} locale="en" onUpdateFrontmatter={vi.fn()} onOpenNote={vi.fn()} />,
    )
    expect(screen.getByText('No tags in this vault yet.')).toBeInTheDocument()
  })
})
```

- [ ] **Step 3: Run to verify failure**

Run: `pnpm vitest run src/components/TagManagerDialog.test.tsx`
Expected: FAIL, "Failed to resolve import ./TagManagerDialog".

- [ ] **Step 4: Implement `TagManagerStatus.tsx`**

```tsx
// src/components/TagManagerStatus.tsx
import { Button } from '@/components/ui/button'
import { translate, type AppLocale } from '../lib/i18n'
import type { TagManagerApplyResult, TagManagerProgress } from '../hooks/useTagManager'

interface TagManagerConfirmProps {
  message: string
  destructive: boolean
  locale: AppLocale
  onCancel: () => void
  onApply: () => void
}

export function TagManagerConfirm({ message, destructive, locale, onCancel, onApply }: TagManagerConfirmProps) {
  return (
    <div className="flex flex-col gap-2 rounded-md border border-border bg-muted/40 p-3" data-testid="tag-manager-confirm">
      <p className="text-sm">{message}</p>
      <div className="flex justify-end gap-2">
        <Button variant="outline" size="sm" onClick={onCancel}>{translate(locale, 'common.cancel')}</Button>
        <Button variant={destructive ? 'destructive' : 'default'} size="sm" onClick={onApply}>
          {translate(locale, 'tagManager.apply')}
        </Button>
      </div>
    </div>
  )
}

export function TagManagerProgressLine({ progress, locale }: { progress: TagManagerProgress; locale: AppLocale }) {
  return (
    <p className="text-sm text-muted-foreground" role="status">
      {translate(locale, 'tagManager.applying', { done: progress.done, total: progress.total })}
    </p>
  )
}

interface TagManagerResultProps {
  result: TagManagerApplyResult
  titleForPath: (path: string) => string
  locale: AppLocale
}

export function TagManagerResult({ result, titleForPath, locale }: TagManagerResultProps) {
  if (result.total === 0) {
    return <p className="text-sm text-muted-foreground" role="status">{translate(locale, 'tagManager.result.noop')}</p>
  }
  if (result.failedPaths.length === 0) {
    return <p className="text-sm" role="status">{translate(locale, 'tagManager.result.success', { count: result.changed })}</p>
  }
  return (
    <div className="flex flex-col gap-1 text-sm" role="status">
      <p>
        {translate(locale, 'tagManager.result.partial', {
          changed: result.changed, total: result.total, failed: result.failedPaths.length,
        })}
      </p>
      <ul className="list-disc pl-5 text-muted-foreground" data-testid="tag-manager-failed-list">
        {result.failedPaths.map((path) => <li key={path}>{titleForPath(path)}</li>)}
      </ul>
    </div>
  )
}
```

- [ ] **Step 5: Implement `TagManagerRow.tsx`**

```tsx
// src/components/TagManagerRow.tsx
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
```

Before relying on `DropdownMenuItem variant="destructive"`, check `src/components/ui/dropdown-menu.tsx` supports a `variant` prop (standard shadcn v4 does). If it does not, use `className="text-destructive"` instead.

- [ ] **Step 6: Implement `TagManagerDialog.tsx`**

```tsx
// src/components/TagManagerDialog.tsx
import { useMemo, useState } from 'react'
import {
  Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useTagManager, type TagManagerApplyResult, type UpdateFrontmatter } from '../hooks/useTagManager'
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

function TagManagerContent({ entries, locale, onUpdateFrontmatter, onOpenNote, onClose }: Omit<TagManagerDialogProps, 'open'>) {
  const manager = useTagManager({ entries, updateFrontmatter: onUpdateFrontmatter })
  const [selectedProperty, setSelectedProperty] = useState<string | null>(null)
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

  const openNote = (entry: VaultEntry) => {
    onOpenNote(entry)
    onClose()
  }

  if (!property) return <p className="py-6 text-center text-sm text-muted-foreground">{translate(locale, 'tagManager.empty')}</p>

  return (
    <div className="flex min-h-0 flex-col gap-3">
      <div className="flex items-center gap-2">
        <PropertyPicker properties={manager.properties} value={property} locale={locale} onChange={setSelectedProperty} />
        <Input
          value={filter}
          placeholder={translate(locale, 'tagManager.filterPlaceholder')}
          onChange={(event) => setFilter(event.target.value)}
          className="h-8"
        />
      </div>
      {pending && (
        <TagManagerConfirm
          message={pending.message}
          destructive={pending.op.kind === 'delete'}
          locale={locale}
          onCancel={() => setPending(null)}
          onApply={() => void applyPending()}
        />
      )}
      {manager.progress && <TagManagerProgressLine progress={manager.progress} locale={locale} />}
      {result && (
        <TagManagerResult
          result={result}
          titleForPath={(path) => entriesByPath.get(path)?.title ?? path}
          locale={locale}
        />
      )}
      {visible.length === 0
        ? <p className="py-4 text-center text-sm text-muted-foreground">{translate(locale, 'tagManager.noMatches')}</p>
        : (
          <ul className="flex min-h-0 flex-col overflow-y-auto">
            {visible.map((usage) => (
              <TagManagerRow
                key={usage.tag}
                usage={usage}
                otherTags={usages.map((other) => other.tag).filter((tag) => tag !== usage.tag)}
                editMode={rowEdit?.tag === usage.tag ? rowEdit.mode : null}
                expanded={expandedTag === usage.tag}
                notes={usage.paths.flatMap((path) => entriesByPath.get(path) ?? [])}
                locale={locale}
                disabled={busy}
                onToggleNotes={() => setExpandedTag((current) => (current === usage.tag ? null : usage.tag))}
                onStartEdit={(mode) => setRowEdit({ tag: usage.tag, mode })}
                onCancelEdit={() => setRowEdit(null)}
                onRename={(next) => {
                  const op = resolveRenameOp(usage.tag, next, usages.map((other) => other.tag))
                  if (op) propose(op, op.kind === 'merge')
                  else setRowEdit(null)
                }}
                onMerge={(target) => propose({ kind: 'merge', sources: [usage.tag], target })}
                onDelete={() => propose({ kind: 'delete', tag: usage.tag })}
                onOpenNote={openNote}
              />
            ))}
          </ul>
        )}
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
```

- [ ] **Step 7: Run tests to verify pass**

Run: `pnpm vitest run src/components/TagManagerDialog.test.tsx`
Expected: PASS (8 tests). If the Radix `DropdownMenu` does not open in jsdom with `pointerDown`, copy the exact event sequence from `src/components/AiAgentsOnboardingPrompt.test.tsx:46` (it passes there).

- [ ] **Step 8: Check size and health.** `TagManagerContent` is the most likely CodeScene hotspot (many state hooks, a large function). If its file review is below 10.0, move the state and handlers into a `useTagManagerDialogState` hook in the same file and re-run until it scores 10.0. Run a Codacy local scan on all three new component files (zero findings).

- [ ] **Step 9: Run typecheck and lint**

Run: `npx tsc --noEmit -p tsconfig.app.json && pnpm lint`
Expected: no errors. The `en.json` keys make the new `TranslationKey` values type-check.

- [ ] **Step 10: Commit**

```bash
git add src/lib/locales/en.json src/components/TagManagerDialog.tsx src/components/TagManagerRow.tsx src/components/TagManagerStatus.tsx src/components/TagManagerDialog.test.tsx
git commit -m "feat: add tag manager dialog"
```

---

### Task 6: Command palette entry and app wiring

**Files:**
- Modify: `src/hooks/useDialogs.ts`
- Modify: `src/hooks/commands/noteCommands.ts` (config interface ~line 23; command list near `create-type` ~line 116)
- Modify: `src/hooks/commands/localizeCommands.ts` (`STATIC_LABEL_KEYS`, next to `'create-type'`)
- Modify: `src/hooks/useCommandRegistry.ts` (config ~line 93; destructure ~lines 167, 221, 235)
- Modify: `src/hooks/useAppCommands.ts` (config ~line 89; `CommandRegistryVaultActions` pick ~line 219; builder ~line 365)
- Modify: `src/App.tsx` (commands config ~line 1624; dialog render next to `<CreateViewDialog …>` ~line 1906)
- Test: `src/hooks/useDialogs.test.ts` (create if missing), `src/hooks/commands/noteCommands.test.ts` (create if missing; otherwise add to the existing note-command test)

**Interfaces:**
- Consumes: `TagManagerDialog` (Task 5); `trackEvent`.
- Produces: `useDialogs()` returns `showTagManager`, `openTagManager`, `closeTagManager`; `NoteCommandsConfig.onManageTags?: () => void`; `CommandRegistryConfig.onManageTags?: () => void`; `AppCommandsConfig.onManageTags?: () => void`.

- [ ] **Step 1: Write the failing tests**

```ts
// src/hooks/useDialogs.test.ts (add this case, or create the file with it)
import { act, renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { useDialogs } from './useDialogs'

describe('useDialogs tag manager', () => {
  it('opens and closes the tag manager', () => {
    const { result } = renderHook(() => useDialogs())
    expect(result.current.showTagManager).toBe(false)
    act(() => result.current.openTagManager())
    expect(result.current.showTagManager).toBe(true)
    act(() => result.current.closeTagManager())
    expect(result.current.showTagManager).toBe(false)
  })
})
```

Find how existing tests call `buildNoteCommands` (`grep -rn "buildNoteCommands" src/hooks --include=*.test.ts`) and reuse their base config. Then add:

```ts
it('exposes a Manage Tags command that opens the tag manager', () => {
  const onManageTags = vi.fn()
  const commands = buildNoteCommands({ ...baseConfig, onManageTags })
  const command = commands.find((item) => item.id === 'manage-tags')
  expect(command).toMatchObject({ label: 'Manage Tags', group: 'Note', enabled: true })
  command?.execute()
  expect(onManageTags).toHaveBeenCalled()
})

it('disables Manage Tags when no handler is wired', () => {
  const commands = buildNoteCommands({ ...baseConfig, onManageTags: undefined })
  expect(commands.find((item) => item.id === 'manage-tags')?.enabled).toBe(false)
})
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run src/hooks/useDialogs.test.ts src/hooks/commands`
Expected: FAIL (`showTagManager` undefined; no `manage-tags` command).

- [ ] **Step 3: Implement `useDialogs`.** Add next to the other state:

```ts
  const [showTagManager, setShowTagManager] = useState(false)
  const openTagManager = useCallback(() => setShowTagManager(true), [])
  const closeTagManager = useCallback(() => setShowTagManager(false), [])
```

and add to the returned object:

```ts
    showTagManager, openTagManager, closeTagManager,
```

- [ ] **Step 4: Implement the command.** In `NoteCommandsConfig` add `onManageTags?: () => void` below `onCreateType`. After the `create-type` command add:

```ts
    createNoteCommand({
      id: 'manage-tags',
      label: 'Manage Tags',
      keywords: ['tags', 'tag', 'rename', 'merge', 'delete', 'labels', 'categories', 'organize'],
      enabled: !!config.onManageTags,
      execute: () => config.onManageTags?.(),
    }),
```

In `localizeCommands.ts` `STATIC_LABEL_KEYS`, add `'manage-tags': 'command.note.manageTags',` after `'create-type'`.

- [ ] **Step 5: Thread the handler through the registry.** Mirror every occurrence of `onCreateType` exactly:
  - `useCommandRegistry.ts`: add `onManageTags?: () => void` to the config interface; add `onManageTags` to the destructure at ~line 167; pass it in the note-commands config at ~line 221; add it to that `useMemo` dependency list at ~line 235.
  - `useAppCommands.ts`: add `onManageTags?: () => void` to `AppCommandsConfig`; add `| 'onManageTags'` to the `CommandRegistryVaultActions` pick; add `onManageTags: config.onManageTags,` in its builder (~line 365). Do **not** add it to the menu-event handlers (no native menu item).

- [ ] **Step 6: Wire `App.tsx`.** Add a tracked opener near the other dialog handlers:

```tsx
  const openTagManager = useCallback(() => {
    trackEvent('tag_manager_opened')
    dialogs.openTagManager()
  }, [dialogs.openTagManager])
```

(Import `trackEvent` from `./lib/telemetry` if `App.tsx` does not already.) In the commands config next to `onCreateType: dialogs.openCreateType,` add `onManageTags: openTagManager,`. Next to `<CreateViewDialog …/>` render:

```tsx
        <TagManagerDialog
          open={dialogs.showTagManager}
          onClose={dialogs.closeTagManager}
          entries={vault.entries}
          locale={appLocale}
          onUpdateFrontmatter={notes.handleUpdateFrontmatter}
          onOpenNote={notes.handleSelectNote}
        />
```

Add `import { TagManagerDialog } from './components/TagManagerDialog'`. Check that `vault.entries` is the variable App already uses for all entries (`grep -n "vault.entries" src/App.tsx`). If App derives a filtered list for workspaces, use the unfiltered one so counts cover the whole vault.

- [ ] **Step 7: Run tests, typecheck, and lint**

Run: `pnpm vitest run src/hooks src/components/TagManagerDialog.test.tsx && npx tsc --noEmit -p tsconfig.app.json && pnpm lint`
Expected: all PASS, no type or lint errors.

- [ ] **Step 8: Gates and commit.** Re-run the CodeScene file review on each modified existing file against its Task 0 baseline. Each must improve, or stay at 10.0 if it started there. `App.tsx` and `useAppCommands.ts` are likely below 10.0, so a one-line addition may not raise their score: make a small in-scope Boy Scout refactor in each (for example extract a helper near the edited code) until the score rises. Re-scan with Codacy against the baselines.

```bash
git add src/hooks/useDialogs.ts src/hooks/useDialogs.test.ts src/hooks/commands/noteCommands.ts src/hooks/commands/localizeCommands.ts src/hooks/commands/*.test.ts src/hooks/useCommandRegistry.ts src/hooks/useAppCommands.ts src/App.tsx
git commit -m "feat: open the tag manager from the command palette"
```

---

### Task 7: Translations

**Files:**
- Modify: every catalog in `src/lib/locales/` except `en.json`: `be-BY`, `be-Latn`, `de-DE`, `es-419`, `es-ES`, `fr-FR`, `id-ID`, `it-IT`, `ja-JP`, `ko-KR`, `pl-PL`, `pt-BR`, `pt-PT`, `ru-RU`, `sk-SK`, `sv-SE`, `uk-UA`, `vi`, `zh-CN`, `zh-TW`.

- [ ] **Step 1: Translate the 26 keys added in Task 5** into each catalog and append them at the end of the file. Follow the fork rule in AGENTS.md: reuse each catalog's existing wording for "tag", "note", "rename", "merge", "delete", "cancel" (grep each catalog for its existing tag/note strings first); keep placeholders `{count}`, `{tag}`, `{from}`, `{to}`, `{done}`, `{total}`, `{changed}`, `{failed}` unchanged; keep the curly quotes or use the locale's native quotation marks; do not reorder existing keys; do not touch `lara.lock`.
- [ ] **Step 2: Validate**

Run: `pnpm l10n:validate`
Expected: passes with no missing keys or placeholder mismatches.

- [ ] **Step 3: Commit**

```bash
git add src/lib/locales/
git commit -m "feat: translate tag manager copy"
```

---

### Task 8: Playwright end-to-end test

**Files:**
- Create: `tests/smoke/tag-manager.spec.ts`

**Interfaces:**
- Consumes: `createFixtureVaultCopy`, `openFixtureVault`, `removeFixtureVaultCopy` from `tests/helpers/fixtureVault.ts`; `openCommandPalette` from `tests/smoke/helpers`.

- [ ] **Step 1: Write the test.** It writes two tagged notes into a temp copy of the fixture vault, renames `blues` → `soul` through the UI, and checks the files on disk.

```ts
// tests/smoke/tag-manager.spec.ts
import fs from 'node:fs'
import path from 'node:path'
import { test, expect } from '@playwright/test'
import { createFixtureVaultCopy, openFixtureVault, removeFixtureVaultCopy } from '../helpers/fixtureVault'
import { openCommandPalette } from './helpers'

let tempVaultDir: string

function writeTaggedNote(fileName: string, title: string, tags: string[]) {
  const tagLines = tags.map((tag) => `  - ${tag}`).join('\n')
  fs.writeFileSync(path.join(tempVaultDir, fileName), `---\ntitle: ${title}\ntags:\n${tagLines}\n---\n\n# ${title}\n`)
}

test.describe('Tag manager', () => {
  test.beforeEach(() => {
    tempVaultDir = createFixtureVaultCopy()
    writeTaggedNote('tagged-one.md', 'Tagged One', ['blues', 'live'])
    writeTaggedNote('tagged-two.md', 'Tagged Two', ['blues'])
  })

  test.afterEach(() => {
    removeFixtureVaultCopy(tempVaultDir)
  })

  test('renames a tag across every note from the command palette', async ({ page }) => {
    await openFixtureVault(page, tempVaultDir)
    await openCommandPalette(page)
    await page.locator('input[placeholder="Type a command..."]').fill('manage tags')
    await page.keyboard.press('Enter')

    const row = page.getByTestId('tag-manager-row-blues')
    await expect(row).toContainText('2 notes')
    await page.getByTestId('tag-manager-menu-blues').click()
    await page.getByRole('menuitem', { name: 'Rename…' }).click()
    await page.getByPlaceholder('New tag name').fill('soul')
    await page.getByRole('button', { name: 'Continue' }).click()
    await expect(page.getByText('Rename “blues” to “soul” in 2 notes?')).toBeVisible()
    await page.getByRole('button', { name: 'Apply' }).click()
    await expect(page.getByText('Updated 2 notes.')).toBeVisible()
    await expect(page.getByTestId('tag-manager-row-soul')).toContainText('2 notes')

    await expect.poll(() => fs.readFileSync(path.join(tempVaultDir, 'tagged-one.md'), 'utf8')).toContain('- soul')
    const one = fs.readFileSync(path.join(tempVaultDir, 'tagged-one.md'), 'utf8')
    expect(one).not.toContain('blues')
    expect(one).toContain('- live')
    expect(one).toContain('# Tagged One')
    expect(fs.readFileSync(path.join(tempVaultDir, 'tagged-two.md'), 'utf8')).toContain('- soul')
  })
})
```

Check `tests/smoke/helpers` exports `openCommandPalette` (it is imported by `command-palette-new-note-dedupe.spec.ts`). The fixture harness writes `update_frontmatter` to disk (`tests/helpers/fixtureVault.ts:451`). If it serializes lists as flow style (`[soul, live]`), change the assertions to match on `soul` / `live` and the absence of `blues`.

- [ ] **Step 2: Run it**

```bash
pnpm dev --port 5201 &
BASE_URL="http://localhost:5201" npx playwright test tests/smoke/tag-manager.spec.ts
```

Expected: 1 passed. Stop the dev server afterwards. Do not tag this test `@smoke` (it isn't a core pre-push flow).

- [ ] **Step 3: Commit**

```bash
git add tests/smoke/tag-manager.spec.ts
git commit -m "test: cover tag manager rename end to end"
```

---

### Task 9: Docs, native QA, and release gates

**Files:**
- Modify: `docs/ARCHITECTURE.md` (right side panels / dialogs area)
- Modify: `docs/ABSTRACTIONS.md` (near the property display modes section, ~line 901)

- [ ] **Step 1: Docs.** In `docs/ABSTRACTIONS.md`, add a "Tag Manager" paragraph. It should say that tags are per tags-mode property; `buildTagInventory` / `planTagRewrite` / `planTagColorChanges` are pure; `useTagManager` applies plans through `handleUpdateFrontmatter` with `{ silent: true }` (no undo-history entries, errors collected) and migrates the global `tag_colors`; and it emits `tag_manager_opened` and `tag_manager_action`. In `docs/ARCHITECTURE.md`, add one line on the `Manage Tags` command opening `TagManagerDialog`. Commit with `docs: document tag manager`.
- [ ] **Step 2: Full check suite**

```bash
pnpm lint && npx tsc --noEmit && pnpm test && pnpm test:coverage
```

Expected: all pass; frontend coverage ≥70%. The Rust suite is unaffected but still runs in pre-push.

- [ ] **Step 3: Native QA.** Run `pnpm tauri dev` and open `demo-vault-v2`, which has `spreadsheet` and `business-plan` tags. Using computer use, the mouse path: open ⌘K, run "Manage Tags", switch property, filter, Show notes → open a note, rename `business-plan` → `plan`, then rename it back. Keyboard path: ⌘K → type → Enter, Escape closes rename, Enter submits rename. Screenshot with `bash ~/.openclaw/skills/tolaria-qa/scripts/screenshot.sh /tmp/qa-tag-manager.png`. Afterwards run `git status --short -- demo-vault demo-vault-v2`; it must be empty, so revert any residue.
- [ ] **Step 4: CodeScene.** Run the file-level review on every touched and new file (new files at 10.0, existing files improved or still 10.0), then `mcp__codescene__pre_commit_code_health_safeguard` and `mcp__codescene__analyze_change_set` with `base_ref=origin/main`. All must pass.
- [ ] **Step 5: Codacy.** Rebuild the scan manifest (`git diff --name-only --diff-filter=ACMR "$(git merge-base HEAD origin/main)" --` plus untracked files). Scan each path individually and compare against the Task 0 baselines: zero findings in new files, fewer or still zero in touched files. Run `pnpm codacy:gate`.
- [ ] **Step 6: Push.** Confirm the target with the repository owner (this work is on `web-client` in the fork), then push. After the push, check that the Codacy dashboard analyzed the exact pushed SHA and shows zero findings on the manifest paths, and record the final CodeScene Hotspot and Average scores.
- [ ] **Step 7: Release record.** In the final handoff, write the AGENTS.md release-readiness record: implementation summary, QA, tests and coverage, CodeScene before/after, Codacy manifest and counts, localization (`pnpm l10n:validate` passed; fork-rule manual translation), PostHog events `tag_manager_opened` and `tag_manager_action`, refactoring done for gates, ADRs: none, docs updated, and demo-vault dirt checked.
