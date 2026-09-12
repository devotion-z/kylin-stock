import { invoke } from '@tauri-apps/api/tauri'
import { open, save } from '@tauri-apps/api/dialog'
import { checkDatabaseIntegrity, closeDatabase, getDatabase, reopenDatabase, withDatabaseAccess, withDatabaseMutation } from './database'

export type BackupType = 'MANUAL' | 'ANNUAL'

export interface BackupRecord {
  id: number
  file_name: string
  file_path: string
  backup_type: BackupType
  backup_year: number | null
  file_size: number | null
  created_at: string
  remark: string | null
}

interface NativeBackupResult {
  fileSize: number
}

interface NativeRestoreResult {
  safetyBackupPath: string | null
  safetyBackupSize: number | null
}

function stamp() {
  const d = new Date()
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  const hh = String(d.getHours()).padStart(2, '0')
  const mm = String(d.getMinutes()).padStart(2, '0')
  return `${y}${m}${day}-${hh}${mm}`
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

async function recordBackup(input: {
  path: string
  type: BackupType
  year?: number | null
  size?: number | null
  remark?: string
}) {
  const now = new Date().toISOString()
  await (await getDatabase()).execute(
    `INSERT INTO backup_records
      (file_name,file_path,backup_type,backup_year,file_size,checksum,created_at,remark)
      VALUES ($1,$2,$3,$4,$5,NULL,$6,$7)`,
    [
      fileName(input.path),
      input.path,
      input.type,
      input.year ?? null,
      input.size ?? null,
      now,
      input.remark?.trim() || null,
    ],
  )
}

async function rollbackFailedRestore(
  safetyBackupPath: string,
  safetyBackupSize: number | null,
  originalError: unknown,
): Promise<never> {
  // A failed restored database may be impossible to reopen, so rollback must
  // happen while the SQL pool remains closed. The caller already owns the
  // global database access queue for the complete restore lifecycle.
  try {
    await closeDatabase()
  } catch (closeError) {
    throw new Error(
      `恢复后的数据库异常（${errorText(originalError)}），且无法安全关闭连接以执行自动回滚：${errorText(closeError)}。` +
      `恢复前安全副本仍位于：${safetyBackupPath}`,
    )
  }

  try {
    await invoke<NativeRestoreResult>('restore_database_backup', { source: safetyBackupPath })
  } catch (rollbackError) {
    throw new Error(
      `恢复后的数据库异常（${errorText(originalError)}），自动写回恢复前安全副本也失败：${errorText(rollbackError)}。` +
      `请停止继续操作并保留安全副本：${safetyBackupPath}`,
    )
  }

  try {
    await reopenDatabase()
    const rollbackHealthy = await checkDatabaseIntegrity()
    if (!rollbackHealthy) {
      throw new Error('恢复前安全副本重新写回后未通过 integrity_check')
    }
  } catch (reopenError) {
    throw new Error(
      `恢复后的数据库异常（${errorText(originalError)}）；系统已尝试写回恢复前安全副本，但数据库仍无法正常打开：${errorText(reopenError)}。` +
      `请停止继续操作并保留安全副本：${safetyBackupPath}`,
    )
  }

  try {
    await recordBackup({
      path: safetyBackupPath,
      type: 'MANUAL',
      size: safetyBackupSize,
      remark: '恢复失败后已自动写回的恢复前安全副本',
    })
  } catch (recordError) {
    // Rollback itself has succeeded; failure to add a history-row must not
    // turn a recovered database back into a failed recovery state.
    console.error('恢复前安全副本已成功写回，但备份记录写入失败', recordError)
  }

  throw new Error(
    `所选备份未能安全恢复：${errorText(originalError)}。系统已自动恢复到本次操作前的数据。`,
  )
}

export async function listBackupRecords(): Promise<BackupRecord[]> {
  return withDatabaseAccess(async () =>
    (await getDatabase()).select<BackupRecord[]>(`
      SELECT id,file_name,file_path,backup_type,backup_year,file_size,created_at,remark
      FROM backup_records
      ORDER BY created_at DESC,id DESC
    `),
  )
}

export async function createBackup(type: BackupType, year?: number) {
  if (type === 'ANNUAL' && (!year || year < 2000 || year > 2100)) {
    throw new Error('请选择正确的年度')
  }

  const defaultName = type === 'ANNUAL'
    ? `KylinStock_${year}年度备份_${stamp()}.db`
    : `KylinStock_即时备份_${stamp()}.db`

  const destination = await save({
    title: type === 'ANNUAL' ? '保存年度数据备份' : '保存数据备份',
    defaultPath: defaultName,
    filters: [{ name: 'KylinStock 数据库备份', extensions: ['db'] }],
  })
  if (!destination) return null

  return withDatabaseMutation(async () => {
    // Rust uses SQLite VACUUM INTO to produce a transactional snapshot. Holding
    // the application access turn additionally prevents a query, stock/master-
    // data write or restore lifecycle from overlapping this requested backup.
    const result = await invoke<NativeBackupResult>('create_database_backup', { destination })

    await recordBackup({
      path: destination,
      type,
      year: type === 'ANNUAL' ? year : null,
      size: result.fileSize,
      remark: type === 'ANNUAL' ? `${year} 年度归档备份` : '用户手动创建的即时备份',
    })
    return destination
  })
}

export async function chooseRestoreFile() {
  const selected = await open({
    title: '选择 KylinStock 备份文件',
    multiple: false,
    directory: false,
    filters: [{ name: 'KylinStock 数据库备份', extensions: ['db'] }],
  })
  return typeof selected === 'string' ? selected : null
}

export async function restoreBackup(source: string) {
  return withDatabaseMutation(async () => {
    // The access turn is held from pool close through swap, reopen,
    // integrity_check and any automatic rollback. Reads and writes that are
    // triggered by navigation or UI events wait until the live DB is stable.
    await closeDatabase()

    let result: NativeRestoreResult
    try {
      result = await invoke<NativeRestoreResult>('restore_database_backup', { source })
    } catch (restoreError) {
      try {
        await reopenDatabase()
      } catch (reopenError) {
        throw new Error(
          `备份文件未完成恢复（${errorText(restoreError)}），随后原业务数据库也无法重新打开：${errorText(reopenError)}`,
        )
      }
      throw restoreError
    }

    try {
      await reopenDatabase()
    } catch (reopenError) {
      if (result.safetyBackupPath) {
        return rollbackFailedRestore(result.safetyBackupPath, result.safetyBackupSize, reopenError)
      }
      throw new Error(
        `恢复文件已替换数据库，但数据库无法重新打开：${errorText(reopenError)}。本次操作前不存在可自动回滚的数据库副本，请停止继续操作。`,
      )
    }

    let integrityError: unknown = null
    try {
      const healthy = await checkDatabaseIntegrity()
      if (!healthy) integrityError = new Error('所选备份未通过 integrity_check')
    } catch (error) {
      integrityError = error
    }

    if (integrityError) {
      if (result.safetyBackupPath) {
        return rollbackFailedRestore(result.safetyBackupPath, result.safetyBackupSize, integrityError)
      }
      throw new Error(`恢复后的数据库完整性检查失败：${errorText(integrityError)}`)
    }

    if (result.safetyBackupPath) {
      await recordBackup({
        path: result.safetyBackupPath,
        type: 'MANUAL',
        size: result.safetyBackupSize,
        remark: '执行数据恢复前自动创建的安全副本',
      })
    }

    return result
  })
}
