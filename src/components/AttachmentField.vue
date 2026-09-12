<script setup lang="ts">
import { ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { chooseAttachmentImages, deleteAttachment, fileNameFromPath, getAttachmentDataUrl, type Attachment } from '../services/attachments'

const props = withDefaults(defineProps<{
  attachments: Attachment[]
  pending?: string[]
  disabled?: boolean
  readonly?: boolean
}>(), { pending: () => [], disabled: false, readonly: false })

const emit = defineEmits<{
  'update:pending': [paths: string[]]
  removed: [id: number]
}>()

const previewVisible = ref(false)
const previewLoading = ref(false)
const previewUrl = ref('')
const previewName = ref('')

async function choose() {
  try {
    const selected = await chooseAttachmentImages()
    if (!selected.length) return
    const combined = [...new Set([...props.pending, ...selected])]
    const remaining = Math.max(0, 10 - props.attachments.length)
    if (combined.length > remaining) ElMessage.warning('每条记录最多添加 10 张图片')
    emit('update:pending', combined.slice(0, remaining))
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : String(e))
  }
}

function removePending(index: number) {
  emit('update:pending', props.pending.filter((_, itemIndex) => itemIndex !== index))
}

async function preview(item: Attachment) {
  previewLoading.value = true
  previewName.value = item.fileName
  previewVisible.value = true
  try {
    previewUrl.value = await getAttachmentDataUrl(item.id)
  } catch (e) {
    previewVisible.value = false
    ElMessage.error(e instanceof Error ? e.message : String(e))
  } finally {
    previewLoading.value = false
  }
}

async function removeExisting(item: Attachment) {
  try {
    await ElMessageBox.confirm(`确认删除单据图片“${item.fileName}”？`, '删除确认')
    await deleteAttachment(item.id)
    emit('removed', item.id)
    ElMessage.success('单据图片已删除')
  } catch (e) {
    if (e !== 'cancel' && e !== 'close') ElMessage.error(e instanceof Error ? e.message : String(e))
  }
}
</script>

<template>
  <div class="attachment-field">
    <el-button v-if="!readonly" :disabled="disabled || attachments.length + pending.length >= 10" @click="choose">添加单据图片</el-button>
    <span class="hint">JPG/PNG/WebP/GIF/BMP，单张不超过 15 MB，最多 10 张</span>
    <div v-if="attachments.length || pending.length" class="file-list">
      <div v-for="item in attachments" :key="`saved-${item.id}`" class="file-item">
        <span class="file-name">{{ item.fileName }}</span>
        <el-button link type="primary" @click="preview(item)">查看</el-button>
        <el-button v-if="!readonly" link type="danger" :disabled="disabled" @click="removeExisting(item)">删除</el-button>
      </div>
      <div v-for="(path, index) in pending" :key="`pending-${path}`" class="file-item">
        <span class="file-name">{{ fileNameFromPath(path) }}（待保存）</span>
        <el-button link type="danger" :disabled="disabled" @click="removePending(index)">移除</el-button>
      </div>
    </div>
    <span v-else-if="readonly" class="hint">无单据图片</span>

    <el-dialog v-model="previewVisible" :title="previewName" width="72%" append-to-body destroy-on-close @closed="previewUrl=''">
      <div v-loading="previewLoading" class="preview-area">
        <img v-if="previewUrl" :src="previewUrl" :alt="previewName" />
      </div>
    </el-dialog>
  </div>
</template>

<style scoped>
.attachment-field { width: 100%; }
.hint { margin-left: 10px; color: var(--el-text-color-secondary); font-size: 12px; }
.file-list { margin-top: 8px; border: 1px solid var(--el-border-color-lighter); border-radius: 4px; padding: 4px 10px; }
.file-item { display: flex; align-items: center; min-height: 32px; gap: 6px; }
.file-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.preview-area { min-height: 180px; display: flex; justify-content: center; }
.preview-area img { max-width: 100%; max-height: 70vh; object-fit: contain; }
</style>
