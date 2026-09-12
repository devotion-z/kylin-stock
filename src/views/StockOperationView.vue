<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { ElMessage } from 'element-plus'
import { listLocations, listMaterials, type Location, type Material } from '../services/masterData'
import { getTransactionIdByNo, stockIn, stockOut } from '../services/inventory'
import { toLocalInputValue } from '../utils/date'
import AttachmentField from '../components/AttachmentField.vue'
import { addAttachment } from '../services/attachments'

const route = useRoute()
const isOut = computed(() => route.path === '/stock-out')
const submitting = ref(false)
const loading = ref(false)
const materials = ref<Material[]>([])
const locations = ref<Location[]>([])
const pendingAttachments = ref<string[]>([])
const form = reactive({ materialId: undefined as number | undefined, locationId: undefined as number | undefined, quantity: 1, occurredAt: toLocalInputValue(), relatedUnit: '', destination: '', handler: '', receiver: '', remark: '' })

async function load() {
  loading.value = true
  try {
    ;[materials.value, locations.value] = await Promise.all([listMaterials(), listLocations()])
    materials.value = materials.value.filter((item) => item.status === 1)
  } catch (e) {
    ElMessage.error(`基础资料加载失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    loading.value = false
  }
}

function onMaterialChange(id: number) {
  const material = materials.value.find((item) => item.id === id)
  if (material?.default_location_id) form.locationId = material.default_location_id
}

function reset() {
  if (submitting.value) return
  Object.assign(form, { materialId: undefined, locationId: undefined, quantity: 1, occurredAt: toLocalInputValue(), relatedUnit: '', destination: '', handler: '', receiver: '', remark: '' })
  pendingAttachments.value = []
}

async function submit() {
  // Loading state alone is not a correctness guard: two click events can enter
  // this function before Vue has rendered the disabled/loading button state.
  if (submitting.value) return
  if (!form.materialId) return ElMessage.warning('请选择物资')
  if (!form.locationId) return ElMessage.warning('请选择存放位置')
  if (!form.occurredAt) return ElMessage.warning('请选择业务时间')
  if (isOut.value && !form.destination.trim()) return ElMessage.warning('请填写出库去向')

  submitting.value = true
  try {
    const payload = { materialId: form.materialId, locationId: form.locationId, quantity: Number(form.quantity), occurredAt: new Date(form.occurredAt).toISOString(), relatedUnit: form.relatedUnit, destination: form.destination, handler: form.handler, receiver: form.receiver, remark: form.remark }
    const transactionNo = isOut.value ? await stockOut(payload) : await stockIn(payload)
    let attachmentWarning = ''
    if (pendingAttachments.value.length) {
      const transactionId = await getTransactionIdByNo(transactionNo)
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
    Object.assign(form, { materialId: undefined, locationId: undefined, quantity: 1, occurredAt: toLocalInputValue(), relatedUnit: '', destination: '', handler: '', receiver: '', remark: '' })
    pendingAttachments.value = []
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
      <el-form-item label="物资名称" required>
        <el-select v-model="form.materialId" filterable style="width:100%" placeholder="请选择物资" @change="onMaterialChange">
          <el-option v-for="item in materials" :key="item.id" :label="`${item.name}${item.unit_name ? `（${item.unit_name}）` : ''}`" :value="item.id" />
        </el-select>
      </el-form-item>
      <el-form-item label="存放位置" required><el-select v-model="form.locationId" style="width:100%"><el-option v-for="item in locations" :key="item.id" :label="item.name" :value="item.id" /></el-select></el-form-item>
      <el-form-item :label="isOut ? '出库数量' : '入库数量'" required><el-input-number v-model="form.quantity" :min="0.001" :precision="3" style="width:100%" /></el-form-item>
      <el-form-item label="业务时间" required><el-date-picker v-model="form.occurredAt" type="datetime" value-format="YYYY-MM-DDTHH:mm" style="width:100%" /></el-form-item>
      <el-form-item :label="isOut ? '领用单位' : '来源单位'"><el-input v-model="form.relatedUnit" /></el-form-item>
      <el-form-item v-if="isOut" label="出库去向" required><el-input v-model="form.destination" placeholder="例如：一车间、XX项目、维修使用" /></el-form-item>
      <el-form-item label="经办人"><el-input v-model="form.handler" /></el-form-item>
      <el-form-item v-if="isOut" label="领用人"><el-input v-model="form.receiver" /></el-form-item>
      <el-form-item label="备注"><el-input v-model="form.remark" type="textarea" :rows="3" /></el-form-item>
      <el-form-item label="单据图片"><AttachmentField v-model:pending="pendingAttachments" :attachments="[]" :disabled="submitting" /></el-form-item>
      <el-form-item><el-button type="primary" :disabled="submitting || !materials.length || !locations.length" :loading="submitting" @click="submit">确认{{ isOut ? '出库' : '入库' }}</el-button><el-button :disabled="submitting" @click="reset">重置</el-button></el-form-item>
    </el-form>
  </el-card>
</template>

<style scoped>.operation-card { min-height: 560px; }</style>
