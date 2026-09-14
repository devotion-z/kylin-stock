<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { ElMessage } from 'element-plus'
import { listLocations, listMaterials, type Location, type Material } from '../services/masterData'
import { getTransactionIdByNo, scanDocument, stockInBatch, stockOutBatch } from '../services/inventory'
import { toLocalDateValue } from '../utils/date'
import AttachmentField from '../components/AttachmentField.vue'
import { addAttachment, chooseAttachmentImages } from '../services/attachments'
import { ensureBusinessOption, listBusinessOptions, type BusinessOption } from '../services/businessOptions'
import { parseQuantityInput } from '../utils/quantity'

const route = useRoute()
const isOut = computed(() => route.path === '/stock-out')
const submitting = ref(false)
const loading = ref(false)
const materials = ref<Material[]>([])
const locations = ref<Location[]>([])
const pendingAttachments = ref<string[]>([])
const relatedUnitOptions = ref<BusinessOption[]>([])
const scanCode = ref('')
const scanTextPreview = ref('')
const scanMatchedCount = ref(0)
const scanInput = ref<{ focus: () => void }>()
interface OperationLine { materialId?: number; locationId?: number; quantity: string }
const lines = ref<OperationLine[]>([{ quantity: '1' }])
const form = reactive({ occurredAt: toLocalDateValue(), adjustmentBasis: '', relatedUnit: '', handler: '', receiver: '', remark: '' })

async function load() {
  loading.value = true
  try {
    ;[materials.value, locations.value, relatedUnitOptions.value] = await Promise.all([
      listMaterials(), listLocations(), listBusinessOptions('RELATED_UNIT'),
    ])
    materials.value = materials.value.filter((item) => item.status === 1)
  } catch (e) {
    ElMessage.error(`基础资料加载失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    loading.value = false
  }
}

function onMaterialChange(index: number, id: number) {
  const material = materials.value.find((item) => item.id === id)
  if (material?.default_location_id) lines.value[index].locationId = material.default_location_id
}

function materialLabel(item: Material) {
  const location = item.default_location_id ? locations.value.find((entry) => entry.id === item.default_location_id)?.name : ''
  const details = [item.unit_name, location].filter(Boolean).join(' · ')
  return details ? `${item.name}（${details}）` : item.name
}

function addLine() { lines.value.push({ quantity: '1' }) }
function removeLine(index: number) { if (lines.value.length > 1) lines.value.splice(index, 1) }

function handleScan() {
  const code = scanCode.value.trim()
  if (!code) return
  const material = materials.value.find((item) => item.barcode?.trim() === code)
  if (!material) {
    ElMessage.warning(`未找到条码为“${code}”的启用物资，请先在物资管理中维护条码`)
  } else {
    const index = lines.value.length - 1
    lines.value[index].materialId = material.id
    onMaterialChange(index, material.id)
    ElMessage.success(`已扫描：${material.name}`)
  }
  scanCode.value = ''
  scanInput.value?.focus()
}

async function importScannedDocument() {
  try {
    const selected = await chooseAttachmentImages()
    if (!selected.length) return
    const text = await scanDocument(selected[0])
    scanTextPreview.value = text.trim()
    const detected: OperationLine[] = []
    const textLines = text.split(/\r?\n/).map((line) => line.trim()).filter(Boolean)
    for (const material of materials.value) {
      const matched = textLines.find((line) => line.includes(material.name))
      if (!matched) continue
      const numbers = matched.match(/\d+(?:[.,]\d+)?/g) ?? []
      const rawQuantity = numbers.length ? numbers[numbers.length - 1].replace(',', '.') : '1'
      const quantity = rawQuantity.replace(/\.0+$/, '').replace(/(\.\d*?[1-9])0+$/, '$1')
      detected.push({ materialId: material.id, locationId: material.default_location_id ?? undefined, quantity })
    }
    scanMatchedCount.value = detected.length
    pendingAttachments.value = [...new Set([...pendingAttachments.value, selected[0]])].slice(0, 10)
    if (detected.length) {
      lines.value = detected
      ElMessage.success(`单据识别完成，匹配到 ${detected.length} 项物资，请核对数量后确认`)
    } else {
      ElMessage.warning('单据图片已添加，但未匹配到物资名称，请检查物资名称或手动补充明细')
    }
  } catch (e) { ElMessage.error(e instanceof Error ? e.message : String(e)) }
}

function reset() {
  if (submitting.value) return
  Object.assign(form, { occurredAt: toLocalDateValue(), adjustmentBasis: '', relatedUnit: '', handler: '', receiver: '', remark: '' })
  lines.value = [{ quantity: '1' }]
  pendingAttachments.value = []
  scanCode.value = ''
  scanTextPreview.value = ''
  scanMatchedCount.value = 0
}

async function submit() {
  // Loading state alone is not a correctness guard: two click events can enter
  // this function before Vue has rendered the disabled/loading button state.
  if (submitting.value) return
  if (!lines.value.length) return ElMessage.warning('请至少添加一项物资')
  if (!form.occurredAt) return ElMessage.warning('请选择业务日期')
  const parsedLines = [] as Array<{ materialId: number; locationId: number; quantity: number }>
  for (let index = 0; index < lines.value.length; index += 1) {
    const line = lines.value[index]
    if (!line.materialId) return ElMessage.warning(`第 ${index + 1} 行请选择物资`)
    if (!line.locationId) return ElMessage.warning(`第 ${index + 1} 行请选择存放位置`)
    try { parsedLines.push({ materialId: line.materialId, locationId: line.locationId, quantity: parseQuantityInput(line.quantity) }) }
    catch (e) { return ElMessage.warning(`第 ${index + 1} 行：${e instanceof Error ? e.message : String(e)}`) }
  }
  if (isOut.value && !form.relatedUnit.trim()) return ElMessage.warning('请填写领用单位')

  submitting.value = true
  try {
    const relatedUnit = await ensureBusinessOption('RELATED_UNIT', form.relatedUnit)
    const destination = isOut.value ? form.relatedUnit.trim() : ''
    const payload = parsedLines.map((line) => ({ ...line, occurredAt: `${form.occurredAt}T00:00:00.000Z`, adjustmentBasis: form.adjustmentBasis, relatedUnit, destination, handler: form.handler, receiver: form.receiver, remark: form.remark }))
    const transactionNos = isOut.value ? await stockOutBatch(payload) : await stockInBatch(payload)
    let attachmentWarning = ''
    if (pendingAttachments.value.length) {
      const transactionId = await getTransactionIdByNo(transactionNos[0])
      try {
        while (pendingAttachments.value.length) {
          await addAttachment('TRANSACTION', transactionId, pendingAttachments.value[0])
          pendingAttachments.value.shift()
        }
      } catch (e) {
        attachmentWarning = e instanceof Error ? e.message : String(e)
      }
    }
    if (attachmentWarning) ElMessage.warning(`登记已成功，但有单据图片未保存：${attachmentWarning}`)
    else ElMessage.success(isOut.value ? '出库登记成功' : '入库登记成功')
    Object.assign(form, { occurredAt: toLocalDateValue(), adjustmentBasis: '', relatedUnit: '', handler: '', receiver: '', remark: '' })
    lines.value = [{ quantity: '1' }]
    pendingAttachments.value = []
    scanCode.value = ''
    scanTextPreview.value = ''
    scanMatchedCount.value = 0
    relatedUnitOptions.value = await listBusinessOptions('RELATED_UNIT')
  } catch (e) { ElMessage.error(e instanceof Error ? e.message : String(e)) }
  finally { submitting.value = false }
}

watch(() => route.path, () => {
  if (!submitting.value) reset()
})
onMounted(load)
</script>

<template>
  <el-card v-loading="loading" shadow="never" class="operation-card">
    <template #header><strong>{{ isOut ? '出库登记' : '入库登记' }}</strong></template>
    <el-alert v-if="!materials.length && !loading" title="还没有可用物资，请先到“物资管理”新增并启用物资。" type="warning" :closable="false" show-icon style="margin-bottom:18px" />
    <el-form label-width="110px" style="max-width: 720px" :disabled="submitting">
      <el-form-item label="扫描条码">
        <el-input ref="scanInput" v-model="scanCode" clearable placeholder="使用扫码枪扫描后自动选择物资，或手动输入条码并回车" @keyup.enter="handleScan">
          <template #append><el-button :disabled="!scanCode.trim()" @click="handleScan">识别</el-button></template>
        </el-input>
      </el-form-item>
      <el-form-item label="扫描单据">
        <div class="scan-box">
          <div><el-button plain :disabled="submitting" @click="importScannedDocument">选择扫描单据并识别</el-button><span class="scan-hint">支持扫描图片；识别后自动填充物资和数量，提交前必须人工核对</span></div>
          <el-alert v-if="scanTextPreview" :title="`已识别 ${scanMatchedCount} 项物资，原始文字仅供核对`" type="success" :closable="false" show-icon />
          <el-input v-if="scanTextPreview" v-model="scanTextPreview" type="textarea" :rows="4" readonly class="scan-preview" />
        </div>
      </el-form-item>
      <el-form-item :label="isOut ? '出库物资明细' : '入库物资明细'" required>
        <div class="line-list">
          <div v-for="(line, index) in lines" :key="index" class="operation-line">
            <el-select v-model="line.materialId" filterable placeholder="物资名称" class="line-material" @change="onMaterialChange(index, line.materialId!)">
              <el-option v-for="item in materials" :key="item.id" :label="materialLabel(item)" :value="item.id" />
            </el-select>
            <el-select v-model="line.locationId" filterable placeholder="存放位置" class="line-location">
              <el-option v-for="item in locations" :key="item.id" :label="item.name" :value="item.id" />
            </el-select>
            <el-input v-model="line.quantity" inputmode="decimal" maxlength="18" placeholder="数量" class="line-quantity" />
            <el-button link type="danger" :disabled="lines.length === 1" @click="removeLine(index)">删除</el-button>
          </div>
          <el-button plain type="primary" :disabled="submitting" @click="addLine">+ 添加一行物资</el-button>
        </div>
      </el-form-item>
      <el-form-item label="业务日期" required><el-date-picker v-model="form.occurredAt" type="date" value-format="YYYY-MM-DD" format="YYYY年MM月DD日" :editable="false" placeholder="选择年月日" style="width:100%" /></el-form-item>
      <el-form-item label="调拨依据"><el-input v-model="form.adjustmentBasis" clearable placeholder="例如：调拨单号、领料单号、采购单号" /></el-form-item>
      <el-form-item :label="isOut ? '领用单位' : '来源单位'">
        <el-select v-model="form.relatedUnit" clearable filterable allow-create default-first-option style="width:100%" placeholder="选择，或输入新单位后按回车">
          <el-option v-for="item in relatedUnitOptions" :key="item.id" :label="item.name" :value="item.name" />
        </el-select>
      </el-form-item>
      <el-form-item label="经办人"><el-input v-model="form.handler" /></el-form-item>
      <el-form-item v-if="isOut" label="领用人"><el-input v-model="form.receiver" /></el-form-item>
      <el-form-item label="备注"><el-input v-model="form.remark" type="textarea" :rows="3" /></el-form-item>
      <el-form-item label="单据图片"><AttachmentField v-model:pending="pendingAttachments" :attachments="[]" :disabled="submitting" /></el-form-item>
      <el-form-item><el-button type="primary" :disabled="submitting || !materials.length || !locations.length" :loading="submitting" @click="submit">确认{{ isOut ? '出库' : '入库' }}</el-button><el-button :disabled="submitting" @click="reset">重置</el-button></el-form-item>
    </el-form>
  </el-card>
</template>

<style scoped>
.operation-card { min-height: 560px; }
.line-list { width: 100%; display: flex; flex-direction: column; gap: 10px; }
.operation-line { display: flex; gap: 8px; align-items: center; }
.line-material { flex: 1.5; min-width: 180px; }
.line-location { flex: 1; min-width: 150px; }
.line-quantity { width: 130px; }
.scan-box { width: 100%; }
.scan-box .el-alert { margin-top: 8px; }
.scan-preview { margin-top: 8px; }
.scan-hint { margin-left: 10px; color: var(--el-text-color-secondary); font-size: 12px; }
@media (max-width: 760px) { .operation-line { flex-wrap: wrap; } .line-material, .line-location { min-width: 45%; } }
</style>
