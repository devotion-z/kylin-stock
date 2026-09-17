use serde::{Deserialize, Serialize};
use sqlx::{Connection, Row, SqliteConnection};
use std::{collections::HashMap, fs, path::PathBuf, process::Command};
use tauri::AppHandle;
use uuid::Uuid;

const DATABASE_FILE: &str = "kylin-stock.db";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockOperationInput {
    material_id: i64,
    location_id: i64,
    quantity: f64,
    occurred_at: String,
    related_unit: Option<String>,
    destination: Option<String>,
    handler: Option<String>,
    receiver: Option<String>,
    remark: Option<String>,
    adjustment_basis: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockTransferInput {
    material_id: i64,
    from_location_id: i64,
    to_location_id: i64,
    quantity: f64,
    occurred_at: String,
    handler: Option<String>,
    remark: Option<String>,
    adjustment_basis: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockTransactionUpdateInput {
    id: i64,
    material_id: i64,
    location_id: i64,
    quantity: f64,
    occurred_at: String,
    related_unit: Option<String>,
    destination: Option<String>,
    handler: Option<String>,
    receiver: Option<String>,
    remark: Option<String>,
    adjustment_basis: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryPageInput {
    keyword: Option<String>,
    unit: Option<String>,
    location_id: Option<i64>,
    page: i64,
    page_size: i64,
    known_total: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct InventoryPageRow {
    material_id: i64,
    material_name: String,
    unit_name: Option<String>,
    location_id: i64,
    location_name: String,
    quantity: f64,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryPageResult {
    rows: Vec<InventoryPageRow>,
    total: i64,
    page: i64,
    page_size: i64,
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
    let path = database_path(app)?;
    let url = format!("sqlite:{}", path.to_string_lossy());
    let mut connection = SqliteConnection::connect(&url)
        .await
        .map_err(|e| format!("无法连接业务数据库：{e}"))?;

    configure_connection(&mut connection).await?;
    Ok(connection)
}

async fn configure_connection(connection: &mut SqliteConnection) -> Result<(), String> {
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await
        .map_err(|e| format!("无法启用数据库外键约束：{e}"))?;
    sqlx::query("PRAGMA busy_timeout = 5000")
        .execute(&mut *connection)
        .await
        .map_err(|e| format!("无法设置数据库等待策略：{e}"))?;
    Ok(())
}

fn validate(input: &StockOperationInput) -> Result<(), String> {
    if input.material_id <= 0 {
        return Err("请选择物资".into());
    }
    if input.location_id <= 0 {
        return Err("请选择存放位置".into());
    }
    if !input.quantity.is_finite() || input.quantity <= 0.0 {
        return Err("数量必须大于 0".into());
    }
    let scaled = input.quantity * 100.0;
    if (scaled - scaled.round()).abs() > 1e-7 {
        return Err("数量最多只能有两位小数，系统不会自动四舍五入".into());
    }
    if input.occurred_at.trim().is_empty() {
        return Err("请选择业务日期".into());
    }
    Ok(())
}

fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn transaction_no(kind: &str) -> String {
    format!("{}-{}", kind, Uuid::new_v4().simple())
}

async fn begin_immediate(connection: &mut SqliteConnection) -> Result<(), String> {
    sqlx::query("BEGIN IMMEDIATE")
        .execute(connection)
        .await
        .map(|_| ())
        .map_err(|e| format!("无法开始库存事务：{e}"))
}

async fn rollback(connection: &mut SqliteConnection) {
    let _ = sqlx::query("ROLLBACK").execute(connection).await;
}

async fn commit(connection: &mut SqliteConnection) -> Result<(), String> {
    sqlx::query("COMMIT")
        .execute(connection)
        .await
        .map(|_| ())
        .map_err(|e| format!("库存事务提交失败：{e}"))
}

fn normalized_like(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{value}%"))
}

fn inventory_filter_sql(
    keyword: &Option<String>,
    unit: &Option<String>,
    location_id: Option<i64>,
) -> String {
    let mut conditions = vec!["b.quantity<>0"];
    if keyword.is_some() {
        conditions.push("m.name LIKE ?");
    }
    if unit.is_some() {
        conditions.push("COALESCE(u.name,'') LIKE ?");
    }
    if location_id.is_some() {
        conditions.push("b.location_id=?");
    }
    conditions.join(" AND ")
}

#[tauri::command]
pub async fn list_inventory_page(
    app: AppHandle,
    input: InventoryPageInput,
) -> Result<InventoryPageResult, String> {
    let page = input.page.max(1);
    let page_size = match input.page_size {
        100 | 200 | 500 => input.page_size,
        _ => 100,
    };
    let offset = (page - 1) * page_size;
    let keyword = normalized_like(&input.keyword);
    let unit = normalized_like(&input.unit);
    let location_id = input.location_id.filter(|id| *id > 0);
    let where_sql = inventory_filter_sql(&keyword, &unit, location_id);
    let total_expression = if input.known_total.is_some() {
        "? AS total_count"
    } else {
        "COUNT(*) OVER() AS total_count"
    };
    let sql = format!(
        r#"SELECT b.material_id,m.name AS material_name,u.name AS unit_name,
                  b.location_id,l.name AS location_name,CAST(b.quantity AS REAL) AS quantity,
                  b.updated_at,{total_expression}
           FROM inventory_balances b
           JOIN materials m ON m.id=b.material_id
           LEFT JOIN units u ON u.id=m.unit_id
           JOIN locations l ON l.id=b.location_id
           WHERE {where_sql}
           ORDER BY CASE WHEN l.name GLOB '[0-9]*' THEN CAST(l.name AS INTEGER) ELSE 2147483647 END,
                    l.name COLLATE NOCASE,m.name COLLATE NOCASE
           LIMIT ? OFFSET ?"#
    );

    let mut connection = open_connection(&app).await?;
    let mut query = sqlx::query(&sql);
    if let Some(total) = input.known_total {
        query = query.bind(total.max(0));
    }
    if let Some(value) = &keyword {
        query = query.bind(value);
    }
    if let Some(value) = &unit {
        query = query.bind(value);
    }
    if let Some(value) = location_id {
        query = query.bind(value);
    }
    let result_rows = query
        .bind(page_size)
        .bind(offset)
        .fetch_all(&mut connection)
        .await
        .map_err(|e| format!("库存分布查询失败：{e}"))?;

    let mut total = result_rows
        .first()
        .and_then(|row| row.try_get::<i64, _>("total_count").ok())
        .unwrap_or(0);
    if result_rows.is_empty() && input.known_total.is_none() && page > 1 {
        let count_sql = format!(
            r#"SELECT COUNT(*)
               FROM inventory_balances b
               JOIN materials m ON m.id=b.material_id
               LEFT JOIN units u ON u.id=m.unit_id
               JOIN locations l ON l.id=b.location_id
               WHERE {where_sql}"#
        );
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        if let Some(value) = &keyword {
            count_query = count_query.bind(value);
        }
        if let Some(value) = &unit {
            count_query = count_query.bind(value);
        }
        if let Some(value) = location_id {
            count_query = count_query.bind(value);
        }
        total = count_query
            .fetch_one(&mut connection)
            .await
            .map_err(|e| format!("库存分布统计失败：{e}"))?;
    }

    let rows = result_rows
        .into_iter()
        .map(|row| {
            Ok(InventoryPageRow {
                material_id: row.try_get("material_id")?,
                material_name: row.try_get("material_name")?,
                unit_name: row.try_get("unit_name")?,
                location_id: row.try_get("location_id")?,
                location_name: row.try_get("location_name")?,
                quantity: row.try_get("quantity")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(|e| format!("库存分布数据转换失败：{e}"))?;

    Ok(InventoryPageResult {
        rows,
        total,
        page,
        page_size,
    })
}

async fn stock_in_on_connection(
    connection: &mut SqliteConnection,
    input: &StockOperationInput,
) -> Result<String, String> {
    validate(input)?;
    begin_immediate(connection).await?;

    let transaction_no = transaction_no("IN");
    let result: Result<(), String> = async {
        sqlx::query(
            r#"INSERT INTO stock_transactions
              (transaction_no,type,material_id,location_id,quantity,occurred_at,related_unit,destination,handler,receiver,remark,adjustment_basis,created_at)
              VALUES (?,'IN',?,?,?,?,?,NULL,?,?,?, ?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))"#,
        )
        .bind(&transaction_no)
        .bind(input.material_id)
        .bind(input.location_id)
        .bind(input.quantity)
        .bind(input.occurred_at.trim())
        .bind(clean(&input.related_unit))
        .bind(clean(&input.handler))
        .bind(clean(&input.receiver))
        .bind(clean(&input.remark))
        .bind(clean(&input.adjustment_basis))
        .execute(&mut *connection)
        .await
        .map_err(|e| e.to_string())?;

        sqlx::query(
            r#"INSERT INTO inventory_balances(material_id,location_id,quantity,updated_at)
               VALUES (?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))
               ON CONFLICT(material_id,location_id) DO UPDATE SET
               quantity = quantity + excluded.quantity,
               updated_at = excluded.updated_at"#,
        )
        .bind(input.material_id)
        .bind(input.location_id)
        .bind(input.quantity)
        .execute(&mut *connection)
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }
    .await;

    if let Err(error) = result {
        rollback(connection).await;
        return Err(format!("入库登记失败：{error}"));
    }
    if let Err(error) = commit(connection).await {
        rollback(connection).await;
        return Err(error);
    }

    Ok(transaction_no)
}

async fn stock_out_on_connection(
    connection: &mut SqliteConnection,
    input: &StockOperationInput,
) -> Result<String, String> {
    validate(input)?;
    let destination = clean(&input.destination).ok_or_else(|| "出库去向不能为空".to_string())?;
    begin_immediate(connection).await?;

    // SQLite NUMERIC affinity may physically store whole values as INTEGER even
    // when they were bound from f64. Cast explicitly so SQLx always decodes REAL.
    let available = match sqlx::query_scalar::<_, f64>(
        "SELECT CAST(quantity AS REAL) FROM inventory_balances WHERE material_id=? AND location_id=?",
    )
    .bind(input.material_id)
    .bind(input.location_id)
    .fetch_optional(&mut *connection)
    .await
    {
        Ok(value) => value.unwrap_or(0.0),
        Err(error) => {
            rollback(connection).await;
            return Err(format!("读取当前库存失败：{error}"));
        }
    };

    if available + f64::EPSILON < input.quantity {
        rollback(connection).await;
        return Err(format!("库存不足，当前可用库存为 {available}"));
    }

    let transaction_no = transaction_no("OUT");
    let result: Result<(), String> = async {
        sqlx::query(
            r#"INSERT INTO stock_transactions
              (transaction_no,type,material_id,location_id,quantity,occurred_at,related_unit,destination,handler,receiver,remark,adjustment_basis,created_at)
              VALUES (?,'OUT',?,?,?,?,?,?,?,?,?, ?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))"#,
        )
        .bind(&transaction_no)
        .bind(input.material_id)
        .bind(input.location_id)
        .bind(input.quantity)
        .bind(input.occurred_at.trim())
        .bind(clean(&input.related_unit))
        .bind(&destination)
        .bind(clean(&input.handler))
        .bind(clean(&input.receiver))
        .bind(clean(&input.remark))
        .bind(clean(&input.adjustment_basis))
        .execute(&mut *connection)
        .await
        .map_err(|e| e.to_string())?;

        let update = sqlx::query(
            r#"UPDATE inventory_balances
               SET quantity = quantity - ?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
               WHERE material_id=? AND location_id=? AND quantity >= ?"#,
        )
        .bind(input.quantity)
        .bind(input.material_id)
        .bind(input.location_id)
        .bind(input.quantity)
        .execute(&mut *connection)
        .await
        .map_err(|e| e.to_string())?;

        if update.rows_affected() != 1 {
            return Err("库存余额发生变化，本次出库已取消，请重试".into());
        }

        Ok(())
    }
    .await;

    if let Err(error) = result {
        rollback(connection).await;
        return Err(format!("出库登记失败：{error}"));
    }
    if let Err(error) = commit(connection).await {
        rollback(connection).await;
        return Err(error);
    }

    Ok(transaction_no)
}

async fn stock_in_batch_on_connection(
    connection: &mut SqliteConnection,
    inputs: &[StockOperationInput],
) -> Result<Vec<String>, String> {
    if inputs.is_empty() {
        return Err("请至少添加一项物资".into());
    }
    for input in inputs {
        validate(input)?;
    }
    begin_immediate(connection).await?;
    let result: Result<Vec<String>, String> = async {
        let mut numbers = Vec::with_capacity(inputs.len());
        for input in inputs {
            let number = transaction_no("IN");
            sqlx::query(r#"INSERT INTO stock_transactions
              (transaction_no,type,material_id,location_id,quantity,occurred_at,related_unit,destination,handler,receiver,remark,adjustment_basis,created_at)
              VALUES (?,'IN',?,?,?,?,?,NULL,?,?,?, ?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))"#)
                .bind(&number).bind(input.material_id).bind(input.location_id).bind(input.quantity)
                .bind(input.occurred_at.trim()).bind(clean(&input.related_unit)).bind(clean(&input.handler))
                .bind(clean(&input.receiver)).bind(clean(&input.remark)).bind(clean(&input.adjustment_basis)).execute(&mut *connection).await.map_err(|e| e.to_string())?;
            sqlx::query(r#"INSERT INTO inventory_balances(material_id,location_id,quantity,updated_at)
               VALUES (?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))
               ON CONFLICT(material_id,location_id) DO UPDATE SET quantity=quantity+excluded.quantity, updated_at=excluded.updated_at"#)
                .bind(input.material_id).bind(input.location_id).bind(input.quantity).execute(&mut *connection).await.map_err(|e| e.to_string())?;
            numbers.push(number);
        }
        Ok(numbers)
    }.await;
    match result {
        Ok(numbers) => {
            commit(connection).await?;
            Ok(numbers)
        }
        Err(error) => {
            rollback(connection).await;
            Err(format!("批量入库失败：{error}"))
        }
    }
}

async fn stock_out_batch_on_connection(
    connection: &mut SqliteConnection,
    inputs: &[StockOperationInput],
) -> Result<Vec<String>, String> {
    if inputs.is_empty() {
        return Err("请至少添加一项物资".into());
    }
    for input in inputs {
        validate(input)?;
        if clean(&input.destination).is_none() {
            return Err("领用单位不能为空".into());
        }
    }
    begin_immediate(connection).await?;
    let result: Result<Vec<String>, String> = async {
        for input in inputs {
            let available = sqlx::query_scalar::<_, f64>("SELECT CAST(quantity AS REAL) FROM inventory_balances WHERE material_id=? AND location_id=?")
                .bind(input.material_id).bind(input.location_id).fetch_optional(&mut *connection).await.map_err(|e| e.to_string())?.unwrap_or(0.0);
            if available + f64::EPSILON < input.quantity { return Err(format!("物资库存不足，当前可用库存为 {available}")); }
        }
        let mut numbers = Vec::with_capacity(inputs.len());
        for input in inputs {
            let number = transaction_no("OUT");
            let destination = clean(&input.destination).ok_or_else(|| "领用单位不能为空".to_string())?;
            sqlx::query(r#"INSERT INTO stock_transactions
              (transaction_no,type,material_id,location_id,quantity,occurred_at,related_unit,destination,handler,receiver,remark,adjustment_basis,created_at)
              VALUES (?,'OUT',?,?,?,?,?,?,?,?,?, ?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))"#)
                .bind(&number).bind(input.material_id).bind(input.location_id).bind(input.quantity)
                .bind(input.occurred_at.trim()).bind(clean(&input.related_unit)).bind(&destination).bind(clean(&input.handler))
                .bind(clean(&input.receiver)).bind(clean(&input.remark)).bind(clean(&input.adjustment_basis)).execute(&mut *connection).await.map_err(|e| e.to_string())?;
            let update = sqlx::query("UPDATE inventory_balances SET quantity=quantity-?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE material_id=? AND location_id=? AND quantity>=?")
                .bind(input.quantity).bind(input.material_id).bind(input.location_id).bind(input.quantity).execute(&mut *connection).await.map_err(|e| e.to_string())?;
            if update.rows_affected() != 1 { return Err("库存余额发生变化，本批出库已取消，请重试".into()); }
            numbers.push(number);
        }
        Ok(numbers)
    }.await;
    match result {
        Ok(numbers) => {
            commit(connection).await?;
            Ok(numbers)
        }
        Err(error) => {
            rollback(connection).await;
            Err(format!("批量出库失败：{error}"))
        }
    }
}

#[tauri::command]
pub async fn stock_in(app: AppHandle, input: StockOperationInput) -> Result<String, String> {
    let mut connection = open_connection(&app).await?;
    stock_in_on_connection(&mut connection, &input).await
}

#[tauri::command]
pub async fn stock_out(app: AppHandle, input: StockOperationInput) -> Result<String, String> {
    let mut connection = open_connection(&app).await?;
    stock_out_on_connection(&mut connection, &input).await
}

#[tauri::command]
pub async fn stock_in_batch(
    app: AppHandle,
    inputs: Vec<StockOperationInput>,
) -> Result<Vec<String>, String> {
    let mut connection = open_connection(&app).await?;
    stock_in_batch_on_connection(&mut connection, &inputs).await
}

#[tauri::command]
pub async fn stock_out_batch(
    app: AppHandle,
    inputs: Vec<StockOperationInput>,
) -> Result<Vec<String>, String> {
    let mut connection = open_connection(&app).await?;
    stock_out_batch_on_connection(&mut connection, &inputs).await
}

/// Delete one ledger entry and reverse the inventory effect in the same
/// SQLite transaction. This is intended for removing mistaken/test entries;
/// historical records are never silently detached from their stock balance.
#[tauri::command]
pub async fn delete_stock_transaction(app: AppHandle, id: i64) -> Result<(), String> {
    if id <= 0 {
        return Err("无效的流水记录".into());
    }
    let mut connection = open_connection(&app).await?;
    begin_immediate(&mut connection).await?;
    let result: Result<(), String> = async {
        let record = sqlx::query_as::<_, (String, String, i64, i64, f64)>(
            "SELECT transaction_no,type,material_id,location_id,CAST(quantity AS REAL) FROM stock_transactions WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&mut connection)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "找不到要删除的流水记录".to_string())?;

        let (number, kind, material_id, location_id, quantity) = record;
        if number.starts_with("TRANSFER-") {
            return Err("库内倒库会生成成对流水，不能单独删除；如需纠正请执行反向倒库".into());
        }
        if kind == "IN" {
            let update = sqlx::query(
                "UPDATE inventory_balances SET quantity=quantity-?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE material_id=? AND location_id=? AND quantity>=?",
            )
            .bind(quantity)
            .bind(material_id)
            .bind(location_id)
            .bind(quantity)
            .execute(&mut connection)
            .await
            .map_err(|e| e.to_string())?;
            if update.rows_affected() != 1 {
                return Err("删除入库记录会导致库存为负数，请先处理后续出库记录".into());
            }
        } else if kind == "OUT" {
            sqlx::query(
                "INSERT INTO inventory_balances(material_id,location_id,quantity,updated_at) VALUES (?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(material_id,location_id) DO UPDATE SET quantity=quantity+excluded.quantity, updated_at=excluded.updated_at",
            )
            .bind(material_id)
            .bind(location_id)
            .bind(quantity)
            .execute(&mut connection)
            .await
            .map_err(|e| e.to_string())?;
        } else {
            return Err("调整类流水暂不支持直接删除".into());
        }

        sqlx::query("DELETE FROM attachments WHERE entity_type='TRANSACTION' AND entity_id=?")
            .bind(id)
            .execute(&mut connection)
            .await
            .map_err(|e| e.to_string())?;
        let deleted = sqlx::query("DELETE FROM stock_transactions WHERE id=?")
            .bind(id)
            .execute(&mut connection)
            .await
            .map_err(|e| e.to_string())?;
        if deleted.rows_affected() != 1 {
            return Err("流水记录删除失败，请重试".into());
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => commit(&mut connection).await,
        Err(error) => {
            rollback(&mut connection).await;
            Err(format!("删除流水失败：{error}"))
        }
    }
}

async fn update_stock_transaction_on_connection(
    connection: &mut SqliteConnection,
    input: &StockTransactionUpdateInput,
) -> Result<(), String> {
    if input.id <= 0 {
        return Err("无效的流水记录".into());
    }
    let validation_input = StockOperationInput {
        material_id: input.material_id,
        location_id: input.location_id,
        quantity: input.quantity,
        occurred_at: input.occurred_at.clone(),
        related_unit: input.related_unit.clone(),
        destination: input.destination.clone(),
        handler: input.handler.clone(),
        receiver: input.receiver.clone(),
        remark: input.remark.clone(),
        adjustment_basis: input.adjustment_basis.clone(),
    };
    validate(&validation_input)?;

    begin_immediate(connection).await?;
    let result: Result<(), String> = async {
        let old = sqlx::query_as::<_, (String, String, i64, i64, f64)>(
            "SELECT transaction_no,type,material_id,location_id,CAST(quantity AS REAL) FROM stock_transactions WHERE id=?",
        )
        .bind(input.id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "找不到要编辑的流水记录".to_string())?;

        let (number, kind, old_material_id, old_location_id, old_quantity) = old;
        if number.starts_with("TRANSFER-") {
            return Err("库内倒库会生成成对流水，不能单独编辑其中一条；请在物资分布中重新倒库".into());
        }
        if kind != "IN" && kind != "OUT" {
            return Err("调整类流水暂不支持编辑".into());
        }
        let destination = if kind == "OUT" {
            Some(clean(&input.destination).ok_or_else(|| "领用单位不能为空".to_string())?)
        } else {
            None
        };

        // Calculate the net effect for each affected material/location first.
        // This avoids a false failure when an entry is edited in place (for
        // example changing one inbound quantity from 100 to 120 while only 50
        // remains after later outbound operations).
        let old_effect = if kind == "IN" { old_quantity } else { -old_quantity };
        let new_effect = if kind == "IN" { input.quantity } else { -input.quantity };
        let mut deltas: HashMap<(i64, i64), f64> = HashMap::new();
        *deltas.entry((old_material_id, old_location_id)).or_default() -= old_effect;
        *deltas.entry((input.material_id, input.location_id)).or_default() += new_effect;

        for ((material_id, location_id), delta) in deltas {
            if delta.abs() <= f64::EPSILON {
                continue;
            }
            let current = sqlx::query_scalar::<_, f64>(
                "SELECT CAST(quantity AS REAL) FROM inventory_balances WHERE material_id=? AND location_id=?",
            )
            .bind(material_id)
            .bind(location_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or(0.0);
            let corrected = current + delta;
            if corrected < -f64::EPSILON {
                return Err(format!(
                    "修改后会导致库存为负数（当前 {current}，变化 {delta}），请先检查后续业务记录"
                ));
            }
            sqlx::query(
                r#"INSERT INTO inventory_balances(material_id,location_id,quantity,updated_at)
                   VALUES (?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                   ON CONFLICT(material_id,location_id) DO UPDATE SET
                   quantity=excluded.quantity,updated_at=excluded.updated_at"#,
            )
            .bind(material_id)
            .bind(location_id)
            .bind(if corrected.abs() <= f64::EPSILON { 0.0 } else { corrected })
            .execute(&mut *connection)
            .await
            .map_err(|e| e.to_string())?;
        }

        let updated = sqlx::query(
            r#"UPDATE stock_transactions SET
               material_id=?,location_id=?,quantity=?,occurred_at=?,related_unit=?,destination=?,
               handler=?,receiver=?,remark=?,adjustment_basis=? WHERE id=?"#,
        )
        .bind(input.material_id)
        .bind(input.location_id)
        .bind(input.quantity)
        .bind(input.occurred_at.trim())
        .bind(clean(&input.related_unit))
        .bind(destination)
        .bind(clean(&input.handler))
        .bind(clean(&input.receiver))
        .bind(clean(&input.remark))
        .bind(clean(&input.adjustment_basis))
        .bind(input.id)
        .execute(&mut *connection)
        .await
        .map_err(|e| e.to_string())?;
        if updated.rows_affected() != 1 {
            return Err("流水记录更新失败，请重试".into());
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => commit(connection).await,
        Err(error) => {
            rollback(connection).await;
            Err(format!("编辑流水失败：{error}"))
        }
    }
}

/// Correct a mistaken inbound/outbound entry and apply the exact net change to
/// inventory in the same SQLite transaction. Existing attachment ids remain
/// attached to the corrected ledger row.
#[tauri::command]
pub async fn update_stock_transaction(
    app: AppHandle,
    input: StockTransactionUpdateInput,
) -> Result<(), String> {
    let mut connection = open_connection(&app).await?;
    update_stock_transaction_on_connection(&mut connection, &input).await
}

/// Move stock between two locations and record both sides of the internal
/// transfer so inventory, distribution and ledger views stay consistent.
#[tauri::command]
pub async fn transfer_stock(app: AppHandle, input: StockTransferInput) -> Result<(), String> {
    if input.material_id <= 0 || input.from_location_id <= 0 || input.to_location_id <= 0 {
        return Err("请选择物资和库位".into());
    }
    if input.from_location_id == input.to_location_id {
        return Err("转入库位不能与原库位相同".into());
    }
    if !input.quantity.is_finite() || input.quantity <= 0.0 {
        return Err("转移数量必须大于 0".into());
    }
    let scaled = input.quantity * 100.0;
    if (scaled - scaled.round()).abs() > 1e-7 {
        return Err("数量最多只能有两位小数，系统不会自动四舍五入".into());
    }
    if input.occurred_at.trim().is_empty() {
        return Err("请选择业务日期".into());
    }

    let mut connection = open_connection(&app).await?;
    begin_immediate(&mut connection).await?;
    let result: Result<(), String> = async {
        let source_name = sqlx::query_scalar::<_, String>("SELECT name FROM locations WHERE id=?")
            .bind(input.from_location_id)
            .fetch_optional(&mut connection)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "原存放位置不存在或已停用".to_string())?;
        let target_name = sqlx::query_scalar::<_, String>("SELECT name FROM locations WHERE id=? AND status=1")
            .bind(input.to_location_id)
            .fetch_optional(&mut connection)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "转入存放位置不存在或已停用".to_string())?;

        let update = sqlx::query(
            "UPDATE inventory_balances SET quantity=quantity-?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE material_id=? AND location_id=? AND quantity>=?",
        )
        .bind(input.quantity)
        .bind(input.material_id)
        .bind(input.from_location_id)
        .bind(input.quantity)
        .execute(&mut connection)
        .await
        .map_err(|e| e.to_string())?;
        if update.rows_affected() != 1 {
            return Err("原库位库存不足，转库已取消".into());
        }

        sqlx::query(
            "INSERT INTO inventory_balances(material_id,location_id,quantity,updated_at) VALUES (?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(material_id,location_id) DO UPDATE SET quantity=quantity+excluded.quantity, updated_at=excluded.updated_at",
        )
        .bind(input.material_id)
        .bind(input.to_location_id)
        .bind(input.quantity)
        .execute(&mut connection)
        .await
        .map_err(|e| e.to_string())?;

        let basis = clean(&input.adjustment_basis).unwrap_or_else(|| "库内调拨".to_string());
        let remark = clean(&input.remark).unwrap_or_else(|| format!("库内调拨：{source_name} → {target_name}"));
        let out_no = transaction_no("TRANSFER-OUT");
        let in_no = transaction_no("TRANSFER-IN");
        sqlx::query("INSERT INTO stock_transactions(transaction_no,type,material_id,location_id,quantity,occurred_at,related_unit,destination,handler,receiver,remark,adjustment_basis,created_at) VALUES (?,'OUT',?,?,?,?, '库内调拨',?,?,?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))")
            .bind(&out_no).bind(input.material_id).bind(input.from_location_id).bind(input.quantity)
            .bind(input.occurred_at.trim()).bind(&target_name).bind(clean(&input.handler))
            .bind(None::<String>).bind(&remark).bind(&basis).execute(&mut connection).await.map_err(|e| e.to_string())?;
        sqlx::query("INSERT INTO stock_transactions(transaction_no,type,material_id,location_id,quantity,occurred_at,related_unit,destination,handler,receiver,remark,adjustment_basis,created_at) VALUES (?,'IN',?,?,?,?, '库内调拨',NULL,?,?,?, ?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))")
            .bind(&in_no).bind(input.material_id).bind(input.to_location_id).bind(input.quantity)
            .bind(input.occurred_at.trim()).bind(clean(&input.handler))
            .bind(None::<String>).bind(&remark).bind(&basis).execute(&mut connection).await.map_err(|e| e.to_string())?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => commit(&mut connection).await,
        Err(error) => {
            rollback(&mut connection).await;
            Err(format!("转库失败：{error}"))
        }
    }
}

/// Remove a material's balance and all ledger rows for one location. This is
/// deliberately explicit because it is intended for cleaning test data.
#[tauri::command]
pub async fn delete_inventory_position(
    app: AppHandle,
    material_id: i64,
    location_id: i64,
) -> Result<(), String> {
    if material_id <= 0 || location_id <= 0 {
        return Err("无效的物资或存放位置".into());
    }
    let mut connection = open_connection(&app).await?;
    begin_immediate(&mut connection).await?;
    let result: Result<(), String> = async {
        let deleted = sqlx::query("DELETE FROM attachments WHERE entity_type='TRANSACTION' AND entity_id IN (SELECT id FROM stock_transactions WHERE material_id=? AND location_id=?)")
            .bind(material_id).bind(location_id).execute(&mut connection).await.map_err(|e| e.to_string())?;
        let _ = deleted;
        sqlx::query("DELETE FROM stock_transactions WHERE material_id=? AND location_id=?")
            .bind(material_id).bind(location_id).execute(&mut connection).await.map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM inventory_balances WHERE material_id=? AND location_id=?")
            .bind(material_id).bind(location_id).execute(&mut connection).await.map_err(|e| e.to_string())?;
        Ok(())
    }.await;
    match result {
        Ok(()) => commit(&mut connection).await,
        Err(error) => {
            rollback(&mut connection).await;
            Err(format!("清除库存失败：{error}"))
        }
    }
}

/// Run the optional system OCR engine against a scanned transfer document.
/// Kylin deployments can install tesseract-ocr-chi-sim; the UI treats the
/// returned text as a draft and always lets the operator verify quantities.
#[tauri::command]
pub async fn scan_document(source_path: String) -> Result<String, String> {
    let path = source_path.trim();
    if path.is_empty() {
        return Err("请选择扫描单据图片".into());
    }
    let source = if let Some(path) = path.strip_prefix("file://") {
        PathBuf::from(path)
    } else {
        PathBuf::from(path)
    };
    if !source.is_file() {
        return Err(format!(
            "扫描单据文件不存在或无法读取：{}",
            source.display()
        ));
    }

    // Use a short ASCII temporary path. This avoids tesseract/pixRead failures
    // on some Kylin builds when the selected image path contains Chinese
    // characters, spaces, URI prefixes, or a transient clipboard directory.
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("img");
    let temporary = std::env::temp_dir().join(format!(
        "kylin-stock-ocr-{}.{}",
        Uuid::new_v4().simple(),
        extension
    ));
    fs::copy(&source, &temporary).map_err(|error| format!("无法准备扫描单据：{error}"))?;
    let temporary_path = temporary.to_string_lossy().to_string();
    let output = Command::new("tesseract")
        .args([
            temporary_path.as_str(),
            "stdout",
            "-l",
            "chi_sim+eng",
            "--psm",
            "6",
        ])
        .output()
        .map_err(|_| "未检测到 OCR 引擎，请在麒麟系统安装 tesseract-ocr 和中文语言包".to_string());
    let _ = fs::remove_file(&temporary);
    let output = output?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            "扫描单据识别失败，请检查图片清晰度".into()
        } else {
            format!("扫描单据识别失败：{detail}")
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open in-memory sqlite");
        configure_connection(&mut connection)
            .await
            .expect("configure sqlite");

        sqlx::query(
            r#"CREATE TABLE inventory_balances (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                material_id INTEGER NOT NULL,
                location_id INTEGER NOT NULL,
                quantity NUMERIC NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                UNIQUE(material_id, location_id)
            )"#,
        )
        .execute(&mut connection)
        .await
        .expect("create inventory_balances");

        sqlx::query(
            r#"CREATE TABLE stock_transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                transaction_no TEXT NOT NULL UNIQUE,
                type TEXT NOT NULL CHECK(type IN ('IN','OUT','ADJUST')),
                material_id INTEGER NOT NULL,
                location_id INTEGER NOT NULL,
                quantity NUMERIC NOT NULL CHECK(quantity > 0),
                occurred_at TEXT NOT NULL,
                related_unit TEXT,
                destination TEXT,
                handler TEXT,
                receiver TEXT,
                remark TEXT,
                adjustment_basis TEXT,
                created_at TEXT NOT NULL
            )"#,
        )
        .execute(&mut connection)
        .await
        .expect("create stock_transactions");

        connection
    }

    fn input(quantity: f64) -> StockOperationInput {
        StockOperationInput {
            material_id: 1,
            location_id: 1,
            quantity,
            occurred_at: "2026-08-16T00:00:00.000Z".into(),
            related_unit: Some("测试单位".into()),
            destination: None,
            handler: Some("测试经办人".into()),
            receiver: None,
            remark: Some("自动化测试".into()),
            adjustment_basis: None,
        }
    }

    #[test]
    fn quantity_precision_is_rejected_instead_of_rounded() {
        assert!(validate(&input(1.23)).is_ok());
        let error = validate(&input(1.234)).expect_err("three decimals must be rejected");
        assert!(error.contains("两位小数"));
    }

    async fn balance(connection: &mut SqliteConnection) -> f64 {
        balance_at(connection, 1, 1).await
    }

    async fn balance_at(
        connection: &mut SqliteConnection,
        material_id: i64,
        location_id: i64,
    ) -> f64 {
        sqlx::query_scalar::<_, f64>(
            "SELECT CAST(COALESCE(quantity,0) AS REAL) FROM inventory_balances WHERE material_id=? AND location_id=?",
        )
        .bind(material_id)
        .bind(location_id)
        .fetch_optional(connection)
        .await
        .expect("read balance")
        .unwrap_or(0.0)
    }

    async fn transaction_count(connection: &mut SqliteConnection, kind: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stock_transactions WHERE type=?")
            .bind(kind)
            .fetch_one(connection)
            .await
            .expect("count transactions")
    }

    fn update_input(id: i64, quantity: f64) -> StockTransactionUpdateInput {
        StockTransactionUpdateInput {
            id,
            material_id: 1,
            location_id: 1,
            quantity,
            occurred_at: "2026-08-17T00:00:00.000Z".into(),
            related_unit: Some("修正来源单位".into()),
            destination: None,
            handler: Some("修正经办人".into()),
            receiver: None,
            remark: Some("修正错账".into()),
            adjustment_basis: Some("更正单".into()),
        }
    }

    #[tokio::test]
    async fn stock_in_creates_ledger_and_balance_atomically() {
        let mut connection = test_connection().await;
        let tx = stock_in_on_connection(&mut connection, &input(10.0))
            .await
            .expect("stock in succeeds");

        assert!(tx.starts_with("IN-"));
        assert_eq!(balance(&mut connection).await, 10.0);
        assert_eq!(transaction_count(&mut connection, "IN").await, 1);
    }

    #[tokio::test]
    async fn stock_out_decrements_balance_and_records_destination() {
        let mut connection = test_connection().await;
        stock_in_on_connection(&mut connection, &input(10.0))
            .await
            .expect("seed stock");

        let mut outbound = input(3.0);
        outbound.destination = Some("一车间".into());
        outbound.receiver = Some("张三".into());
        let tx = stock_out_on_connection(&mut connection, &outbound)
            .await
            .expect("stock out succeeds");

        assert!(tx.starts_with("OUT-"));
        assert_eq!(balance(&mut connection).await, 7.0);
        assert_eq!(transaction_count(&mut connection, "OUT").await, 1);
        let destination = sqlx::query_scalar::<_, String>(
            "SELECT destination FROM stock_transactions WHERE type='OUT' LIMIT 1",
        )
        .fetch_one(&mut connection)
        .await
        .expect("read destination");
        assert_eq!(destination, "一车间");
    }

    #[tokio::test]
    async fn insufficient_stock_leaves_balance_and_ledger_unchanged() {
        let mut connection = test_connection().await;
        stock_in_on_connection(&mut connection, &input(5.0))
            .await
            .expect("seed stock");

        let mut outbound = input(6.0);
        outbound.destination = Some("XX项目".into());
        let error = stock_out_on_connection(&mut connection, &outbound)
            .await
            .expect_err("insufficient stock must fail");

        assert!(error.contains("库存不足"));
        assert_eq!(balance(&mut connection).await, 5.0);
        assert_eq!(transaction_count(&mut connection, "OUT").await, 0);
    }

    #[tokio::test]
    async fn outbound_without_destination_is_rejected_before_mutation() {
        let mut connection = test_connection().await;
        stock_in_on_connection(&mut connection, &input(5.0))
            .await
            .expect("seed stock");

        let error = stock_out_on_connection(&mut connection, &input(1.0))
            .await
            .expect_err("missing destination must fail");

        assert!(error.contains("出库去向不能为空"));
        assert_eq!(balance(&mut connection).await, 5.0);
        assert_eq!(transaction_count(&mut connection, "OUT").await, 0);
    }

    #[tokio::test]
    async fn batch_in_records_all_lines_in_one_operation() {
        let mut connection = test_connection().await;
        let mut second = input(2.5);
        second.material_id = 2;
        let numbers = stock_in_batch_on_connection(&mut connection, &[input(10.0), second])
            .await
            .expect("batch stock in succeeds");
        assert_eq!(numbers.len(), 2);
        assert_eq!(transaction_count(&mut connection, "IN").await, 2);
    }

    #[tokio::test]
    async fn editing_inbound_quantity_updates_balance_atomically() {
        let mut connection = test_connection().await;
        stock_in_on_connection(&mut connection, &input(10.0))
            .await
            .expect("seed stock");
        let id = sqlx::query_scalar::<_, i64>("SELECT id FROM stock_transactions LIMIT 1")
            .fetch_one(&mut connection)
            .await
            .expect("read transaction id");

        update_stock_transaction_on_connection(&mut connection, &update_input(id, 12.5))
            .await
            .expect("edit transaction");

        assert_eq!(balance(&mut connection).await, 12.5);
        let corrected = sqlx::query_as::<_, (f64, String)>(
            "SELECT CAST(quantity AS REAL),remark FROM stock_transactions WHERE id=?",
        )
        .bind(id)
        .fetch_one(&mut connection)
        .await
        .expect("read corrected transaction");
        assert_eq!(corrected, (12.5, "修正错账".into()));
    }

    #[tokio::test]
    async fn moving_inbound_to_another_location_updates_outbound_availability() {
        let mut connection = test_connection().await;
        stock_in_on_connection(&mut connection, &input(10.0))
            .await
            .expect("seed stock in location 1");
        let id = sqlx::query_scalar::<_, i64>("SELECT id FROM stock_transactions LIMIT 1")
            .fetch_one(&mut connection)
            .await
            .expect("read transaction id");
        let mut correction = update_input(id, 10.0);
        correction.location_id = 4;

        update_stock_transaction_on_connection(&mut connection, &correction)
            .await
            .expect("move inbound to location 4");

        assert_eq!(balance_at(&mut connection, 1, 1).await, 0.0);
        assert_eq!(balance_at(&mut connection, 1, 4).await, 10.0);
        let mut outbound = input(3.0);
        outbound.location_id = 4;
        outbound.destination = Some("四号库领用测试".into());
        stock_out_on_connection(&mut connection, &outbound)
            .await
            .expect("outbound from corrected location succeeds");
        assert_eq!(balance_at(&mut connection, 1, 4).await, 7.0);
    }

    #[tokio::test]
    async fn editing_inbound_below_consumed_quantity_rolls_back() {
        let mut connection = test_connection().await;
        stock_in_on_connection(&mut connection, &input(10.0))
            .await
            .expect("seed stock");
        let mut outbound = input(8.0);
        outbound.destination = Some("测试领用单位".into());
        stock_out_on_connection(&mut connection, &outbound)
            .await
            .expect("consume stock");
        let id = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM stock_transactions WHERE type='IN' LIMIT 1",
        )
        .fetch_one(&mut connection)
        .await
        .expect("read inbound id");

        let error = update_stock_transaction_on_connection(&mut connection, &update_input(id, 5.0))
            .await
            .expect_err("negative balance edit must fail");

        assert!(error.contains("库存为负数"));
        assert_eq!(balance(&mut connection).await, 2.0);
        let original = sqlx::query_scalar::<_, f64>(
            "SELECT CAST(quantity AS REAL) FROM stock_transactions WHERE id=?",
        )
        .bind(id)
        .fetch_one(&mut connection)
        .await
        .expect("read original transaction");
        assert_eq!(original, 10.0);
    }
}
