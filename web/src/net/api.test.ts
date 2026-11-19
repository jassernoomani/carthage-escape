import { afterEach, describe, expect, it, vi } from 'vitest'
import { fetchHealth } from './api'

function mockFetch(status: number, body: unknown) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => new Response(JSON.stringify(body), { status })),
  )
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('fetchHealth', () => {
  it('returns the health payload on 200', async () => {
    mockFetch(200, { status: 'ok', version: '0.1.0', database: 'ok' })
    await expect(fetchHealth()).resolves.toEqual({
      status: 'ok',
      version: '0.1.0',
      database: 'ok',
    })
  })

  it('still parses the body on 503 (database down)', async () => {
    mockFetch(503, { status: 'degraded', version: '0.1.0', database: 'degraded' })
    await expect(fetchHealth()).resolves.toMatchObject({ status: 'degraded' })
  })

  it('throws on other errors', async () => {
    mockFetch(500, {})
    await expect(fetchHealth()).rejects.toThrow('HTTP 500')
  })
})
