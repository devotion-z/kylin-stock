use serde::Serialize;
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

use crate::migration::LATEST_SCHEMA_VERSION;

const DATABASE_FILE: &str = "kylin-stock.db";
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub file_size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub safety_backup_path: Option<String>,
    pub safety_backup_size: Option<u64>,
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_config = app
        .path_resolver()
        .app_config_dir()
        .ok_or_else(|| "无法获取应用数据目录".to_string())?;
    fs::create_dir_all(&app_config).map_err(|e| format!("无法创建应用数据目录：{e}"))?;
    Ok(app_config.join(DATABASE_FILE))
}

fn sync_file(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("无法将文件同步到磁盘：{e}"))
}

fn copy_and_sync(source: &Path, destination: &Path) -> Result<u64, String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("无法创建目标目录：{e}"))?;
    }
    let size = fs::copy(source, destination).map_err(|e| format!("复制数据库失败：{e}"))?;
    sync_file(destination)?;
    Ok(size)
}

fn validate_sqlite_file(path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|e| format!("无法读取备份文件：{e}"))?;
    if !metadata.is_file() || metadata.len() < 100 {
        return Err("所选文件不是有效的 SQLite 数据库备份".into());
    }

    let mut file = File::open(path).map_err(|e| format!("无法打开备份文件：{e}"))?;
    let mut header = [0_u8; 16];
    file.read_exact(&mut header)
        .map_err(|e| format!("无法读取备份文件头：{e}"))?;
    if &header != SQLITE_HEADER {
        return Err("所选文件不是有效的 SQLite 数据库文件".into());
    }
    Ok(())
}

async fn validate_kylinstock_database(path: &Path) -> Result<(), String> {
    validate_sqlite_file(path)?;

    let url = format!("sqlite:{}", path.to_string_lossy());
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(|e| format!("备份数据库路径无效：{e}"))?
        .read_only(true)
        .busy_timeout(Duration::from_secs(10));
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .map_err(|e| format!("无法以只读方式打开备份数据库：{e}"))?;

    let quick_check = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_one(&mut connection)
        .await
        .map_err(|e| format!("无法检查备份数据库完整性：{e}"))?;
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Err(format!("备份数据库未通过完整性检查：{quick_check}"));
    }

    let schema_version = sqlx::query_scalar::<_, i64>("PRAGMA user_version")
        .fetch_one(&mut connection)
        .await
        .map_err(|e| format!("无法读取备份数据库结构版本：{e}"))?;
    if schema_version > LATEST_SCHEMA_VERSION {
        return Err(format!(
            "备份数据库结构版本 {schema_version} 高于当前程序支持的版本 {LATEST_SCHEMA_VERSION}，请使用创建该备份的同版本或更新版本程序恢复"
        ));
    }

    let core_table_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM sqlite_master
           WHERE type='table' AND name IN ('materials','inventory_balances','stock_transactions')"#,
    )
    .fetch_one(&mut connection)
    .await
    .map_err(|e| format!("无法识别备份数据库结构：{e}"))?;
    if core_table_count != 3 {
        return Err("所选 SQLite 文件不是可识别的 KylinStock 业务备份".into());
    }

    Ok(())
}

async fn vacuum_into_snapshot(source: &Path, output: &Path) -> Result<u64, String> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("无法创建备份目标目录：{e}"))?;
    }
    if output.exists() {
        fs::remove_file(output).map_err(|e| format!("无法清理临时备份文件：{e}"))?;
    }

    let url = format!("sqlite:{}", source.to_string_lossy());
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(|e| format!("业务数据库路径无效：{e}"))?
        .busy_timeout(Duration::from_secs(10));
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .map_err(|e| format!("无法打开业务数据库用于备份：{e}"))?;

    let output_name = output.to_string_lossy().into_owned();
    if let Err(error) = sqlx::query("VACUUM INTO ?")
        .bind(&output_name)
        .execute(&mut connection)
        .await
    {
        let _ = connection.close().await;
        let _ = fs::remove_file(output);
        return Err(format!("SQLite 一致性快照创建失败：{error}"));
    }

    connection
        .close()
        .await
        .map_err(|e| format!("备份快照完成后关闭数据库连接失败：{e}"))?;
    sync_file(output)?;

    if let Err(error) = validate_kylinstock_database(output).await {
        let _ = fs::remove_file(output);
        return Err(format!("新建备份快照校验失败：{error}"));
    }

    fs::metadata(output)
        .map(|metadata| metadata.len())
        .map_err(|e| format!("无法读取备份快照大小：{e}"))
}

#[tauri::command]
pub async fn create_database_backup(
    app: AppHandle,
    destination: String,
) -> Result<BackupResult, String> {
    let source = database_path(&app)?;
    if !source.exists() {
        return Err("当前业务数据库不存在，无法创建备份".into());
    }

    let destination = PathBuf::from(destination);
    if destination == source {
        return Err("备份文件不能覆盖正在使用的业务数据库".into());
    }
    if destination.exists() {
        if let (Ok(source_real), Ok(destination_real)) =
            (fs::canonicalize(&source), fs::canonicalize(&destination))
        {
            if source_real == destination_real {
                return Err("备份文件不能覆盖正在使用的业务数据库".into());
            }
        }
    }

    validate_kylinstock_database(&source)
        .await
        .map_err(|e| format!("当前业务数据库不适合创建备份：{e}"))?;

    let temp = destination.with_extension(format!("backup-{}.tmp", timestamp_millis()));
    let size = vacuum_into_snapshot(&source, &temp).await?;

    if destination.exists() {
        if let Err(error) = fs::remove_file(&destination) {
            let _ = fs::remove_file(&temp);
            return Err(format!("无法覆盖已有备份文件：{error}"));
        }
    }
    if let Err(error) = fs::rename(&temp, &destination) {
        let _ = fs::remove_file(&temp);
        return Err(format!("无法完成备份文件写入：{error}"));
    }
    sync_file(&destination)?;

    Ok(BackupResult { file_size: size })
}

#[tauri::command]
pub async fn restore_database_backup(
    app: AppHandle,
    source: String,
) -> Result<RestoreResult, String> {
    let source = PathBuf::from(source);
    validate_kylinstock_database(&source).await?;

    let target = database_path(&app)?;
    if source == target {
        return Err("不能从当前正在使用的数据库文件执行恢复".into());
    }

    let app_config = target
        .parent()
        .ok_or_else(|| "无法确定应用数据目录".to_string())?
        .to_path_buf();
    let safety_dir = app_config.join("backups");
    fs::create_dir_all(&safety_dir).map_err(|e| format!("无法创建恢复安全目录：{e}"))?;

    let (safety_backup_path, safety_backup_size) = if target.exists() {
        let safety = safety_dir.join(format!("pre-restore-{}.db", timestamp_millis()));
        let size = copy_and_sync(&target, &safety)?;
        (Some(safety), Some(size))
    } else {
        (None, None)
    };

    let temp = app_config.join("kylin-stock.restore.tmp");
    let old = app_config.join("kylin-stock.pre-swap.old");
    if temp.exists() {
        fs::remove_file(&temp).map_err(|e| format!("无法清理恢复临时文件：{e}"))?;
    }
    if old.exists() {
        fs::remove_file(&old).map_err(|e| format!("无法清理旧恢复文件：{e}"))?;
    }

    copy_and_sync(&source, &temp)?;
    // Validate the actual copied bytes before the current database is moved.
    validate_kylinstock_database(&temp).await?;

    if target.exists() {
        fs::rename(&target, &old).map_err(|e| format!("无法准备当前数据库用于恢复：{e}"))?;
    }

    if let Err(error) = fs::rename(&temp, &target) {
        if old.exists() {
            let _ = fs::rename(&old, &target);
        }
        return Err(format!("无法替换业务数据库：{error}"));
    }

    if let Err(error) = sync_file(&target) {
        let failed = app_config.join(format!("failed-restore-{}.db", timestamp_millis()));
        let _ = fs::rename(&target, &failed);
        if old.exists() {
            if let Err(rollback_error) = fs::rename(&old, &target) {
                return Err(format!(
                    "恢复文件落盘失败：{error}；同时旧数据库回滚失败：{rollback_error}。恢复前安全副本仍保存在 backups 目录，请停止继续操作并人工恢复。"
                ));
            }
            let _ = sync_file(&target);
        }
        return Err(format!(
            "恢复文件未能安全落盘：{error}，已回滚到恢复前数据库"
        ));
    }

    if old.exists() {
        let _ = fs::remove_file(&old);
    }
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{}", target.display(), suffix));
        if sidecar.exists() {
            let _ = fs::remove_file(sidecar);
        }
    }

    Ok(RestoreResult {
        safety_backup_path: safety_backup_path.map(|path| path.to_string_lossy().into_owned()),
        safety_backup_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;

    fn temp_database_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kylinstock-{label}-{}-{}.db",
            std::process::id(),
            timestamp_millis()
        ))
    }

    async fn create_candidate(path: &Path, user_version: i64, with_core_tables: bool) {
        let url = format!("sqlite:{}", path.to_string_lossy());
        let options = SqliteConnectOptions::from_str(&url)
            .expect("valid sqlite path")
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options)
            .await
            .expect("create sqlite candidate");

        if with_core_tables {
            for statement in [
                "CREATE TABLE materials(id INTEGER PRIMARY KEY)",
                "CREATE TABLE inventory_balances(id INTEGER PRIMARY KEY)",
                "CREATE TABLE stock_transactions(id INTEGER PRIMARY KEY)",
            ] {
                connection
                    .execute(statement)
                    .await
                    .expect("create core table");
            }
        }

        let pragma = format!("PRAGMA user_version = {user_version}");
        connection
            .execute(pragma.as_str())
            .await
            .expect("set version");
        connection.close().await.expect("close candidate");
    }

    #[tokio::test]
    async fn valid_kylinstock_candidate_passes_restore_preflight() {
        let path = temp_database_path("valid-restore");
        create_candidate(&path, LATEST_SCHEMA_VERSION, true).await;

        validate_kylinstock_database(&path)
            .await
            .expect("valid KylinStock backup should pass");

        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn unrelated_sqlite_database_is_rejected() {
        let path = temp_database_path("unrelated-restore");
        create_candidate(&path, 0, false).await;

        let error = validate_kylinstock_database(&path)
            .await
            .expect_err("generic SQLite file must not be accepted as KylinStock backup");
        assert!(error.contains("不是可识别的 KylinStock"));

        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn future_schema_backup_is_rejected() {
        let path = temp_database_path("future-restore");
        create_candidate(&path, LATEST_SCHEMA_VERSION + 1, true).await;

        let error = validate_kylinstock_database(&path)
            .await
            .expect_err("future schema backup must be rejected");
        assert!(error.contains("高于当前程序支持"));

        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn vacuum_snapshot_is_consistent_while_another_write_is_uncommitted() {
        let source = temp_database_path("snapshot-source");
        let snapshot = temp_database_path("snapshot-copy");
        create_candidate(&source, LATEST_SCHEMA_VERSION, true).await;

        let url = format!("sqlite:{}", source.to_string_lossy());
        let mut writer = SqliteConnection::connect(&url)
            .await
            .expect("open writer connection");
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut writer)
            .await
            .expect("begin uncommitted write");
        sqlx::query("INSERT INTO materials(id) VALUES (1)")
            .execute(&mut writer)
            .await
            .expect("write uncommitted row");

        vacuum_into_snapshot(&source, &snapshot)
            .await
            .expect("create consistent live snapshot");

        let snapshot_url = format!("sqlite:{}", snapshot.to_string_lossy());
        let mut reader = SqliteConnection::connect(&snapshot_url)
            .await
            .expect("open snapshot");
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM materials")
            .fetch_one(&mut reader)
            .await
            .expect("count snapshot rows");
        assert_eq!(
            count, 0,
            "uncommitted writer data must not leak into backup"
        );
        reader.close().await.expect("close snapshot reader");

        sqlx::query("ROLLBACK")
            .execute(&mut writer)
            .await
            .expect("rollback writer");
        writer.close().await.expect("close writer");

        let _ = fs::remove_file(source);
        let _ = fs::remove_file(snapshot);
    }

    #[tokio::test]
    async fn vacuum_snapshot_preserves_attachment_image_bytes() {
        let source = temp_database_path("attachment-source");
        let snapshot = temp_database_path("attachment-copy");
        create_candidate(&source, LATEST_SCHEMA_VERSION, true).await;

        let url = format!("sqlite:{}", source.to_string_lossy());
        let mut writer = SqliteConnection::connect(&url)
            .await
            .expect("open source database");
        sqlx::query(
            "CREATE TABLE attachments(id INTEGER PRIMARY KEY, entity_type TEXT, entity_id INTEGER, file_name TEXT, mime_type TEXT, file_size INTEGER, data BLOB, created_at TEXT)",
        )
        .execute(&mut writer)
        .await
        .expect("create attachments table");
        let image_bytes = vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4];
        sqlx::query("INSERT INTO attachments(entity_type,entity_id,file_name,mime_type,file_size,data,created_at) VALUES ('MATERIAL',1,'receipt.png','image/png',8,?,'2026-09-12')")
            .bind(&image_bytes)
            .execute(&mut writer)
            .await
            .expect("insert attachment");
        writer.close().await.expect("close source database");

        vacuum_into_snapshot(&source, &snapshot)
            .await
            .expect("snapshot database with attachment");

        let snapshot_url = format!("sqlite:{}", snapshot.to_string_lossy());
        let mut reader = SqliteConnection::connect(&snapshot_url)
            .await
            .expect("open snapshot");
        let copied = sqlx::query_scalar::<_, Vec<u8>>("SELECT data FROM attachments WHERE id=1")
            .fetch_one(&mut reader)
            .await
            .expect("read copied attachment");
        assert_eq!(copied, image_bytes);
        reader.close().await.expect("close snapshot");

        let _ = fs::remove_file(source);
        let _ = fs::remove_file(snapshot);
    }
}
