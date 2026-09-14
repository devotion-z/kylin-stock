<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { loadDashboard, listCategoryInventory, type CategoryInventoryRow, type CategorySummary, type RecentTransaction, type StockOverviewRow } from '../services/dashboard'
import { formatBusinessDate } from '../utils/date'

const loading = ref(false)
const recent = ref<RecentTransaction[]>([])
const overview = ref<StockOverviewRow[]>([])
const categories = ref<CategorySummary[]>([])
const selectedCategory = ref('')
const categoryInventory = ref<CategoryInventoryRow[]>([])
const categoryLoading = ref(false)

const categoryCards = computed(() => categories.value.map((item, index) => ({
  ...item,
  color: ['blue', 'green', 'orange', 'purple', 'cyan', 'red'][index % 6],
})))

async function selectCategory(category: string) {
  selectedCategory.value = category
  categoryLoading.value = true
  try {
    categoryInventory.value = await listCategoryInventory(category)
  } catch (e) {
    ElMessage.error(`分类库存加载失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    categoryLoading.value = false
  }
}

async function refresh() {
  loading.value = true
  try {
    const data = await loadDashboard()
    categories.value = data.categories
    recent.value = data.recent
    overview.value = data.overview
    const nextCategory = selectedCategory.value && data.categories.some((item) => item.category_name === selectedCategory.value)
      ? selectedCategory.value
      : data.categories[0]?.category_name ?? ''
    if (nextCategory) await selectCategory(nextCategory)
    else { selectedCategory.value = ''; categoryInventory.value = [] }
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

    <el-empty v-if="!categoryCards.length" description="暂未维护物资分类，请先到“物资管理”添加分类" />
    <el-row v-else :gutter="16">
      <el-col v-for="card in categoryCards" :key="card.category_name" :xs="12" :sm="8" :md="6" :lg="4">
        <el-card shadow="hover" class="category-card" :class="[`category-${card.color}`, { selected: selectedCategory === card.category_name }]" @click="selectCategory(card.category_name)">
          <div class="category-name">{{ card.category_name }}</div>
          <div class="category-count">{{ card.material_count }}<span>种物资</span></div>
          <div class="category-note">其中 {{ card.stocked_count }} 种有库存</div>
        </el-card>
      </el-col>
    </el-row>

    <el-card v-if="selectedCategory" v-loading="categoryLoading" shadow="never" class="category-detail-card">
      <template #header><div class="section-header"><strong>{{ selectedCategory }}库存明细</strong><span>点击上方分类可切换</span></div></template>
      <el-table v-if="categoryInventory.length" :data="categoryInventory" border stripe size="small">
        <el-table-column prop="material_name" label="物资名称" min-width="180" />
        <el-table-column prop="unit_name" label="计量单位" width="110" />
        <el-table-column prop="quantity" label="当前库存" width="120" />
        <el-table-column prop="location_name" label="存放位置" min-width="150" />
      </el-table>
      <el-empty v-else description="该分类暂无库存" />
    </el-card>

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
.category-card { min-height:132px; cursor:pointer; border-top:4px solid transparent; transition:transform .18s ease, box-shadow .18s ease; }
.category-card:hover { transform:translateY(-2px); }
.category-card.selected { box-shadow:0 0 0 2px var(--el-color-primary-light-5); }
.category-name { color:#344054; font-size:15px; font-weight:700; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.category-count { margin:13px 0 6px; color:#101828; font-size:29px; font-weight:700; line-height:1; }
.category-count span { margin-left:6px; color:#667085; font-size:12px; font-weight:400; }
.category-note, .section-header span { color:#667085; font-size:12px; }
.category-blue { border-top-color:#409eff; background:linear-gradient(150deg,#f5faff,#fff); }
.category-green { border-top-color:#67c23a; background:linear-gradient(150deg,#f6fff2,#fff); }
.category-orange { border-top-color:#e6a23c; background:linear-gradient(150deg,#fffaf0,#fff); }
.category-purple { border-top-color:#8e62d9; background:linear-gradient(150deg,#faf7ff,#fff); }
.category-cyan { border-top-color:#13c2c2; background:linear-gradient(150deg,#f1ffff,#fff); }
.category-red { border-top-color:#f56c6c; background:linear-gradient(150deg,#fff6f6,#fff); }
.category-detail-card { margin-top:0; }
.section-header { display:flex; justify-content:space-between; align-items:center; }
@media (max-width: 991px) { .el-col { margin-bottom:16px; } }
</style>
