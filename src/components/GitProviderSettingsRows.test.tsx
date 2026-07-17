import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { createTranslator } from '../lib/i18n'
import { GitProviderSettingsRows } from './GitProviderSettingsRows'
import type { GitProviderProbe, GitProviderStatus } from '../types'

const invokeMock = vi.fn()
const mockInvokeMock = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

vi.mock('../mock-tauri', () => ({
  isTauri: () => false,
  mockInvoke: (...args: unknown[]) => mockInvokeMock(...args),
}))

const t = createTranslator()

const mockWslProbe: GitProviderProbe = {
  provider: 'wsl',
  label: 'WSL2 Git',
  available: true,
  version: 'git version 2.43.0',
  distro: 'Ubuntu',
  path: null,
  message: 'WSL2 Git is available: git version 2.43.0',
}

const mockProviderStatus: GitProviderStatus = {
  selected_provider: 'native',
  selected_wsl_distro: null,
  native: {
    provider: 'native',
    label: 'Native Git',
    available: true,
    version: 'git version 2.45.0',
    distro: null,
    path: null,
    message: 'Native Git is available: git version 2.45.0',
  },
  wsl_distributions: [mockWslProbe],
}

describe('GitProviderSettingsRows', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    mockInvokeMock.mockReset()
  })

  it('falls back to the mock handler when the web transport resolves undefined instead of throwing', async () => {
    // Mirrors src/web/transport.ts: an unimplemented server command resolves to
    // `undefined` on a 501 rather than rejecting the promise.
    invokeMock.mockResolvedValue(undefined)
    mockInvokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'git_provider_status') return mockProviderStatus
      return undefined
    })

    const setGitWslDistro = vi.fn()

    expect(() =>
      render(
        <GitProviderSettingsRows
          gitProvider="wsl"
          gitWslDistro={null}
          setGitProvider={vi.fn()}
          setGitWslDistro={setGitWslDistro}
          t={t}
        />,
      ),
    ).not.toThrow()

    await waitFor(() => {
      expect(setGitWslDistro).toHaveBeenCalledWith('Ubuntu')
    })
  })

  it('does not crash when both the real transport and the mock handler resolve undefined', async () => {
    invokeMock.mockResolvedValue(undefined)
    mockInvokeMock.mockResolvedValue(undefined)

    expect(() =>
      render(
        <GitProviderSettingsRows
          gitProvider="native"
          gitWslDistro={null}
          setGitProvider={vi.fn()}
          setGitWslDistro={vi.fn()}
          t={t}
        />,
      ),
    ).not.toThrow()

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('git_provider_status', {})
    })
  })

  it('reports a failed test instead of crashing when test_git_provider resolves undefined', async () => {
    invokeMock.mockResolvedValue(undefined)
    mockInvokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'git_provider_status') return mockProviderStatus
      return undefined
    })

    render(
      <GitProviderSettingsRows
        gitProvider="native"
        gitWslDistro={null}
        setGitProvider={vi.fn()}
        setGitWslDistro={vi.fn()}
        t={t}
      />,
    )

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('git_provider_status', {})
    })

    fireEvent.click(screen.getByTestId('settings-git-provider-test'))

    await waitFor(() => {
      expect(screen.getByText(t('settings.git.providerTestFailed', { message: 'No response from the git provider test.' }))).toBeInTheDocument()
    })
  })
})
