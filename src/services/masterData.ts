import { invoke } from '@tauri-apps/api/tauri'
import { getDatabase, withDatabaseAccess, withDatabaseMutation } from './database'

export interface Unit { id: number; name: string; status: number }
export interface Location { id: number; name: string; remark: string | null; status: number }
export type MasterDataChoice = number | string | undefined
interface NamedChoice { id: number; name: string }
export interface Material {
  id: number
  name: string
  barcode: string | null
  unit_id: number | null
  unit_name: string | null
  category: string | null
  default_location_id: number | null
  location_name: string | null
  remark: string | null
  status: number
  created_at: string
  updated_at: string
  attachment_count: number
}

const now = () => new Date().toISOString()

export async function listUnits(): Promise<Unit[]> {
  return withDatabaseAccess(async () =>
    (await getDatabase()).select<Unit[]>('SELECT id, name, status FROM units WHERE status = 1 ORDER BY name'),
  )
}

export async function createUnit(name: string) {
  const value = name.trim()
  if (!value) throw new Error('单位名称不能为空')
  return withDatabaseMutation(async () =>
    (await getDatabase()).execute('INSERT INTO units(name, status) VALUES ($1, 1)', [value]),
  )
}

export async function resolveMasterDataChoice(
  choice: MasterDataChoice,
  items: NamedChoice[],
  create: (name: string) => Promise<{ lastInsertId: number }>,
) {
  if (typeof choice === 'number') return choice
  const name = choice?.trim()
  if (!name) return undefined
  const existing = items.find((item) => item.name.trim().toLocaleLowerCase() === name.toLocaleLowerCase())
  if (existing) return existing.id
  const result = await create(name)
  return Number(result.lastInsertId)
}

export function resolveUnitChoice(choice: MasterDataChoice, units: Unit[]) {
  return resolveMasterDataChoice(choice, units, createUnit)
}

export async function listLocations(): Promise<Location[]> {
  return withDatabaseAccess(async () => {
    const rows = await (await getDatabase()).select<Location[]>('SELECT id, name, remark, status FROM locations WHERE status = 1 ORDER BY name')
    const collator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' })
    return rows.sort((left, right) => collator.compare(left.name, right.name))
  })
}

export async function createLocation(name: string, remark = '') {
  const value = name.trim()
  const normalizedRemark = remark.trim() || null
  if (!value) throw new Error('存放位置不能为空')
  return withDatabaseMutation(async () =>
    (await getDatabase()).execute('INSERT INTO locations(name, remark, status) VALUES ($1, $2, 1)', [value, normalizedRemark]),
  )
}

export async function deleteLocation(id: number) {
  const locationId = Number(id)
  return withDatabaseMutation(() => invoke('delete_location', { id: locationId }))
}

export async function deleteUnit(id: number) {
  const unitId = Number(id)
  return withDatabaseMutation(async () => {
    const db = await getDatabase()
    const references = await db.select<{ count: number }[]>('SELECT COUNT(*) AS count FROM materials WHERE unit_id=$1', [unitId])
    if (Number(references[0]?.count ?? 0) > 0) throw new Error('该计量单位已被物资使用，不能删除')
    return db.execute('DELETE FROM units WHERE id=$1', [unitId])
  })
}

export async function resolveLocationChoice(choice: MasterDataChoice, locations: Location[]) {
  return resolveMasterDataChoice(choice, locations, (name) => createLocation(name))
}

export async function listMaterials(keyword = ''): Promise<Material[]> {
  const q = `%${keyword.trim()}%`
  return withDatabaseAccess(async () =>
    (await getDatabase()).select<Material[]>(`
      SELECT m.id, m.name, m.barcode, m.unit_id, u.name AS unit_name, m.category,
             m.default_location_id, l.name AS location_name, m.remark,
             m.status, m.created_at, m.updated_at,
             (SELECT COUNT(*) FROM attachments a WHERE a.entity_type='MATERIAL' AND a.entity_id=m.id) AS attachment_count
      FROM materials m
      LEFT JOIN units u ON u.id = m.unit_id
      LEFT JOIN locations l ON l.id = m.default_location_id
      WHERE ($1 = '%%' OR m.name LIKE $1 OR COALESCE(m.category, '') LIKE $1)
      ORDER BY m.status DESC, m.name
    `, [q]),
  )
}

export async function saveMaterial(input: {
  id?: number
  name: string
  barcode?: string
  unitId?: number | null
  category?: string
  locationId?: number | null
  remark?: string
}) {
  const normalized = {
    id: input.id,
    name: input.name.trim(),
    barcode: input.barcode?.trim() || null,
    unitId: input.unitId ?? null,
    category: input.category?.trim() || null,
    locationId: input.locationId ?? null,
    remark: input.remark?.trim() || null,
  }
  if (!normalized.name) throw new Error('物资名称不能为空')

  return withDatabaseMutation(async () => {
    const db = await getDatabase()

    // V1 intentionally has no user-visible product/SKU code. Therefore the
    // material name is the human-facing identity. Keep duplicate detection and
    // the following INSERT/UPDATE in the same application mutation turn so two
    // callers cannot both pass the precheck concurrently.
    const conflicts = await db.select<{ id: number; status: number }[]>(`
      SELECT id, status
      FROM materials
      WHERE name = $1 COLLATE NOCASE
        AND (($3 IS NULL AND default_location_id IS NULL) OR default_location_id = $3)
        AND ($2 IS NULL OR id <> $2)
        LIMIT 1
    `, [normalized.name, normalized.id ?? null, normalized.locationId])
    if (conflicts.length) {
      throw new Error(conflicts[0].status === 0
        ? '同名同库位物资已停用，请直接重新启用原物资'
        : '同名同库位物资已存在，请勿重复添加')
    }

    if (normalized.barcode) {
      const barcodeConflicts = await db.select<{ id: number }[]>(`
        SELECT id FROM materials WHERE barcode = $1 AND ($2 IS NULL OR id <> $2) LIMIT 1
      `, [normalized.barcode, normalized.id ?? null])
      if (barcodeConflicts.length) throw new Error('条码已被其他物资使用，请更换后再保存')
    }

    const timestamp = now()
    if (normalized.id) {
      return db.execute(`UPDATE materials SET name=$1, barcode=$2, unit_id=$3, category=$4,
        default_location_id=$5, remark=$6, updated_at=$7 WHERE id=$8`, [
        normalized.name, normalized.barcode, normalized.unitId, normalized.category,
        normalized.locationId, normalized.remark, timestamp, normalized.id,
      ])
    }
    return db.execute(`INSERT INTO materials
      (name, barcode, unit_id, category, default_location_id, remark, status, created_at, updated_at)
      VALUES ($1,$2,$3,$4,$5,$6,1,$7,$7)`, [
      normalized.name, normalized.barcode, normalized.unitId, normalized.category,
      normalized.locationId, normalized.remark, timestamp,
    ])
  })
}

export async function deleteMaterial(id: number) {
  const materialId = Number(id)
  return withDatabaseMutation(async () => {
    const db = await getDatabase()
    const used = await db.select<{ count: number }[]>(
      `SELECT COUNT(*) AS count FROM stock_transactions WHERE material_id=$1`, [materialId],
    )
    const balances = await db.select<{ count: number }[]>(
      `SELECT COUNT(*) AS count FROM inventory_balances WHERE material_id=$1 AND quantity <> 0`, [materialId],
    )
    if (Number(used[0]?.count ?? 0) > 0 || Number(balances[0]?.count ?? 0) > 0) {
      throw new Error('该物资已有库存或出入库记录，不能删除，请先停用')
    }
    await db.execute(`DELETE FROM attachments WHERE entity_type='MATERIAL' AND entity_id=$1`, [materialId])
    await db.execute('DELETE FROM inventory_balances WHERE material_id=$1', [materialId])
    return db.execute('DELETE FROM materials WHERE id=$1', [materialId])
  })
}

export async function setMaterialStatus(id: number, status: 0 | 1) {
  const materialId = Number(id)
  const nextStatus = status
  return withDatabaseMutation(async () =>
    (await getDatabase()).execute('UPDATE materials SET status=$1, updated_at=$2 WHERE id=$3', [nextStatus, now(), materialId]),
  )
}
