import { open } from '@tauri-apps/api/dialog'
import { invoke } from '@tauri-apps/api/tauri'
import { withDatabaseMutation, withDatabaseRead } from './database'

export type AttachmentEntityType = 'MATERIAL' | 'TRANSACTION'

export interface Attachment {
  id: number
  entityType: AttachmentEntityType
  entityId: number
  fileName: string
  mimeType: string
  fileSize: number
  createdAt: string
}

export async function chooseAttachmentImages(): Promise<string[]> {
  const selected = await open({
    multiple: true,
    title: '选择单据图片',
    filters: [{ name: '图片', extensions: ['jpg', 'jpeg', 'jfif', 'png', 'webp', 'gif', 'bmp'] }],
  })
  if (!selected) return []
  const paths = Array.isArray(selected) ? selected : [selected]
  return paths.map(normalizeNativePath)
}

export function normalizeNativePath(value: string) {
  if (!value.startsWith('file://')) return value
  try {
    return decodeURIComponent(new URL(value).pathname)
  } catch {
    return value.replace(/^file:\/\//, '')
  }
}

export function listAttachments(entityType: AttachmentEntityType, entityId: number) {
  return withDatabaseRead(() => invoke<Attachment[]>('list_attachments', { entityType, entityId }))
}

export function addAttachment(entityType: AttachmentEntityType, entityId: number, sourcePath: string) {
  return withDatabaseMutation(() => invoke<Attachment>('add_attachment', { entityType, entityId, sourcePath }))
}

export function deleteAttachment(id: number) {
  return withDatabaseMutation(() => invoke<void>('delete_attachment', { id }))
}

export async function getAttachmentDataUrl(id: number) {
  const result = await withDatabaseRead(() => invoke<{ mimeType: string; data: string }>('get_attachment_data', { id }))
  return `data:${result.mimeType};base64,${result.data}`
}

export function openAttachmentExternal(id: number) {
  return withDatabaseRead(() => invoke<void>('open_attachment_external', { id }))
}

export function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path
}
