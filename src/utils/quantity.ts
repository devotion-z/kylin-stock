export function parseQuantityInput(value: string): number {
  const normalized = value.trim()
  if (!/^\d+(?:\.\d{1,2})?$/.test(normalized)) {
    throw new Error('数量只能输入正数，最多两位小数；系统不会自动四舍五入')
  }
  const quantity = Number(normalized)
  if (!Number.isFinite(quantity) || quantity <= 0) throw new Error('数量必须大于 0')
  return quantity
}
