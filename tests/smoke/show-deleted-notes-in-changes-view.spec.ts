import { test, expect } from '@playwright/test'
import { openCommandPalette, executeCommand } from './helpers'

/** The diff view's back button, localized by a2f8ba72 (`editor.toolbar.rawReturn`). */
const BACK_TO_EDITOR = 'Return to the editor'

async function navigateToChanges(page: import('@playwright/test').Page) {
  await openCommandPalette(page)
  await executeCommand(page, 'Go to Changes')
  await page.waitForTimeout(500)
}

test.describe('Show deleted notes in Changes view', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')
  })

  test('changes rows show title, filename, and diff summary while keyboard navigation still opens notes', async ({ page }) => {
    await navigateToChanges(page)

    const deletedRow = page.locator('[data-change-status="deleted"]').filter({ hasText: 'Old Draft' }).first()
    await expect(deletedRow).toContainText('Old Draft')
    await expect(deletedRow).toContainText('old-draft.md')
    await expect(deletedRow).toContainText('Diff unavailable')

    // Since 347df47c browser mode opens the mock vault, whose notes match the
    // other mock changes, so Old Draft is no longer the only (first) row.
    // Deleted rows are listed after live ones: arrow down to it, then open it.
    const noteList = page.getByTestId('note-list-container')
    await noteList.focus()
    const rowCount = await noteList.locator('[data-change-status]').count()
    for (let row = 1; row < rowCount; row += 1) await page.keyboard.press('ArrowDown')
    await expect(deletedRow).toHaveAttribute('data-highlighted', 'true')
    await page.keyboard.press('Enter')

    await expect(page.getByText(BACK_TO_EDITOR)).toBeVisible({ timeout: 5_000 })
    await expect(page.getByText('This note was deleted.')).toBeVisible({ timeout: 5_000 })
  })

  test('changes view shows deleted notes as rows instead of a banner', async ({ page }) => {
    await navigateToChanges(page)
    const deletedRow = page.locator('[data-change-status="deleted"]').filter({ hasText: 'old-draft.md' })
    await expect(deletedRow).toBeVisible({ timeout: 5000 })
    await expect(page.locator('[data-testid="deleted-notes-banner"]')).toHaveCount(0)
  })

  test('clicking a deleted row opens its deleted diff preview', async ({ page }) => {
    await navigateToChanges(page)
    await page.getByText('old-draft.md').click()
    await expect(page.getByText(BACK_TO_EDITOR)).toBeVisible({ timeout: 5000 })
    await expect(page.getByText('This note was deleted.')).toBeVisible({ timeout: 5000 })
  })

  test('deleted rows expose a restore action from the context menu', async ({ page }) => {
    await navigateToChanges(page)
    await page.getByText('old-draft.md').click({ button: 'right' })
    await expect(page.locator('[data-testid="changes-context-menu"]')).toBeVisible({ timeout: 5000 })
    await expect(page.locator('[data-testid="restore-note-button"]')).toBeVisible({ timeout: 5000 })
  })
})
