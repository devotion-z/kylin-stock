use serde::Serialize;
use serde_json::{Map, Number, Value};
use sqlx::{
    sqlite::{SqliteArguments, SqliteQueryResult, SqliteRow},
    Column, Connection, Row, Sqlite, SqliteConnection, TypeInfo, ValueRef,
};
use std::{fs, path::PathBuf};
use tauri::AppHandle;

const DATABASE_FILE: &str = "kylin-stock.db";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResult {
    rows_affected: u64,
    last_insert_id: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteLocationResult {
    rows_affected: u64,
    cleared_material_defaults: u64,
    removed_zero_balances: u64,
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_config = app
        .path_resolver()
        .app_config_dir()
        .ok_or_else(|| "无法获取应用数据目录".to_string())?;
    fs::create_dir_all(&app_config).map_err(|e| format!("无法创建应用数据目录：{e}"))?;
    Ok(app_config.join(DATABASE_FILE))
}

pub(crate) async fn open_connection(app: &AppHandle) -> Result<SqliteConnection, String> {
    let url = format!("sqlite:{}", database_path(app)?.to_string_lossy());
    let mut connection = SqliteConnection::connect(&url)
        .await
        .map_err(|e| format!("无法连接业务数据库：{e}"))?;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .map_err(|e| format!("无法启用数据库外键约束：{e}"))?;
    sqlx::query("PRAGMA busy_timeout = 5000")
        .execute(&mut connection)
        .await
        .map_err(|e| format!("无法设置数据库等待策略：{e}"))?;
    Ok(connection)
}

fn bind_values<'q>(
    mut query: sqlx::query::Query<'q, Sqlite, SqliteArguments<'q>>,
    values: Vec<Value>,
) -> sqlx::query::Query<'q, Sqlite, SqliteArguments<'q>> {
    for value in values {
        query = match value {
            Value::Null => query.bind(Option::<String>::None),
            Value::Bool(value) => query.bind(value),
            Value::Number(value) if value.is_i64() => query.bind(value.as_i64().unwrap()),
            Value::Number(value) if value.is_u64() => query.bind(value.as_u64().unwrap() as i64),
            Value::Number(value) => query.bind(value.as_f64().unwrap()),
            Value::String(value) => query.bind(value),
            other => query.bind(other.to_string()),
        };
    }
    query
}

fn cell_value(row: &SqliteRow, index: usize) -> Result<Value, String> {
    let raw = row.try_get_raw(index).map_err(|e| e.to_string())?;
    if raw.is_null() {
        return Ok(Value::Null);
    }

    match raw.type_info().name() {
        "INTEGER" | "INT" => row
            .try_get::<i64, _>(index)
            .map(|value| Value::Number(value.into()))
            .map_err(|e| e.to_string()),
        "REAL" | "FLOAT" | "DOUBLE" => row
            .try_get::<f64, _>(index)
            .map_err(|e| e.to_string())
            .and_then(|value| {
                Number::from_f64(value)
                    .map(Value::Number)
                    .ok_or_else(|| "数据库包含非有限浮点数".into())
            }),
        "BLOB" => row
            .try_get::<Vec<u8>, _>(index)
            .map(|bytes| {
                Value::Array(
                    bytes
                        .into_iter()
                        .map(|byte| Value::Number(byte.into()))
                        .collect(),
                )
            })
            .map_err(|e| e.to_string()),
        _ => row
            .try_get::<String, _>(index)
            .map(Value::String)
            .map_err(|e| e.to_string()),
    }
}

fn row_value(row: SqliteRow) -> Result<Value, String> {
    let mut object = Map::new();
    for (index, column) in row.columns().iter().enumerate() {
        object.insert(column.name().to_string(), cell_value(&row, index)?);
    }
    Ok(Value::Object(object))
}

#[tauri::command]
pub async fn database_select(
    app: AppHandle,
    sql: String,
    values: Vec<Value>,
) -> Result<Vec<Value>, String> {
    let mut connection = open_connection(&app).await?;
    let rows = bind_values(sqlx::query(&sql), values)
        .fetch_all(&mut connection)
        .await
        .map_err(|e| format!("数据库查询失败：{e}"))?;
    rows.into_iter().map(row_value).collect()
}

#[tauri::command]
pub async fn database_execute(
    app: AppHandle,
    sql: String,
    values: Vec<Value>,
) -> Result<ExecuteResult, String> {
    let mut connection = open_connection(&app).await?;
    let result: SqliteQueryResult = bind_values(sqlx::query(&sql), values)
        .execute(&mut connection)
        .await
        .map_err(|e| format!("数据库写入失败：{e}"))?;
    Ok(ExecuteResult {
        rows_affected: result.rows_affected(),
        last_insert_id: result.last_insert_rowid(),
    })
}

async fn delete_location_from_connection(
    connection: &mut SqliteConnection,
    location_id: i64,
) -> Result<DeleteLocationResult, String> {
    let mut transaction = connection
        .begin()
        .await
        .map_err(|e| format!("无法开始删除存放位置：{e}"))?;

    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM locations WHERE id=?)")
        .bind(location_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|e| format!("无法检查存放位置：{e}"))?;
    if !exists {
        return Err("存放位置不存在或已删除".into());
    }

    let transaction_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stock_transactions WHERE location_id=?")
            .bind(location_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|e| format!("无法检查历史业务记录：{e}"))?;
    let nonzero_balance_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM inventory_balances WHERE location_id=? AND ABS(quantity) > 0.0000001",
    )
    .bind(location_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|e| format!("无法检查库位库存：{e}"))?;
    if transaction_count > 0 || nonzero_balance_count > 0 {
        return Err(
            "该存放位置仍有出入库流水或非零库存，不能删除；请先在出入库明细中处理相关记录".into(),
        );
    }

    // A deleted test ledger can leave harmless zero-balance rows behind, and a
    // material may still point at the location as its optional default. Clear
    // both atomically so they do not permanently prevent master-data cleanup.
    let cleared_defaults = sqlx::query(
        "UPDATE materials SET default_location_id=NULL, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE default_location_id=?",
    )
    .bind(location_id)
    .execute(&mut *transaction)
    .await
    .map_err(|e| format!("无法清理物资默认库位：{e}"))?;
    let removed_balances = sqlx::query(
        "DELETE FROM inventory_balances WHERE location_id=? AND ABS(quantity) <= 0.0000001",
    )
    .bind(location_id)
    .execute(&mut *transaction)
    .await
    .map_err(|e| format!("无法清理零库存记录：{e}"))?;
    let deleted = sqlx::query("DELETE FROM locations WHERE id=?")
        .bind(location_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| format!("删除存放位置失败：{e}"))?;

    transaction
        .commit()
        .await
        .map_err(|e| format!("提交存放位置删除失败：{e}"))?;
    Ok(DeleteLocationResult {
        rows_affected: deleted.rows_affected(),
        cleared_material_defaults: cleared_defaults.rows_affected(),
        removed_zero_balances: removed_balances.rows_affected(),
    })
}

#[tauri::command]
pub async fn delete_location(app: AppHandle, id: i64) -> Result<DeleteLocationResult, String> {
    if id <= 0 {
        return Err("存放位置无效".into());
    }
    let mut connection = open_connection(&app).await?;
    delete_location_from_connection(&mut connection, id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn converts_sqlite_rows_to_json_without_losing_types() {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open database");
        let row = sqlx::query("SELECT 7 AS id, 1.5 AS quantity, '网线' AS name, NULL AS remark")
            .fetch_one(&mut connection)
            .await
            .expect("select row");
        let value = row_value(row).expect("convert row");

        assert_eq!(value["id"], 7);
        assert_eq!(value["quantity"], 1.5);
        assert_eq!(value["name"], "网线");
        assert!(value["remark"].is_null());
    }

    #[tokio::test]
    async fn binds_frontend_values_in_positional_order() {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open database");
        sqlx::query("CREATE TABLE sample(id INTEGER PRIMARY KEY, name TEXT, quantity REAL)")
            .execute(&mut connection)
            .await
            .expect("create table");

        bind_values(
            sqlx::query("INSERT INTO sample(name, quantity) VALUES ($1, $2)"),
            vec![Value::String("电缆".into()), Value::from(2.5)],
        )
        .execute(&mut connection)
        .await
        .expect("insert bound values");

        let row = sqlx::query("SELECT name, quantity FROM sample")
            .fetch_one(&mut connection)
            .await
            .expect("select inserted row");
        assert_eq!(row.try_get::<String, _>("name").unwrap(), "电缆");
        assert_eq!(row.try_get::<f64, _>("quantity").unwrap(), 2.5);
    }

    #[tokio::test]
    async fn deletes_location_after_cleaning_default_and_zero_balance_references() {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open database");
        for statement in [
            "PRAGMA foreign_keys=ON",
            "CREATE TABLE locations(id INTEGER PRIMARY KEY)",
            "CREATE TABLE materials(id INTEGER PRIMARY KEY, default_location_id INTEGER REFERENCES locations(id), updated_at TEXT)",
            "CREATE TABLE inventory_balances(id INTEGER PRIMARY KEY, location_id INTEGER REFERENCES locations(id), quantity REAL)",
            "CREATE TABLE stock_transactions(id INTEGER PRIMARY KEY, location_id INTEGER REFERENCES locations(id))",
            "INSERT INTO locations(id) VALUES (7)",
            "INSERT INTO materials(id,default_location_id) VALUES (1,7)",
            "INSERT INTO inventory_balances(id,location_id,quantity) VALUES (1,7,0)",
        ] {
            sqlx::query(statement)
                .execute(&mut connection)
                .await
                .expect("prepare fixture");
        }

        let result = delete_location_from_connection(&mut connection, 7)
            .await
            .expect("delete location");
        assert_eq!(result.rows_affected, 1);
        assert_eq!(result.cleared_material_defaults, 1);
        assert_eq!(result.removed_zero_balances, 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM locations")
                .fetch_one(&mut connection)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn keeps_location_when_real_business_history_exists() {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open database");
        for statement in [
            "CREATE TABLE locations(id INTEGER PRIMARY KEY)",
            "CREATE TABLE materials(id INTEGER PRIMARY KEY, default_location_id INTEGER, updated_at TEXT)",
            "CREATE TABLE inventory_balances(id INTEGER PRIMARY KEY, location_id INTEGER, quantity REAL)",
            "CREATE TABLE stock_transactions(id INTEGER PRIMARY KEY, location_id INTEGER)",
            "INSERT INTO locations(id) VALUES (9)",
            "INSERT INTO stock_transactions(id,location_id) VALUES (1,9)",
        ] {
            sqlx::query(statement)
                .execute(&mut connection)
                .await
                .expect("prepare fixture");
        }

        let error = delete_location_from_connection(&mut connection, 9)
            .await
            .expect_err("business history must prevent deletion");
        assert!(error.contains("出入库流水"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM locations WHERE id=9")
                .fetch_one(&mut connection)
                .await
                .unwrap(),
            1
        );
    }
}
