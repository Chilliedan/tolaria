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
