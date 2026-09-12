<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { createLocation, createUnit, listLocations, listMaterials, listUnits, resolveLocationChoice, resolveUnitChoice, saveMaterial, setMaterialStatus, type Location, type MasterDataChoice, type Material, type Unit } from '../services/masterData'
import { exportMaterialRows } from '../services/export'
import AttachmentField from '../components/AttachmentField.vue'
import { addAttachment, listAttachments, type Attachment } from '../services/attachments'

const loading = ref(false)
const exporting = ref(false)
const mutating = ref(false)
const operationBusy = computed(() => loading.value || exporting.value || mutating.value)
const keyword = ref('')
const materials = ref<Material[]>([])
const units = ref<Unit[]>([])
const locations = ref<Location[]>([])
const dialogVisible = ref(false)
const dialogTitle = ref('新增物资')
const attachments = ref<Attachment[]>([])
const pendingAttachments = ref<string[]>([])
const form = reactive({ id: undefined as number | undefined, name: '', unitId: undefined as MasterDataChoice, category: '', locationId: undefined as MasterDataChoice, remark: '' })

async function loadData(query: string) {
  ;[materials.value, units.value, locations.value] = await Promise.all([listMaterials(query), listUnits(), listLocations()])
}

async function refresh() {
  if (operationBusy.value) return
  const query = keyword.value
  loading.value = true
  try {
    await loadData(query)
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    loading.value = false
  }
}

function resetSearch() {
  if (operationBusy.value) return
  keyword.value = ''
  refresh()
}

function openCreate() {
  if (operationBusy.value) return
  Object.assign(form, { id: undefined, name: '', unitId: undefined, category: '', locationId: undefined, remark: '' })
  dialogTitle.value = '新增物资'
  attachments.value = []
  pendingAttachments.value = []
  dialogVisible.value = true
}

async function openEdit(row: Material) {
  if (operationBusy.value) return
  Object.assign(form, { id: row.id, name: row.name, unitId: row.unit_id ?? undefined, category: row.category ?? '', locationId: row.default_location_id ?? undefined, remark: row.remark ?? '' })
  dialogTitle.value = '编辑物资'
  pendingAttachments.value = []
  dialogVisible.value = true
  try {
    attachments.value = await listAttachments('MATERIAL', row.id)
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  }
}

async function submit() {
  // Prevent two Save events from both passing the duplicate-name precheck and
  // inserting the same material before the UI has rendered a loading state.
  if (mutating.value) return
  mutating.value = true
  try {
    const unitId = await resolveUnitChoice(form.unitId, units.value)
    const locationId = await resolveLocationChoice(form.locationId, locations.value)
    const result = await saveMaterial({ ...form, unitId, locationId })
    const materialId = form.id ?? Number(result.lastInsertId)
    form.id = materialId
    while (pendingAttachments.value.length) {
      const saved = await addAttachment('MATERIAL', materialId, pendingAttachments.value[0])
      attachments.value.push(saved)
      pendingAttachments.value.shift()
    }
    dialogVisible.value = false
    ElMessage.success('保存成功')
    await loadData(keyword.value)
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    mutating.value = false
  }
}

async function toggle(row: Material) {
  if (operationBusy.value) return
  mutating.value = true
  try {
    const next = row.status === 1 ? 0 : 1
    await ElMessageBox.confirm(`确认${next ? '启用' : '停用'}“${row.name}”？`, '操作确认')
    await setMaterialStatus(row.id, next as 0 | 1)
    ElMessage.success('操作成功')
    await loadData(keyword.value)
  } catch (e) {
    if (e !== 'cancel' && e !== 'close') ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    mutating.value = false
  }
}

async function quickUnit() {
  if (operationBusy.value) return
  mutating.value = true
  try {
    const { value } = await ElMessageBox.prompt('请输入计量单位，例如：个、箱、kg', '新增单位', { inputPattern: /\S+/, inputErrorMessage: '单位不能为空' })
    await createUnit(value)
    ElMessage.success('单位已添加')
    await loadData(keyword.value)
  } catch (e) {
    if (e !== 'cancel' && e !== 'close') ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    mutating.value = false
  }
}

async function quickLocation() {
  if (operationBusy.value) return
  mutating.value = true
  try {
    const { value } = await ElMessageBox.prompt('请输入存放位置，例如：一号库、A区货架', '新增存放位置', { inputPattern: /\S+/, inputErrorMessage: '位置不能为空' })
    await createLocation(value)
    ElMessage.success('位置已添加')
    await loadData(keyword.value)
  } catch (e) {
    if (e !== 'cancel' && e !== 'close') ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    mutating.value = false
  }
}

async function exportCurrent() {
  if (operationBusy.value) return
  if (!materials.value.length) return ElMessage.warning('当前没有可导出的物资数据')

  const exportRows = materials.value.slice()
  exporting.value = true
  try {
    const path = await exportMaterialRows(exportRows)
    if (path) ElMessage.success(`物资明细已导出（${exportRows.length} 条）`)
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
      <div class="search-group">
        <el-input v-model="keyword" :disabled="operationBusy" clearable placeholder="搜索物资名称或分类" style="width: 280px" @keyup.enter="refresh" />
        <el-button type="primary" :loading="loading" :disabled="operationBusy" @click="refresh">查询</el-button>
        <el-button :disabled="operationBusy" @click="resetSearch">重置</el-button>
        <el-button type="success" :loading="exporting" :disabled="operationBusy || !materials.length" @click="exportCurrent">导出当前结果（{{ materials.length }}）</el-button>
      </div>
      <div>
        <el-button :disabled="operationBusy" @click="quickUnit">新增单位</el-button>
        <el-button :disabled="operationBusy" @click="quickLocation">新增位置</el-button>
        <el-button type="primary" :disabled="operationBusy" @click="openCreate">新增物资</el-button>
      </div>
    </div>

    <el-table v-loading="loading" :data="materials" border stripe empty-text="暂无物资，请先新增">
      <el-table-column prop="name" label="物资名称" min-width="180" />
      <el-table-column prop="unit_name" label="单位" width="100" />
      <el-table-column prop="category" label="分类" min-width="130" />
      <el-table-column prop="location_name" label="默认存放位置" min-width="160" />
      <el-table-column prop="remark" label="备注" min-width="180" show-overflow-tooltip />
      <el-table-column label="单据图片" width="100"><template #default="{ row }">{{ row.attachment_count ? `${row.attachment_count} 张` : '-' }}</template></el-table-column>
      <el-table-column label="状态" width="90"><template #default="{ row }"><el-tag :type="row.status === 1 ? 'success' : 'info'">{{ row.status === 1 ? '正常' : '停用' }}</el-tag></template></el-table-column>
      <el-table-column label="操作" width="180"><template #default="{ row }"><el-button link type="primary" :disabled="operationBusy" @click="openEdit(row)">编辑</el-button><el-button link :disabled="operationBusy" :type="row.status === 1 ? 'danger' : 'success'" @click="toggle(row)">{{ row.status === 1 ? '停用' : '启用' }}</el-button></template></el-table-column>
    </el-table>
  </el-card>

  <el-dialog v-model="dialogVisible" :title="dialogTitle" width="520px" :close-on-click-modal="!mutating" :close-on-press-escape="!mutating" :show-close="!mutating">
    <el-form label-width="110px" :disabled="mutating">
      <el-form-item label="物资名称" required><el-input v-model="form.name" maxlength="100" /></el-form-item>
      <el-form-item label="计量单位">
        <el-select v-model="form.unitId" clearable filterable allow-create default-first-option style="width:100%" placeholder="请选择，或输入新单位后按回车" no-data-text="输入单位名称后按回车创建">
          <el-option v-for="item in units" :key="item.id" :label="item.name" :value="item.id" />
        </el-select>
      </el-form-item>
      <el-form-item label="物资分类"><el-input v-model="form.category" /></el-form-item>
      <el-form-item label="存放位置">
        <el-select v-model="form.locationId" clearable filterable allow-create default-first-option style="width:100%" placeholder="请选择，或输入新位置后按回车" no-data-text="输入位置名称后按回车创建">
          <el-option v-for="item in locations" :key="item.id" :label="item.name" :value="item.id" />
        </el-select>
      </el-form-item>
      <el-form-item label="备注"><el-input v-model="form.remark" type="textarea" :rows="3" /></el-form-item>
      <el-form-item label="单据图片">
        <AttachmentField v-model:pending="pendingAttachments" :attachments="attachments" :disabled="mutating" @removed="id => attachments = attachments.filter(item => item.id !== id)" />
      </el-form-item>
    </el-form>
    <template #footer><el-button :disabled="mutating" @click="dialogVisible=false">取消</el-button><el-button type="primary" :loading="mutating" :disabled="mutating" @click="submit">保存</el-button></template>
  </el-dialog>
</template>

<style scoped>
.toolbar { display:flex; justify-content:space-between; gap:16px; margin-bottom:18px; flex-wrap:wrap; }
.search-group { display:flex; gap:10px; flex-wrap:wrap; }
</style>
