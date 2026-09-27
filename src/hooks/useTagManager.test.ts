import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { makeEntry } from '../test-utils/noteListTestUtils'
import { useTagManager, type TagManagerApplyResult } from './useTagManager'

const { trackEventMock, setTagColorMock, colors } = vi.hoisted(() => ({
  trackEventMock: vi.fn(),
  setTagColorMock: vi.fn(),
  colors: {} as Record<string, string>,
}))

vi.mock('../lib/telemetry', () => ({ trackEvent: trackEventMock }))
vi.mock('../utils/tagStyles', () => ({
  getTagColorKey: (tag: string) => (Reflect.get(colors, tag) as string | undefined) ?? null,
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

    let outcome: TagManagerApplyResult | undefined
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

    let outcome: TagManagerApplyResult | undefined
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

    let outcome: TagManagerApplyResult | undefined
    await act(async () => {
      outcome = await result.current.apply('tags', { kind: 'delete', tag: 'missing' })
    })

    expect(outcome).toEqual({ total: 0, changed: 0, failedPaths: [] })
    expect(updateFrontmatter).not.toHaveBeenCalled()
    expect(trackEventMock).not.toHaveBeenCalled()
  })
})
