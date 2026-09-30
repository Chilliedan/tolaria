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

interface PostWriteEffects {
  op: TagRewriteOp
  inventory: TagInventory
  property: string
  changed: number
  failed: number
}

// Colour migration and analytics run after the notes were already written, so a failure
// here must not turn a completed rewrite into a reported write failure.
function runPostWriteEffects({ op, inventory, property, changed, failed }: PostWriteEffects): void {
  try {
    if (changed > 0) migrateColors(op, inventory, property, failed > 0)
    trackEvent('tag_manager_action', { action: op.kind, notes_changed: changed, failed })
  } catch (error) {
    console.warn('Tag manager post-write step failed:', error)
  }
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
    try {
      const failedPaths = await writeSteps(steps, property, updateFrontmatter, (done) => {
        setProgress({ done, total: steps.length })
      })
      const changed = steps.length - failedPaths.length
      runPostWriteEffects({ op, inventory, property, changed, failed: failedPaths.length })
      return { total: steps.length, changed, failedPaths }
    } finally {
      setProgress(null)
    }
  }, [entries, inventory, updateFrontmatter])

  return { inventory, properties, progress, countAffected, apply }
}
