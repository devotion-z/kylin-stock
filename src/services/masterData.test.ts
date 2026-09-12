import { describe, expect, it, vi } from 'vitest'
import { resolveMasterDataChoice } from './masterData'

describe('resolveMasterDataChoice', () => {
  it('keeps an existing numeric selection', async () => {
    const create = vi.fn()
    await expect(resolveMasterDataChoice(7, [], create)).resolves.toBe(7)
    expect(create).not.toHaveBeenCalled()
  })

  it('treats blank input as no selection', async () => {
    const create = vi.fn()
    await expect(resolveMasterDataChoice('   ', [], create)).resolves.toBeUndefined()
    expect(create).not.toHaveBeenCalled()
  })

  it('reuses an existing choice without creating a duplicate', async () => {
    const create = vi.fn()
    await expect(resolveMasterDataChoice('  KG ', [{ id: 3, name: 'kg' }], create)).resolves.toBe(3)
    expect(create).not.toHaveBeenCalled()
  })

  it('creates a new choice from typed text and returns its id', async () => {
    const create = vi.fn().mockResolvedValue({ lastInsertId: 12 })
    await expect(resolveMasterDataChoice(' 一号库 ', [], create)).resolves.toBe(12)
    expect(create).toHaveBeenCalledWith('一号库')
  })
})
