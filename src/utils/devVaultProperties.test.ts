import { describe, expect, it } from 'vitest'
import { frontmatterPropertyValue } from './devVaultProperties'

// These rules mirror the Rust vault parser's `scalar_array_property_value` /
// `extract_properties` (src-tauri/crates/tolaria-core/src/vault/frontmatter.rs:287),
// which is the source of truth the dev/web backend must match:
//   - a single-item array collapses to its scalar
//   - a longer array of scalars (string/number/boolean) is kept as an array
//   - any array item that isn't representable as a scalar (a wikilink string, an
//     object, null, or a nested array) drops the whole property
describe('frontmatterPropertyValue', () => {
  it('keeps a multi-item array of strings as an array', () => {
    expect(frontmatterPropertyValue(['a', 'b'])).toEqual(['a', 'b'])
  })

  it('collapses a single-item array to its scalar', () => {
    expect(frontmatterPropertyValue(['a'])).toBe('a')
  })

  it('keeps a mixed array of strings, numbers, and booleans', () => {
    expect(frontmatterPropertyValue(['a', 1, true])).toEqual(['a', 1, true])
  })

  it('drops the property when an array item contains a wikilink', () => {
    expect(frontmatterPropertyValue(['a', '[[Note B]]'])).toBeUndefined()
  })

  it('sanitizes a non-empty object item to a joined "key: value" string (matches Rust sanitize_array_item)', () => {
    // gray-matter mis-parses unquoted-colon list items (`- Bitcoin: Net Unrealized`)
    // into objects. Rust's sanitize_array_item converts these back to a string
    // instead of dropping the property; the JS backend must do the same.
    expect(frontmatterPropertyValue(['a', { key: 'value' }])).toEqual(['a', 'key: value'])
  })

  it('drops the property when an array item is an empty object', () => {
    expect(frontmatterPropertyValue(['a', {}])).toBeUndefined()
  })

  it('drops the property when an array item is null', () => {
    expect(frontmatterPropertyValue(['a', null])).toBeUndefined()
  })

  it('drops the property when an array item is a nested array', () => {
    expect(frontmatterPropertyValue(['a', ['nested']])).toBeUndefined()
  })

  it('keeps scalar values unchanged', () => {
    expect(frontmatterPropertyValue('plain text')).toBe('plain text')
    expect(frontmatterPropertyValue(5)).toBe(5)
    expect(frontmatterPropertyValue(true)).toBe(true)
    expect(frontmatterPropertyValue(null)).toBeNull()
  })

  it('drops a scalar string containing a wikilink', () => {
    expect(frontmatterPropertyValue('[[Note B]]')).toBeUndefined()
  })

  it('drops a non-array, non-scalar value such as an object', () => {
    expect(frontmatterPropertyValue({ key: 'value' })).toBeUndefined()
  })
})
