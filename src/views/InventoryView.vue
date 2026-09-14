<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useRoute } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { deleteInventoryPosition, listInventory, transferStock, type InventoryRow } from '../services/inventory'
import { listLocations, type Location } from '../services/masterData'
import { exportInventoryRows } from '../services/export'
import { formatDateTime, toLocalDateValue } from '../utils/date'

const loading = ref(false)
const route = useRoute()
const isDistribution = computed(() => route.path === '/distribution')
const exporting = ref(false)
const managing = ref(false)
const operationBusy = computed(() => loading.value || exporting.value || managing.value)
const rows = ref<InventoryRow[]>([])
const locations = ref<Location[]>([])
const filters = reactive({ keyword: '', unit: '', location: '' })
const transferDialogVisible = ref(false)
const transferRow = ref<InventoryRow | null>(null)
const transferForm = reactive({ toLocationId: undefined as number | undefined, quantity: '', occurredAt: toLocalDateValue(), adjustmentBasis: '库内调拨', handler: '', remark: '' })

async function refresh() {
  if (operationBusy.value) return
  const query = { ...filters, location: isDistribution.value ? filters.location : '' }
  loading.value = true
  try {
    rows.value = await listInventory(query)
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    loading.value = false
  }
}

function reset() {
  if (operationBusy.value) return
  Object.assign(filters, { keyword: '', unit: '', location: '' })
  refresh()
}

async function exportCurrent() {
  if (operationBusy.value) return
  if (!rows.value.length) return ElMessage.warning('当前没有可导出的库存数据')

  const exportRows = rows.value.slice()
  exporting.value = true
  try {
    const path = await exportInventoryRows(exportRows)
    if (path) ElMessage.success(`库存物资分布已导出（${exportRows.length} 条）`)
  } catch (e) {
    ElMessage.error(`导出失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    exporting.value = false
  }
}

function openTransfer(row: InventoryRow) {
  if (operationBusy.value) return
  transferRow.value = row
  Object.assign(transferForm, { toLocationId: undefined, quantity: String(row.quantity), occurredAt: toLocalDateValue(), adjustmentBasis: '库内调拨', handler: '', remark: '' })
  transferDialogVisible.value = true
}

async function submitTransfer() {
  const row = transferRow.value
  if (!row || !transferForm.toLocationId) return ElMessage.warning('请选择转入库位')
  const quantity = Number(transferForm.quantity)
  if (!Number.isFinite(quantity) || quantity <= 0) return ElMessage.warning('请输入正确的转移数量')
  managing.value = true
  try {
    await transferStock({ materialId: row.material_id, fromLocationId: row.location_id, toLocationId: transferForm.toLocationId, quantity, occurredAt: `${transferForm.occurredAt}T00:00:00.000Z`, adjustmentBasis: transferForm.adjustmentBasis, handler: transferForm.handler, remark: transferForm.remark })
    ElMessage.success(`已将“${row.material_name}”转入新库位`)
    transferDialogVisible.value = false
    managing.value = false
    await refresh()
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally { managing.value = false }
}

async function clearInventory(row: InventoryRow) {
  if (operationBusy.value) return
  try {
    await ElMessageBox.confirm(`这会清除“${row.material_name}”在“${row.location_name}”的库存，并删除该物资该库位的全部出入库流水和单据图片。此操作不可恢复，确定继续？`, '清除库存确认', { type: 'warning', confirmButtonText: '清除', cancelButtonText: '取消' })
  } catch { return }
  managing.value = true
  try {
    await deleteInventoryPosition(row.material_id, row.location_id)
    ElMessage.success('库存及对应测试流水已清除')
    managing.value = false
    await refresh()
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally { managing.value = false }
}

function handleManage(command: string, row: InventoryRow) {
  if (command === 'transfer') openTransfer(row)
  else if (command === 'clear') clearInventory(row)
}

onMounted(async () => { await Promise.all([refresh(), listLocations().then(value => { locations.value = value }).catch(() => undefined)]) })
</script>

<template>
  <el-card shadow="never">
    <div class="toolbar">
      <el-input v-model="filters.keyword" :disabled="operationBusy" clearable placeholder="物资名称" style="width:220px" @keyup.enter="refresh" />
      <el-input v-model="filters.unit" :disabled="operationBusy" clearable placeholder="计量单位" style="width:150px" @keyup.enter="refresh" />
      <el-select v-if="isDistribution" v-model="filters.location" :disabled="operationBusy" clearable filterable placeholder="存放位置" style="width:180px">
        <el-option v-for="item in locations" :key="item.id" :label="item.name" :value="item.name" />
      </el-select>
      <el-button type="primary" :loading="loading" :disabled="operationBusy" @click="refresh">查询</el-button>
      <el-button :disabled="operationBusy" @click="reset">重置</el-button>
      <el-button type="success" :loading="exporting" :disabled="operationBusy || !rows.length" @click="exportCurrent">
        导出当前结果（{{ rows.length }}）
      </el-button>
    </div>

    <el-table v-loading="loading" :data="rows" border stripe empty-text="暂无库存">
      <el-table-column prop="material_name" label="物资名称" min-width="180" />
      <el-table-column prop="unit_name" label="单位" width="100" />
      <el-table-column prop="quantity" label="当前库存" width="140" />
      <el-table-column v-if="isDistribution" prop="location_name" label="存放位置" min-width="160" />
      <el-table-column label="最后更新时间" min-width="180"><template #default="{ row }">{{ formatDateTime(row.updated_at) }}</template></el-table-column>
      <el-table-column label="管理" width="100" fixed="right">
        <template #default="{ row }">
          <el-dropdown :disabled="operationBusy" @command="handleManage($event, row)">
            <el-button link type="primary">管理<i class="el-icon--right">⌄</i></el-button>
            <template #dropdown>
              <el-dropdown-menu>
                <el-dropdown-item command="transfer">转移到其他库位</el-dropdown-item>
                <el-dropdown-item command="clear" divided>清除库存和流水</el-dropdown-item>
              </el-dropdown-menu>
            </template>
          </el-dropdown>
        </template>
      </el-table-column>
    </el-table>
  </el-card>

  <el-dialog v-model="transferDialogVisible" title="库内转移" width="520px" :close-on-click-modal="!managing" :close-on-press-escape="!managing">
    <el-alert v-if="transferRow" :title="`${transferRow.material_name}：${transferRow.location_name} → 请选择新库位`" type="info" :closable="false" show-icon style="margin-bottom:16px" />
    <el-form label-width="100px" :disabled="managing">
      <el-form-item label="转入库位" required><el-select v-model="transferForm.toLocationId" filterable placeholder="请选择库位" style="width:100%"><el-option v-for="item in locations.filter(item => item.id !== transferRow?.location_id)" :key="item.id" :label="item.name" :value="item.id" /></el-select></el-form-item>
      <el-form-item label="转移数量" required><el-input v-model="transferForm.quantity" inputmode="decimal" /></el-form-item>
      <el-form-item label="业务日期" required><el-date-picker v-model="transferForm.occurredAt" type="date" value-format="YYYY-MM-DD" format="YYYY年MM月DD日" :editable="false" style="width:100%" /></el-form-item>
      <el-form-item label="调拨依据"><el-input v-model="transferForm.adjustmentBasis" /></el-form-item>
      <el-form-item label="经办人"><el-input v-model="transferForm.handler" /></el-form-item>
      <el-form-item label="备注"><el-input v-model="transferForm.remark" type="textarea" :rows="2" /></el-form-item>
    </el-form>
    <template #footer><el-button :disabled="managing" @click="transferDialogVisible=false">取消</el-button><el-button type="primary" :loading="managing" @click="submitTransfer">确认转库</el-button></template>
  </el-dialog>
</template>

<style scoped>
.toolbar { display:flex; gap:10px; flex-wrap:wrap; margin-bottom:18px; align-items:center; }
</style>
