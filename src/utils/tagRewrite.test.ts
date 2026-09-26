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
