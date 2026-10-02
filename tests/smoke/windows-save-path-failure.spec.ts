import { test, expect, type APIRequestContext, type Page } from '@playwright/test'
import { createFixtureVaultCopy, openFixtureVault, removeFixtureVaultCopy } from '../helpers/fixtureVault'
import { executeCommand, openCommandPalette, sendShortcut } from './helpers'

// Browser mode opens the mock vault (347df47c), whose path does not contain
// the mock notes, and since d211d89a edits outside the active vault are never
// queued for saving ("Nothing to save"). Use a real temp copy of the fixture
// vault instead, so the save path under test is the one users hit.
const NOTE_TITLE = 'Note B'
const NOTE_RELATIVE_PATH = 'note/note-b.md'
const DRAFT = '# Retryable Windows Save\n\nDraft that must survive failure'

const RAW_EDITOR = '.cm-content'

let tempVaultDir: string

async function openNote(page: Page, title: string) {
  await page.getByTestId('note-list-container').getByText(title, { exact: true }).click()
  await expect(page.locator('.bn-editor')).toBeVisible({ timeout: 5_000 })
}

async function readVaultFile(request: APIRequestContext, relativePath: string): Promise<string> {
  const response = await request.get('/api/vault/content', { params: { path: `${tempVaultDir}/${relativePath}` } })
  expect(response.ok()).toBe(true)
  const { content } = await response.json() as { content: string }
  return content
}

async function setRawEditorContent(page: Page, content: string) {
  await page.evaluate((nextContent) => {
    const el = document.querySelector('.cm-content')
    if (!el) throw new Error('CodeMirror content element is missing')
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const view = (el as any).cmTile?.view
    if (!view) throw new Error('CodeMirror view is missing')
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: nextContent },
    })
    view.focus()
  }, content)
}

async function openRawMode(page: Page) {
  await openCommandPalette(page)
  await executeCommand(page, 'Toggle Raw')
  await expect(page.locator(RAW_EDITOR)).toBeVisible({ timeout: 5_000 })
}

const WINDOWS_PATH_ERROR = 'The filename, directory name, or volume label syntax is incorrect. (os error 123)'

interface SaveRoute {
  failing: boolean
  failedRequests: number
  savedContents: string[]
}

/**
 * The fixture vault saves through the dev server's `/api/vault/save`, so the
 * Windows path error is injected there: saves fail until the test lets them
 * through, and the content of every save that reaches the disk is recorded.
 */
async function routeSaves(page: Page): Promise<SaveRoute> {
  const saves: SaveRoute = { failing: true, failedRequests: 0, savedContents: [] }
  await page.route('**/api/vault/save', async (route) => {
    if (saves.failing) {
      saves.failedRequests += 1
      await route.fulfill({ status: 500, contentType: 'application/json', body: JSON.stringify({ error: WINDOWS_PATH_ERROR }) })
      return
    }
    const { content } = route.request().postDataJSON() as { content: string }
    saves.savedContents.push(content)
    await route.continue()
  })
  return saves
}

test.beforeEach(() => {
  tempVaultDir = createFixtureVaultCopy()
})

test.afterEach(() => {
  removeFixtureVaultCopy(tempVaultDir)
})

test('failed Windows path saves show a recoverable toast and retry the draft', async ({ page, request }) => {
  const pageErrors: string[] = []
  page.on('pageerror', (err) => { pageErrors.push(err.message) })

  await openFixtureVault(page, tempVaultDir)
  await openNote(page, NOTE_TITLE)
  const originalContent = await readVaultFile(request, NOTE_RELATIVE_PATH)
  const saves = await routeSaves(page)

  await openRawMode(page)
  await setRawEditorContent(page, DRAFT)
  await page.waitForTimeout(550)

  await sendShortcut(page, 's', ['Control'])
  await expect(page.locator('.fixed.bottom-8')).toContainText('note path is invalid on this platform', { timeout: 5_000 })
  expect(pageErrors.filter((message) => message.includes('os error 123'))).toHaveLength(0)
  expect(saves.failedRequests).toBeGreaterThan(0)
  expect(await readVaultFile(request, NOTE_RELATIVE_PATH)).toBe(originalContent)

  saves.failing = false
  await sendShortcut(page, 's', ['Control'])
  await expect(page.locator('.fixed.bottom-8')).toContainText('Saved', { timeout: 5_000 })

  // The retry writes the draft that survived the failed save
  expect(saves.savedContents).toEqual([DRAFT])
  expect(await readVaultFile(request, NOTE_RELATIVE_PATH)).toBe(DRAFT)
})
