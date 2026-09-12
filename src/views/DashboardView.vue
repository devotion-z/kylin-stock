<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { loadDashboard, type DashboardStats, type RecentTransaction, type StockOverviewRow } from '../services/dashboard'
import { formatBusinessDate } from '../utils/date'

const loading = ref(false)
const stats = ref<DashboardStats>({ materialCount: 0, stockedMaterialCount: 0, todayInCount: 0, todayOutCount: 0 })
const recent = ref<RecentTransaction[]>([])
const overview = ref<StockOverviewRow[]>([])

const cards = computed(() => [
  { label: '物资品种', value: stats.value.materialCount, note: '当前正常建档物资' },
  { label: '有库存物资', value: stats.value.stockedMaterialCount, note: '当前库存大于 0 的物资种类' },
  { label: '今日入库', value: stats.value.todayInCount, note: '今日入库业务笔数' },
  { label: '今日出库', value: stats.value.todayOutCount, note: '今日出库业务笔数' },
])

async function refresh() {
  loading.value = true
  try {
    const data = await loadDashboard()
    stats.value = data.stats
    recent.value = data.recent
    overview.value = data.overview
  } catch (e) {
    ElMessage.error(`首页数据加载失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    loading.value = false
  }
}

onMounted(refresh)
</script>

<template>
  <div v-loading="loading" class="page-stack">
    <div class="page-actions">
      <span class="page-hint">不同计量单位不做无意义的跨单位数量相加</span>
      <el-button @click="refresh">刷新数据</el-button>
    </div>

    <el-row :gutter="16">
      <el-col v-for="stat in cards" :key="stat.label" :xs="12" :sm="12" :md="6">
        <el-card shadow="never" class="stat-card">
          <div class="stat-label">{{ stat.label }}</div>
          <div class="stat-value">{{ stat.value }}</div>
          <div class="stat-note">{{ stat.note }}</div>
        </el-card>
      </el-col>
    </el-row>

    <el-row :gutter="16">
      <el-col :xs="24" :lg="15">
        <el-card shadow="never">
          <template #header><strong>最近出入库记录</strong></template>
          <el-table v-if="recent.length" :data="recent" border stripe size="small">
            <el-table-column label="类型" width="82">
              <template #default="{ row }">
                <el-tag :type="row.type === 'IN' ? 'success' : row.type === 'OUT' ? 'warning' : 'info'" size="small">
                  {{ row.type === 'IN' ? '入库' : row.type === 'OUT' ? '出库' : '调整' }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column prop="material_name" label="物资名称" min-width="140" />
            <el-table-column prop="quantity" label="数量" width="90" />
            <el-table-column prop="unit_name" label="单位" width="80" />
            <el-table-column prop="destination" label="出库去向" min-width="120" show-overflow-tooltip />
            <el-table-column label="业务日期" min-width="130">
              <template #default="{ row }">{{ formatBusinessDate(row.occurred_at) }}</template>
            </el-table-column>
          </el-table>
          <el-empty v-else description="暂无业务记录，请先登记入库" />
        </el-card>
      </el-col>

      <el-col :xs="24" :lg="9">
        <el-card shadow="never">
          <template #header><strong>库存概览</strong></template>
          <el-table v-if="overview.length" :data="overview" border stripe size="small">
            <el-table-column prop="material_name" label="物资名称" min-width="140" />
            <el-table-column prop="quantity" label="库存" width="100" />
            <el-table-column prop="unit_name" label="单位" width="80" />
          </el-table>
          <el-empty v-else description="暂无库存数据" />
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<style scoped>
.page-stack { display:flex; flex-direction:column; gap:16px; min-height:500px; }
.page-actions { display:flex; justify-content:flex-end; align-items:center; gap:12px; }
.page-hint { color:#909399; font-size:13px; }
.stat-card { margin-bottom:0; }
.stat-label { color:#606266; font-size:14px; }
.stat-value { font-size:30px; font-weight:700; margin:10px 0 6px; line-height:1.1; }
.stat-note { color:#909399; font-size:12px; }
@media (max-width: 991px) { .el-col { margin-bottom:16px; } }
</style>
