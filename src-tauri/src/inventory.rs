use serde::Deserialize;
use sqlx::{Connection, SqliteConnection};
use std::{fs, path::PathBuf, process::Command};
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
        let record = sqlx::query_as::<_, (String, i64, i64, f64)>(
            "SELECT type, material_id, location_id, CAST(quantity AS REAL) FROM stock_transactions WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&mut connection)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "找不到要删除的流水记录".to_string())?;

        let (kind, material_id, location_id, quantity) = record;
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

/// Run the optional system OCR engine against a scanned transfer document.
/// Kylin deployments can install tesseract-ocr-chi-sim; the UI treats the
/// returned text as a draft and always lets the operator verify quantities.
#[tauri::command]
pub async fn scan_document(source_path: String) -> Result<String, String> {
    let path = source_path.trim();
    if path.is_empty() {
        return Err("请选择扫描单据图片".into());
    }
    let output = Command::new("tesseract")
        .args([path, "stdout", "-l", "chi_sim+eng", "--psm", "6"])
        .output()
        .map_err(|_| {
            "未检测到 OCR 引擎，请在麒麟系统安装 tesseract-ocr 和中文语言包".to_string()
        })?;
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
        sqlx::query_scalar::<_, f64>(
            "SELECT CAST(COALESCE(quantity,0) AS REAL) FROM inventory_balances WHERE material_id=1 AND location_id=1",
        )
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
}
