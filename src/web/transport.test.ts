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
})
