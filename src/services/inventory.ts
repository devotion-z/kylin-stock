import { invoke } from '@tauri-apps/api/tauri'
import { getDatabase, withDatabaseAccess, withDatabaseMutation } from './database'

export interface InventoryRow {
  material_id: number
  material_name: string
  unit_name: string | null
  location_id: number
  location_name: string
  quantity: number
  updated_at: string
}

export interface LedgerRow {
  id: number
  transaction_no: string
  type: 'IN' | 'OUT' | 'ADJUST'
  material_name: string
  unit_name: string | null
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
}

export interface LedgerFilters {
  basis?: string
  material?: string
  type?: string
  relatedUnit?: string
  destination?: string
  startAt?: string
  endAt?: string
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

export function scanDocument(sourcePath: string) {
  return invoke<string>('scan_document', { sourcePath })
}

export async function getTransactionIdByNo(transactionNo: string): Promise<number> {
  return withDatabaseAccess(async () => {
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
  const keyword = `%${(normalized.keyword ?? '').trim()}%`
  const unit = `%${(normalized.unit ?? '').trim()}%`
  const location = `%${(normalized.location ?? '').trim()}%`

  return withDatabaseAccess(async () =>
    (await getDatabase()).select<InventoryRow[]>(`
      SELECT b.material_id, m.name AS material_name, u.name AS unit_name,
             b.location_id, l.name AS location_name, b.quantity, b.updated_at
      FROM inventory_balances b
      JOIN materials m ON m.id=b.material_id
      LEFT JOIN units u ON u.id=m.unit_id
      JOIN locations l ON l.id=b.location_id
      WHERE b.quantity <> 0
        AND ($1='%%' OR m.name LIKE $1)
        AND ($2='%%' OR COALESCE(u.name,'') LIKE $2)
        AND ($3='%%' OR l.name LIKE $3)
      ORDER BY COALESCE(m.category, '') COLLATE NOCASE, l.name COLLATE NOCASE, m.name COLLATE NOCASE`, [keyword, unit, location]),
  )
}

export async function listLedger(filters: LedgerFilters = {}): Promise<LedgerRow[]> {
  const material = `%${(filters.material ?? '').trim()}%`
  const basis = `%${(filters.basis ?? '').trim()}%`
  const relatedUnit = `%${(filters.relatedUnit ?? '').trim()}%`
  const destination = `%${(filters.destination ?? '').trim()}%`

  return withDatabaseAccess(async () =>
    (await getDatabase()).select<LedgerRow[]>(`
      SELECT t.id,t.transaction_no,t.type,m.name AS material_name,u.name AS unit_name,
             l.name AS location_name,t.quantity,t.occurred_at,t.related_unit,t.destination,
             t.handler,t.receiver,t.remark,t.adjustment_basis,
             (SELECT COUNT(*) FROM attachments a WHERE a.entity_type='TRANSACTION' AND a.entity_id=t.id) AS attachment_count
      FROM stock_transactions t
      JOIN materials m ON m.id=t.material_id
      LEFT JOIN units u ON u.id=m.unit_id
      JOIN locations l ON l.id=t.location_id
      WHERE ($1='%%' OR m.name LIKE $1)
        AND ($2='%%' OR COALESCE(t.adjustment_basis,'') LIKE $2)
        AND ($3='' OR t.type=$3)
        AND ($4='%%' OR COALESCE(t.related_unit,'') LIKE $4)
        AND ($5='%%' OR COALESCE(t.destination,'') LIKE $5)
        AND ($6='' OR t.occurred_at >= $6)
        AND ($7='' OR t.occurred_at <= $7)
      ORDER BY t.occurred_at DESC,t.id DESC`, [
        material,
        basis,
        filters.type === 'ALL' ? '' : filters.type ?? '',
        relatedUnit,
        destination,
        filters.startAt ?? '',
        filters.endAt ?? '',
      ]),
  )
}
