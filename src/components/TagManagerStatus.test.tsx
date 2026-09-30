import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TagManagerProgressLine } from './TagManagerStatus'

describe('TagManagerProgressLine', () => {
  it('uses the plural copy when several notes are being updated', () => {
    render(<TagManagerProgressLine progress={{ done: 1, total: 2 }} locale="en" />)
    expect(screen.getByRole('status')).toHaveTextContent('Updating 1 of 2 notes…')
  })

  it('uses the singular copy when a single note is being updated', () => {
    render(<TagManagerProgressLine progress={{ done: 0, total: 1 }} locale="en" />)
    expect(screen.getByRole('status')).toHaveTextContent('Updating 0 of 1 note…')
  })
})
