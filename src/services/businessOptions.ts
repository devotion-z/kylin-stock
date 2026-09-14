import { getDatabase, withDatabaseAccess, withDatabaseMutation } from './database'

export type BusinessOptionKind = 'RELATED_UNIT' | 'DESTINATION'

export interface BusinessOption {
  id: number
  kind: BusinessOptionKind
  name: string
}

export function listBusinessOptions(kind: BusinessOptionKind): Promise<BusinessOption[]> {
  return withDatabaseAccess(async () =>
    (await getDatabase()).select<BusinessOption[]>(
      'SELECT id,kind,name FROM business_options WHERE kind=$1 AND status=1 ORDER BY name',
      [kind],
    ),
  )
}

export async function ensureBusinessOption(kind: BusinessOptionKind, name: string) {
  const normalized = name.trim()
  if (!normalized) return ''
  await withDatabaseMutation(async () =>
    (await getDatabase()).execute(
      'INSERT OR IGNORE INTO business_options(kind,name,status) VALUES ($1,$2,1)',
      [kind, normalized],
    ),
  )
  return normalized
}

export async function deleteBusinessOption(id: number) {
  return withDatabaseMutation(() => (getDatabase()).then((db) => db.execute('DELETE FROM business_options WHERE id=$1', [Number(id)])))
}
