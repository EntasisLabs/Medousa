//! Local user media under `medousa/media/` (P5a — no cloud).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::daemon_api::{MediaRef, MediaUploadResponse};

#[derive(Debug, Clone, Default)]
pub struct MediaPromptMergeOptions {
    pub vision_active: bool,
    pub vision_image_ids: std::collections::HashSet<String>,
}

const MEDIA_INDEX_FILE: &str = "index.jsonl";
const MAX_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaRecord {
    pub media_id: String,
    pub session_id: String,
    pub mime: String,
    pub kind: String,
    pub byte_size: u64,
    pub stored_at_utc: DateTime<Utc>,
    pub payload_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract_chars: Option<usize>,
    #[serde(default)]
    pub extract_truncated: bool,
}

pub fn media_root() -> PathBuf {
    data_local_medousa_dir().join("media")
}

pub fn persist_user_media(
    session_id: &str,
    bytes: &[u8],
    mime: &str,
    label: Option<&str>,
) -> Result<MediaUploadResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }

    let byte_size = bytes.len() as u64;
    if byte_size == 0 {
        return Err("empty file".to_string());
    }
    if byte_size > MAX_UPLOAD_BYTES {
        return Err(format!(
            "file exceeds max size ({} bytes)",
            MAX_UPLOAD_BYTES
        ));
    }

    let mime = infer_mime(bytes, mime, label);
    if !mime_allowed(&mime) {
        return Err(format!("mime type not allowed: {mime}"));
    }

    let media_id = format!("usr:{}:{}", short_session(session_id), Uuid::new_v4().simple());
    let ext = extension_for_mime(&mime);
    let dir = media_root().join(session_id);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;

    let filename = if ext.is_empty() {
        media_id.clone()
    } else {
        format!("{media_id}.{ext}")
    };
    let payload_path = dir.join(&filename);
    fs::write(&payload_path, bytes).map_err(|err| err.to_string())?;

    let label = label
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut extract_path = None;
    let mut extract_chars = None;
    let mut extract_truncated = false;
    if let Some(extract) =
        crate::media_text_extract::extract_media_text(bytes, &mime, label.as_deref())
    {
        let path = crate::media_text_extract::extract_path_for_media(
            payload_path.to_string_lossy().as_ref(),
        );
        fs::write(&path, &extract.text).map_err(|err| err.to_string())?;
        extract_chars = Some(extract.text.chars().count());
        extract_truncated = extract.truncated;
        extract_path = Some(path.to_string_lossy().to_string());
    }

    let record = MediaRecord {
        media_id: media_id.clone(),
        session_id: session_id.to_string(),
        mime: mime.clone(),
        kind: media_kind_from_mime(&mime).to_string(),
        byte_size,
        stored_at_utc: Utc::now(),
        payload_path: payload_path.to_string_lossy().to_string(),
        label: label.clone(),
        extract_path: extract_path.clone(),
        extract_chars,
        extract_truncated,
    };
    append_index_record(&record)?;

    Ok(MediaUploadResponse {
        media_id,
        mime,
        byte_size,
        label,
        text_extracted: extract_path.is_some(),
    })
}

pub fn get_media_record(session_id: &str, media_id: &str) -> Option<MediaRecord> {
    let session_id = session_id.trim();
    let media_id = media_id.trim();
    if session_id.is_empty() || media_id.is_empty() {
        return None;
    }

    read_index_records()
        .into_iter()
        .find(|record| record.session_id == session_id && record.media_id == media_id)
}

pub fn open_media_payload(record: &MediaRecord) -> Result<Vec<u8>, String> {
    if !Path::new(&record.payload_path).exists() {
        return Err("media file missing on disk".to_string());
    }
    fs::read(&record.payload_path).map_err(|err| err.to_string())
}

pub fn media_ref_from_record(record: &MediaRecord) -> MediaRef {
    MediaRef {
        media_id: record.media_id.clone(),
        kind: record.kind.clone(),
        mime: record.mime.clone(),
        label: record.label.clone(),
    }
}

pub fn validate_media_refs(session_id: &str, refs: &[MediaRef]) -> Result<(), String> {
    if refs.len() > crate::media_vision::MAX_MEDIA_REFS_PER_TURN {
        return Err(format!(
            "too many attachments (max {})",
            crate::media_vision::MAX_MEDIA_REFS_PER_TURN
        ));
    }
    for media_ref in refs {
        let media_id = media_ref.media_id.trim();
        if media_id.is_empty() {
            return Err("media_ref.media_id is required".to_string());
        }
        if get_media_record(session_id, media_id).is_none() {
            return Err(format!("unknown media_id '{media_id}' for session"));
        }
    }
    Ok(())
}

pub fn read_media_extract(record: &MediaRecord) -> Option<String> {
    let path = record
        .extract_path
        .as_deref()
        .filter(|value| !value.is_empty())?;
    if !Path::new(path).exists() {
        return None;
    }
    std::fs::read_to_string(path)
        .ok()
        .filter(|text| !text.trim().is_empty())
}

/// Read cached extract or run text extraction from the payload at turn time.
pub fn resolve_media_extract(record: &MediaRecord) -> Option<(String, bool)> {
    if let Some(text) = read_media_extract(record) {
        return Some((text, record.extract_truncated));
    }
    let bytes = open_media_payload(record).ok()?;
    let mime = infer_mime_for_record(record);
    let extract = crate::media_text_extract::extract_media_text(
        &bytes,
        &mime,
        record.label.as_deref(),
    )?;
    if extract.text.trim().is_empty() {
        return None;
    }
    Some((extract.text, extract.truncated))
}

fn append_extract_block(block: &mut String, text: &str, truncated: bool) {
    block.push_str("  ```\n");
    for line in text.lines() {
        block.push_str("  ");
        block.push_str(line);
        block.push('\n');
    }
    block.push_str("  ```\n");
    if truncated {
        block.push_str("  (extract truncated at import)\n");
    }
}

pub fn merge_media_refs_into_prompt(
    prompt: &str,
    session_id: &str,
    media_refs: &[MediaRef],
    options: &MediaPromptMergeOptions,
) -> String {
    if media_refs.is_empty() {
        return prompt.to_string();
    }

    let mut block = String::from("\n\n[Attachments]\n");
    for media_ref in media_refs {
        let name = media_ref
            .label
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("attachment");
        block.push_str(&format!(
            "- {name} ({}, kind={}, id={})\n",
            media_ref.mime, media_ref.kind, media_ref.media_id
        ));

        let is_image = media_ref.kind == "image"
            || media_ref.mime.trim().to_ascii_lowercase().starts_with("image/");

        if is_image {
            if options.vision_active && options.vision_image_ids.contains(&media_ref.media_id) {
                block.push_str("  (included as image content for this turn)\n");
                continue;
            }
            if !options.vision_active {
                block.push_str(
                    "  (image attached — current model cannot see images; describe it in text or switch to a vision-capable model)\n",
                );
            }
        }

        if let Some(record) = get_media_record(session_id, &media_ref.media_id) {
            if let Some((extract, truncated)) = resolve_media_extract(&record) {
                append_extract_block(&mut block, &extract, truncated);
            } else if is_pdf_attachment(media_ref, &record) {
                block.push_str(
                    "  (no text layer — scanned PDF may need vision/OCR in a later release)\n",
                );
            }
        }
    }
    format!("{prompt}{block}")
}

pub fn media_kind_from_mime(mime: &str) -> &'static str {
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

fn normalize_mime(mime: &str) -> String {
    let mime = mime.trim().to_ascii_lowercase();
    if mime.is_empty() {
        "application/octet-stream".to_string()
    } else {
        mime
    }
}

fn infer_mime(bytes: &[u8], mime: &str, label: Option<&str>) -> String {
    let mime = normalize_mime(mime);
    if mime != "application/octet-stream" {
        return mime;
    }
    if bytes.starts_with(b"%PDF") {
        return "application/pdf".to_string();
    }
    if let Some(label) = label
        && let Some(from_name) = mime_from_filename(label) {
            return from_name;
        }
    mime
}

fn mime_from_filename(name: &str) -> Option<String> {
    let lower = name.trim().to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        return Some("application/pdf".to_string());
    }
    if lower.ends_with(".csv") {
        return Some("text/csv".to_string());
    }
    if lower.ends_with(".tsv") {
        return Some("text/tab-separated-values".to_string());
    }
    if lower.ends_with(".md") {
        return Some("text/markdown".to_string());
    }
    if lower.ends_with(".txt") {
        return Some("text/plain".to_string());
    }
    if lower.ends_with(".xlsx") {
        return Some(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
        );
    }
    if lower.ends_with(".xls") {
        return Some("application/vnd.ms-excel".to_string());
    }
    None
}

fn is_pdf_attachment(media_ref: &MediaRef, record: &MediaRecord) -> bool {
    if infer_mime_for_record(record) == "application/pdf" {
        return true;
    }
    media_ref.mime.trim().eq_ignore_ascii_case("application/pdf")
}

fn infer_mime_for_record(record: &MediaRecord) -> String {
    let mime = normalize_mime(&record.mime);
    if mime != "application/octet-stream" {
        return mime;
    }
    if let Ok(bytes) = open_media_payload(record)
        && bytes.starts_with(b"%PDF") {
            return "application/pdf".to_string();
        }
    if let Some(label) = record.label.as_deref()
        && let Some(from_name) = mime_from_filename(label) {
            return from_name;
        }
    mime
}

fn mime_allowed(mime: &str) -> bool {
    matches!(
        mime,
        "image/jpeg"
            | "image/png"
            | "image/gif"
            | "image/webp"
            | "application/pdf"
            | "text/plain"
            | "text/markdown"
            | "text/csv"
            | "text/tab-separated-values"
            | "application/vnd.ms-excel"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    ) || mime.starts_with("image/")
}

fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "text/markdown" => "md",
        "text/csv" => "csv",
        "text/tab-separated-values" => "tsv",
        "application/vnd.ms-excel" => "xls",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        _ => "",
    }
}

fn append_index_record(record: &MediaRecord) -> Result<(), String> {
    fs::create_dir_all(media_root()).map_err(|err| err.to_string())?;
    let index_path = media_root().join(MEDIA_INDEX_FILE);
    let line = serde_json::to_string(record).map_err(|err| err.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .map_err(|err| err.to_string())?;
    writeln!(file, "{line}").map_err(|err| err.to_string())
}

fn read_index_records() -> Vec<MediaRecord> {
    let index_path = media_root().join(MEDIA_INDEX_FILE);
    if !index_path.exists() {
        return Vec::new();
    }
    fs::read_to_string(&index_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                None
            } else {
                serde_json::from_str::<MediaRecord>(line).ok()
            }
        })
        .collect()
}

fn data_local_medousa_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_media_refs_appends_block() {
        let merged = merge_media_refs_into_prompt(
            "hello",
            "session-a",
            &[MediaRef {
                media_id: "usr:abc:1".into(),
                kind: "image".into(),
                mime: "image/png".into(),
                label: Some("shot.png".into()),
            }],
            &MediaPromptMergeOptions::default(),
        );
        assert!(merged.contains("[Attachments]"));
        assert!(merged.contains("shot.png"));
    }
}
