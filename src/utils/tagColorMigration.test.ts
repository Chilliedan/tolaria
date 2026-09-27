import { describe, expect, it } from 'vitest'
import { planTagColorChanges } from './tagColorMigration'

function context(colors: Record<string, string>, stillUsed: string[] = []) {
  return {
    getColorKey: (tag: string) => (Reflect.get(colors, tag) as string | undefined) ?? null,
    isTagStillUsed: (tag: string) => stillUsed.includes(tag),
  }
}

describe('planTagColorChanges', () => {
  it('moves a renamed tag colour to the new name', () => {
    expect(planTagColorChanges({ kind: 'rename', from: 'blues', to: 'soul' }, context({ blues: 'red' }))).toEqual([
      { tag: 'soul', colorKey: 'red' },
      { tag: 'blues', colorKey: null },
    ])
  })

  it('keeps the target colour on merge and drops the source colours', () => {
    const op = { kind: 'merge' as const, sources: ['blues', 'live'], target: 'jazz' }
    expect(planTagColorChanges(op, context({ blues: 'red', live: 'green', jazz: 'blue' }))).toEqual([
      { tag: 'blues', colorKey: null },
      { tag: 'live', colorKey: null },
    ])
  })

  it('gives an uncoloured merge target the first source colour', () => {
    const op = { kind: 'merge' as const, sources: ['folk', 'blues'], target: 'jazz' }
    expect(planTagColorChanges(op, context({ blues: 'red' }))).toEqual([
      { tag: 'jazz', colorKey: 'red' },
      { tag: 'blues', colorKey: null },
    ])
  })

  it('keeps a source colour while the tag is still used elsewhere', () => {
    expect(planTagColorChanges({ kind: 'delete', tag: 'blues' }, context({ blues: 'red' }, ['blues']))).toEqual([])
  })

  it('drops a deleted tag colour and ignores uncoloured tags', () => {
    expect(planTagColorChanges({ kind: 'delete', tag: 'blues' }, context({ blues: 'red' }))).toEqual([
      { tag: 'blues', colorKey: null },
    ])
    expect(planTagColorChanges({ kind: 'delete', tag: 'folk' }, context({}))).toEqual([])
  })
})
