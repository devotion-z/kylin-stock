use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use sqlx::Row;
use std::{fs, path::Path};
use tauri::AppHandle;

use crate::database::open_connection;

const MAX_FILE_SIZE: u64 = 15 * 1024 * 1024;
const MAX_ATTACHMENTS_PER_ENTITY: i64 = 10;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    id: i64,
    entity_type: String,
    entity_id: i64,
    file_name: String,
    mime_type: String,
    file_size: i64,
    created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentData {
    mime_type: String,
    data: String,
}

fn validate_entity_type(value: &str) -> Result<&str, String> {
    match value {
        "MATERIAL" | "TRANSACTION" => Ok(value),
        _ => Err("不支持的附件归属类型".into()),
    }
}

fn detect_image_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(&[0xff, 0xd8]) {
        Some("image/jpeg")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

fn mime_from_extension(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "jfif" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

async fn entity_exists(
    connection: &mut sqlx::SqliteConnection,
    entity_type: &str,
    entity_id: i64,
) -> Result<bool, String> {
    let table = match entity_type {
        "MATERIAL" => "materials",
        "TRANSACTION" => "stock_transactions",
        _ => return Err("不支持的附件归属类型".into()),
    };
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?)");
    sqlx::query_scalar::<_, bool>(&sql)
        .bind(entity_id)
        .fetch_one(connection)
        .await
        .map_err(|e| format!("无法校验附件所属记录：{e}"))
}

#[tauri::command]
pub async fn add_attachment(
    app: AppHandle,
    entity_type: String,
    entity_id: i64,
    source_path: String,
) -> Result<AttachmentInfo, String> {
    let entity_type = validate_entity_type(&entity_type)?;
    if entity_id <= 0 {
        return Err("附件所属记录无效".into());
    }

    let path = Path::new(&source_path);
    let metadata = fs::metadata(path).map_err(|e| format!("无法读取图片信息：{e}"))?;
    if !metadata.is_file() {
        return Err("所选内容不是图片文件".into());
    }
    if metadata.len() == 0 {
        return Err("不能添加空图片".into());
    }
    if metadata.len() > MAX_FILE_SIZE {
        return Err("单张图片不能超过 15 MB".into());
    }

    let data = fs::read(path).map_err(|e| format!("无法读取图片：{e}"))?;
    // A few Kylin/phone image exporters write non-standard JPEG marker
    // layouts even though the OS image viewer opens the file correctly. Keep
    // signature detection as the first choice and use a supported extension as
    // a compatibility fallback instead of dropping the receipt after booking.
    let mime_type = detect_image_mime(&data)
        .or_else(|| mime_from_extension(path))
        .ok_or_else(|| "仅支持 JPG、PNG、WebP、GIF 或 BMP 图片".to_string())?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| "图片文件名无效".to_string())?;

    let mut connection = open_connection(&app).await?;
    if !entity_exists(&mut connection, entity_type, entity_id).await? {
        return Err("要添加附件的业务记录不存在".into());
    }
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM attachments WHERE entity_type=? AND entity_id=?",
    )
    .bind(entity_type)
    .bind(entity_id)
    .fetch_one(&mut connection)
    .await
    .map_err(|e| format!("无法读取附件数量：{e}"))?;
    if count >= MAX_ATTACHMENTS_PER_ENTITY {
        return Err("每条记录最多添加 10 张图片".into());
    }

    let result = sqlx::query(
        r#"INSERT INTO attachments(entity_type,entity_id,file_name,mime_type,file_size,data,created_at)
           VALUES (?,?,?,?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))"#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(file_name)
    .bind(mime_type)
    .bind(metadata.len() as i64)
    .bind(data)
    .execute(&mut connection)
    .await
    .map_err(|e| format!("保存单据图片失败：{e}"))?;

    let id = result.last_insert_rowid();
    let row = sqlx::query(
        "SELECT id,entity_type,entity_id,file_name,mime_type,file_size,created_at FROM attachments WHERE id=?",
    )
    .bind(id)
    .fetch_one(&mut connection)
    .await
    .map_err(|e| format!("读取已保存附件失败：{e}"))?;
    Ok(AttachmentInfo {
        id: row.get("id"),
        entity_type: row.get("entity_type"),
        entity_id: row.get("entity_id"),
        file_name: row.get("file_name"),
        mime_type: row.get("mime_type"),
        file_size: row.get("file_size"),
        created_at: row.get("created_at"),
    })
}

#[tauri::command]
pub async fn list_attachments(
    app: AppHandle,
    entity_type: String,
    entity_id: i64,
) -> Result<Vec<AttachmentInfo>, String> {
    let entity_type = validate_entity_type(&entity_type)?;
    let mut connection = open_connection(&app).await?;
    let rows = sqlx::query(
        "SELECT id,entity_type,entity_id,file_name,mime_type,file_size,created_at FROM attachments WHERE entity_type=? AND entity_id=? ORDER BY id",
    )
    .bind(entity_type)
    .bind(entity_id)
    .fetch_all(&mut connection)
    .await
    .map_err(|e| format!("读取单据图片失败：{e}"))?;
    Ok(rows
        .into_iter()
        .map(|row| AttachmentInfo {
            id: row.get("id"),
            entity_type: row.get("entity_type"),
            entity_id: row.get("entity_id"),
            file_name: row.get("file_name"),
            mime_type: row.get("mime_type"),
            file_size: row.get("file_size"),
            created_at: row.get("created_at"),
        })
        .collect())
}

#[tauri::command]
pub async fn get_attachment_data(app: AppHandle, id: i64) -> Result<AttachmentData, String> {
    let mut connection = open_connection(&app).await?;
    let row = sqlx::query("SELECT mime_type,data FROM attachments WHERE id=?")
        .bind(id)
        .fetch_optional(&mut connection)
        .await
        .map_err(|e| format!("读取单据图片失败：{e}"))?
        .ok_or_else(|| "单据图片不存在".to_string())?;
    let bytes: Vec<u8> = row.get("data");
    Ok(AttachmentData {
        mime_type: row.get("mime_type"),
        data: STANDARD.encode(bytes),
    })
}

#[tauri::command]
pub async fn delete_attachment(app: AppHandle, id: i64) -> Result<(), String> {
    let mut connection = open_connection(&app).await?;
    let result = sqlx::query("DELETE FROM attachments WHERE id=?")
        .bind(id)
        .execute(&mut connection)
        .await
        .map_err(|e| format!("删除单据图片失败：{e}"))?;
    if result.rows_affected() == 0 {
        return Err("单据图片不存在".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_image_signatures() {
        assert_eq!(
            detect_image_mime(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(
            detect_image_mime(&[0xff, 0xd8, 0xff, 0xe0]),
            Some("image/jpeg")
        );
        assert_eq!(detect_image_mime(b"RIFF1234WEBPdata"), Some("image/webp"));
        assert_eq!(detect_image_mime(b"GIF89arest"), Some("image/gif"));
        assert_eq!(detect_image_mime(b"BMrest"), Some("image/bmp"));
    }

    #[test]
    fn rejects_files_that_only_claim_to_be_images() {
        assert_eq!(detect_image_mime(b"not an image"), None);
    }

    #[test]
    fn accepts_common_image_extensions_as_kylin_compatibility_fallback() {
        assert_eq!(
            mime_from_extension(Path::new("现场照片.JFIF")),
            Some("image/jpeg")
        );
        assert_eq!(
            mime_from_extension(Path::new("单据.jpg")),
            Some("image/jpeg")
        );
        assert_eq!(mime_from_extension(Path::new("说明.txt")), None);
    }
}
