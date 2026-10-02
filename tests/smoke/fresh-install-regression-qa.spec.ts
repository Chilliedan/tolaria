import { test, expect, type Locator, type Page } from '@playwright/test'
import { sendShortcut, openCommandPalette, findCommand, waitForAppReady } from './helpers'

/** `ai.panel.empty.withContextDescription` / `noContextDescription` in en.json. */
const NOTE_CONTEXT_DESCRIPTION = 'Summarize, find connections, expand ideas'
const NO_CONTEXT_DESCRIPTION = 'The AI will use the active note as context'

/**
 * Opening a note also loads the lazy editor module. Until it loads, the AI
 * workspace renders inside `EditorStartupFallback`, and it remounts (dropping
 * any typed prompt) when the real editor replaces the fallback, so wait for
 * the editor first. That remount is reported separately (f6-report, D1).
 */
async function selectFirstNote(page: Page): Promise<void> {
  await page.locator('.app__note-list .cursor-pointer').first().click()
  await expect(page.getByTestId('editor-module-loading')).toHaveCount(0)
}

async function openAiPanel(page: Page): Promise<Locator> {
  await sendShortcut(page, 'L', ['Control', 'Shift'])
  const panel = page.getByTestId('ai-panel')
  await expect(panel).toBeVisible({ timeout: 3000 })
  return panel
}

/**
 * Fresh-install regression QA: verify all 7 Done tasks work on a fresh
 * MacBook without pre-configured environment.
 *
 * Tasks under test:
 * 1. MCP server foundation + auto-registration
 * 3. AGENTS.md vault-level instructions
 * 4. AI Agent panel: vault-native AI
 * 5. Claude API wiring + full agent loop
 * 6. AI panel UI (workspace header, empty state, composer + blue glow)
 * 7. /api/ai/agent endpoint fix (uses Tauri invoke, not fetch)
 */

test.describe('Fresh-install regression: AI panel renders and works', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/api/vault/ping', route => route.fulfill({ status: 503 }))
    await page.goto('/')
    await waitForAppReady(page)
  })

  // The March 2026 "3-layer" panel (own "AI Chat" header, context bar, plain
  // <input>) was replaced: d540d76f (2026-04-13) moved the composer to the
  // wikilink chat input, 8828516d (2026-05-26) hosted the panel inside the AI
  // workspace, and 78892c46 (2026-05-29) made that a side workspace whose
  // context shows in the empty state instead of a context bar.
  test('AI panel opens with Cmd+Shift+L with workspace header, empty state and composer', async ({ page }) => {
    await selectFirstNote(page)
    const panel = await openAiPanel(page)

    // Header: the workspace chrome around the panel
    const workspace = page.getByTestId('ai-workspace')
    await expect(workspace.getByRole('button', { name: 'New chat' })).toBeVisible()
    await expect(workspace.getByRole('button', { name: 'Close AI workspace' })).toBeVisible()

    // Message area: empty state for the active agent
    await expect(panel.getByText(NOTE_CONTEXT_DESCRIPTION)).toBeVisible()

    // Composer: input with send button
    await expect(panel.getByTestId('agent-input')).toBeVisible()
    await expect(panel.getByTestId('agent-send')).toBeVisible()
  })

  test('AI panel uses the selected note as context', async ({ page }) => {
    await selectFirstNote(page)
    const panel = await openAiPanel(page)

    await expect(panel.getByText(NOTE_CONTEXT_DESCRIPTION)).toBeVisible()
    await expect(panel.getByText(NO_CONTEXT_DESCRIPTION)).toHaveCount(0)
  })

  test('AI panel input is focusable and sendable', async ({ page }) => {
    await selectFirstNote(page)
    const panel = await openAiPanel(page)

    const input = panel.getByTestId('agent-input')
    await input.click()
    await expect(input).toBeFocused()
    await input.fill('Test message')

    // Send button should be enabled when input has text
    const sendBtn = panel.getByTestId('agent-send')
    await expect(sendBtn).toBeEnabled()

    // Click send — should produce a response (mock in dev mode)
    await sendBtn.click()
    const response = panel.getByTestId('ai-message').last()
    await expect(response).toBeVisible({ timeout: 5000 })
  })

  test('AI panel close with Escape key', async ({ page }) => {
    const noteItem = page.locator('.app__note-list .cursor-pointer').first()
    await noteItem.click()
    await page.waitForTimeout(300)

    await sendShortcut(page, 'L', ['Control', 'Shift'])
    const panel = page.getByTestId('ai-panel')
    await expect(panel).toBeVisible({ timeout: 3000 })

    // Focus the panel and press Escape
    await panel.focus()
    await page.keyboard.press('Escape')
    await expect(panel).not.toBeVisible({ timeout: 2000 })
  })

  test('AI panel blue glow animation CSS exists', async ({ page }) => {
    const noteItem = page.locator('.app__note-list .cursor-pointer').first()
    await noteItem.click()
    await page.waitForTimeout(300)

    await sendShortcut(page, 'L', ['Control', 'Shift'])
    const panel = page.getByTestId('ai-panel')
    await expect(panel).toBeVisible({ timeout: 3000 })

    // Verify the ai-border-pulse keyframe is defined in the page CSS
    const hasAnimation = await page.evaluate(() => {
      for (const sheet of document.styleSheets) {
        try {
          for (const rule of sheet.cssRules) {
            if (rule instanceof CSSKeyframesRule && rule.name === 'ai-border-pulse') {
              return true
            }
          }
        } catch { /* cross-origin sheets */ }
      }
      return false
    })
    expect(hasAnimation).toBe(true)
  })
})

test.describe('Fresh-install regression: search and command palette', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/api/vault/ping', route => route.fulfill({ status: 503 }))
    await page.goto('/')
    await waitForAppReady(page)
  })

  test('search UI renders and is accessible via Cmd+P', async ({ page }) => {
    // Cmd+P should open search/note switcher
    await sendShortcut(page, 'p', ['Control'])
    await page.waitForTimeout(500)

    // Search input should be visible
    const searchInput = page.locator('input[placeholder*="Search"]').or(
      page.locator('input[placeholder*="command"]'),
    )
    await expect(searchInput).toBeVisible({ timeout: 2000 })
  })

  test('Cmd+K command palette includes Repair Vault', async ({ page }) => {
    await openCommandPalette(page)
    const found = await findCommand(page, 'Repair Vault')
    expect(found).toBe(true)
  })
})

test.describe('Fresh-install regression: no /api/ai/agent endpoint', () => {
  test('fetching /api/ai/agent returns 404 or no response', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)

    // In dev mode, the Vite proxy plugin handles /api/ai/agent,
    // but in production Tauri there is no HTTP server at all.
    // The key verification is that ai-agent.ts uses invoke(), not fetch().
    // We verify this indirectly by checking the AiPanel doesn't make fetch calls.
    const fetchCalls: string[] = []
    page.on('request', req => {
      if (req.url().includes('/api/ai/agent')) {
        fetchCalls.push(req.url())
      }
    })

    // Open AI panel and send a message
    await selectFirstNote(page)
    const panel = await openAiPanel(page)
    await panel.getByTestId('agent-input').fill('Test')
    await panel.getByTestId('agent-send').click()
    await expect(panel.getByTestId('ai-message').last()).toBeVisible({ timeout: 5000 })
    await page.waitForTimeout(1000)

    // No fetch to /api/ai/agent should have been made
    expect(fetchCalls).toHaveLength(0)
  })
})
