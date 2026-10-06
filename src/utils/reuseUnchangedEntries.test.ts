import { describe, expect, it } from 'vitest'
import type { VaultEntry, WorkspaceIdentity } from '../types'
import { reuseUnchangedEntries } from './reuseUnchangedEntries'

function workspace(path: string): WorkspaceIdentity {
  return {
    id: path,
    label: path,
    alias: path,
    path,
    shortLabel: path,
    color: null,
    icon: null,
    mounted: true,
    available: true,
    defaultForNewNotes: false,
  }
}

function entry(path: string, overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path,
    filename: path.split('/').pop() ?? path,
    title: path,
    workspace: workspace('/vault'),
    isA: 'Note',
    aliases: ['alias'],
    belongsTo: ['[[parent]]'],
    relatedTo: [],
    status: null,
    archived: false,
    modifiedAt: 1700000000,
    createdAt: 1700000000,
    fileSize: 100,
    snippet: 'snippet',
    wordCount: 3,
    relationships: { Topics: ['[[a]]', '[[b]]'] },
    icon: null,
    color: null,
    order: null,
    sidebarLabel: null,
    template: null,
    sort: null,
    view: null,
    visible: null,
    organized: false,
    favorite: false,
    favoriteIndex: null,
    listPropertiesDisplay: [],
    outgoingLinks: ['a', 'b'],
    properties: { Owner: 'Luca', Tags: ['x', 'y'], Rating: 3 },
    hasH1: true,
    ...overrides,
  }
}

/** Deep copy, as a fresh `list_vault` result would be. */
function fresh(entries: VaultEntry[]): VaultEntry[] {
  return structuredClone(entries)
}

const previous = [entry('/vault/a.md'), entry('/vault/b.md'), entry('/vault/c.md')]

describe('reuseUnchangedEntries', () => {
  it('keeps the previous array when a refresh returns identical data', () => {
    expect(reuseUnchangedEntries(previous, fresh(previous))).toBe(previous)
  })

  it('replaces only the entry whose data changed', () => {
    const next = fresh(previous)
    next[1] = { ...next[1], title: 'Renamed' }

    const result = reuseUnchangedEntries(previous, next)

    expect(result).not.toBe(previous)
    expect(result[0]).toBe(previous[0])
    expect(result[1]).toBe(next[1])
    expect(result[1].title).toBe('Renamed')
    expect(result[2]).toBe(previous[2])
  })

  it('treats a change inside a nested property array as a change', () => {
    const next = fresh(previous)
    next[0].properties = { ...next[0].properties, Tags: ['x', 'z'] }

    const result = reuseUnchangedEntries(previous, next)

    expect(result[0]).toBe(next[0])
    expect(result[1]).toBe(previous[1])
  })

  it('treats nested relationship, link and workspace changes as changes', () => {
    const next = fresh(previous)
    next[0].relationships.Topics.push('[[c]]')
    next[1].outgoingLinks = ['a']
    next[2].workspace = { ...workspace('/vault'), color: 'red' }

    const result = reuseUnchangedEntries(previous, next)

    expect(result[0]).toBe(next[0])
    expect(result[1]).toBe(next[1])
    expect(result[2]).toBe(next[2])
  })

  it('treats an added or removed nested key as a change', () => {
    const added = fresh(previous)
    added[0].properties = { ...added[0].properties, Extra: null }
    const removed = fresh(previous)
    removed[1].relationships = {}

    expect(reuseUnchangedEntries(previous, added)[0]).toBe(added[0])
    expect(reuseUnchangedEntries(previous, removed)[1]).toBe(removed[1])
  })

  it('treats a newly present optional field as a change', () => {
    const next = fresh(previous)
    next[0].fileKind = 'markdown'

    expect(reuseUnchangedEntries(previous, next)[0]).toBe(next[0])
  })

  it('adds new entries and drops removed ones', () => {
    const added = entry('/vault/d.md')
    const next = [...fresh(previous).slice(1), added]

    const result = reuseUnchangedEntries(previous, next)

    expect(result).toEqual([previous[1], previous[2], added])
    expect(result[0]).toBe(previous[1])
    expect(result[1]).toBe(previous[2])
    expect(result[2]).toBe(added)
  })

  it('keeps the refreshed order while reusing reordered entries', () => {
    const next = fresh(previous).reverse()

    const result = reuseUnchangedEntries(previous, next)

    expect(result).not.toBe(previous)
    expect(result[0]).toBe(previous[2])
    expect(result[1]).toBe(previous[1])
    expect(result[2]).toBe(previous[0])
  })

  it('matches entries with the same path by workspace', () => {
    const inVault = entry('/shared/a.md', { workspace: workspace('/vault') })
    const inOther = entry('/shared/a.md', { workspace: workspace('/other'), title: 'Other' })
    const before = [inVault, inOther]
    const next = fresh(before).reverse()

    const result = reuseUnchangedEntries(before, next)

    expect(result[0]).toBe(inOther)
    expect(result[1]).toBe(inVault)
  })

  it('keeps the previous array for unchanged entries that share a path and workspace', () => {
    const before = [entry('/vault/a.md'), entry('/vault/a.md', { title: 'Duplicate' })]

    expect(reuseUnchangedEntries(before, fresh(before))).toBe(before)
  })

  it('returns the next array when nothing can be reused', () => {
    const next = [entry('/vault/x.md')]

    expect(reuseUnchangedEntries(previous, next)).toBe(next)
    expect(reuseUnchangedEntries([], next)).toBe(next)
  })

  it('returns an empty next list when everything was removed', () => {
    const next: VaultEntry[] = []

    expect(reuseUnchangedEntries(previous, next)).toBe(next)
  })

  it('does not treat different value types as equal', () => {
    const next = fresh(previous)
    next[0].properties = { ...next[0].properties, Rating: '3' }
    next[1].aliases = { 0: 'alias', length: 1 } as unknown as string[]

    const result = reuseUnchangedEntries(previous, next)

    expect(result[0]).toBe(next[0])
    expect(result[1]).toBe(next[1])
  })
})
