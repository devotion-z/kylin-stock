import { expect, it, vi } from 'vitest'
const native = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/tauri', () => ({ invoke: native.invoke }))
import { listInventoryPage } from './inventory'
import { withDatabaseMutation } from './database'

it('waits for a pending edit before serving cached distribution data', async () => {
  native.invoke.mockResolvedValueOnce({ rows: [{ quantity: 20 }], total: 1 })
  await listInventoryPage()
  let release!: () => void
  const hold = new Promise<void>(resolve => { release = resolve })
  const write = withDatabaseMutation(() => hold)
  native.invoke.mockResolvedValueOnce({ rows: [{ quantity: 20 }, { quantity: 30 }], total: 2 })
  let delivered = false
  const read = listInventoryPage().then(result => { delivered = true; return result })
  await new Promise(resolve => setTimeout(resolve, 0))
  expect(delivered).toBe(false)
  release()
  await write
  expect((await read).total).toBe(2)
  expect(native.invoke).toHaveBeenCalledTimes(2)
  // Count hints must not bypass the same page cache.
  expect((await listInventoryPage({}, 1, 100, 2)).total).toBe(2)
  expect(native.invoke).toHaveBeenCalledTimes(2)
})
