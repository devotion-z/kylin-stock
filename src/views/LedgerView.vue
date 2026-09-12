<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import dayjs from 'dayjs'
import { ElMessage } from 'element-plus'
import { listLedger, type LedgerRow } from '../services/inventory'
import { exportLedgerRows } from '../services/export'
import { formatDateTime } from '../utils/date'
import AttachmentField from '../components/AttachmentField.vue'
import { listAttachments, type Attachment } from '../services/attachments'

const loading = ref(false)
const exporting = ref(false)
const operationBusy = computed(() => loading.value || exporting.value)
const rows = ref<LedgerRow[]>([])
const dateRange = ref<string[]>([])
const filters = reactive({ material: '', type: 'ALL', relatedUnit: '', destination: '' })
const attachmentDialogVisible = ref(false)
const attachmentDialogTitle = ref('单据图片')
const attachments = ref<Attachment[]>([])

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

function reset() {
  if (operationBusy.value) return
  Object.assign(filters, { material: '', type: 'ALL', relatedUnit: '', destination: '' })
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

onMounted(refresh)
</script>

<template>
  <el-card shadow="never">
    <div class="toolbar">
      <el-input v-model="filters.material" :disabled="operationBusy" clearable placeholder="物资名称" style="width:180px" />
      <el-date-picker
        v-model="dateRange"
        :disabled="operationBusy"
        type="daterange"
        value-format="YYYY-MM-DD"
        start-placeholder="开始日期"
        end-placeholder="结束日期"
        range-separator="至"
        style="width:260px"
      />
      <el-input v-model="filters.relatedUnit" :disabled="operationBusy" clearable placeholder="单位" style="width:170px" />
      <el-select v-model="filters.type" :disabled="operationBusy" placeholder="业务类型" style="width:130px">
        <el-option label="全部" value="ALL" />
        <el-option label="入库" value="IN" />
        <el-option label="出库" value="OUT" />
        <el-option label="调整" value="ADJUST" />
      </el-select>
      <el-input v-model="filters.destination" :disabled="operationBusy" clearable placeholder="出库去向" style="width:180px" />
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
      <el-table-column label="类型" width="90">
        <template #default="{row}">
          <el-tag :type="row.type==='IN'?'success':row.type==='OUT'?'warning':'info'">
            {{ row.type==='IN'?'入库':row.type==='OUT'?'出库':'调整' }}
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column prop="material_name" label="物资名称" min-width="160" />
      <el-table-column prop="quantity" label="数量" width="110" />
      <el-table-column prop="unit_name" label="计量单位" width="100" />
      <el-table-column prop="location_name" label="存放位置" min-width="130" />
      <el-table-column prop="related_unit" label="相关单位" min-width="140" />
      <el-table-column prop="destination" label="出库去向" min-width="150" />
      <el-table-column prop="handler" label="经办人" width="100" />
      <el-table-column prop="receiver" label="领用人" width="100" />
      <el-table-column label="业务时间" min-width="170"><template #default="{ row }">{{ formatDateTime(row.occurred_at) }}</template></el-table-column>
      <el-table-column prop="remark" label="备注" min-width="180" show-overflow-tooltip />
      <el-table-column label="单据图片" width="100">
        <template #default="{ row }"><el-button v-if="row.attachment_count" link type="primary" @click="showAttachments(row)">查看（{{ row.attachment_count }}）</el-button><span v-else>-</span></template>
      </el-table-column>
    </el-table>
  </el-card>

  <el-dialog v-model="attachmentDialogVisible" :title="attachmentDialogTitle" width="620px">
    <AttachmentField :attachments="attachments" readonly />
  </el-dialog>
</template>

<style scoped>
.toolbar { display:flex; gap:10px; flex-wrap:wrap; margin-bottom:18px; align-items:center; }
</style>
