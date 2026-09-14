use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};
use std::{fs, path::PathBuf, str::FromStr, time::Duration};
use tauri::AppHandle;

const DATABASE_FILE: &str = "kylin-stock.db";
pub(crate) const LATEST_SCHEMA_VERSION: i64 = 6;

struct Migration {
    version: i64,
    statements: &'static [&'static str],
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        statements: &[
        r#"CREATE TABLE IF NOT EXISTS units (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            status INTEGER NOT NULL DEFAULT 1
        )"#,
        r#"CREATE TABLE IF NOT EXISTS locations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            remark TEXT,
            status INTEGER NOT NULL DEFAULT 1
        )"#,
        r#"CREATE TABLE IF NOT EXISTS materials (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            unit_id INTEGER,
            category TEXT,
            default_location_id INTEGER,
            remark TEXT,
            status INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(unit_id) REFERENCES units(id),
            FOREIGN KEY(default_location_id) REFERENCES locations(id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS inventory_balances (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            material_id INTEGER NOT NULL,
            location_id INTEGER NOT NULL,
            quantity NUMERIC NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL,
            UNIQUE(material_id, location_id),
            FOREIGN KEY(material_id) REFERENCES materials(id),
            FOREIGN KEY(location_id) REFERENCES locations(id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS stock_transactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            transaction_no TEXT NOT NULL UNIQUE,
            type TEXT NOT NULL CHECK(type IN ('IN', 'OUT', 'ADJUST')),
            material_id INTEGER NOT NULL,
            location_id INTEGER NOT NULL,
            quantity NUMERIC NOT NULL CHECK(quantity > 0),
            occurred_at TEXT NOT NULL,
            related_unit TEXT,
            destination TEXT,
            handler TEXT,
            receiver TEXT,
            remark TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(material_id) REFERENCES materials(id),
            FOREIGN KEY(location_id) REFERENCES locations(id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS backup_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            backup_type TEXT NOT NULL CHECK(backup_type IN ('MANUAL', 'ANNUAL')),
            backup_year INTEGER,
            file_size INTEGER,
            checksum TEXT,
            created_at TEXT NOT NULL,
            remark TEXT
        )"#,
        r#"CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT,
            updated_at TEXT NOT NULL
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_materials_name ON materials(name)",
        "CREATE INDEX IF NOT EXISTS idx_transactions_material ON stock_transactions(material_id)",
        "CREATE INDEX IF NOT EXISTS idx_transactions_occurred_at ON stock_transactions(occurred_at)",
        "CREATE INDEX IF NOT EXISTS idx_transactions_type ON stock_transactions(type)",
        "CREATE INDEX IF NOT EXISTS idx_transactions_destination ON stock_transactions(destination)",
        ],
    },
    Migration {
        version: 2,
        statements: &[r#"INSERT OR IGNORE INTO units(name, status) VALUES
            ('发', 1),
            ('支', 1),
            ('具', 1),
            ('件', 1),
            ('套', 1),
            ('枚', 1),
            ('米', 1),
            ('公斤', 1),
            ('箱', 1)"#],
    },
    Migration {
        version: 3,
        statements: &[
            r#"CREATE TABLE IF NOT EXISTS attachments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL CHECK(entity_type IN ('MATERIAL', 'TRANSACTION')),
                entity_id INTEGER NOT NULL,
                file_name TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                data BLOB NOT NULL,
                created_at TEXT NOT NULL
            )"#,
            "CREATE INDEX IF NOT EXISTS idx_attachments_entity ON attachments(entity_type, entity_id)",
        ],
    },
    Migration {
        version: 4,
        statements: &[
            r#"CREATE TABLE IF NOT EXISTS business_options (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL CHECK(kind IN ('RELATED_UNIT', 'DESTINATION')),
                name TEXT NOT NULL COLLATE NOCASE,
                status INTEGER NOT NULL DEFAULT 1,
                UNIQUE(kind, name)
            )"#,
            r#"INSERT OR IGNORE INTO business_options(kind,name,status)
               SELECT 'RELATED_UNIT', TRIM(related_unit), 1 FROM stock_transactions
               WHERE TRIM(COALESCE(related_unit,'')) <> ''"#,
            r#"INSERT OR IGNORE INTO business_options(kind,name,status)
               SELECT 'DESTINATION', TRIM(destination), 1 FROM stock_transactions
               WHERE TRIM(COALESCE(destination,'')) <> ''"#,
            "CREATE INDEX IF NOT EXISTS idx_business_options_kind_name ON business_options(kind, name)",
        ],
    },
    Migration {
        version: 5,
        statements: &[
            "ALTER TABLE materials ADD COLUMN barcode TEXT",
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_materials_barcode ON materials(barcode) WHERE barcode IS NOT NULL AND barcode <> ''",
        ],
    },
    Migration {
        version: 6,
        statements: &["ALTER TABLE stock_transactions ADD COLUMN adjustment_basis TEXT"],
    },
];

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_config = app
        .path_resolver()
        .app_config_dir()
        .ok_or_else(|| "无法获取应用数据目录".to_string())?;
    fs::create_dir_all(&app_config).map_err(|e| format!("无法创建应用数据目录：{e}"))?;
    Ok(app_config.join(DATABASE_FILE))
}

async fn open_database(app: &AppHandle) -> Result<SqliteConnection, String> {
    let path = database_path(app)?;
    let url = format!("sqlite:{}", path.to_string_lossy());
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(|e| format!("数据库路径无效：{e}"))?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    SqliteConnection::connect_with(&options)
        .await
        .map_err(|e| format!("无法打开业务数据库：{e}"))
}

pub(crate) async fn run_migrations_on_connection(
    connection: &mut SqliteConnection,
) -> Result<i64, String> {
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await
        .map_err(|e| format!("无法启用数据库外键约束：{e}"))?;
    sqlx::query("PRAGMA busy_timeout = 5000")
        .execute(&mut *connection)
        .await
        .map_err(|e| format!("无法设置数据库等待策略：{e}"))?;

    let current_version = sqlx::query_scalar::<_, i64>("PRAGMA user_version")
        .fetch_one(&mut *connection)
        .await
        .map_err(|e| format!("无法读取数据库结构版本：{e}"))?;

    if current_version > LATEST_SCHEMA_VERSION {
        return Err(format!(
            "数据库结构版本 {current_version} 高于当前程序支持的版本 {LATEST_SCHEMA_VERSION}，为避免旧程序破坏新数据，本次启动已中止"
        ));
    }

    let mut applied_version = current_version;
    for migration in MIGRATIONS.iter().filter(|m| m.version > current_version) {
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(|e| format!("无法开始数据库升级事务 v{}：{e}", migration.version))?;

        let result: Result<(), String> = async {
            for statement in migration.statements {
                if let Err(error) = sqlx::query(statement).execute(&mut *connection).await {
                    // `ALTER TABLE ... ADD COLUMN` has no portable SQLite
                    // IF NOT EXISTS form. A legacy/unversioned database may
                    // already contain the column while still needing the
                    // migration's remaining statements (for example, its
                    // index), so treat this one idempotent case as success.
                    if !((migration.version == 5
                        && statement.contains("ALTER TABLE materials ADD COLUMN"))
                        || (migration.version == 6
                            && statement.contains("ALTER TABLE stock_transactions ADD COLUMN")))
                        || !error.to_string().contains("duplicate column name")
                    {
                        return Err(format!(
                            "数据库升级 v{} 执行失败：{error}",
                            migration.version
                        ));
                    }
                }
            }

            let set_version = format!("PRAGMA user_version = {}", migration.version);
            sqlx::query(&set_version)
                .execute(&mut *connection)
                .await
                .map_err(|e| format!("无法写入数据库结构版本 v{}：{e}", migration.version))?;
            Ok(())
        }
        .await;

        if let Err(error) = result {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            return Err(error);
        }

        if let Err(error) = sqlx::query("COMMIT").execute(&mut *connection).await {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            return Err(format!(
                "数据库升级 v{} 提交失败：{error}",
                migration.version
            ));
        }

        applied_version = migration.version;
    }

    Ok(applied_version)
}

#[tauri::command]
pub async fn initialize_database_schema(app: AppHandle) -> Result<i64, String> {
    let mut connection = open_database(&app).await?;
    run_migrations_on_connection(&mut connection).await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn memory_database() -> SqliteConnection {
        SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open in-memory sqlite")
    }

    async fn user_version(connection: &mut SqliteConnection) -> i64 {
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(connection)
            .await
            .expect("read user_version")
    }

    #[tokio::test]
    async fn fresh_database_is_created_at_latest_version() {
        let mut connection = memory_database().await;
        let version = run_migrations_on_connection(&mut connection)
            .await
            .expect("migrate fresh database");

        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert_eq!(user_version(&mut connection).await, LATEST_SCHEMA_VERSION);

        let required_tables = [
            "units",
            "locations",
            "materials",
            "inventory_balances",
            "stock_transactions",
            "backup_records",
            "app_settings",
            "attachments",
            "business_options",
        ];
        for table in required_tables {
            let count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            )
            .bind(table)
            .fetch_one(&mut connection)
            .await
            .expect("inspect schema");
            assert_eq!(count, 1, "missing table {table}");
        }

        let units = sqlx::query_scalar::<_, String>("SELECT name FROM units ORDER BY id")
            .fetch_all(&mut connection)
            .await
            .expect("read default units");
        assert_eq!(
            units,
            ["发", "支", "具", "件", "套", "枚", "米", "公斤", "箱"]
        );
    }

    #[tokio::test]
    async fn unversioned_existing_database_keeps_business_data() {
        let mut connection = memory_database().await;
        run_migrations_on_connection(&mut connection)
            .await
            .expect("create legacy-compatible schema");

        sqlx::query("PRAGMA user_version = 0")
            .execute(&mut connection)
            .await
            .expect("simulate pre-versioning database");
        sqlx::query(
            "INSERT INTO materials(name,status,created_at,updated_at) VALUES ('网线',1,'2026-08-16','2026-08-16')",
        )
        .execute(&mut connection)
        .await
        .expect("seed legacy row");

        run_migrations_on_connection(&mut connection)
            .await
            .expect("upgrade legacy database");

        let name = sqlx::query_scalar::<_, String>("SELECT name FROM materials WHERE id=1")
            .fetch_one(&mut connection)
            .await
            .expect("read preserved row");
        assert_eq!(name, "网线");
        assert_eq!(user_version(&mut connection).await, LATEST_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn rerunning_migrations_is_idempotent() {
        let mut connection = memory_database().await;
        run_migrations_on_connection(&mut connection)
            .await
            .expect("first migration pass");
        sqlx::query("INSERT INTO units(name,status) VALUES ('卷',1)")
            .execute(&mut connection)
            .await
            .expect("seed unit");

        let version = run_migrations_on_connection(&mut connection)
            .await
            .expect("second migration pass");
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM units WHERE name='卷'")
            .fetch_one(&mut connection)
            .await
            .expect("count preserved unit");

        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn business_option_migration_imports_existing_transaction_values() {
        let mut connection = memory_database().await;
        run_migrations_on_connection(&mut connection)
            .await
            .expect("create current schema");
        sqlx::query("INSERT INTO locations(name,status) VALUES ('一号库',1)")
            .execute(&mut connection)
            .await
            .expect("seed location");
        sqlx::query("INSERT INTO materials(name,status,created_at,updated_at) VALUES ('网线',1,'2026-09-12','2026-09-12')")
            .execute(&mut connection)
            .await
            .expect("seed material");
        sqlx::query("INSERT INTO stock_transactions(transaction_no,type,material_id,location_id,quantity,occurred_at,related_unit,destination,created_at) VALUES ('OUT-1','OUT',1,1,1,'2026-09-12','维修组','一车间','2026-09-12')")
            .execute(&mut connection)
            .await
            .expect("seed transaction");
        sqlx::query("DELETE FROM business_options")
            .execute(&mut connection)
            .await
            .expect("simulate schema before option migration");
        sqlx::query("PRAGMA user_version = 3")
            .execute(&mut connection)
            .await
            .expect("rewind schema version");

        run_migrations_on_connection(&mut connection)
            .await
            .expect("run option migration");
        let options = sqlx::query_as::<_, (String, String)>(
            "SELECT kind,name FROM business_options ORDER BY kind",
        )
        .fetch_all(&mut connection)
        .await
        .expect("read imported options");
        assert_eq!(
            options,
            vec![
                ("DESTINATION".into(), "一车间".into()),
                ("RELATED_UNIT".into(), "维修组".into()),
            ]
        );
    }

    #[tokio::test]
    async fn newer_database_is_rejected_instead_of_downgraded() {
        let mut connection = memory_database().await;
        sqlx::query("PRAGMA user_version = 99")
            .execute(&mut connection)
            .await
            .expect("set future schema version");

        let error = run_migrations_on_connection(&mut connection)
            .await
            .expect_err("future database must be rejected");

        assert!(error.contains("高于当前程序支持"));
        assert_eq!(user_version(&mut connection).await, 99);
    }
}
