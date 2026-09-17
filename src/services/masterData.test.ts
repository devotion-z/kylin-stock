import { beforeEach, describe, expect, it, vi } from 'vitest'

const databaseMocks = vi.hoisted(() => ({
  select: vi.fn(),
  execute: vi.fn(),
}))

vi.mock('./database', () => ({
  getDatabase: vi.fn().mockResolvedValue(databaseMocks),
  withDatabaseRead: (operation: () => Promise<unknown>) => operation(),
  withDatabaseMutation: (operation: () => Promise<unknown>) => operation(),
}))

import { resolveMasterDataChoice, saveMaterial } from './masterData'

beforeEach(() => {
  databaseMocks.select.mockReset()
  databaseMocks.execute.mockReset()
})

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

describe('saveMaterial', () => {
  it('updates an existing material without running the create-only duplicate preflight', async () => {
    databaseMocks.execute.mockResolvedValue({ rowsAffected: 1, lastInsertId: 27 })

    await saveMaterial({ id: 27, name: '毛巾', unitId: 3, locationId: 4 })

    expect(databaseMocks.select).not.toHaveBeenCalled()
    expect(databaseMocks.execute).toHaveBeenCalledOnce()
    expect(databaseMocks.execute.mock.calls[0][1]).toEqual([
      '毛巾', null, 3, null, 4, null, expect.any(String), 27,
    ])
  })

  it('still blocks a true duplicate when creating a new material', async () => {
    databaseMocks.select.mockResolvedValue([{ id: 27, status: 1 }])

    await expect(saveMaterial({ name: '毛巾', unitId: 3 }))
      .rejects.toThrow('同名同计量单位物资已存在')
    expect(databaseMocks.execute).not.toHaveBeenCalled()
  })
})
