import { getDatabase, withDatabaseRead } from './database'
import { localDayIsoRange } from '../utils/date'

export interface DashboardStats {
  materialCount: number
  stockedMaterialCount: number
  todayInCount: number
  todayOutCount: number
}

export interface RecentTransaction {
  id: number
  type: 'IN' | 'OUT' | 'ADJUST'
  material_name: string
  quantity: number
  unit_name: string | null
  location_name: string
  destination: string | null
  occurred_at: string
}

export interface StockOverviewRow {
  material_name: string
  unit_name: string | null
  quantity: number
}

export interface CategorySummary {
  category_name: string
  material_count: number
  stocked_count: number
}

export interface CategoryInventoryRow {
  material_id: number
  material_name: string
  unit_name: string | null
  location_name: string
  quantity: number
  updated_at: string
}

export async function listCategoryInventory(category: string): Promise<CategoryInventoryRow[]> {
  const value = category.trim() || '未分类'
  return withDatabaseRead(async () =>
    (await getDatabase()).select<CategoryInventoryRow[]>(`
      SELECT m.id AS material_id,m.name AS material_name,u.name AS unit_name,
             COALESCE(l.name,dl.name,'未设置') AS location_name,
             COALESCE(SUM(b.quantity),0) AS quantity,MAX(b.updated_at) AS updated_at
      FROM materials m
      LEFT JOIN units u ON u.id=m.unit_id
      LEFT JOIN inventory_balances b ON b.material_id=m.id AND b.quantity <> 0
      LEFT JOIN locations l ON l.id=b.location_id
      LEFT JOIN locations dl ON dl.id=m.default_location_id
      WHERE m.status=1
        AND COALESCE(NULLIF(TRIM(m.category),''),'未分类')=$1
      GROUP BY m.id,m.name,u.name,l.name,dl.name
      ORDER BY m.name COLLATE NOCASE,COALESCE(l.name,dl.name,'') COLLATE NOCASE`, [value]),
  )
}

export async function loadDashboard() {
  return withDatabaseRead(async () => {
    const db = await getDatabase()
    const { start, end } = localDayIsoRange()

    const [materialRows, stockedRows, inRows, outRows, categories, recent, overview] = await Promise.all([
      db.select<{ value: number }[]>(`SELECT COUNT(*) AS value FROM materials WHERE status=1`),
      db.select<{ value: number }[]>(`
        SELECT COUNT(DISTINCT material_id) AS value
        FROM inventory_balances
        WHERE quantity > 0
      `),
      db.select<{ value: number }[]>(`
        SELECT COUNT(*) AS value FROM stock_transactions
        WHERE type='IN' AND occurred_at >= $1 AND occurred_at <= $2
      `, [start, end]),
      db.select<{ value: number }[]>(`
        SELECT COUNT(*) AS value FROM stock_transactions
        WHERE type='OUT' AND occurred_at >= $1 AND occurred_at <= $2
      `, [start, end]),
      db.select<CategorySummary[]>(`
        SELECT COALESCE(NULLIF(TRIM(m.category),''),'未分类') AS category_name,
               COUNT(*) AS material_count,
               COUNT(DISTINCT CASE WHEN EXISTS (
                 SELECT 1 FROM inventory_balances b2
                 WHERE b2.material_id=m.id AND b2.quantity > 0
               ) THEN m.id END) AS stocked_count
        FROM materials m
        WHERE m.status=1
        GROUP BY COALESCE(NULLIF(TRIM(m.category),''),'未分类')
        ORDER BY category_name COLLATE NOCASE
      `),
      db.select<RecentTransaction[]>(`
        SELECT t.id,t.type,m.name AS material_name,t.quantity,u.name AS unit_name,
               l.name AS location_name,t.destination,t.occurred_at
        FROM stock_transactions t
        JOIN materials m ON m.id=t.material_id
        LEFT JOIN units u ON u.id=m.unit_id
        JOIN locations l ON l.id=t.location_id
        ORDER BY t.occurred_at DESC,t.id DESC
        LIMIT 8
      `),
      db.select<StockOverviewRow[]>(`
        SELECT m.name AS material_name,u.name AS unit_name,SUM(b.quantity) AS quantity
        FROM inventory_balances b
        JOIN materials m ON m.id=b.material_id
        LEFT JOIN units u ON u.id=m.unit_id
        GROUP BY b.material_id,m.name,u.name
        HAVING SUM(b.quantity) > 0
        ORDER BY m.name
        LIMIT 8
      `),
    ])

    const stats: DashboardStats = {
      materialCount: Number(materialRows[0]?.value ?? 0),
      stockedMaterialCount: Number(stockedRows[0]?.value ?? 0),
      todayInCount: Number(inRows[0]?.value ?? 0),
      todayOutCount: Number(outRows[0]?.value ?? 0),
    }

    return { stats, categories, recent, overview }
  })
}
