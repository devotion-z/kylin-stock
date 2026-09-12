use serde::Serialize;
use serde_json::{Map, Number, Value};
use sqlx::{
    sqlite::{SqliteArguments, SqliteQueryResult, SqliteRow},
    Column, Connection, Row, Sqlite, SqliteConnection, TypeInfo, ValueRef,
};
use std::{fs, path::PathBuf};
use tauri::AppHandle;

const DATABASE_FILE: &str = "kylin-stock.db";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResult {
    rows_affected: u64,
    last_insert_id: i64,
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_config = app
        .path_resolver()
        .app_config_dir()
        .ok_or_else(|| "无法获取应用数据目录".to_string())?;
    fs::create_dir_all(&app_config).map_err(|e| format!("无法创建应用数据目录：{e}"))?;
    Ok(app_config.join(DATABASE_FILE))
}

async fn open_connection(app: &AppHandle) -> Result<SqliteConnection, String> {
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
