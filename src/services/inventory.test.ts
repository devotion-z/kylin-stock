import { describe, expect, it } from 'vitest'
import { sortInventoryByLocation, type InventoryRow } from './inventory'

function row(location_name: string, material_name: string): InventoryRow {
  return {
    material_id: 1,
    material_name,
    unit_name: '件',
    location_id: 1,
    location_name,
    quantity: 1,
    updated_at: '2026-09-15T00:00:00.000Z',
  }
}

describe('sortInventoryByLocation', () => {
  it('uses warehouse number order instead of lexical order', () => {
    const rows = [row('10号库', '毛巾'), row('2号库', '牙刷'), row('1号库', '水泥')]
    expect(sortInventoryByLocation(rows).map((item) => item.location_name)).toEqual(['1号库', '2号库', '10号库'])
  })

  it('keeps materials in name order inside the same warehouse', () => {
    const rows = [row('1号库', '水泥'), row('1号库', '钢筋')]
    expect(sortInventoryByLocation(rows).map((item) => item.material_name)).toEqual(['钢筋', '水泥'])
  })
})
