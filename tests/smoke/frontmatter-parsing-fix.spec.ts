import { test, expect } from '@playwright/test'
import { sendShortcut, waitForAppReady } from './helpers'

const QUICK_OPEN_INPUT = 'input[placeholder="Search notes..."]'

async function openQuickOpen(page: import('@playwright/test').Page) {
  await page.locator('body').click()
  await sendShortcut(page, 'p', ['Control'])
  await expect(page.locator(QUICK_OPEN_INPUT)).toBeVisible()
}

test.describe('Frontmatter parsing: type badge displays correctly', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
  })

  test('procedure note shows type badge in Quick Open', async ({ page }) => {
    await openQuickOpen(page)
    // Query the Procedure fixture by its full title: a bare "Weekly" prefix
    // ranks hundreds of shorter generated "Weekly Plan N" notes (none of them
    // Procedures) above it, so the Procedure note never reaches the results.
    await page.locator(QUICK_OPEN_INPUT).fill('Write Weekly Essays')
    await page.waitForTimeout(400)
    // The Badge component renders the type name as text content
    const result = page.getByTestId('quick-open-palette').getByRole('button').filter({ hasText: 'Write Weekly Essays' })
    const badge = result.getByText('Procedure', { exact: true })
    await expect(badge.first()).toBeVisible({ timeout: 3000 })
  })

  test('responsibility note shows type badge in Quick Open', async ({ page }) => {
    await openQuickOpen(page)
    await page.locator(QUICK_OPEN_INPUT).fill('Newsletter')
    await page.waitForTimeout(400)
    const badge = page.locator('.fixed.inset-0').locator('text=Responsibility')
    await expect(badge.first()).toBeVisible({ timeout: 3000 })
  })

  test('project note shows type badge in Quick Open', async ({ page }) => {
    await openQuickOpen(page)
    await page.locator(QUICK_OPEN_INPUT).fill('Laputa')
    await page.waitForTimeout(400)
    const badge = page.locator('.fixed.inset-0').locator('text=Project')
    await expect(badge.first()).toBeVisible({ timeout: 3000 })
  })

  test('sidebar shows type sections', async ({ page }) => {
    // Sidebar sections are rendered as nav items — look for them in the page
    await expect(page.locator('text=Projects').first()).toBeVisible({ timeout: 3000 })
    await expect(page.locator('text=Responsibilities').first()).toBeVisible({ timeout: 3000 })
  })
})
