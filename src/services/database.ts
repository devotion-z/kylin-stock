import { invoke } from '@tauri-apps/api/tauri'

interface ExecuteResult {
  rowsAffected: number
  lastInsertId: number
}

class NativeDatabase {
  select<T>(sql: string, values: unknown[] = []): Promise<T> {
    return invoke<T>('database_select', { sql, values })
  }

  execute(sql: string, values: unknown[] = []): Promise<ExecuteResult> {
    return invoke<ExecuteResult>('database_execute', { sql, values })
  }

  async close() {
    // Each native command owns a short-lived SQLite connection, so no pool
    // remains open when backup/restore needs to replace the database file.
    return true
  }
}

let database: NativeDatabase | null = null
let initialization: Promise<NativeDatabase> | null = null
let accessTail: Promise<void> = Promise.resolve()

export async function initializeDatabase() {
  if (database) return database
  if (initialization) return initialization

  initialization = (async () => {
    // Schema creation/upgrades run in Rust on one dedicated SQLite connection.
    // This guarantees that BEGIN/COMMIT and PRAGMA user_version belong to the
    // same connection instead of relying on several calls through a SQL pool.
    await invoke<number>('initialize_database_schema')

    const opened = new NativeDatabase()
    try {
      await opened.execute('PRAGMA foreign_keys = ON')
      database = opened
      return opened
    } catch (error) {
      await opened.close().catch(() => false)
      throw error
    }
  })()

  try {
    return await initialization
  } finally {
    initialization = null
  }
}

export async function getDatabase() {
  return database ?? initializeDatabase()
}

/**
 * Serialize application-level SQLite access within the single KylinStock
 * webview process. The workload is a single-user desktop application, so the
 * negligible serialization cost is preferable to allowing a query to race a
 * restore that closes and replaces the live database file.
 *
 * A caller may execute several SQL statements (including Promise.all reads)
 * while it owns one access turn. Stock/master-data writes, user backups and the
 * complete restore close/swap/reopen/rollback lifecycle use this same gate.
 *
 * Do not acquire this gate recursively from inside an operation that already
 * owns it; nested acquisition would wait on itself. Internal helpers used by an
 * owning operation should call getDatabase() directly.
 */
export async function withDatabaseAccess<T>(operation: () => Promise<T>): Promise<T> {
  let release!: () => void
  const turn = new Promise<void>((resolve) => { release = resolve })
  const previous = accessTail
  accessTail = turn

  await previous
  try {
    return await operation()
  } finally {
    release()
  }
}

export function withDatabaseMutation<T>(operation: () => Promise<T>): Promise<T> {
  return withDatabaseAccess(operation)
}

export async function closeDatabase() {
  if (!database) return
  const current = database
  database = null
  const closed = await current.close()
  if (!closed) {
    database = current
    throw new Error('数据库连接未能安全关闭')
  }
}

export async function reopenDatabase() {
  if (database) await closeDatabase()
  return initializeDatabase()
}

export async function checkDatabaseIntegrity() {
  const rows = await (await getDatabase()).select<Record<string, string>[]>('PRAGMA integrity_check')
  const result = rows[0] ? String(Object.values(rows[0])[0] ?? '') : ''
  return result.toLowerCase() === 'ok'
}
