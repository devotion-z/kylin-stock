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
type AccessKind = 'read' | 'write'
interface AccessWaiter { kind: AccessKind; grant: (release: () => void) => void }
const accessQueue: AccessWaiter[] = []
let activeReaders = 0
let writerActive = false
let databaseRevision = 0

function drainAccessQueue() {
  if (writerActive) return
  if (activeReaders > 0 && accessQueue[0]?.kind === 'write') return

  if (activeReaders === 0 && accessQueue[0]?.kind === 'write') {
    writerActive = true
    const waiter = accessQueue.shift()!
    let released = false
    waiter.grant(() => {
      if (released) return
      released = true
      writerActive = false
      drainAccessQueue()
    })
    return
  }

  while (accessQueue[0]?.kind === 'read' && !writerActive) {
    const waiter = accessQueue.shift()!
    activeReaders += 1
    let released = false
    waiter.grant(() => {
      if (released) return
      released = true
      activeReaders -= 1
      drainAccessQueue()
    })
  }
}

function acquireDatabaseAccess(kind: AccessKind): Promise<() => void> {
  return new Promise((grant) => {
    accessQueue.push({ kind, grant })
    drainAccessQueue()
  })
}

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
 * Exclusive access for writes, backup and restore. New writes are fair: once
 * queued, later reads wait behind them, while already-running reads finish.
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
  const release = await acquireDatabaseAccess('write')
  try {
    return await operation()
  } finally {
    release()
  }
}

export async function withDatabaseRead<T>(operation: () => Promise<T>): Promise<T> {
  const release = await acquireDatabaseAccess('read')
  try {
    return await operation()
  } finally {
    release()
  }
}

export async function withDatabaseMutation<T>(operation: () => Promise<T>): Promise<T> {
  const result = await withDatabaseAccess(operation)
  databaseRevision += 1
  return result
}

export function getDatabaseRevision() {
  return databaseRevision
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
