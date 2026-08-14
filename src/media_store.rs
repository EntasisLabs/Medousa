//! Local user media under `medousa/media/` (P5a — no cloud).

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::daemon_api::{MediaRef, MediaUploadResponse};
use crate::store_root::StorePath;

#[derive(Debug, Clone, Default)]
pub struct MediaPromptMergeOptions {
    pub vision_active: bool,
    pub vision_image_ids: std::collections::HashSet<String>,
}

const MEDIA_INDEX_FILE: &str = "index.jsonl";
const MAX_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;
const MAX_EXTRACT_BYTES: u64 = (crate::media_text_extract::MAX_MEDIA_EXTRACT_CHARS as u64) * 4;
const MEDIA_PAYLOAD_DOMAIN: &[u8] = b"media-payload";
const MEDIA_EXTRACT_DOMAIN: &[u8] = b"media-extract";

static MEDIA_STORE: Lazy<crate::session_storage::SessionDirectoryStore> = Lazy::new(|| {
    crate::session_storage::SessionDirectoryStore::new(
        crate::paths::medousa_data_dir().join("media"),
    )
});

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

pub fn persist_user_media(
    session_id: &str,
    bytes: &[u8],
    mime: &str,
    label: Option<&str>,
) -> Result<MediaUploadResponse, String> {
    let session_id =
        crate::session_storage::SessionId::parse(session_id).map_err(|error| error.to_string())?;

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

    let media_id = format!(
        "usr:{}:{}",
        short_session(session_id.as_str()),
        Uuid::new_v4().simple()
    );
    let ext = extension_for_mime(&mime);
    let payload_path = media_payload_path(&media_id, ext);
    MEDIA_STORE
        .atomic_write(&session_id, &payload_path, bytes)
        .map_err(|err| err.to_string())?;

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
        let path = media_extract_path(&media_id);
        MEDIA_STORE
            .atomic_write(&session_id, &path, extract.text.as_bytes())
            .map_err(|err| err.to_string())?;
        extract_chars = Some(extract.text.chars().count());
        extract_truncated = extract.truncated;
        extract_path = Some(path.file_name().to_string());
    }

    let record = MediaRecord {
        media_id: media_id.clone(),
        session_id: session_id.to_string(),
        mime: mime.clone(),
        kind: media_kind_from_mime(&mime).to_string(),
        byte_size,
        stored_at_utc: Utc::now(),
        payload_path: payload_path.file_name().to_string(),
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
    open_media_payload_from(&MEDIA_STORE, record)
}

fn open_media_payload_from(
    store: &crate::session_storage::SessionDirectoryStore,
    record: &MediaRecord,
) -> Result<Vec<u8>, String> {
    let session_id = crate::session_storage::SessionId::parse(&record.session_id)
        .map_err(|error| error.to_string())?;
    store
        .read_limited(
            &session_id,
            &media_payload_path(&record.media_id, extension_for_mime(&record.mime)),
            MAX_UPLOAD_BYTES,
        )
        .map_err(|err| err.to_string())
}

pub fn media_ref_from_record(record: &MediaRecord) -> MediaRef {
    MediaRef {
        media_id: record.media_id.clone(),
        kind: record.kind.clone(),
        mime: record.mime.clone(),
        label: record.label.clone(),
    }
}

pub fn delete_media_for_session(session_id: &str) -> Result<(), String> {
    let session_id =
        crate::session_storage::SessionId::parse(session_id).map_err(|error| error.to_string())?;
    let remaining = read_index_records()
        .into_iter()
        .filter(|record| record.session_id != session_id.as_str())
        .collect::<Vec<_>>();
    overwrite_index_records(&remaining)?;
    MEDIA_STORE
        .remove_session(&session_id)
        .map_err(|error| error.to_string())
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
    read_media_extract_from(&MEDIA_STORE, record)
}

fn read_media_extract_from(
    store: &crate::session_storage::SessionDirectoryStore,
    record: &MediaRecord,
) -> Option<String> {
    record
        .extract_path
        .as_deref()
        .filter(|value| !value.is_empty())?;
    let session_id = crate::session_storage::SessionId::parse(&record.session_id).ok()?;
    store
        .read_limited(
            &session_id,
            &media_extract_path(&record.media_id),
            MAX_EXTRACT_BYTES,
        )
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|text| !text.trim().is_empty())
}

/// Read cached extract or run text extraction from the payload at turn time.
pub fn resolve_media_extract(record: &MediaRecord) -> Option<(String, bool)> {
    if let Some(text) = read_media_extract(record) {
        return Some((text, record.extract_truncated));
    }
    let bytes = open_media_payload(record).ok()?;
    let mime = infer_mime_for_record(record);
    let extract =
        crate::media_text_extract::extract_media_text(&bytes, &mime, record.label.as_deref())?;
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
            || media_ref
                .mime
                .trim()
                .to_ascii_lowercase()
                .starts_with("image/");

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
        && let Some(from_name) = mime_from_filename(label)
    {
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
    media_ref
        .mime
        .trim()
        .eq_ignore_ascii_case("application/pdf")
}

fn infer_mime_for_record(record: &MediaRecord) -> String {
    let mime = normalize_mime(&record.mime);
    if mime != "application/octet-stream" {
        return mime;
    }
    if let Ok(bytes) = open_media_payload(record)
        && bytes.starts_with(b"%PDF")
    {
        return "application/pdf".to_string();
    }
    if let Some(label) = record.label.as_deref()
        && let Some(from_name) = mime_from_filename(label)
    {
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
    let mut line = serde_json::to_vec(record).map_err(|err| err.to_string())?;
    line.push(b'\n');
    MEDIA_STORE
        .append_root(&index_path(), &line)
        .map_err(|err| err.to_string())
}

fn read_index_records() -> Vec<MediaRecord> {
    let Ok(bytes) = MEDIA_STORE.read_root(&index_path()) else {
        return Vec::new();
    };
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .filter_map(|line| serde_json::from_slice::<MediaRecord>(line).ok())
        .collect()
}

fn overwrite_index_records(records: &[MediaRecord]) -> Result<(), String> {
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
    }
    MEDIA_STORE
        .atomic_write_root(&index_path(), &bytes)
        .map_err(|error| error.to_string())
}

fn media_payload_path(media_id: &str, extension: &str) -> StorePath {
    let extension = if extension.is_empty() {
        "bin"
    } else {
        extension
    };
    crate::session_storage::session_object_path(MEDIA_PAYLOAD_DOMAIN, media_id, extension)
}

fn media_extract_path(media_id: &str) -> StorePath {
    crate::session_storage::session_object_path(MEDIA_EXTRACT_DOMAIN, media_id, "txt")
}

fn index_path() -> StorePath {
    StorePath::parse(MEDIA_INDEX_FILE).expect("static media index path must be valid")
}

fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(session_id: &str, media_id: &str) -> MediaRecord {
        MediaRecord {
            media_id: media_id.to_string(),
            session_id: session_id.to_string(),
            mime: "text/plain".to_string(),
            kind: "document".to_string(),
            byte_size: 7,
            stored_at_utc: Utc::now(),
            payload_path: "/outside/attacker-selected.txt".to_string(),
            label: None,
            extract_path: Some("/outside/attacker-selected.extract.txt".to_string()),
            extract_chars: Some(7),
            extract_truncated: false,
        }
    }

    #[test]
    fn media_reads_derive_capability_paths_instead_of_trusting_record_strings() {
        let temp = tempfile::tempdir().unwrap();
        let store = crate::session_storage::SessionDirectoryStore::new(temp.path().join("media"));
        let session_id = crate::session_storage::SessionId::parse("session-media").unwrap();
        let record = record(session_id.as_str(), "usr:session:secret");
        store
            .atomic_write(
                &session_id,
                &media_payload_path(&record.media_id, "txt"),
                b"payload",
            )
            .unwrap();
        store
            .atomic_write(
                &session_id,
                &media_extract_path(&record.media_id),
                b"extract",
            )
            .unwrap();

        assert_eq!(
            open_media_payload_from(&store, &record).unwrap(),
            b"payload"
        );
        assert_eq!(
            read_media_extract_from(&store, &record).as_deref(),
            Some("extract")
        );
    }

    #[test]
    fn media_object_paths_are_safe_and_domain_separated() {
        let payload = media_payload_path("usr:session:secret", "png");
        let extract = media_extract_path("usr:session:secret");
        assert_ne!(payload, extract);
        assert!(payload.file_name().starts_with("o1-"));
        assert!(!payload.file_name().contains(':'));
    }

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
