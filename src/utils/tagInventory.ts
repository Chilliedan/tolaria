import type { VaultEntry, VaultPropertyValue } from '../types'
import type { PropertyDisplayMode } from './propertyTypes'
import { getEffectiveDisplayMode } from './propertyTypes'
import type { FrontmatterValue } from '../components/Inspector'

export interface TagUsage {
  tag: string
  count: number
  paths: string[]
}

export type TagInventory = Map<string, TagUsage[]>
export type DisplayModeOverrides = Record<string, PropertyDisplayMode>
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
  const propertyUsages = inventory.get(property)
  if (!propertyUsages?.some((usage) => usage.tag === tag)) return false
  return [...inventory.entries()].some(
    ([otherProperty, usages]) => otherProperty !== property && usages.some((usage) => usage.tag === tag),
  )
}
