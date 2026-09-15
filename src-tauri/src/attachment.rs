use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::ImageOutputFormat;
use serde::Serialize;
use sqlx::Row;
use std::{fs, io::Cursor, path::Path, process::Command};
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

fn normalize_image_for_preview(data: &[u8]) -> Result<(&'static str, Vec<u8>), String> {
    let image = image::load_from_memory(data)
        .map_err(|_| "图片内容已损坏或格式不兼容，请换一张图片后重试".to_string())?;
    let output_format = if image.color().has_alpha() {
        ImageOutputFormat::Png
    } else {
        ImageOutputFormat::Jpeg(90)
    };
    let mime_type = if image.color().has_alpha() {
        "image/png"
    } else {
        "image/jpeg"
    };
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, output_format)
        .map_err(|e| format!("无法转换图片用于预览：{e}"))?;
    Ok((mime_type, output.into_inner()))
}

fn prepare_image_for_storage(
    path: &Path,
    source_data: Vec<u8>,
) -> Result<(&'static str, Vec<u8>), String> {
    let extension_mime = mime_from_extension(path)
        .ok_or_else(|| "仅支持 JPG、PNG、WebP、GIF 或 BMP 图片".to_string())?;
    let fallback_mime = detect_image_mime(&source_data).unwrap_or(extension_mime);
    Ok(normalize_image_for_preview(&source_data).unwrap_or_else(|_| (fallback_mime, source_data)))
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

    let source_data = fs::read(path).map_err(|e| format!("无法读取图片：{e}"))?;
    // Decode and re-encode standard images for reliable WebKitGTK display.
    // Scanner files with a supported suffix but non-standard bytes are still
    // preserved and can be opened through the operating system image viewer.
    let (mime_type, data) = prepare_image_for_storage(path, source_data)?;
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
    .bind(data.len() as i64)
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
    let row = sqlx::query("SELECT data FROM attachments WHERE id=?")
        .bind(id)
        .fetch_optional(&mut connection)
        .await
        .map_err(|e| format!("读取单据图片失败：{e}"))?
        .ok_or_else(|| "单据图片不存在".to_string())?;
    let bytes: Vec<u8> = row.get("data");
    // Older releases stored the original bytes, so normalize on read as well.
    // This repairs previews for already-imported compatible images without
    // asking the user to re-enter the stock transaction.
    let (mime_type, preview_bytes) = normalize_image_for_preview(&bytes)?;
    Ok(AttachmentData {
        mime_type: mime_type.to_string(),
        data: STANDARD.encode(preview_bytes),
    })
}

#[tauri::command]
pub async fn open_attachment_external(app: AppHandle, id: i64) -> Result<(), String> {
    let mut connection = open_connection(&app).await?;
    let row = sqlx::query("SELECT file_name,data FROM attachments WHERE id=?")
        .bind(id)
        .fetch_optional(&mut connection)
        .await
        .map_err(|e| format!("读取单据图片失败：{e}"))?
        .ok_or_else(|| "单据图片不存在".to_string())?;
    let file_name: String = row.get("file_name");
    let bytes: Vec<u8> = row.get("data");
    let extension = Path::new(&file_name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| {
            value
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        })
        .unwrap_or("jpg");
    let cache_dir = app
        .path_resolver()
        .app_cache_dir()
        .ok_or_else(|| "无法获取图片预览目录".to_string())?
        .join("attachment-preview");
    fs::create_dir_all(&cache_dir).map_err(|e| format!("无法创建图片预览目录：{e}"))?;
    let preview_path = cache_dir.join(format!("单据图片-{id}.{extension}"));
    fs::write(&preview_path, bytes).map_err(|e| format!("无法准备图片预览：{e}"))?;

    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    command
        .arg(&preview_path)
        .spawn()
        .map_err(|e| format!("无法调用系统图片查看器：{e}"))?;
    Ok(())
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
    fn normalizes_supported_images_and_rejects_broken_data() {
        let source = image::DynamicImage::new_rgb8(2, 2);
        let mut encoded = Cursor::new(Vec::new());
        source
            .write_to(&mut encoded, ImageOutputFormat::Png)
            .expect("encode fixture");

        let (mime_type, normalized) =
            normalize_image_for_preview(&encoded.into_inner()).expect("normalize image");
        assert_eq!(mime_type, "image/jpeg");
        assert_eq!(detect_image_mime(&normalized), Some("image/jpeg"));
        assert!(normalize_image_for_preview(b"not an image").is_err());
    }

    #[test]
    fn accepts_supported_extensions_for_system_viewer_fallback() {
        assert_eq!(
            mime_from_extension(Path::new("调拨单据.JFIF")),
            Some("image/jpeg")
        );
        assert_eq!(
            mime_from_extension(Path::new("调拨单据.jpg")),
            Some("image/jpeg")
        );
        assert_eq!(mime_from_extension(Path::new("说明.txt")), None);

        let unusual_bytes = b"scanner-specific image payload".to_vec();
        let (mime_type, stored) = prepare_image_for_storage(
            Path::new("2026-09-14-11-24-15_001.jpg"),
            unusual_bytes.clone(),
        )
        .expect("accept non-standard scanner JPG");
        assert_eq!(mime_type, "image/jpeg");
        assert_eq!(stored, unusual_bytes);
    }
}
