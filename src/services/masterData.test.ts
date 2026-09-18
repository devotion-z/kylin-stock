import { beforeEach, describe, expect, it, vi } from 'vitest'

const databaseMocks = vi.hoisted(() => ({
  select: vi.fn(),
  execute: vi.fn(),
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/tauri', () => ({ invoke: databaseMocks.invoke }))

vi.mock('./database', () => ({
  getDatabase: vi.fn().mockResolvedValue(databaseMocks),
  withDatabaseRead: (operation: () => Promise<unknown>) => operation(),
  withDatabaseMutation: (operation: () => Promise<unknown>) => operation(),
}))

import { resolveMasterDataChoice, saveMaterial } from './masterData'

beforeEach(() => {
  databaseMocks.select.mockReset()
  databaseMocks.execute.mockReset()
  databaseMocks.invoke.mockReset()
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
  it('updates an existing material when no other row has the same name and unit', async () => {
    databaseMocks.select.mockResolvedValue([])
    databaseMocks.execute.mockResolvedValue({ rowsAffected: 1, lastInsertId: 27 })

    await saveMaterial({ id: 27, name: '毛巾', unitId: 3, locationId: 4 })

    expect(databaseMocks.select).toHaveBeenCalledOnce()
    expect(databaseMocks.execute).toHaveBeenCalledOnce()
    expect(databaseMocks.execute.mock.calls[0][1]).toEqual([
      '毛巾', null, 3, null, 4, null, expect.any(String), 27,
    ])
  })

  it('reuses the existing archive when adding the same name and unit in another warehouse', async () => {
    databaseMocks.select.mockResolvedValue([{ id: 27, status: 1 }])
    databaseMocks.invoke.mockResolvedValue({ rowsAffected: 1, lastInsertId: 27, reused: true })

    await expect(saveMaterial({ name: '毛巾', unitId: 3, locationId: 2, remark: '不覆盖原备注' }))
      .resolves.toEqual({ rowsAffected: 1, lastInsertId: 27, reused: true })
    expect(databaseMocks.invoke).toHaveBeenCalledWith('reuse_material', {
      id: 27, name: '毛巾', unitId: 3, locationId: 2,
    })
    expect(databaseMocks.execute).not.toHaveBeenCalled()
  })

  it('still creates a separate material for a different unit', async () => {
    databaseMocks.select.mockResolvedValue([])
    databaseMocks.execute.mockResolvedValue({ rowsAffected: 1, lastInsertId: 28 })
    await expect(saveMaterial({ name: '毛巾', unitId: 8, locationId: 2 }))
      .resolves.toEqual({ rowsAffected: 1, lastInsertId: 28 })
    expect(databaseMocks.execute.mock.calls[0][0]).toContain('INSERT INTO materials')
    expect(databaseMocks.invoke).not.toHaveBeenCalled()
  })

  it('merges an edited material into the matching name and unit row', async () => {
    databaseMocks.select.mockResolvedValue([{ id: 19, status: 1 }])
    databaseMocks.invoke.mockResolvedValue({ rowsAffected: 1, lastInsertId: 19, merged: true })

    await expect(saveMaterial({ id: 27, name: '消防工具', unitId: 3, locationId: 5 }))
      .resolves.toEqual({ rowsAffected: 1, lastInsertId: 19, merged: true })
    expect(databaseMocks.invoke).toHaveBeenCalledWith('merge_materials', expect.objectContaining({
      sourceId: 27,
      targetId: 19,
      name: '消防工具',
      unitId: 3,
      locationId: 5,
    }))
    expect(databaseMocks.execute).not.toHaveBeenCalled()
  })
})
