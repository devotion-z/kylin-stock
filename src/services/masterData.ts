import { getDatabase, withDatabaseAccess, withDatabaseMutation } from './database'

export interface Unit { id: number; name: string; status: number }
export interface Location { id: number; name: string; remark: string | null; status: number }
export type MasterDataChoice = number | string | undefined
interface NamedChoice { id: number; name: string }
export interface Material {
  id: number
  name: string
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
  return withDatabaseAccess(async () =>
    (await getDatabase()).select<Location[]>('SELECT id, name, remark, status FROM locations WHERE status = 1 ORDER BY name'),
  )
}

export async function createLocation(name: string, remark = '') {
  const value = name.trim()
  const normalizedRemark = remark.trim() || null
  if (!value) throw new Error('存放位置不能为空')
  return withDatabaseMutation(async () =>
    (await getDatabase()).execute('INSERT INTO locations(name, remark, status) VALUES ($1, $2, 1)', [value, normalizedRemark]),
  )
}

export async function resolveLocationChoice(choice: MasterDataChoice, locations: Location[]) {
  return resolveMasterDataChoice(choice, locations, (name) => createLocation(name))
}

export async function listMaterials(keyword = ''): Promise<Material[]> {
  const q = `%${keyword.trim()}%`
  return withDatabaseAccess(async () =>
    (await getDatabase()).select<Material[]>(`
      SELECT m.id, m.name, m.unit_id, u.name AS unit_name, m.category,
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
  unitId?: number | null
  category?: string
  locationId?: number | null
  remark?: string
}) {
  const normalized = {
    id: input.id,
    name: input.name.trim(),
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
        AND ($2 IS NULL OR id <> $2)
      LIMIT 1
    `, [normalized.name, normalized.id ?? null])
    if (conflicts.length) {
      throw new Error(conflicts[0].status === 0
        ? '已存在同名物资，但当前处于停用状态，请直接重新启用原物资'
        : '已存在同名物资，请勿重复添加')
    }

    const timestamp = now()
    if (normalized.id) {
      return db.execute(`UPDATE materials SET name=$1, unit_id=$2, category=$3,
        default_location_id=$4, remark=$5, updated_at=$6 WHERE id=$7`, [
        normalized.name, normalized.unitId, normalized.category,
        normalized.locationId, normalized.remark, timestamp, normalized.id,
      ])
    }
    return db.execute(`INSERT INTO materials
      (name, unit_id, category, default_location_id, remark, status, created_at, updated_at)
      VALUES ($1,$2,$3,$4,$5,1,$6,$6)`, [
      normalized.name, normalized.unitId, normalized.category,
      normalized.locationId, normalized.remark, timestamp,
    ])
  })
}

export async function setMaterialStatus(id: number, status: 0 | 1) {
  const materialId = Number(id)
  const nextStatus = status
  return withDatabaseMutation(async () =>
    (await getDatabase()).execute('UPDATE materials SET status=$1, updated_at=$2 WHERE id=$3', [nextStatus, now(), materialId]),
  )
}
