function additionLinesFor(additions, file) {
  if (additions.has(file)) return additions.get(file)
  const lines = new Set()
  lines.auditedRustUnsafeLines = new Set()
  additions.set(file, lines)
  return lines
}

function recordAddedLine(lines, lineNumber, source, previousAddedLine) {
  lines.add(lineNumber)
  const audited = previousAddedLine?.trimStart().startsWith('// SAFETY:')
    && source.includes('unsafe')
  if (audited) lines.auditedRustUnsafeLines.add(lineNumber)
}

export function addedLinesFromDiff(diff, repositoryRoot) {
  const additions = new Map()
  let file = null
  let nextLine = 0
  let previousAddedLine = null

  for (const line of diff.split('\n')) {
    if (line.startsWith('+++ b/')) {
      file = `${repositoryRoot}/${line.slice(6)}`
      additionLinesFor(additions, file)
      previousAddedLine = null
      continue
    }
    const hunk = line.match(/^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@/)
    if (hunk) {
      nextLine = Number(hunk[1])
      continue
    }
    if (!file || line.startsWith('---')) continue
    if (line.startsWith('+')) {
      const addedLine = line.slice(1)
      const lines = additions.get(file)
      recordAddedLine(lines, nextLine, addedLine, previousAddedLine)
      previousAddedLine = addedLine
      nextLine += 1
    } else if (!line.startsWith('-')) {
      previousAddedLine = null
      nextLine += 1
    }
  }
  return additions
}

export function biomeGateFailures(report, additions, repositoryRoot) {
  return (report.diagnostics ?? []).flatMap((diagnostic) => {
    const path = resolveDiagnosticPath(diagnostic.location?.path, repositoryRoot)
    const line = diagnostic.location?.start?.line
    if (!path || !line || !additions.get(path)?.has(line)) return []
    return [{
      line,
      message: diagnostic.message ?? 'Biome issue',
      path,
      rule: diagnostic.category ?? 'unknown rule',
      tool: 'Biome',
    }]
  })
}

export function eslintGateFailures(report, additions) {
  return report.flatMap(file => (file.messages ?? []).flatMap((message) => {
    if (!message.line || !additions.get(file.filePath)?.has(message.line)) return []
    return [{
      line: message.line,
      message: message.message ?? 'ESLint issue',
      path: file.filePath,
      rule: message.ruleId ?? 'unknown rule',
      tool: 'Codacy ESLint security rules',
    }]
  }))
}

function resolveDiagnosticPath(path, repositoryRoot) {
  if (!path) return ''
  return path.startsWith('/') ? path : `${repositoryRoot}/${path}`
}

/**
 * Pick the commit the gate measures additions against.
 *
 * `main` is gated against `origin/main`, as the upstream release branch. Any
 * other branch is gated against its own last pushed state: a long-lived branch
 * carries files that do not exist upstream, and diffing those against
 * `origin/main` reports the whole file as new on every push, so pre-existing
 * findings can never be cleared by the push that would fix them. Falls back to
 * `origin/main` when the branch has never been pushed.
 */
export function resolveBaseRef({ branch, envBase, upstreamRef }) {
  if (envBase) return envBase
  if (branch === 'main') return 'origin/main'
  return upstreamRef || 'origin/main'
}

/** Combine per-file addition maps (one per changed file) into a single map. */
export function mergeAdditions(perFileAdditions) {
  return new Map(perFileAdditions.flatMap((additions) => [...additions]))
}
