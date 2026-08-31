use crate::daemon::types::{MediaRef, MediaUploadResponse};
use crate::workshop_transport::MultipartField;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::workshop_http;
use super::DaemonState;

const MAX_MEDIA_BYTES: usize = 25 * 1024 * 1024;
const MAX_MEDIA_BASE64_BYTES: usize = MAX_MEDIA_BYTES.div_ceil(3) * 4;

#[derive(Debug, Serialize)]
pub struct MediaPayloadResponse {
    pub mime: String,
    pub bytes_base64: String,
}

#[derive(Debug, Serialize)]
pub struct MediaPathResponse {
    pub filename: String,
    pub mime: String,
    pub bytes_base64: String,
}

#[tauri::command]
pub async fn media_upload(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: String,
    filename: String,
    mime: String,
    bytes_base64: String,
    label: Option<String>,
) -> Result<MediaUploadResponse, String> {
    if bytes_base64.len() > MAX_MEDIA_BASE64_BYTES {
        return Err("file exceeds max size".to_string());
    }
    let bytes = STANDARD
        .decode(bytes_base64)
        .map_err(|_| "attachment data is not valid base64".to_string())?;
    media_upload_bytes(
        state,
        _embedded_state,
        session_id,
        filename,
        mime,
        bytes,
        label,
    )
    .await
}

async fn media_upload_bytes(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: String,
    filename: String,
    mime: String,
    bytes: Vec<u8>,
    label: Option<String>,
) -> Result<MediaUploadResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    if bytes.is_empty() {
        return Err("empty file".to_string());
    }
    if bytes.len() > MAX_MEDIA_BYTES {
        return Err("file exceeds max size".to_string());
    }

    let filename = filename.trim();
    let filename = if filename.is_empty() {
        "attachment".to_string()
    } else {
        filename.to_string()
    };
    let mime = mime.trim();
    let mime = if mime.is_empty() {
        "application/octet-stream".to_string()
    } else {
        mime.to_string()
    };

    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        let effective_label = label
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(filename.as_str());
        return client
            .upload_media(
                session_id,
                &bytes,
                &mime,
                (!effective_label.is_empty()).then_some(effective_label),
            )
            .map_err(|error| error.to_string());
    }

    let mut fields = vec![MultipartField {
        name: "file".to_string(),
        filename: Some(filename),
        mime: Some(mime),
        data: bytes,
    }];
    if let Some(label) = label
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        fields.push(MultipartField {
            name: "label".to_string(),
            filename: None,
            mime: None,
            data: label.into_bytes(),
        });
    }

    workshop_http::post_multipart(
        &state,
        &format!("/v1/media/upload?session_id={session_id}"),
        &fields,
    )
    .await
}

#[tauri::command]
pub async fn media_upload_path(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: String,
    path: String,
    label: Option<String>,
) -> Result<MediaUploadResponse, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("path is required".to_string());
    }
    let metadata = std::fs::metadata(path).map_err(|err| err.to_string())?;
    if metadata.len() > MAX_MEDIA_BYTES as u64 {
        return Err("file exceeds max size".to_string());
    }
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("attachment")
        .to_string();
    media_upload_bytes(
        state,
        _embedded_state,
        session_id,
        filename,
        guess_mime_from_path(path),
        bytes,
        label,
    )
    .await
}

#[tauri::command]
pub async fn media_read(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: String,
    media_id: String,
) -> Result<MediaPayloadResponse, String> {
    let session_id = required_value(&session_id, "session_id")?;
    let media_id = required_value(&media_id, "media_id")?;

    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        let (stored_mime, bytes) = client
            .read_media(session_id, media_id)
            .map_err(|error| error.to_string())?;
        let mime = preview_image_mime(&bytes, Some(&stored_mime))?;
        return Ok(MediaPayloadResponse {
            mime,
            bytes_base64: STANDARD.encode(bytes),
        });
    }

    let path = format!(
        "/v1/media/{}?session_id={}",
        urlencoding::encode(media_id),
        urlencoding::encode(session_id)
    );
    let mut stream = workshop_http::get_bytes_stream(&state, &path).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > MAX_MEDIA_BYTES {
            return Err("media exceeds max preview size".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    let mime = preview_image_mime(&bytes, None)?;
    Ok(MediaPayloadResponse {
        mime,
        bytes_base64: STANDARD.encode(bytes),
    })
}

/// Read a native dropped image so formats that are not web/provider-safe can be
/// normalized by the same frontend pipeline used by browser file pickers.
#[tauri::command]
pub async fn media_read_image_path(path: String) -> Result<MediaPathResponse, String> {
    let path = required_value(&path, "path")?;
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_MEDIA_BYTES as u64 {
        return Err("file exceeds max size".to_string());
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let mime = preview_image_mime(&bytes, Some(&guess_mime_from_path(path)))?;
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("attachment")
        .to_string();
    Ok(MediaPathResponse {
        filename,
        mime,
        bytes_base64: STANDARD.encode(bytes),
    })
}

fn required_value<'a>(value: &'a str, name: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{name} is required"))
    } else {
        Ok(value)
    }
}

fn preview_image_mime(bytes: &[u8], declared_mime: Option<&str>) -> Result<String, String> {
    let sniffed = sniff_image_mime(bytes);
    if let Some(mime) = sniffed {
        return Ok(mime.to_string());
    }
    let declared = declared_mime
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        declared.as_str(),
        "image/jpeg"
            | "image/png"
            | "image/gif"
            | "image/webp"
            | "image/heic"
            | "image/heif"
            | "image/avif"
            | "image/bmp"
            | "image/tiff"
    ) {
        return Ok(declared);
    }
    Err("attachment is not a supported image".to_string())
}

fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return Some("image/tiff");
    }
    iso_bmff_image_mime(bytes)
}

fn iso_bmff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return None;
    }
    let brands = bytes[8..bytes.len().min(64)].chunks_exact(4);
    if brands
        .clone()
        .any(|brand| brand == b"avif" || brand == b"avis")
    {
        return Some("image/avif");
    }
    for brand in brands {
        if [
            b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"mif1", b"msf1",
        ]
        .iter()
        .any(|candidate| brand == *candidate)
        {
            return Some("image/heic");
        }
    }
    None
}

fn guess_mime_from_path(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png".into()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".into()
    } else if lower.ends_with(".gif") {
        "image/gif".into()
    } else if lower.ends_with(".webp") {
        "image/webp".into()
    } else if lower.ends_with(".heic") {
        "image/heic".into()
    } else if lower.ends_with(".heif") {
        "image/heif".into()
    } else if lower.ends_with(".avif") {
        "image/avif".into()
    } else if lower.ends_with(".bmp") {
        "image/bmp".into()
    } else if lower.ends_with(".tif") || lower.ends_with(".tiff") {
        "image/tiff".into()
    } else if lower.ends_with(".pdf") {
        "application/pdf".into()
    } else if lower.ends_with(".csv") {
        "text/csv".into()
    } else if lower.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into()
    } else if lower.ends_with(".txt") || lower.ends_with(".md") {
        "text/plain".into()
    } else {
        "application/octet-stream".into()
    }
}

pub fn media_ref_from_upload(response: &MediaUploadResponse, kind_hint: Option<&str>) -> MediaRef {
    MediaRef {
        media_id: response.media_id.clone(),
        kind: kind_hint
            .map(str::to_string)
            .unwrap_or_else(|| media_kind_from_mime(&response.mime).to_string()),
        mime: response.mime.clone(),
        label: response.label.clone(),
    }
}

fn media_kind_from_mime(mime: &str) -> &'static str {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("image/") {
        "image"
    } else if mime.contains("spreadsheet")
        || mime.contains("excel")
        || mime == "text/csv"
        || mime == "text/tab-separated-values"
    {
        "spreadsheet"
    } else if mime.starts_with("audio/") {
        "audio"
    } else {
        "document"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_common_and_iphone_image_formats() {
        assert_eq!(sniff_image_mime(b"\xff\xd8\xffrest"), Some("image/jpeg"));
        assert_eq!(
            sniff_image_mime(b"\0\0\0\x18ftypheic\0\0\0\0mif1"),
            Some("image/heic")
        );
        assert_eq!(
            sniff_image_mime(b"\0\0\0\x18ftypavif\0\0\0\0mif1"),
            Some("image/avif")
        );
    }

    #[test]
    fn refuses_non_image_preview_payloads() {
        assert!(preview_image_mime(b"<html>nope</html>", Some("text/html")).is_err());
    }
}
