<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import dayjs from 'dayjs'
import { ElMessage, ElMessageBox } from 'element-plus'
import { deleteStockTransaction, listLedger, updateStockTransaction, type LedgerRow } from '../services/inventory'
import { listLocations, listMaterials, type Location, type Material } from '../services/masterData'
import { exportLedgerRows } from '../services/export'
import { formatBusinessDate } from '../utils/date'
import AttachmentField from '../components/AttachmentField.vue'
import { addAttachment, listAttachments, type Attachment } from '../services/attachments'
import { parseQuantityInput } from '../utils/quantity'

const loading = ref(false)
const exporting = ref(false)
const deletingId = ref<number | null>(null)
const editingId = ref<number | null>(null)
const operationBusy = computed(() => loading.value || exporting.value || deletingId.value !== null || editingId.value !== null)
const rows = ref<LedgerRow[]>([])
const dateRange = ref<string[]>([])
const filters = reactive({ basis: '', material: '', type: 'ALL', relatedUnit: '' })
const attachmentDialogVisible = ref(false)
const attachmentDialogTitle = ref('单据图片')
const attachments = ref<Attachment[]>([])
const materialOptions = ref<Material[]>([])
const locationOptions = ref<Location[]>([])
const editDialogVisible = ref(false)
const editRow = ref<LedgerRow | null>(null)
const editAttachments = ref<Attachment[]>([])
const editPendingAttachments = ref<string[]>([])
const editForm = reactive({ materialId: undefined as number | undefined, locationId: undefined as number | undefined, quantity: '', occurredAt: '', relatedUnit: '', handler: '', receiver: '', remark: '', adjustmentBasis: '' })

async function showAttachments(row: LedgerRow) {
  try {
    attachments.value = await listAttachments('TRANSACTION', row.id)
    attachmentDialogTitle.value = `单据图片 · ${row.transaction_no}`
    attachmentDialogVisible.value = true
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  }
}

async function refresh() {
  if (operationBusy.value) return
  loading.value = true
  try {
    rows.value = await listLedger({
      ...filters,
      startAt: dateRange.value[0] ? dayjs(dateRange.value[0]).startOf('day').toISOString() : '',
      endAt: dateRange.value[1] ? dayjs(dateRange.value[1]).endOf('day').toISOString() : '',
    })
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    loading.value = false
  }
}

async function loadMaterialOptions() {
  try {
    ;[materialOptions.value, locationOptions.value] = await Promise.all([listMaterials(), listLocations()])
  } catch (e) { ElMessage.error(`基础资料加载失败：${e instanceof Error ? e.message : String(e)}`) }
}

async function openEdit(row: LedgerRow) {
  if (operationBusy.value) return
  if (row.transaction_no.startsWith('TRANSFER-')) {
    return ElMessage.warning('库内倒库会生成成对流水，不能单独编辑；请到“物资分布”重新倒库')
  }
  if (row.type === 'ADJUST') return ElMessage.warning('调整类流水暂不支持编辑')
  editRow.value = row
  Object.assign(editForm, {
    materialId: row.material_id,
    locationId: row.location_id,
    quantity: String(row.quantity),
    occurredAt: dayjs(row.occurred_at).format('YYYY-MM-DD'),
    relatedUnit: row.related_unit ?? row.destination ?? '',
    handler: row.handler ?? '',
    receiver: row.receiver ?? '',
    remark: row.remark ?? '',
    adjustmentBasis: row.adjustment_basis ?? '',
  })
  editPendingAttachments.value = []
  try {
    editAttachments.value = await listAttachments('TRANSACTION', row.id)
    editDialogVisible.value = true
  } catch (e) { ElMessage.error(e instanceof Error ? e.message : String(e)) }
}

async function submitEdit() {
  const row = editRow.value
  if (!row || !editForm.materialId || !editForm.locationId) return ElMessage.warning('请选择物资和存放位置')
  if (!editForm.occurredAt) return ElMessage.warning('请选择业务日期')
  let quantity: number
  try { quantity = parseQuantityInput(editForm.quantity) }
  catch (e) { return ElMessage.warning(e instanceof Error ? e.message : String(e)) }
  if (row.type === 'OUT' && !editForm.relatedUnit.trim()) return ElMessage.warning('请填写领用单位')

  editingId.value = row.id
  try {
    await updateStockTransaction({
      id: row.id,
      materialId: editForm.materialId,
      locationId: editForm.locationId,
      quantity,
      occurredAt: `${editForm.occurredAt}T00:00:00.000Z`,
      relatedUnit: editForm.relatedUnit,
      destination: row.type === 'OUT' ? editForm.relatedUnit : '',
      handler: editForm.handler,
      receiver: editForm.receiver,
      remark: editForm.remark,
      adjustmentBasis: editForm.adjustmentBasis,
    })
    while (editPendingAttachments.value.length) {
      const path = editPendingAttachments.value[0]
      const saved = await addAttachment('TRANSACTION', row.id, path)
      editAttachments.value.push(saved)
      editPendingAttachments.value.shift()
    }
    ElMessage.success('流水已修改，相关库存已同步更新')
    editDialogVisible.value = false
    editingId.value = null
    await refresh()
  } catch (e) { ElMessage.error(e instanceof Error ? e.message : String(e)) }
  finally { editingId.value = null }
}

function reset() {
  if (operationBusy.value) return
  Object.assign(filters, { basis: '', material: '', type: 'ALL', relatedUnit: '' })
  dateRange.value = []
  refresh()
}

async function exportCurrent() {
  // Never export the previous rows while a new query is still resolving.
  if (operationBusy.value) return
  if (!rows.value.length) return ElMessage.warning('当前没有可导出的查询结果')

  const exportRows = rows.value.slice()
  exporting.value = true
  try {
    const path = await exportLedgerRows(exportRows)
    if (path) ElMessage.success(`当前查询结果已导出（${exportRows.length} 条）`)
  } catch (e) {
    ElMessage.error(`导出失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    exporting.value = false
  }
}

async function removeRow(row: LedgerRow) {
  if (operationBusy.value) return
  try {
    await ElMessageBox.confirm(`删除后会自动回滚库存，并删除该流水的单据图片。`, `确认删除流水“${row.transaction_no}”？`, {
      type: 'warning', confirmButtonText: '删除', cancelButtonText: '取消',
    })
  } catch { return }
  deletingId.value = row.id
  try {
    await deleteStockTransaction(row.id)
    ElMessage.success('流水已删除，库存已回滚')
    deletingId.value = null
    await refresh()
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    deletingId.value = null
  }
}

onMounted(() => { refresh(); loadMaterialOptions() })
</script>

<template>
  <el-card shadow="never">
    <div class="toolbar">
      <el-input v-model="filters.basis" :disabled="operationBusy" clearable placeholder="调拨依据" style="width:180px" />
      <el-select v-model="filters.material" :disabled="operationBusy" clearable filterable placeholder="物资名称" style="width:180px">
        <el-option v-for="item in materialOptions" :key="item.id" :label="item.name" :value="item.name" />
      </el-select>
      <el-date-picker
        v-model="dateRange"
        :disabled="operationBusy"
        type="daterange"
        value-format="YYYY-MM-DD"
        start-placeholder="开始日期"
        end-placeholder="结束日期"
        range-separator="至"
        format="YYYY年MM月DD日"
        style="width:260px"
      />
      <el-input v-model="filters.relatedUnit" :disabled="operationBusy" clearable placeholder="单位" style="width:170px" />
      <el-select v-model="filters.type" :disabled="operationBusy" placeholder="业务类型" style="width:130px">
        <el-option label="全部" value="ALL" />
        <el-option label="入库" value="IN" />
        <el-option label="出库" value="OUT" />
        <el-option label="调整" value="ADJUST" />
      </el-select>
      <el-button type="primary" :loading="loading" :disabled="operationBusy" @click="refresh">查询</el-button>
      <el-button :disabled="operationBusy" @click="reset">重置</el-button>
      <el-button type="success" :loading="exporting" :disabled="operationBusy || !rows.length" @click="exportCurrent">
        导出当前结果（{{ rows.length }}）
      </el-button>
    </div>

    <el-alert
      title="导出遵循“查询什么，就导出什么”：查询执行期间会暂时锁定筛选和导出，保证文件与当前表格结果一致。"
      type="info"
      :closable="false"
      show-icon
      style="margin-bottom:14px"
    />

    <el-table v-loading="loading" :data="rows" border stripe empty-text="暂无出入库记录">
      <el-table-column prop="transaction_no" label="流水号" min-width="190" />
      <el-table-column prop="adjustment_basis" label="调拨依据" min-width="150" />
      <el-table-column label="类型" width="90">
        <template #default="{row}">
          <el-tag :type="row.type==='IN'?'success':row.type==='OUT'?'warning':'info'">
            {{ row.type==='IN'?'入库':row.type==='OUT'?'出库':'调整' }}
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column prop="material_name" label="物资名称" min-width="160" />
      <el-table-column prop="unit_name" label="计量单位" width="100" />
      <el-table-column prop="quantity" label="数量" width="110" />
      <el-table-column prop="location_name" label="存放位置" min-width="130" />
      <el-table-column prop="related_unit" label="领用/来源单位" min-width="160" />
      <el-table-column prop="handler" label="经办人" width="100" />
      <el-table-column prop="receiver" label="领用人" width="100" />
      <el-table-column label="业务日期" min-width="130"><template #default="{ row }">{{ formatBusinessDate(row.occurred_at) }}</template></el-table-column>
      <el-table-column prop="remark" label="备注" min-width="180" show-overflow-tooltip />
      <el-table-column label="单据图片" width="100">
        <template #default="{ row }"><el-button v-if="row.attachment_count" link type="primary" @click="showAttachments(row)">查看（{{ row.attachment_count }}）</el-button><span v-else>-</span></template>
      </el-table-column>
      <el-table-column label="操作" width="140" fixed="right">
        <template #default="{ row }">
          <el-button link type="primary" :disabled="operationBusy" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" :loading="deletingId === row.id" :disabled="operationBusy" @click="removeRow(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>
  </el-card>

  <el-dialog v-model="attachmentDialogVisible" :title="attachmentDialogTitle" width="620px">
    <AttachmentField :attachments="attachments" readonly />
  </el-dialog>

  <el-dialog v-model="editDialogVisible" title="编辑出入库记录" width="640px" :close-on-click-modal="editingId === null" :close-on-press-escape="editingId === null">
    <el-alert v-if="editRow" :title="`正在修正${editRow.type === 'IN' ? '入库' : '出库'}流水：${editRow.transaction_no}`" type="warning" :closable="false" show-icon style="margin-bottom:16px" />
    <el-form label-width="110px" :disabled="editingId !== null">
      <el-form-item label="物资名称" required><el-select v-model="editForm.materialId" filterable style="width:100%"><el-option v-for="item in materialOptions" :key="item.id" :label="item.name" :value="item.id" /></el-select></el-form-item>
      <el-form-item label="存放位置" required><el-select v-model="editForm.locationId" filterable style="width:100%"><el-option v-for="item in locationOptions" :key="item.id" :label="item.name" :value="item.id" /></el-select></el-form-item>
      <el-form-item label="数量" required><el-input v-model="editForm.quantity" inputmode="decimal" maxlength="18" /></el-form-item>
      <el-form-item label="业务日期" required><el-date-picker v-model="editForm.occurredAt" type="date" value-format="YYYY-MM-DD" format="YYYY年MM月DD日" :editable="false" style="width:100%" /></el-form-item>
      <el-form-item label="调拨依据"><el-input v-model="editForm.adjustmentBasis" /></el-form-item>
      <el-form-item :label="editRow?.type === 'OUT' ? '领用单位' : '来源单位'" :required="editRow?.type === 'OUT'"><el-input v-model="editForm.relatedUnit" /></el-form-item>
      <el-form-item label="经办人"><el-input v-model="editForm.handler" /></el-form-item>
      <el-form-item v-if="editRow?.type === 'OUT'" label="领用人"><el-input v-model="editForm.receiver" /></el-form-item>
      <el-form-item label="备注"><el-input v-model="editForm.remark" type="textarea" :rows="2" /></el-form-item>
      <el-form-item label="单据图片"><AttachmentField v-model:pending="editPendingAttachments" :attachments="editAttachments" :disabled="editingId !== null" @removed="id => editAttachments = editAttachments.filter(item => item.id !== id)" /></el-form-item>
    </el-form>
    <template #footer><el-button :disabled="editingId !== null" @click="editDialogVisible=false">取消</el-button><el-button type="primary" :loading="editingId !== null" @click="submitEdit">保存并同步库存</el-button></template>
  </el-dialog>
</template>

<style scoped>
.toolbar { display:flex; gap:10px; flex-wrap:wrap; margin-bottom:18px; align-items:center; }
</style>
