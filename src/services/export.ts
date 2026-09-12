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
    ['物资名称', '计量单位', '分类', '存放位置', '备注', '状态'],
    ...rows.map((row) => [
      row.name,
      row.unit_name ?? '',
      row.category ?? '',
      row.location_name ?? '',
      row.remark ?? '',
      row.status === 1 ? '正常' : '停用',
    ]),
  ]
  const sheet = XLSX.utils.aoa_to_sheet(data)
  setColumnWidths(sheet, [22, 12, 16, 20, 28, 10])
  const book = XLSX.utils.book_new()
  XLSX.utils.book_append_sheet(book, sheet, '物资明细')
  return saveWorkbook(book, `物资明细_${safeDateStamp()}.xlsx`)
}

export async function exportLedgerRows(rows: LedgerRow[]) {
  const data = [
    ['流水号', '业务类型', '物资名称', '数量', '单位', '存放位置', '相关单位', '出库去向', '经办人', '领用人', '业务日期', '备注'],
    ...rows.map((row) => [
      row.transaction_no,
      row.type === 'IN' ? '入库' : row.type === 'OUT' ? '出库' : '调整',
      row.material_name,
      row.quantity,
      row.unit_name ?? '',
      row.location_name,
      row.related_unit ?? '',
      row.destination ?? '',
      row.handler ?? '',
      row.receiver ?? '',
      formatBusinessDate(row.occurred_at),
      row.remark ?? '',
    ]),
  ]
  const sheet = XLSX.utils.aoa_to_sheet(data)
  setColumnWidths(sheet, [24, 10, 20, 12, 10, 18, 18, 20, 12, 12, 20, 24])
  const book = XLSX.utils.book_new()
  XLSX.utils.book_append_sheet(book, sheet, '出入库明细')
  return saveWorkbook(book, `出入库明细_${safeDateStamp()}.xlsx`)
}

export async function exportInventoryRows(rows: InventoryRow[]) {
  const data = [
    ['物资名称', '单位', '存放位置', '当前库存', '最后更新时间'],
    ...rows.map((row) => [
      row.material_name,
      row.unit_name ?? '',
      row.location_name,
      row.quantity,
      formatDateTime(row.updated_at),
    ]),
  ]
  const sheet = XLSX.utils.aoa_to_sheet(data)
  setColumnWidths(sheet, [22, 10, 20, 14, 20])
  const book = XLSX.utils.book_new()
  XLSX.utils.book_append_sheet(book, sheet, '库存物资分布')
  return saveWorkbook(book, `库存物资分布_${safeDateStamp()}.xlsx`)
}
