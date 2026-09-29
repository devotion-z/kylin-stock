import type { Material } from './masterData'
import type { ScanDocumentResult } from './inventory'
import { parseQuantityInput } from '../utils/quantity'

interface OcrWord { text: string; left: number; top: number; width: number; height: number }
interface OcrRow { words: OcrWord[]; center: number }

export interface RecognizedNoticeLine {
  name: string
  specification: string
  unit: string
  quantity: string
  materialId?: number
  locationId?: number
}

export interface RecognizedTransferNotice {
  isTransferNotice: boolean
  basis: string
  supplier: string
  receivingUnit: string
  lines: RecognizedNoticeLine[]
}

function clean(value: string) { return value.replace(/[\s:：|｜]/g, '').trim() }
function center(word: OcrWord) { return word.left + word.width / 2 }

export function parseTesseractWords(tsv: string): OcrWord[] {
  return tsv.split(/\r?\n/).slice(1).flatMap((line) => {
    const cells = line.split('\t')
    if (cells.length < 12 || cells[0] !== '5') return []
    const [left, top, width, height] = cells.slice(6, 10).map(Number)
    const text = cells.slice(11).join('\t').trim()
    return text && [left, top, width, height].every(Number.isFinite)
      ? [{ text, left, top, width, height }]
      : []
  })
}

function groupRows(words: OcrWord[]): OcrRow[] {
  const rows: OcrRow[] = []
  for (const word of [...words].sort((a, b) => a.top + a.height / 2 - b.top - b.height / 2)) {
    const y = word.top + word.height / 2
    const row = rows.find(item => Math.abs(item.center - y) <= Math.max(8, word.height * 0.65))
    if (row) {
      row.center = (row.center * row.words.length + y) / (row.words.length + 1)
      row.words.push(word)
    } else rows.push({ words: [word], center: y })
  }
  for (const row of rows) row.words.sort((a, b) => a.left - b.left)
  return rows.sort((a, b) => a.center - b.center)
}

function rowText(row: OcrRow) { return clean(row.words.map(word => word.text).join('')) }

function wordFor(row: OcrRow, keyword: string): OcrWord | undefined {
  return row.words.find(word => clean(word.text).includes(keyword))
}

function fieldValue(rows: OcrRow[], label: string, pageWidth: number): string {
  const row = rows.find(item => rowText(item).includes(label))
  if (!row) return ''
  const joined = row.words.map(word => clean(word.text)).join('')
  const start = joined.indexOf(label)
  let position = 0
  let labelWordIndex = -1
  for (let index = 0; index < row.words.length; index += 1) {
    position += clean(row.words[index].text).length
    if (position >= start + label.length) { labelWordIndex = index; break }
  }
  if (labelWordIndex < 0) return ''
  const labelWord = row.words[labelWordIndex]
  const sameWord = clean(labelWord.text).split(label)[1]
  if (sameWord) return sameWord
  const following = row.words.slice(labelWordIndex + 1)
  const first = following[0]
  if (!first || first.left - (labelWord.left + labelWord.width) > pageWidth * 0.16) return ''
  const nearby = following.filter(word => word.left - first.left < pageWidth * 0.16)
  return clean(nearby.map(word => word.text).join(''))
}

function normalizeQuantity(value: string): string {
  const candidate = clean(value).replace(/[，,]/g, '').replace(/[OoＯ]/g, '0')
  try { return String(parseQuantityInput(candidate)) } catch { return '' }
}

function matchMaterial(name: string, specification: string, unit: string, materials: Material[]) {
  const itemText = clean(name + specification).toLocaleLowerCase()
  const candidates = materials.filter(item => {
    const materialName = clean(item.name).toLocaleLowerCase()
    return materialName.length >= 2 && itemText.includes(materialName)
      && (!unit || !item.unit_name || clean(unit) === clean(item.unit_name))
  }).sort((a, b) => b.name.length - a.name.length)
  const match = candidates[0]
  return match && (candidates.length === 1 || candidates[1].name.length < match.name.length) ? match : undefined
}

export function recognizeTransferNotice(scan: ScanDocumentResult, materials: Material[]): RecognizedTransferNotice {
  const words = parseTesseractWords(scan.tsv)
  const rows = groupRows(words)
  const pageWidth = Math.max(1, ...words.map(word => word.left + word.width))
  const combinedText = clean(scan.text + rows.map(rowText).join(''))
  const isTransferNotice = combinedText.includes('调拨') && (combinedText.includes('供应单位') || combinedText.includes('接收单位'))
  const basis = fieldValue(rows, '调拨依据', pageWidth)
  const supplier = fieldValue(rows, '供应单位', pageWidth)
  const receivingUnit = fieldValue(rows, '接收单位', pageWidth)
  const header = rows.find(row => rowText(row).includes('名称') && (rowText(row).includes('规格') || rowText(row).includes('型号')))
    ?? rows.find(row => rowText(row).includes('名称') && rows.some(other =>
      Math.abs(other.center - row.center) < 25 && (rowText(other).includes('规格') || rowText(other).includes('型号'))))
  if (!header) return { isTransferNotice, basis, supplier, receivingUnit, lines: [] }

  const headerWords = rows.filter(row => Math.abs(row.center - header.center) < 25).flatMap(row => row.words)
  const tableHeader = { ...header, words: headerWords }
  const nameWord = wordFor(tableHeader, '名称')
  const specWord = wordFor(tableHeader, '规格') ?? wordFor(tableHeader, '型号')
  if (!nameWord || !specWord) return { isTransferNotice, basis, supplier, receivingUnit, lines: [] }
  const unitWord = wordFor(tableHeader, '单位')
  const serialWord = wordFor(tableHeader, '序号')
  const priceWord = wordFor(tableHeader, '单价')
  const gradeWord = wordFor(tableHeader, '等级')
  const quantityWord = headerWords.find(word => clean(word.text).includes('数量') && center(word) > center(specWord))
    ?? headerWords.find(word => clean(word.text).includes('数') && center(word) > center(specWord))
  const nameX = center(nameWord)
  const specX = center(specWord)
  const unitX = unitWord ? center(unitWord) : specX + (specX - nameX) * 0.7
  const quantityX = quantityWord ? center(quantityWord) : pageWidth * 0.51
  const gradeX = gradeWord ? center(gradeWord) : priceWord ? center(priceWord) : quantityX - pageWidth * 0.08
  const nextQuantityX = headerWords.map(center).filter(x => x > quantityX + pageWidth * 0.045).sort((a, b) => a - b)[0]
  const nameLeft = serialWord ? (center(serialWord) + nameX) / 2 : Math.max(0, nameX - (specX - nameX) * 0.6)
  const nameRight = (nameX + specX) / 2
  const specRight = (specX + unitX) / 2
  const unitRight = priceWord ? (unitX + center(priceWord)) / 2 : unitX + pageWidth * 0.03
  const quantityLeft = (gradeX + quantityX) / 2
  const quantityRight = nextQuantityX ? (quantityX + nextQuantityX) / 2 : quantityX + pageWidth * 0.045
  const footer = rows.find(row => row.center > header.center && rowText(row).includes('签章'))?.center ?? Infinity
  const lines: RecognizedNoticeLine[] = []
  for (const row of rows) {
    if (row.center <= header.center + nameWord.height * 0.8 || row.center >= footer) continue
    const inBand = (left: number, right: number) => clean(row.words.filter(word => center(word) >= left && center(word) < right).map(word => word.text).join(''))
    const name = inBand(nameLeft, nameRight)
    const specification = inBand(nameRight, specRight)
    const unit = inBand(specRight, unitRight)
    const quantity = normalizeQuantity(inBand(quantityLeft, quantityRight))
    if (!name && !specification) continue
    const material = matchMaterial(name, specification, unit, materials)
    lines.push({ name, specification, unit, quantity, materialId: material?.id, locationId: material?.default_location_id ?? undefined })
  }
  return { isTransferNotice, basis, supplier, receivingUnit, lines }
}
