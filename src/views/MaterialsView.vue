<script setup lang="ts">
import { computed, onActivated, reactive, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { createLocation, createUnit, deleteLocation, deleteUnit, deleteMaterial, listLocations, listMaterials, listUnits, resolveLocationChoice, resolveUnitChoice, saveMaterial, setMaterialStatus, type Location, type MasterDataChoice, type Material, type Unit } from '../services/masterData'
import { exportMaterialRows } from '../services/export'
import AttachmentField from '../components/AttachmentField.vue'
import { addAttachment, listAttachments, type Attachment } from '../services/attachments'
import { deleteBusinessOption, ensureBusinessOption, listBusinessOptions, type BusinessOption, type BusinessOptionKind } from '../services/businessOptions'

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
const form = reactive({ id: undefined as number | undefined, name: '', barcode: '', unitId: undefined as MasterDataChoice, category: '', locationId: undefined as MasterDataChoice, remark: '' })
const editingMaterialId = ref<number | undefined>()
const masterDialogVisible = ref(false)
const masterTab = ref<'locations' | 'units' | 'related'>('locations')
const relatedUnits = ref<BusinessOption[]>([])

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
  editingMaterialId.value = undefined
  Object.assign(form, { id: undefined, name: '', barcode: '', unitId: undefined, category: '', locationId: undefined, remark: '' })
  dialogTitle.value = '新增物资'
  attachments.value = []
  pendingAttachments.value = []
  dialogVisible.value = true
}

async function openEdit(row: Material) {
  if (operationBusy.value) return
  editingMaterialId.value = Number(row.id)
  Object.assign(form, { id: editingMaterialId.value, name: row.name, barcode: row.barcode ?? '', unitId: row.unit_id ?? undefined, category: row.category ?? '', locationId: row.default_location_id ?? undefined, remark: row.remark ?? '' })
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
    const result = await saveMaterial({ ...form, id: editingMaterialId.value, unitId, locationId })
    const materialId = result.merged ? Number(result.lastInsertId) : editingMaterialId.value ?? Number(result.lastInsertId)
    editingMaterialId.value = materialId
    form.id = materialId
    if (result.merged) attachments.value = await listAttachments('MATERIAL', materialId)
    while (pendingAttachments.value.length) {
      const saved = await addAttachment('MATERIAL', materialId, pendingAttachments.value[0])
      attachments.value.push(saved)
      pendingAttachments.value.shift()
    }
    dialogVisible.value = false
    ElMessage.success(result.merged ? '保存成功，重复物资及其库存记录已合并' : '保存成功')
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

async function remove(row: Material) {
  if (operationBusy.value) return
  mutating.value = true
  try {
    await ElMessageBox.confirm(`确认删除物资“${row.name}”？删除后不可恢复。`, '删除确认', { type: 'warning', confirmButtonText: '删除', cancelButtonText: '取消' })
    await deleteMaterial(row.id)
    ElMessage.success('物资已删除')
    await loadData(keyword.value)
  } catch (e) {
    if (e !== 'cancel' && e !== 'close') ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally { mutating.value = false }
}

async function quickUnit() {
  if (operationBusy.value) return
  mutating.value = true
  try {
    const { value } = await ElMessageBox.prompt('请输入计量单位，例如：发、件、套、箱', '新增单位', { inputPattern: /\S+/, inputErrorMessage: '单位不能为空', confirmButtonText: '确定', cancelButtonText: '取消' })
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
    const { value } = await ElMessageBox.prompt('请输入存放位置，例如：建材一号库、日用品一号库', '新增存放位置', { inputPattern: /\S+/, inputErrorMessage: '位置不能为空', confirmButtonText: '确定', cancelButtonText: '取消' })
    await createLocation(value)
    ElMessage.success('位置已添加')
    await loadData(keyword.value)
  } catch (e) {
    if (e !== 'cancel' && e !== 'close') ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    mutating.value = false
  }
}

async function quickBusinessOption(kind: BusinessOptionKind, title: string, hint: string) {
  if (operationBusy.value) return
  mutating.value = true
  try {
    const { value } = await ElMessageBox.prompt(hint, title, { inputPattern: /\S+/, inputErrorMessage: '名称不能为空', confirmButtonText: '确定', cancelButtonText: '取消' })
    await ensureBusinessOption(kind, value)
    ElMessage.success(`${title}已添加`)
  } catch (e) {
    if (e !== 'cancel' && e !== 'close') ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally { mutating.value = false }
}

async function openMasterData() {
  try {
    ;[locations.value, units.value, relatedUnits.value] = await Promise.all([listLocations(), listUnits(), listBusinessOptions('RELATED_UNIT')])
    masterDialogVisible.value = true
  } catch (e) { ElMessage.error(e instanceof Error ? e.message : String(e)) }
}

async function removeMasterItem(item: Location | Unit | BusinessOption) {
  try {
    const locationHint = masterTab.value === 'locations' ? ' 系统会同时清理关联的物资默认库位和零库存残留；真实流水或非零库存仍会受到保护。' : ''
    await ElMessageBox.confirm(`确认删除“${item.name}”？删除后不可恢复。${locationHint}`, '删除确认', { type: 'warning', confirmButtonText: '删除', cancelButtonText: '取消' })
    if (masterTab.value === 'locations') await deleteLocation(item.id)
    else if (masterTab.value === 'units') await deleteUnit(item.id)
    else await deleteBusinessOption(item.id)
    await openMasterData()
    await loadData(keyword.value)
    ElMessage.success('已删除')
  } catch (e) { if (e !== 'cancel' && e !== 'close') ElMessage.error(e instanceof Error ? e.message : String(e)) }
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

onActivated(refresh)
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
        <el-button :disabled="operationBusy" @click="quickBusinessOption('RELATED_UNIT', '新增领用单位', '请输入领用单位名称')">新增领用单位</el-button>
        <el-button :disabled="operationBusy" @click="quickBusinessOption('RELATED_UNIT', '新增来源单位', '请输入来源单位名称')">新增来源单位</el-button>
        <el-button :disabled="operationBusy" @click="openMasterData">管理基础数据</el-button>
        <el-button type="primary" :disabled="operationBusy" @click="openCreate">新增物资</el-button>
      </div>
    </div>

    <el-table v-loading="loading" :data="materials" border stripe empty-text="暂无物资，请先新增">
      <el-table-column prop="name" label="物资名称" min-width="180" />
      <el-table-column prop="unit_name" label="单位" width="100" />
      <el-table-column prop="barcode" label="条码" min-width="150" />
      <el-table-column prop="category" label="分类" min-width="130" />
      <el-table-column label="库存所在库房" min-width="180"><template #default="{ row }">{{ row.stock_locations || '暂无库存' }}</template></el-table-column>
      <el-table-column prop="location_name" label="默认入库位置" min-width="140" />
      <el-table-column prop="remark" label="备注" min-width="180" show-overflow-tooltip />
      <el-table-column label="单据图片" width="100"><template #default="{ row }">{{ row.attachment_count ? `${row.attachment_count} 张` : '-' }}</template></el-table-column>
      <el-table-column label="状态" width="90"><template #default="{ row }"><el-tag :type="row.status === 1 ? 'success' : 'info'">{{ row.status === 1 ? '正常' : '停用' }}</el-tag></template></el-table-column>
      <el-table-column label="操作" width="240"><template #default="{ row }"><el-button link type="primary" :disabled="operationBusy" @click="openEdit(row)">编辑</el-button><el-button link :disabled="operationBusy" :type="row.status === 1 ? 'danger' : 'success'" @click="toggle(row)">{{ row.status === 1 ? '停用' : '启用' }}</el-button><el-button link type="danger" :disabled="operationBusy" @click="remove(row)">删除</el-button></template></el-table-column>
    </el-table>
  </el-card>

  <el-dialog v-model="dialogVisible" :title="dialogTitle" width="520px" :close-on-click-modal="!mutating" :close-on-press-escape="!mutating" :show-close="!mutating">
    <el-form label-width="110px" :disabled="mutating">
      <el-form-item label="物资名称" required><el-input v-model="form.name" maxlength="100" /></el-form-item>
      <el-form-item label="物资条码"><el-input v-model="form.barcode" maxlength="100" clearable placeholder="可填条码，供扫码出入库使用" /></el-form-item>
      <el-form-item label="计量单位">
        <el-select v-model="form.unitId" clearable filterable allow-create default-first-option style="width:100%" placeholder="请选择，或输入新单位后按回车" no-data-text="输入单位名称后按回车创建">
          <el-option v-for="item in units" :key="item.id" :label="item.name" :value="item.id" />
        </el-select>
      </el-form-item>
      <el-form-item label="物资分类"><el-input v-model="form.category" /></el-form-item>
      <el-form-item label="默认入库位置">
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

  <el-dialog v-model="masterDialogVisible" title="基础数据管理" width="620px">
    <el-alert title="删除存放位置时会自动清理物资默认库位和零库存残留；仍有真实流水或非零库存的位置会继续受到保护。" type="info" :closable="false" show-icon style="margin-bottom:12px" />
    <el-tabs v-model="masterTab">
      <el-tab-pane label="存放位置" name="locations"><el-table :data="locations" border max-height="360"><el-table-column prop="name" label="名称" /><el-table-column label="操作" width="100"><template #default="{ row }"><el-button link type="danger" @click="removeMasterItem(row)">删除</el-button></template></el-table-column></el-table></el-tab-pane>
      <el-tab-pane label="计量单位" name="units"><el-table :data="units" border max-height="360"><el-table-column prop="name" label="名称" /><el-table-column label="操作" width="100"><template #default="{ row }"><el-button link type="danger" @click="removeMasterItem(row)">删除</el-button></template></el-table-column></el-table></el-tab-pane>
      <el-tab-pane label="领用/来源单位" name="related"><el-table :data="relatedUnits" border max-height="360"><el-table-column prop="name" label="名称" /><el-table-column label="操作" width="100"><template #default="{ row }"><el-button link type="danger" @click="removeMasterItem(row)">删除</el-button></template></el-table-column></el-table></el-tab-pane>
    </el-tabs>
  </el-dialog>
</template>

<style scoped>
.toolbar { display:flex; justify-content:space-between; gap:16px; margin-bottom:18px; flex-wrap:wrap; }
.search-group { display:flex; gap:10px; flex-wrap:wrap; }
</style>
