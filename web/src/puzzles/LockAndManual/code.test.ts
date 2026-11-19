import { describe, expect, it } from 'vitest'
import { parseCode, sanitizeDigit } from './code'

describe('parseCode', () => {
  it('needs every box filled with one digit', () => {
    expect(parseCode(['4', '7', '2'])).toEqual([4, 7, 2])
    expect(parseCode(['4', '', '2'])).toBeNull()
    expect(parseCode(['4', 'x', '2'])).toBeNull()
    expect(parseCode(['0'])).toEqual([0])
  })
})

describe('sanitizeDigit', () => {
  it('keeps only the last digit typed', () => {
    expect(sanitizeDigit('5')).toBe('5')
    expect(sanitizeDigit('45')).toBe('5')
    expect(sanitizeDigit('a')).toBe('')
    expect(sanitizeDigit('')).toBe('')
  })
})
