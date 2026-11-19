import { describe, expect, it } from 'vitest'
import { canStart, normalizeCode } from './lobby'

const p = (ready: boolean, connected = true) => ({ nickname: 'x', role_index: 0, ready, connected })

describe('canStart', () => {
  it('needs a full room of connected, ready players', () => {
    expect(canStart([p(true)], 2)).toBe(false)
    expect(canStart([p(true), p(false)], 2)).toBe(false)
    expect(canStart([p(true), p(true, false)], 2)).toBe(false)
    expect(canStart([p(true), p(true)], 2)).toBe(true)
  })
})

describe('normalizeCode', () => {
  it('uppercases, strips spaces and caps at 6 chars', () => {
    expect(normalizeCode(' ab c23 4x')).toBe('ABC234')
  })
})
