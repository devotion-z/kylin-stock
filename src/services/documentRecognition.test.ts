import { describe, expect, it } from 'vitest'
import { recognizeTransferNotice } from './documentRecognition'
import type { Material } from './masterData'

function tsv(words: Array<[string, number, number, number]>) {
  const header = 'level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext'
  return [header, ...words.map(([text, left, top, width], index) =>
    `5\t1\t1\t1\t${index + 1}\t1\t${left}\t${top}\t${width}\t20\t90\t${text}`)].join('\n')
}

const materials = [
  { id: 1, name: '粉笔', unit_name: '盒', default_location_id: 8 },
  { id: 2, name: '橡皮', unit_name: '个', default_location_id: 8 },
] as Material[]

describe('recognizeTransferNotice', () => {
  it('reads the highlighted receipt fields and planned quantity column without using price or actual quantity', () => {
    const scan = {
      text: '调拨（接收）通知单\n调拨依据 2026年计划\n供应单位 仓晖\n接收单位 超市',
      tsv: tsv([
        ['调拨依据', 535, 250, 58], ['2026年计划', 600, 250, 115],
        ['供应单位', 5, 285, 55], ['仓晖', 68, 285, 43],
        ['接收单位', 5, 320, 55], ['超市', 70, 320, 42],
        ['序号', 5, 400, 30], ['名称', 75, 400, 35], ['规格型号', 205, 400, 78],
        ['单位', 330, 400, 35], ['单价', 370, 400, 40], ['等级', 425, 400, 35],
        ['数量', 480, 400, 35], ['等级', 545, 400, 35], ['数量', 600, 400, 35],
        ['1', 10, 440, 10], ['粉笔', 75, 440, 42], ['10.9型粉笔', 205, 440, 105],
        ['盒', 335, 440, 18], ['5', 380, 440, 12], ['新品', 427, 440, 40],
        ['1000', 486, 440, 42], ['900', 605, 440, 34],
        ['2', 10, 480, 10], ['欙皮', 75, 480, 42], ['20型橡皮', 205, 480, 100],
        ['个', 335, 480, 18], ['10', 380, 480, 20], ['新品', 427, 480, 40],
        ['1000', 486, 480, 42], ['950', 605, 480, 34],
        ['供应单位（签章）', 330, 750, 140],
      ]),
    }
    const result = recognizeTransferNotice(scan, materials)
    expect(result).toMatchObject({ isTransferNotice: true, basis: '2026年计划', supplier: '仓库', receivingUnit: '超市' })
    expect(result.corrections).toEqual(['供应单位：识别为“仓晖”，已建议改为“仓库”，请对照原单核对'])
    expect(result.lines).toEqual([
      { name: '粉笔', specification: '10.9型粉笔', unit: '盒', quantity: '1000', materialId: 1, locationId: 8 },
      { name: '欙皮', specification: '20型橡皮', unit: '个', quantity: '1000', materialId: 2, locationId: 8 },
    ])
  })

  it('leaves unreadable quantities and unmatched materials for human correction', () => {
    const scan = { text: '调拨接收通知单 供应单位 仓库', tsv: tsv([
      ['名称', 75, 400, 35], ['规格型号', 205, 400, 78], ['单位', 330, 400, 35], ['数量', 480, 400, 35],
      ['未知物资', 75, 440, 70], ['型号X', 205, 440, 58], ['盒', 335, 440, 18], ['O0O?', 486, 440, 52],
    ]) }
    const result = recognizeTransferNotice(scan, materials)
    expect(result.lines).toHaveLength(1)
    expect(result.lines[0].materialId).toBeUndefined()
    expect(result.lines[0].quantity).toBe('')
  })

  it('only suggests a known unit when one close match exists', () => {
    const scan = { text: '调拨接收通知单', tsv: tsv([['供应单位', 5, 285, 55], ['超巿', 68, 285, 43]]) }
    const unique = recognizeTransferNotice(scan, materials, ['超市'])
    expect(unique.supplier).toBe('超市')
    expect(unique.corrections).toHaveLength(1)
    const ambiguous = recognizeTransferNotice(scan, materials, ['超市', '超巷'])
    expect(ambiguous.supplier).toBe('超巿')
    expect(ambiguous.corrections).toHaveLength(0)
  })
})
