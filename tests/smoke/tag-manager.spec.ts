import fs from 'node:fs'
import path from 'node:path'
import { test, expect } from '@playwright/test'
import { createFixtureVaultCopy, openFixtureVault, removeFixtureVaultCopy } from '../helpers/fixtureVault'
import { openCommandPalette } from './helpers'

let tempVaultDir: string

function writeTaggedNote(fileName: string, title: string, tags: string[]) {
  const tagLines = tags.map((tag) => `  - ${tag}`).join('\n')
  fs.writeFileSync(
    path.join(tempVaultDir, fileName),
    `---\ntitle: ${title}\ntags:\n${tagLines}\n---\n\n# ${title}\n`,
  )
}

test.describe('Tag manager', () => {
  test.beforeEach(() => {
    tempVaultDir = createFixtureVaultCopy()
    writeTaggedNote('tagged-one.md', 'Tagged One', ['blues', 'live'])
    writeTaggedNote('tagged-two.md', 'Tagged Two', ['blues'])
  })

  test.afterEach(() => {
    removeFixtureVaultCopy(tempVaultDir)
  })

  test('renames a tag across every note from the command palette', async ({ page }) => {
    await openFixtureVault(page, tempVaultDir)
    await openCommandPalette(page)
    await page.locator('input[placeholder="Type a command..."]').fill('manage tags')
    await page.keyboard.press('Enter')

    const row = page.getByTestId('tag-manager-row-blues')
    await expect(row).toContainText('2 notes')
    await page.getByTestId('tag-manager-menu-blues').click()
    await page.getByRole('menuitem', { name: 'Rename…' }).click()
    // Type through the keyboard (not fill) so the test proves the inline editor
    // actually holds focus once the row menu has closed.
    const renameInput = page.getByPlaceholder('New tag name')
    await expect(renameInput).toBeFocused()
    await page.keyboard.press('ControlOrMeta+A')
    await page.keyboard.type('soul')
    await expect(renameInput).toHaveValue('soul')
    await page.getByRole('button', { name: 'Continue' }).click()
    await expect(page.getByText('Rename “blues” to “soul” in 2 notes?')).toBeVisible()
    // Mark this document so a full page reload (which would rebuild the inventory
    // from disk) cannot make the refreshed-row assertions below pass by accident.
    await page.evaluate(() => { Reflect.set(window, '__tagManagerNoReload', true) })
    await page.getByRole('button', { name: 'Apply' }).click()
    await expect(page.getByText('Updated 2 notes.')).toBeVisible()
    await expect(page.getByTestId('tag-manager-row-soul')).toContainText('2 notes')
    await expect(page.getByTestId('tag-manager-row-blues')).toHaveCount(0)
    await expect(page.getByTestId('tag-manager-row-live')).toContainText('1 notes')
    expect(await page.evaluate(() => Reflect.get(window, '__tagManagerNoReload'))).toBe(true)

    // The fixture harness's mocked `update_frontmatter` command serializes array
    // values as block-style YAML with each item JSON-stringified (quoted), e.g.
    // `  - "soul"` rather than the unquoted `  - soul` this test wrote initially.
    await expect.poll(() => fs.readFileSync(path.join(tempVaultDir, 'tagged-one.md'), 'utf8'))
      .toContain('- "soul"')
    const one = fs.readFileSync(path.join(tempVaultDir, 'tagged-one.md'), 'utf8')
    expect(one).not.toContain('blues')
    expect(one).toContain('- "live"')
    expect(one).toContain('# Tagged One')
    expect(fs.readFileSync(path.join(tempVaultDir, 'tagged-two.md'), 'utf8')).toContain('- "soul"')
  })
})
