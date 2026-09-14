<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { chooseRestoreFile, createBackup, listBackupRecords, restoreBackup, type BackupRecord } from '../services/backup'
import { formatDateTime } from '../utils/date'

const loading = ref(false)
const backingUp = ref(false)
const restoring = ref(false)
const operationBusy = computed(() => backingUp.value || restoring.value)
const year = ref(new Date().getFullYear())
const rows = ref<BackupRecord[]>([])

function formatSize(bytes: number | null) {
  if (bytes == null) return '-'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`
}

async function refresh() {
  if (operationBusy.value) return
  loading.value = true
  try { rows.value = await listBackupRecords() }
  catch (e) { ElMessage.error(e instanceof Error ? e.message : String(e)) }
  finally { loading.value = false }
}

async function runBackup(type: 'MANUAL' | 'ANNUAL') {
  if (operationBusy.value) return
  backingUp.value = true
  try {
    const path = await createBackup(type, type === 'ANNUAL' ? year.value : undefined)
    if (path) {
      ElMessage.success(type === 'ANNUAL' ? '年度备份创建成功' : '数据备份创建成功')
      // The operation lock remains active until finally. Refresh directly here
      // instead of calling refresh(), which intentionally rejects concurrent work.
      rows.value = await listBackupRecords()
    }
  } catch (e) {
    ElMessage.error(`备份失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    backingUp.value = false
  }
}

async function runRestore() {
  if (operationBusy.value) return
  restoring.value = true
  try {
    const source = await chooseRestoreFile()
    if (!source) return

    try {
      await ElMessageBox.confirm(
        '恢复操作会使用所选备份替换当前业务数据。系统会先自动保留一份恢复前安全副本。确认继续吗？',
        '确认恢复数据',
        {
          type: 'warning',
          confirmButtonText: '确认恢复',
          cancelButtonText: '取消',
          distinguishCancelAndClose: true,
        },
      )
    } catch {
      return
    }

    const result = await restoreBackup(source)
    ElMessage.success('数据恢复完成，数据库完整性检查通过')
    if (result.safetyBackupPath) {
      ElMessage.info('恢复前数据已自动保存为安全副本')
    }
    rows.value = await listBackupRecords()
  } catch (e) {
    ElMessage.error(`恢复失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    restoring.value = false
  }
}

onMounted(refresh)
</script>

<template>
  <div class="backup-page">
    <el-alert
      title="业务数据保存在本机。建议定期创建备份，并将重要年度备份复制到 U 盘或移动硬盘。"
      type="warning"
      :closable="false"
      show-icon
    />

    <el-card shadow="never">
      <template #header><strong>创建备份</strong></template>
      <div class="actions">
        <div class="action-block">
          <div class="action-title">即时备份</div>
          <div class="action-desc">选择保存文件夹后自动生成中文备份文件名，适合日常操作前、重要数据录入后随时创建。</div>
          <el-button type="primary" :loading="backingUp" :disabled="operationBusy" @click="runBackup('MANUAL')">创建即时备份</el-button>
        </div>

        <div class="action-block">
          <div class="action-title">年度归档</div>
          <div class="action-desc">选择保存文件夹后为指定年度创建明确标记的归档副本。</div>
          <div class="annual-row">
            <el-input-number v-model="year" :min="2000" :max="2100" :step="1" :controls="false" :disabled="operationBusy" style="width:120px" />
            <el-button type="success" :loading="backingUp" :disabled="operationBusy" @click="runBackup('ANNUAL')">创建年度备份</el-button>
          </div>
        </div>

        <div class="action-block danger-block">
          <div class="action-title">从备份恢复</div>
          <div class="action-desc">选择 `.db` 备份恢复。恢复前会自动创建当前数据的安全副本。</div>
          <el-button type="danger" :loading="restoring" :disabled="operationBusy" @click="runRestore">选择备份并恢复</el-button>
        </div>
      </div>
    </el-card>

    <el-card shadow="never">
      <template #header>
        <div class="history-header">
          <strong>备份记录</strong>
          <el-button :disabled="operationBusy" @click="refresh">刷新</el-button>
        </div>
      </template>
      <el-table v-loading="loading" :data="rows" border stripe empty-text="暂无备份记录">
        <el-table-column prop="file_name" label="备份文件" min-width="250" show-overflow-tooltip />
        <el-table-column label="类型" width="110">
          <template #default="{ row }">
            <el-tag :type="row.backup_type === 'ANNUAL' ? 'success' : 'info'">
              {{ row.backup_type === 'ANNUAL' ? '年度备份' : '即时备份' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="backup_year" label="年度" width="100" />
        <el-table-column label="文件大小" width="120"><template #default="{ row }">{{ formatSize(row.file_size) }}</template></el-table-column>
        <el-table-column label="创建时间" min-width="170"><template #default="{ row }">{{ formatDateTime(row.created_at) }}</template></el-table-column>
        <el-table-column prop="remark" label="说明" min-width="220" show-overflow-tooltip />
        <el-table-column prop="file_path" label="保存位置" min-width="300" show-overflow-tooltip />
      </el-table>
    </el-card>
  </div>
</template>

<style scoped>
.backup-page { display:flex; flex-direction:column; gap:16px; }
.actions { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:16px; }
.action-block { border:1px solid #e4e7ed; border-radius:8px; padding:18px; min-height:150px; }
.action-title { font-size:16px; font-weight:600; margin-bottom:8px; }
.action-desc { color:#606266; line-height:1.6; min-height:52px; margin-bottom:14px; }
.annual-row { display:flex; gap:10px; }
.danger-block { border-color:#f5c2c7; }
.history-header { display:flex; align-items:center; justify-content:space-between; }
@media (max-width: 1100px) { .actions { grid-template-columns:1fr; } }
</style>
