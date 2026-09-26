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
