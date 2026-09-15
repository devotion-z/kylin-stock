import { save } from '@tauri-apps/api/dialog'
import { writeBinaryFile } from '@tauri-apps/api/fs'
import * as XLSX from 'xlsx'
import type { InventoryRow, LedgerRow } from './inventory'
import type { Material } from './masterData'
import { formatBusinessDate, formatDateTime } from '../utils/date'

function safeDateStamp() {
  const d = new Date()
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  const hh = String(d.getHours()).padStart(2, '0')
  const mm = String(d.getMinutes()).padStart(2, '0')
  return `${y}-${m}-${day}_${hh}${mm}`
}

async function saveWorkbook(workbook: XLSX.WorkBook, defaultName: string) {
  const path = await save({
    title: '导出表格',
    defaultPath: defaultName,
    filters: [{ name: 'Excel 工作簿', extensions: ['xlsx'] }],
  })
  if (!path) return null

  const output = XLSX.write(workbook, {
    type: 'array',
    bookType: 'xlsx',
    compression: true,
  }) as ArrayBuffer
  await writeBinaryFile(path, new Uint8Array(output))
  return path
}

function setColumnWidths(sheet: XLSX.WorkSheet, widths: number[]) {
  sheet['!cols'] = widths.map((wch) => ({ wch }))
}

export async function exportMaterialRows(rows: Material[]) {
  const data = [
    ['物资名称', '条码', '计量单位', '分类', '存放位置', '备注', '状态'],
    ...rows.map((row) => [
      row.name,
      row.barcode ?? '',
      row.unit_name ?? '',
      row.category ?? '',
      row.location_name ?? '',
      row.remark ?? '',
      row.status === 1 ? '正常' : '停用',
    ]),
  ]
  const sheet = XLSX.utils.aoa_to_sheet(data)
  setColumnWidths(sheet, [22, 18, 12, 16, 20, 28, 10])
  const book = XLSX.utils.book_new()
  XLSX.utils.book_append_sheet(book, sheet, '物资明细')
  return saveWorkbook(book, `物资明细_${safeDateStamp()}.xlsx`)
}

export async function exportLedgerRows(rows: LedgerRow[]) {
  const data = [
    ['流水号', '调拨依据', '业务类型', '物资名称', '计量单位', '数量', '存放位置', '领用/来源单位', '经办人', '领用人', '业务日期', '备注'],
    ...rows.map((row) => [
      row.transaction_no,
      row.adjustment_basis ?? '',
      row.type === 'IN' ? '入库' : row.type === 'OUT' ? '出库' : '调整',
      row.material_name,
      row.unit_name ?? '',
      row.quantity,
      row.location_name,
      row.related_unit ?? '',
      row.handler ?? '',
      row.receiver ?? '',
      formatBusinessDate(row.occurred_at),
      row.remark ?? '',
    ]),
  ]
  const sheet = XLSX.utils.aoa_to_sheet(data)
  setColumnWidths(sheet, [24, 18, 10, 20, 12, 12, 18, 20, 12, 12, 20, 24])
  const book = XLSX.utils.book_new()
  XLSX.utils.book_append_sheet(book, sheet, '出入库明细')
  return saveWorkbook(book, `出入库明细_${safeDateStamp()}.xlsx`)
}

export async function exportInventoryRows(rows: InventoryRow[], includeLocation = true) {
  const data = includeLocation
    ? [
        ['物资名称', '单位', '当前库存', '存放位置', '最后更新时间'],
        ...rows.map((row) => [row.material_name, row.unit_name ?? '', row.quantity, row.location_name, formatDateTime(row.updated_at)]),
      ]
    : [
        ['物资名称', '单位', '当前库存', '最后更新时间'],
        ...rows.map((row) => [row.material_name, row.unit_name ?? '', row.quantity, formatDateTime(row.updated_at)]),
      ]
  const sheet = XLSX.utils.aoa_to_sheet(data)
  setColumnWidths(sheet, includeLocation ? [22, 10, 14, 20, 20] : [22, 10, 14, 20])
  const book = XLSX.utils.book_new()
  XLSX.utils.book_append_sheet(book, sheet, includeLocation ? '库存物资分布' : '当前库存')
  return saveWorkbook(book, `${includeLocation ? '库存物资分布' : '当前库存'}_${safeDateStamp()}.xlsx`)
}
