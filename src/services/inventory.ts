import { invoke } from '@tauri-apps/api/tauri'
import { getDatabase, getDatabaseRevision, withDatabaseMutation, withDatabaseRead } from './database'

export interface InventoryRow {
  material_id: number
  material_name: string
  unit_name: string | null
  location_id: number
  location_name: string
  quantity: number
  updated_at: string
}

export function indexInventory(rows: InventoryRow[]) {
  const byMaterial = new Map<number, InventoryRow[]>()
  const totals = new Map<number, number>()
  const quantities = new Map<string, number>()
  for (const row of rows) {
    const materialId = Number(row.material_id)
    const key = `${materialId}:${Number(row.location_id)}`
    const quantity = Number(row.quantity)
    const positions = byMaterial.get(materialId) ?? []
    positions.push(row)
    byMaterial.set(materialId, positions)
    totals.set(materialId, (totals.get(materialId) ?? 0) + quantity)
    quantities.set(key, (quantities.get(key) ?? 0) + quantity)
  }
  return { byMaterial, totals, quantities }
}

export interface LedgerRow {
  id: number
  transaction_no: string
  type: 'IN' | 'OUT' | 'ADJUST'
  material_id: number
  material_name: string
  unit_name: string | null
  location_id: number
  location_name: string
  quantity: number
  occurred_at: string
  related_unit: string | null
  destination: string | null
  handler: string | null
  receiver: string | null
  remark: string | null
  adjustment_basis: string | null
  attachment_count: number
}

export interface InventoryFilters {
  keyword?: string
  unit?: string
  location?: string
  locationId?: number
  summary?: boolean
}

export interface InventoryPage {
  rows: InventoryRow[]
  total: number
  page: number
  pageSize: number
}

export interface LedgerFilters {
  basis?: string
  materialId?: number
  type?: string
  relatedUnit?: string
  destination?: string
  startAt?: string
  endAt?: string
}

export interface LedgerPage {
  rows: LedgerRow[]
  total: number
  page: number
  pageSize: number
}

export interface StockOperationInput {
  materialId: number
  locationId: number
  quantity: number
  occurredAt: string
  relatedUnit?: string
  destination?: string
  handler?: string
  receiver?: string
  remark?: string
  adjustmentBasis?: string
}

export interface StockTransferInput {
  materialId: number
  fromLocationId: number
  toLocationId: number
  quantity: number
  occurredAt: string
  handler?: string
  remark?: string
  adjustmentBasis?: string
}

export interface StockTransactionUpdateInput extends StockOperationInput {
  id: number
}

function snapshotStockInput(input: StockOperationInput): StockOperationInput {
  return {
    materialId: Number(input.materialId),
    locationId: Number(input.locationId),
    quantity: Number(input.quantity),
    occurredAt: String(input.occurredAt),
    relatedUnit: input.relatedUnit,
    destination: input.destination,
    handler: input.handler,
    receiver: input.receiver,
    remark: input.remark,
    adjustmentBasis: input.adjustmentBasis,
  }
}

function validate(input: StockOperationInput) {
  if (!input.materialId) throw new Error('请选择物资')
  if (!input.locationId) throw new Error('请选择存放位置')
  if (!Number.isFinite(input.quantity) || input.quantity <= 0) throw new Error('数量必须大于 0')
  if (!input.occurredAt) throw new Error('请选择业务日期')
}

export async function stockIn(input: StockOperationInput) {
  const payload = snapshotStockInput(input)
  validate(payload)
  return withDatabaseMutation(() => invoke<string>('stock_in', { input: payload }))
}

export async function stockOut(input: StockOperationInput) {
  const payload = snapshotStockInput(input)
  validate(payload)
  if (!payload.destination?.trim()) throw new Error('出库去向不能为空')
  return withDatabaseMutation(() => invoke<string>('stock_out', { input: payload }))
}

export async function stockInBatch(inputs: StockOperationInput[]) {
  const payload = inputs.map(snapshotStockInput)
  payload.forEach(validate)
  return withDatabaseMutation(() => invoke<string[]>('stock_in_batch', { inputs: payload }))
}

export async function stockOutBatch(inputs: StockOperationInput[]) {
  const payload = inputs.map(snapshotStockInput)
  payload.forEach(validate)
  if (payload.some((item) => !item.destination?.trim())) throw new Error('领用单位不能为空')
  return withDatabaseMutation(() => invoke<string[]>('stock_out_batch', { inputs: payload }))
}

export function deleteStockTransaction(id: number) {
  if (!Number.isInteger(id) || id <= 0) throw new Error('无效的流水记录')
  return withDatabaseMutation(() => invoke<void>('delete_stock_transaction', { id }))
}

export function updateStockTransaction(input: StockTransactionUpdateInput) {
  const payload = { ...snapshotStockInput(input), id: Number(input.id) }
  if (!Number.isInteger(payload.id) || payload.id <= 0) throw new Error('无效的流水记录')
  validate(payload)
  return withDatabaseMutation(() => invoke<void>('update_stock_transaction', { input: payload }))
}

export function transferStock(input: StockTransferInput) {
  if (!input.materialId || !input.fromLocationId || !input.toLocationId) throw new Error('请选择物资和库位')
  if (input.fromLocationId === input.toLocationId) throw new Error('转入库位不能与原库位相同')
  if (!Number.isFinite(input.quantity) || input.quantity <= 0) throw new Error('转移数量必须大于 0')
  return withDatabaseMutation(() => invoke<void>('transfer_stock', { input }))
}

export function deleteInventoryPosition(materialId: number, locationId: number) {
  if (!materialId || !locationId) throw new Error('无效的物资或存放位置')
  return withDatabaseMutation(() => invoke<void>('delete_inventory_position', { materialId, locationId }))
}

export function scanDocument(sourcePath: string) {
  return invoke<string>('scan_document', { sourcePath })
}

export function sortInventoryByLocation(rows: InventoryRow[]) {
  const collator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' })
  return rows.sort((left, right) =>
    collator.compare(left.location_name, right.location_name)
    || collator.compare(left.material_name, right.material_name))
}

export function buildInventoryWhere(filters: InventoryFilters, summary = false) {
  const clauses: string[] = []
  const values: unknown[] = []
  const add = (sql: string, value: unknown) => {
    clauses.push(sql)
    values.push(value)
  }
  if (filters.keyword?.trim()) add('m.name LIKE ?', `%${filters.keyword.trim()}%`)
  if (filters.unit?.trim()) add("COALESCE(u.name,'') LIKE ?", `%${filters.unit.trim()}%`)
  if (!summary && Number(filters.locationId) > 0) add('b.location_id=?', Number(filters.locationId))
  else if (!summary && filters.location?.trim()) add('l.name LIKE ?', `%${filters.location.trim()}%`)
  return { sql: clauses.length ? `AND ${clauses.join(' AND ')}` : '', values }
}

const distributionColumnsSql = `
  SELECT b.material_id, m.name AS material_name, u.name AS unit_name,
         b.location_id, l.name AS location_name, CAST(b.quantity AS REAL) AS quantity, b.updated_at`
const distributionFromSql = `
  FROM inventory_balances b
  JOIN materials m ON m.id=b.material_id
  LEFT JOIN units u ON u.id=m.unit_id
  JOIN locations l ON l.id=b.location_id`
const distributionOrderSql = `
  ORDER BY CASE WHEN l.name GLOB '[0-9]*' THEN CAST(l.name AS INTEGER) ELSE 2147483647 END,
           l.name COLLATE NOCASE, m.name COLLATE NOCASE`
const inventoryPageCache = new Map<string, InventoryPage>()
let inventoryPageCacheRevision = -1

export async function getTransactionIdByNo(transactionNo: string): Promise<number> {
  return withDatabaseRead(async () => {
    const rows = await (await getDatabase()).select<{ id: number }[]>(
      'SELECT id FROM stock_transactions WHERE transaction_no=$1 LIMIT 1',
      [transactionNo],
    )
    if (!rows.length) throw new Error('找不到刚保存的出入库记录')
    return rows[0].id
  })
}

export async function listInventory(filters: InventoryFilters | string = {}): Promise<InventoryRow[]> {
  const normalized: InventoryFilters = typeof filters === 'string' ? { keyword: filters } : filters

  return withDatabaseRead(async () => {
    const db = await getDatabase()
    if (normalized.summary) {
      const where = buildInventoryWhere(normalized, true)
      return db.select<InventoryRow[]>(`
        SELECT MIN(b.material_id) AS material_id, m.name AS material_name, u.name AS unit_name,
               0 AS location_id, '' AS location_name,
               CAST(SUM(b.quantity) AS REAL) AS quantity, MAX(b.updated_at) AS updated_at
        FROM inventory_balances b
        JOIN materials m ON m.id=b.material_id
        LEFT JOIN units u ON u.id=m.unit_id
        WHERE 1=1 ${where.sql}
        GROUP BY m.name COLLATE NOCASE, COALESCE(u.name,'') COLLATE NOCASE
        HAVING SUM(b.quantity) <> 0
        ORDER BY m.name COLLATE NOCASE, COALESCE(u.name,'') COLLATE NOCASE`, where.values)
    }

    const where = buildInventoryWhere(normalized)
    const rows = await db.select<InventoryRow[]>(`${distributionColumnsSql}
      ${distributionFromSql}
      WHERE b.quantity<>0 ${where.sql}
      ${distributionOrderSql}`, where.values)

    return sortInventoryByLocation(rows)
  })
}

export async function listInventoryPage(
  filters: InventoryFilters = {},
  page = 1,
  pageSize = 100,
  knownTotal?: number,
): Promise<InventoryPage> {
  const safePage = Math.max(1, Math.trunc(page) || 1)
  const safePageSize = [100, 200, 500].includes(pageSize) ? pageSize : 100
  return withDatabaseRead(async () => {
    const revision = getDatabaseRevision()
    if (revision !== inventoryPageCacheRevision) {
      inventoryPageCache.clear()
      inventoryPageCacheRevision = revision
    }
    const input = {
      keyword: filters.keyword?.trim() || undefined,
      unit: filters.unit?.trim() || undefined,
      locationId: Number(filters.locationId) > 0 ? Number(filters.locationId) : undefined,
      page: safePage,
      pageSize: safePageSize,
      knownTotal,
    }
    const key = JSON.stringify({ ...input, knownTotal: undefined })
    const cached = inventoryPageCache.get(key)
    if (cached) return cached

    const result = await invoke<InventoryPage>('list_inventory_page', { input })
    if (revision === getDatabaseRevision()) {
      if (inventoryPageCache.size >= 30) inventoryPageCache.delete(inventoryPageCache.keys().next().value!)
      inventoryPageCache.set(key, result)
    }
    return result
  })
}

export async function preloadInventoryDistribution() {
  await listInventoryPage({}, 1, 100)
}

export function buildLedgerWhere(filters: LedgerFilters = {}) {
  const clauses: string[] = []
  const values: unknown[] = []
  const add = (sql: string, value: unknown) => {
    clauses.push(sql)
    values.push(value)
  }

  if (Number(filters.materialId) > 0) add('t.material_id=?', Number(filters.materialId))
  if (filters.basis?.trim()) add("COALESCE(t.adjustment_basis,'') LIKE ?", `%${filters.basis.trim()}%`)
  if (filters.type && filters.type !== 'ALL') add('t.type=?', filters.type)
  if (filters.relatedUnit?.trim()) add("COALESCE(t.related_unit,'') LIKE ?", `%${filters.relatedUnit.trim()}%`)
  if (filters.destination?.trim()) add("COALESCE(t.destination,'') LIKE ?", `%${filters.destination.trim()}%`)
  if (filters.startAt) add('t.occurred_at>=?', filters.startAt)
  if (filters.endAt) add('t.occurred_at<=?', filters.endAt)

  return { sql: clauses.length ? `WHERE ${clauses.join(' AND ')}` : '', values }
}

const ledgerFromSql = `
  FROM stock_transactions t
  JOIN materials m ON m.id=t.material_id
  LEFT JOIN units u ON u.id=m.unit_id
  JOIN locations l ON l.id=t.location_id`

const ledgerColumnsSql = `
  SELECT t.id,t.transaction_no,t.type,t.material_id,m.name AS material_name,u.name AS unit_name,
         t.location_id,l.name AS location_name,t.quantity,t.occurred_at,t.related_unit,t.destination,
         t.handler,t.receiver,t.remark,t.adjustment_basis`

export async function listLedgerPage(
  filters: LedgerFilters = {},
  page = 1,
  pageSize = 50,
  knownTotal?: number,
): Promise<LedgerPage> {
  const safePage = Math.max(1, Math.trunc(page) || 1)
  const safePageSize = [50, 100, 200].includes(pageSize) ? pageSize : 50
  const where = buildLedgerWhere(filters)
  const offset = (safePage - 1) * safePageSize

  return withDatabaseRead(async () => {
    const db = await getDatabase()
    const rowQuery = `${ledgerColumnsSql},
      (SELECT COUNT(*) FROM attachments a WHERE a.entity_type='TRANSACTION' AND a.entity_id=t.id) AS attachment_count
      ${ledgerFromSql} ${where.sql}
      ORDER BY t.occurred_at DESC,t.id DESC LIMIT ? OFFSET ?`
    if (knownTotal !== undefined) {
      const rows = await db.select<LedgerRow[]>(rowQuery, [...where.values, safePageSize, offset])
      return { rows, total: knownTotal, page: safePage, pageSize: safePageSize }
    }
    const [rows, counts] = await Promise.all([
      db.select<LedgerRow[]>(rowQuery, [...where.values, safePageSize, offset]),
      db.select<{ total: number }[]>(`SELECT COUNT(*) AS total FROM stock_transactions t ${where.sql}`, where.values),
    ])
    return { rows, total: Number(counts[0]?.total ?? 0), page: safePage, pageSize: safePageSize }
  })
}

export async function listLedger(filters: LedgerFilters = {}): Promise<LedgerRow[]> {
  const where = buildLedgerWhere(filters)
  return withDatabaseRead(async () =>
    (await getDatabase()).select<LedgerRow[]>(`${ledgerColumnsSql}, 0 AS attachment_count
      ${ledgerFromSql} ${where.sql}
      ORDER BY t.occurred_at DESC,t.id DESC`, where.values),
  )
}
