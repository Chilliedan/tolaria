import { test, expect, type APIRequestContext, type Page } from '@playwright/test'
import { createFixtureVaultCopy, openFixtureVault, removeFixtureVaultCopy } from '../helpers/fixtureVault'
import { openPropertiesPanel, sendShortcut } from './helpers'

// The notes this spec used ("Sponsorships", "Start Laputa App") existed only in
// the old demo vault, which browser mode no longer opens (347df47c), so the
// spec writes equivalent notes into a temp copy of the fixture vault. Files are
// written and read through the dev server's vault API, which hits the real disk.
const CUSTOM_RELATIONSHIPS_NOTE = 'sponsorships.md'
const CUSTOM_RELATIONSHIPS_CONTENT = `---
title: Sponsorships
Is A: Project
Status: Open
Has Measures:
  - "[[Note B]]"
Has Procedures:
  - "[[Note C]]"
---

# Sponsorships
`
const STANDARD_RELATIONSHIPS_NOTE = 'start-laputa-app.md'
const STANDARD_RELATIONSHIPS_CONTENT = `---
title: Start Laputa App
Is A: Project
Status: Active
Belongs to: "[[Spring 2026]]"
Owner: "[[Note B]]"
---

# Start Laputa App
`

let tempVaultDir: string

function vaultFilePath(fileName: string): string {
  return `${tempVaultDir}/${fileName}`
}

async function writeVaultFile(request: APIRequestContext, fileName: string, content: string): Promise<void> {
  const response = await request.post('/api/vault/save', { data: { path: vaultFilePath(fileName), content } })
  expect(response.ok()).toBe(true)
}

async function readVaultFile(request: APIRequestContext, fileName: string): Promise<string> {
  const response = await request.get('/api/vault/content', { params: { path: vaultFilePath(fileName) } })
  expect(response.ok()).toBe(true)
  const { content } = await response.json() as { content: string }
  return content
}

async function openNoteViaQuickOpen(page: Page, title: string) {
  await page.locator('body').click()
  await sendShortcut(page, 'p', ['Control'])
  const searchInput = page.locator('input[placeholder="Search notes..."]')
  await expect(searchInput).toBeVisible()
  await searchInput.fill(title)
  await expect(page.getByTestId('quick-open-palette').getByText(title, { exact: true })).toBeVisible()
  await page.keyboard.press('Enter')
  await expect(page.getByRole('heading', { name: title, level: 1 })).toBeVisible()
  await openPropertiesPanel(page)
}

function editableProperty(page: Page, key: string) {
  return page.getByTestId('editable-property').filter({ hasText: key })
}

/**
 * Label of a relationship the note actually has. Empty "suggested" slots reuse
 * the same label element and still offer `belongs_to` when the note has a
 * spaced `Belongs to` key (f6-report, D2), so they are excluded here.
 */
function relationshipLabel(page: Page, key: string) {
  return page
    .locator('[data-testid="relationship-section-label"]:not([data-testid="suggested-relationship"] *)')
    .filter({ hasText: key })
}

test.describe('Dynamic wikilink relationship detection', () => {
  test.beforeEach(async ({ page, request }) => {
    await page.setViewportSize({ width: 1600, height: 900 })
    tempVaultDir = createFixtureVaultCopy()
    await writeVaultFile(request, CUSTOM_RELATIONSHIPS_NOTE, CUSTOM_RELATIONSHIPS_CONTENT)
    await writeVaultFile(request, STANDARD_RELATIONSHIPS_NOTE, STANDARD_RELATIONSHIPS_CONTENT)
    await openFixtureVault(page, tempVaultDir)
  })

  test.afterEach(() => {
    removeFixtureVaultCopy(tempVaultDir)
  })

  test('wikilink fields render as relationships, plain-text fields as properties', async ({ page, request }) => {
    // Has Measures/Has Procedures are custom wikilink arrays; Status is plain text
    await openNoteViaQuickOpen(page, 'Sponsorships')

    const statusProp = editableProperty(page, 'Status')
    await expect(statusProp).toBeVisible({ timeout: 5000 })
    await expect(statusProp.getByText('Open')).toBeVisible()

    // 'Has Measures' has wikilinks → should NOT be in Properties
    await expect(editableProperty(page, 'Has Measures')).not.toBeVisible()

    // 'Has Measures' should appear as a relationship label
    await expect(relationshipLabel(page, 'Has Measures')).toBeVisible()

    // 'Has Procedures' has wikilinks → should be in Relationships
    await expect(relationshipLabel(page, 'Has Procedures')).toBeVisible()

    // Detection is render-only: the note's frontmatter keys stay as written
    expect(await readVaultFile(request, CUSTOM_RELATIONSHIPS_NOTE)).toBe(CUSTOM_RELATIONSHIPS_CONTENT)
  })

  test('existing wikilink relationships still render correctly', async ({ page, request }) => {
    // Belongs to and Owner are single wikilinks; Status is plain text
    await openNoteViaQuickOpen(page, 'Start Laputa App')

    await expect(editableProperty(page, 'Status')).toBeVisible({ timeout: 5000 })

    // 'Belongs to' has wikilink → should be in Relationships (not Properties)
    await expect(editableProperty(page, 'Belongs to')).not.toBeVisible()

    // 'Belongs to' should appear as a relationship label
    await expect(relationshipLabel(page, 'Belongs to')).toBeVisible()

    // 'Owner' has wikilink → should be in Relationships
    await expect(editableProperty(page, 'Owner')).not.toBeVisible()
    await expect(relationshipLabel(page, 'Owner')).toBeVisible()

    expect(await readVaultFile(request, STANDARD_RELATIONSHIPS_NOTE)).toBe(STANDARD_RELATIONSHIPS_CONTENT)
  })
})
