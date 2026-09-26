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
    expect(isTagUsedOutside(inventory, 'blues', 'genre')).toBe(true)
    expect(isTagUsedOutside(inventory, 'blues', 'categories')).toBe(false)
    expect(isTagUsedOutside(inventory, 'chicago', 'tags')).toBe(false)
  })
})
