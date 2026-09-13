<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useRoute } from 'vue-router'
import { ElMessage } from 'element-plus'
import { listInventory, type InventoryRow } from '../services/inventory'
import { listLocations, type Location } from '../services/masterData'
import { exportInventoryRows } from '../services/export'
import { formatDateTime } from '../utils/date'

const loading = ref(false)
const route = useRoute()
const isDistribution = computed(() => route.path === '/distribution')
const exporting = ref(false)
const operationBusy = computed(() => loading.value || exporting.value)
const rows = ref<InventoryRow[]>([])
const locations = ref<Location[]>([])
const filters = reactive({ keyword: '', unit: '', location: '' })

async function refresh() {
  if (operationBusy.value) return
  const query = { ...filters }
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
    </el-table>
  </el-card>
</template>

<style scoped>
.toolbar { display:flex; gap:10px; flex-wrap:wrap; margin-bottom:18px; align-items:center; }
</style>
