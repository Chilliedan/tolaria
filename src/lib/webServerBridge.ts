/**
 * Marks whether the app is running against the real web server bridge (as
 * opposed to the desktop Tauri shell or a pure-mock dev/test environment).
 *
 * `src/web/mockBridge.ts` — the module the web build aliases `mock-tauri` to —
 * calls `markWebServerBridge()` once at load. Desktop and tests never load that
 * module, so the flag stays `false`.
 *
 * Code that must choose a real server round-trip over an in-browser JS mock
 * (e.g. frontmatter edits, cross-vault moves) reads this instead of a
 * `mock-tauri` export, so tests that mock `mock-tauri` don't each have to
 * declare it.
 */
let webServerBridge = false

/** Called once by the web bridge at module load. */
export function markWebServerBridge(): void {
  webServerBridge = true
}

/** True only in the real web deployment. */
export function isWebServerBridge(): boolean {
  return webServerBridge
}
