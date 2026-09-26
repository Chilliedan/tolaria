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
