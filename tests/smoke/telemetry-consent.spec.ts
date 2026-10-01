import { test, expect } from '@playwright/test'
import { waitForAppReady } from './helpers'

test.describe('Telemetry consent dialog', () => {
  test('dialog does not appear when consent was already given', async ({ page }) => {
    // Default mock settings have telemetry_consent: false
    // The consent dialog should NOT appear (only appears when null)
    await page.goto('/')
    await page.waitForLoadState('networkidle')
    await expect(page.getByText('Help improve Tolaria')).not.toBeVisible({ timeout: 5000 })
  })

  test('privacy toggles are visible in settings panel', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)

    // Open settings via keyboard shortcut
    await page.keyboard.press('Meta+,')
    await expect(page.getByTestId('settings-panel')).toBeVisible({ timeout: 5000 })

    // Privacy section should be present
    // The section was renamed from "Privacy & Telemetry" to "Telemetry" in aaf03367
    await expect(page.getByTestId('settings-panel').getByText('Telemetry', { exact: true }).first()).toBeVisible()
    await expect(page.getByTestId('settings-crash-reporting')).toBeVisible()
    await expect(page.getByTestId('settings-analytics')).toBeVisible()
  })
})
