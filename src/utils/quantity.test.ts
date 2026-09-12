import { describe, expect, it } from 'vitest'
import { parseQuantityInput } from './quantity'

describe('parseQuantityInput', () => {
  it.each([
    ['1', 1],
    ['1.2', 1.2],
    ['1.23', 1.23],
    ['1000.01', 1000.01],
  ])('keeps valid input %s without display rounding', (input, expected) => {
    expect(parseQuantityInput(input)).toBe(expected)
  })

  it.each(['0', '-1', '1.234', '1.', '.5', 'abc'])('rejects invalid input %s instead of rounding it', (input) => {
    expect(() => parseQuantityInput(input)).toThrow()
  })
})
