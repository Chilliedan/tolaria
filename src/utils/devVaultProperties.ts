/**
 * Custom-property value handling for the dev/web vault backend (`vite.config.ts`).
 *
 * This mirrors the Rust vault parser's `scalar_array_property_value` and the
 * per-value match in `extract_properties`
 * (src-tauri/crates/tolaria-core/src/vault/frontmatter.rs:287-333), which is the
 * source of truth: a single-item array collapses to its scalar, a longer array
 * of scalars (string/number/boolean) is kept as an array, and any array item
 * that isn't representable as a scalar (a wikilink string, an object, null, or
 * a nested array) drops the whole property.
 */

export type FrontmatterScalar = string | number | boolean | null
export type FrontmatterPropertyValue = FrontmatterScalar | Array<string | number | boolean>

/** Mirrors Rust's `contains_wikilink`: a bare substring check, not a full parse. */
function containsWikilink(value: string): boolean {
  return value.includes('[[') && value.includes(']]')
}

function isScalarFrontmatterProperty(value: unknown): value is number | boolean {
  return typeof value === 'number' || typeof value === 'boolean'
}

/**
 * Sanitize one array item down to a scalar, mirroring Rust's `sanitize_array_item`.
 * Returns `undefined` when the item can't be represented as a scalar at all
 * (null, an empty object, or a nested array) — the caller drops the whole
 * property in that case, matching the Rust function's early-return via `?`.
 */
function sanitizeArrayItem(item: unknown): string | number | boolean | undefined {
  if (item === null || Array.isArray(item)) return undefined
  if (typeof item === 'object') {
    const entries = Object.entries(item as Record<string, unknown>)
    if (entries.length === 0) return undefined
    return entries.map(([key, value]) => `${key}: ${typeof value === 'string' ? value : String(value)}`).join(', ')
  }
  if (typeof item === 'string' || typeof item === 'number' || typeof item === 'boolean') return item
  return undefined
}

/** Sanitizes one array item, then drops it (returns `undefined`) if it's a wikilink string. */
function sanitizedArrayScalar(item: unknown): string | number | boolean | undefined {
  const sanitized = sanitizeArrayItem(item)
  if (typeof sanitized !== 'string') return sanitized
  return containsWikilink(sanitized) ? undefined : sanitized
}

/** Mirrors Rust's `scalar_array_property_value`. */
function scalarArrayPropertyValue(arr: unknown[]): FrontmatterPropertyValue | undefined {
  const values: Array<string | number | boolean> = []
  for (const item of arr) {
    const sanitized = sanitizedArrayScalar(item)
    if (sanitized === undefined) return undefined
    values.push(sanitized)
  }
  return values.length === 1 ? values[0] : values
}

/** Mirrors Rust's per-value match inside `extract_properties`. */
export function frontmatterPropertyValue(value: unknown): FrontmatterPropertyValue | undefined {
  if (value === null) return null
  if (isScalarFrontmatterProperty(value)) return value
  if (typeof value === 'string') return containsWikilink(value) ? undefined : value
  if (Array.isArray(value)) return scalarArrayPropertyValue(value)
  return undefined
}
