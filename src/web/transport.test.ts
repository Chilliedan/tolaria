import { describe, it, expect, vi, beforeEach } from 'vitest'
import { invoke } from './transport'

describe('web invoke transport', () => {
  beforeEach(() => { vi.restoreAllMocks() })

  it('POSTs to /api/cmd/:command and returns parsed JSON', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(['a', 'b']), { status: 200, headers: { 'content-type': 'application/json' } }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const result = await invoke<string[]>('list_vault', { path: '/v' })
    expect(fetchMock).toHaveBeenCalledWith('/api/cmd/list_vault', expect.objectContaining({ method: 'POST' }))
    expect(result).toEqual(['a', 'b'])
  })

  it('returns undefined for a 501 unsupported command instead of throwing', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ error: 'unsupported' }), { status: 501 }),
    ))
    const result = await invoke('save_note_content', { path: '/v/n.md', content: 'x' })
    expect(result).toBeUndefined()
  })

  it('returns undefined and redirects to /login on 401', async () => {
    const assign = vi.fn()
    vi.stubGlobal('window', { location: { assign } })
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ error: 'not authenticated' }), { status: 401 }),
    ))
    const result = await invoke('list_vault', { path: '/v' })
    expect(result).toBeUndefined()
    expect(assign).toHaveBeenCalledWith('/login')
  })

  it('sends X-CSRF-Token header from the tolaria_csrf cookie', async () => {
    vi.stubGlobal('document', { cookie: 'tolaria_csrf=tok123; other=x' } as unknown as Document)
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(['a']), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    await invoke('list_vault', { path: '/v' })
    const init = fetchMock.mock.calls[0][1] as RequestInit
    expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('tok123')
  })

  it('sends baseHash on save from a prior get_note_content and reloads on 409', async () => {
    vi.stubGlobal('document', { cookie: 'tolaria_csrf=t' } as unknown as Document)
    // First: read content so the transport records its hash.
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify('hello'), { status: 200 })))
    await invoke('get_note_content', { path: '/v/n.md' })
    // Then a 409 save → warns + reloads, returns undefined.
    const reload = vi.fn()
    vi.stubGlobal('window', { location: { reload, assign: vi.fn() }, alert: vi.fn() })
    const saveFetch = vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: 'conflict', currentContent: 'server' }), { status: 409 }))
    vi.stubGlobal('fetch', saveFetch)
    const result = await invoke('save_note_content', { path: '/v/n.md', content: 'mine' })
    const body = JSON.parse((saveFetch.mock.calls[0][1] as RequestInit).body as string)
    expect(body.baseHash).toBeDefined()
    expect(result).toBeUndefined()
    expect(reload).toHaveBeenCalled()
  })
})
