use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::IntoFuture;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

const ARTIFACT_INDEX_TABLE: &str = "artifact_record";

const ARTIFACT_SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE artifact_record SCHEMAFULL",
    "DEFINE FIELD artifact_id ON TABLE artifact_record TYPE string",
    "DEFINE FIELD session_id ON TABLE artifact_record TYPE string",
    "DEFINE FIELD tool_name ON TABLE artifact_record TYPE string",
    "DEFINE FIELD direction ON TABLE artifact_record TYPE string",
    "DEFINE FIELD hash64 ON TABLE artifact_record TYPE string",
    "DEFINE FIELD byte_size ON TABLE artifact_record TYPE int",
    "DEFINE FIELD stored_at_utc ON TABLE artifact_record TYPE datetime",
    "DEFINE FIELD payload_path ON TABLE artifact_record TYPE string",
    "DEFINE FIELD content_type ON TABLE artifact_record TYPE option<string>",
    "DEFINE FIELD label ON TABLE artifact_record TYPE option<string>",
    "DEFINE FIELD presentation ON TABLE artifact_record TYPE option<string>",
    "DEFINE FIELD height_px ON TABLE artifact_record TYPE option<int>",
    "DEFINE FIELD supersedes_artifact_id ON TABLE artifact_record TYPE option<string>",
    "DEFINE FIELD root_artifact_id ON TABLE artifact_record TYPE option<string>",
    "DEFINE INDEX idx_artifact_record_session ON TABLE artifact_record COLUMNS session_id",
    "DEFINE INDEX idx_artifact_record_id ON TABLE artifact_record COLUMNS artifact_id UNIQUE",
];

static ARTIFACT_INDEX_STORE: Lazy<RwLock<Arc<dyn ArtifactIndexStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(FileArtifactIndexStore)));

/// When true, the primary index lives in SurrealDB; UI artifacts are also mirrored to `index.jsonl`.
static ARTIFACT_INDEX_USES_SURREAL: AtomicBool = AtomicBool::new(false);

pub async fn init_artifact_store_with_runtime(runtime: &RuntimeComposition) {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            let store = SurrealArtifactIndexStore::new(rt.job_store.db());
            if let Err(err) = store.ensure_schema().await {
                eprintln!(
                    "Surreal artifact index schema init error: {err}; keeping file-backed index"
                );
                return;
            }
            ARTIFACT_INDEX_USES_SURREAL.store(true, Ordering::Release);
            set_artifact_index_store(Arc::new(store));
            eprintln!("Surreal runtime detected; artifact index switched to SurrealDB backend");
            repair_ui_artifact_index_from_disk();
        }
        _ => {}
    }
}

fn set_artifact_index_store(store: Arc<dyn ArtifactIndexStore>) {
    let mut guard = ARTIFACT_INDEX_STORE.write().unwrap();
    *guard = store;
}

trait ArtifactIndexStore: Send + Sync {
    fn read_all(&self) -> Vec<ArtifactRecord>;
    fn append(&self, record: &ArtifactRecord) -> std::result::Result<(), String>;
    fn overwrite_all(&self, records: &[ArtifactRecord]) -> std::result::Result<(), String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactRecord {
    pub artifact_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub direction: String,
    pub hash64: String,
    pub byte_size: usize,
    pub stored_at_utc: DateTime<Utc>,
    pub payload_path: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_px: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_artifact_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FetchedArtifact {
    pub record: ArtifactRecord,
    pub body: String,
    pub mime: String,
}

pub const UI_ARTIFACT_MAX_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone)]
pub struct StoredArtifact {
    pub record: ArtifactRecord,
    pub payload: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ArtifactMaintenanceReport {
    pub records_before: usize,
    pub records_after: usize,
    pub missing_payload_pruned: usize,
    pub deduped_records_pruned: usize,
    pub retention_pruned: usize,
    pub payload_files_deleted: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ArtifactIndexStats {
    pub records: usize,
    pub unique_hashes: usize,
    pub total_bytes: usize,
}

pub fn persist_tool_artifact(
    session_id: &str,
    tool_name: &str,
    direction: &str,
    hash64: &str,
    byte_size: usize,
    payload: &Value,
) -> std::result::Result<ArtifactRecord, String> {
    let now = Utc::now();
    let tool_slug = slugify_tool_name(tool_name);
    let hash_short = hash64.chars().take(12).collect::<String>();
    let artifact_id = format!(
        "art:{}:{}:{}:{}",
        short_session(session_id),
        tool_slug,
        direction,
        hash_short
    );

    let payload_dir = artifacts_root()
        .join(session_id)
        .join(&tool_slug)
        .join(direction);
    std::fs::create_dir_all(&payload_dir).map_err(|err| err.to_string())?;

    let payload_path = payload_dir.join(format!("{}.json", hash64));
    if !payload_path.exists() {
        let raw = serde_json::to_vec_pretty(payload).map_err(|err| err.to_string())?;
        std::fs::write(&payload_path, raw).map_err(|err| err.to_string())?;
    }

    let record = ArtifactRecord {
        artifact_id,
        session_id: session_id.to_string(),
        tool_name: tool_name.to_string(),
        direction: direction.to_string(),
        hash64: hash64.to_string(),
        byte_size,
        stored_at_utc: now,
        payload_path: payload_path.to_string_lossy().to_string(),
        content_type: "application/json".to_string(),
        label: None,
        presentation: None,
        height_px: None,
        supersedes_artifact_id: None,
        root_artifact_id: None,
    };

    append_index_record(&record)?;
    Ok(record)
}

pub fn persist_ui_artifact(
    session_id: &str,
    html: &str,
    label: &str,
    presentation: &str,
    height_px: Option<u32>,
) -> std::result::Result<ArtifactRecord, String> {
    persist_ui_artifact_revision(session_id, html, label, presentation, height_px, None)
}

pub fn persist_ui_artifact_revision(
    session_id: &str,
    html: &str,
    label: &str,
    presentation: &str,
    height_px: Option<u32>,
    supersedes_artifact_id: Option<&str>,
) -> std::result::Result<ArtifactRecord, String> {
    let wrapped = wrap_html_document(html);
    let byte_size = wrapped.len();
    if byte_size > UI_ARTIFACT_MAX_BYTES {
        return Err(format!(
            "HTML artifact exceeds {} KB cap (got {} bytes)",
            UI_ARTIFACT_MAX_BYTES / 1024,
            byte_size
        ));
    }
    if label.trim().is_empty() {
        return Err("title/label is required".to_string());
    }
    let presentation = normalize_presentation(presentation)?;
    let hash64 = crate::payload_receipt::hash_text(&wrapped);
    let now = Utc::now();
    let tool_name = "cognition_ui_present";
    let tool_slug = slugify_tool_name(tool_name);
    let hash_short = hash64.chars().take(12).collect::<String>();
    let artifact_id = format!(
        "art:{}:{}:ui:{}",
        short_session(session_id),
        tool_slug,
        hash_short
    );

    let payload_dir = artifacts_root()
        .join(session_id)
        .join(&tool_slug)
        .join("ui");
    std::fs::create_dir_all(&payload_dir).map_err(|err| err.to_string())?;

    let payload_path = payload_dir.join(format!("{}.html", hash64));
    if !payload_path.exists() {
        std::fs::write(&payload_path, wrapped.as_bytes()).map_err(|err| err.to_string())?;
    }

    let (supersedes, root_artifact_id) = if let Some(previous_id) = supersedes_artifact_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let previous = fetch_artifact_at_id(session_id, previous_id).ok_or_else(|| {
            format!("supersedes artifact not found in this session: {previous_id}")
        })?;
        if previous.mime != "text/html" || previous.record.direction != "ui" {
            return Err("supersedes artifact must be a UI HTML presentation".to_string());
        }
        let root = previous
            .record
            .root_artifact_id
            .clone()
            .unwrap_or_else(|| previous.record.artifact_id.clone());
        (Some(previous.record.artifact_id), Some(root))
    } else {
        (None, None)
    };

    let record = ArtifactRecord {
        artifact_id,
        session_id: session_id.to_string(),
        tool_name: tool_name.to_string(),
        direction: "ui".to_string(),
        hash64,
        byte_size,
        stored_at_utc: now,
        payload_path: payload_path.to_string_lossy().to_string(),
        content_type: "text/html".to_string(),
        label: Some(label.trim().to_string()),
        presentation: Some(presentation),
        height_px,
        supersedes_artifact_id: supersedes,
        root_artifact_id,
    };

    append_index_record(&record)?;
    Ok(record)
}

pub fn fetch_artifact(session_id: &str, artifact_id: &str) -> Option<FetchedArtifact> {
    let query = artifact_id.trim();
    if query.is_empty() {
        return None;
    }

    let latest_id = resolve_latest_artifact_id(session_id, query).unwrap_or_else(|| query.to_string());
    fetch_artifact_at_id(session_id, &latest_id)
}

pub fn fetch_artifact_at_id(session_id: &str, artifact_id: &str) -> Option<FetchedArtifact> {
    let query = artifact_id.trim();
    if query.is_empty() {
        return None;
    }

    let records = read_index_records();
    let record = records
        .iter()
        .find(|record| {
            record.session_id == session_id
                && (record.artifact_id == query || record.artifact_id.starts_with(query))
        })
        .or_else(|| {
            records.iter().find(|record| {
                record.artifact_id == query || record.artifact_id.starts_with(query)
            })
        })
        .cloned()
        .or_else(|| fetch_ui_artifact_record_from_disk(session_id, query))?;

    load_fetched_from_record(record)
}

pub fn is_ui_html_record(record: &ArtifactRecord) -> bool {
    record.direction == "ui"
        && (record.content_type == "text/html"
            || record.payload_path.ends_with(".html"))
}

pub fn list_ui_artifacts(
    session_id: Option<&str>,
    limit: usize,
    query: Option<&str>,
) -> Vec<ArtifactRecord> {
    let limit = limit.clamp(1, 500);
    let query = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);

    let mut records: Vec<ArtifactRecord> = read_index_records()
        .into_iter()
        .filter(|record| is_ui_html_record(record))
        .filter(|record| {
            session_id.is_none_or(|sid| record.session_id == sid)
        })
        .filter(|record| {
            query.as_ref().is_none_or(|needle| {
                record.artifact_id.to_ascii_lowercase().contains(needle)
                    || record
                        .label
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(needle)
            })
        })
        .collect();

    records.sort_by(|a, b| b.stored_at_utc.cmp(&a.stored_at_utc));
    dedupe_ui_artifacts_to_latest(records)
        .into_iter()
        .take(limit)
        .collect()
}

fn dedupe_ui_artifacts_to_latest(mut records: Vec<ArtifactRecord>) -> Vec<ArtifactRecord> {
    records.sort_by(|a, b| b.stored_at_utc.cmp(&a.stored_at_utc));
    let mut seen_roots = HashSet::new();
    let mut kept = Vec::new();
    for record in records {
        let root = record
            .root_artifact_id
            .clone()
            .unwrap_or_else(|| record.artifact_id.clone());
        if seen_roots.insert(root) {
            kept.push(record);
        }
    }
    kept.sort_by(|a, b| b.stored_at_utc.cmp(&a.stored_at_utc));
    kept
}

pub fn resolve_latest_artifact_id(session_id: &str, artifact_id: &str) -> Option<String> {
    let query = artifact_id.trim();
    if query.is_empty() {
        return None;
    }
    let records = read_index_records();
    let mut current = records
        .iter()
        .find(|record| {
            record.session_id == session_id
                && (record.artifact_id == query || record.artifact_id.starts_with(query))
        })
        .or_else(|| {
            records
                .iter()
                .find(|record| record.artifact_id == query || record.artifact_id.starts_with(query))
        })
        .cloned()
        .or_else(|| fetch_ui_artifact_record_from_disk(session_id, query))?;

    for _ in 0..64 {
        let next = records.iter().find(|record| {
            record.session_id == current.session_id
                && record
                    .supersedes_artifact_id
                    .as_deref()
                    .is_some_and(|value| value == current.artifact_id)
        });
        match next {
            Some(record) if record.artifact_id != current.artifact_id => current = record.clone(),
            _ => return Some(current.artifact_id),
        }
    }
    Some(current.artifact_id)
}

pub fn grep_ui_artifact(
    session_id: &str,
    artifact_id: &str,
    pattern: &str,
    context_lines: usize,
    limit: usize,
) -> std::result::Result<crate::line_grep::LineGrepResult, String> {
    let fetched = fetch_artifact(session_id, artifact_id)
        .ok_or_else(|| format!("artifact not found: {artifact_id}"))?;
    if fetched.mime != "text/html" {
        return Err("artifact is not HTML".to_string());
    }
    crate::line_grep::grep_lines(&fetched.body, pattern, context_lines, limit)
}

pub fn read_ui_artifact_excerpt(
    session_id: &str,
    artifact_id: &str,
    line_start: Option<usize>,
    line_end: Option<usize>,
    max_chars: usize,
) -> std::result::Result<crate::line_grep::LineExcerpt, String> {
    let fetched = fetch_artifact(session_id, artifact_id)
        .ok_or_else(|| format!("artifact not found: {artifact_id}"))?;
    if fetched.mime != "text/html" {
        return Err("artifact is not HTML".to_string());
    }
    Ok(crate::line_grep::excerpt_lines(
        &fetched.body,
        line_start,
        line_end,
        max_chars,
    ))
}

fn load_fetched_from_record(record: ArtifactRecord) -> Option<FetchedArtifact> {
    let path = Path::new(&record.payload_path);
    if !path.exists() {
        return None;
    }

    let mime = if record.content_type.is_empty() {
        if path.extension().and_then(|ext| ext.to_str()) == Some("html") {
            "text/html".to_string()
        } else {
            "application/json".to_string()
        }
    } else {
        record.content_type.clone()
    };

    let body = if mime == "text/html" {
        std::fs::read_to_string(path).ok()?
    } else {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .map(|value| serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()))
            .or_else(|| std::fs::read_to_string(path).ok())?
    };

    Some(FetchedArtifact {
        record,
        body,
        mime,
    })
}

fn ui_artifact_hash_short(artifact_id: &str) -> Option<&str> {
    let hash_short = artifact_id.rsplit(':').next()?.trim();
    if hash_short.len() < 8 {
        return None;
    }
    Some(hash_short)
}

fn build_ui_artifact_id(session_id: &str, hash64: &str) -> String {
    format!(
        "art:{}:cognition_ui_present:ui:{}",
        short_session(session_id),
        hash64.chars().take(12).collect::<String>()
    )
}

fn ui_artifact_record_from_path(session_id: &str, path: &Path) -> Option<ArtifactRecord> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("html") {
        return None;
    }
    let hash64 = path.file_stem()?.to_str()?.to_string();
    if hash64.is_empty() {
        return None;
    }
    let byte_size = std::fs::metadata(path).ok()?.len() as usize;
    let stored_at_utc = std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|duration| {
                    DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
                })
        })
        .unwrap_or_else(Utc::now);

    Some(ArtifactRecord {
        artifact_id: build_ui_artifact_id(session_id, &hash64),
        session_id: session_id.to_string(),
        tool_name: "cognition_ui_present".to_string(),
        direction: "ui".to_string(),
        hash64,
        byte_size,
        stored_at_utc,
        payload_path: path.to_string_lossy().to_string(),
        content_type: "text/html".to_string(),
        label: Some("Presentation".to_string()),
        presentation: Some("inline".to_string()),
        height_px: None,
        supersedes_artifact_id: None,
        root_artifact_id: None,
    })
}

fn find_ui_payload_in_session_dir(session_id: &str, hash_short: &str) -> Option<ArtifactRecord> {
    let ui_dir = artifacts_root()
        .join(session_id)
        .join("cognition_ui_present")
        .join("ui");
    let path = std::fs::read_dir(ui_dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        let stem = path.file_stem().and_then(|stem| stem.to_str())?;
        if stem.starts_with(hash_short) {
            Some(path)
        } else {
            None
        }
    })?;
    ui_artifact_record_from_path(session_id, &path)
}

fn fetch_ui_artifact_record_from_disk(session_id: &str, artifact_id: &str) -> Option<ArtifactRecord> {
    let hash_short = ui_artifact_hash_short(artifact_id)?;
    if let Some(record) = find_ui_payload_in_session_dir(session_id, hash_short) {
        return Some(record);
    }

    let root = artifacts_root();
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        if !entry.file_type().ok()?.is_dir() {
            continue;
        }
        let candidate_session = entry.file_name().to_string_lossy().to_string();
        if candidate_session == session_id {
            continue;
        }
        if let Some(record) = find_ui_payload_in_session_dir(&candidate_session, hash_short) {
            return Some(record);
        }
    }
    None
}

fn repair_ui_artifact_index_from_disk() {
    let indexed_keys: HashSet<(String, String)> = artifact_index_store()
        .read_all()
        .into_iter()
        .map(|record| (record.session_id, record.hash64))
        .collect();

    let root = artifacts_root();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };

    for entry in entries.flatten() {
        if !entry.file_type().ok().is_some_and(|kind| kind.is_dir()) {
            continue;
        }
        let session_id = entry.file_name().to_string_lossy().to_string();
        let ui_dir = root
            .join(&session_id)
            .join("cognition_ui_present")
            .join("ui");
        let Ok(files) = std::fs::read_dir(ui_dir) else {
            continue;
        };
        for file in files.flatten() {
            let Some(record) = ui_artifact_record_from_path(&session_id, &file.path()) else {
                continue;
            };
            if indexed_keys.contains(&(record.session_id.clone(), record.hash64.clone())) {
                continue;
            }
            if append_index_record(&record).is_ok() {
                eprintln!(
                    "Repaired missing UI artifact index entry {} ({})",
                    record.artifact_id, record.session_id
                );
            }
        }
    }
}

const ARTIFACT_HOST_STYLE: &str = concat!(
    "<style id=\"medousa-artifact-host\">",
    ":root{--medousa-host-bg:transparent;--medousa-host-fg:inherit;--medousa-host-muted:inherit}",
    "html,body{margin:0;padding:0;background:var(--medousa-host-bg,transparent);overflow:hidden;",
    "scrollbar-width:none;-ms-overflow-style:none}",
    "html::-webkit-scrollbar,body::-webkit-scrollbar{display:none;width:0;height:0}",
    "/* Agent utility: .medousa-fill { min-height:100%; width:100% } */",
    "</style>"
);

fn inject_artifact_host_styles(html: &str) -> String {
    if html.contains("medousa-artifact-host") {
        return html.to_string();
    }
    let lower = html.to_ascii_lowercase();
    if let Some(idx) = lower.find("</head>") {
        let mut out = String::with_capacity(html.len() + ARTIFACT_HOST_STYLE.len());
        out.push_str(&html[..idx]);
        out.push_str(ARTIFACT_HOST_STYLE);
        out.push_str(&html[idx..]);
        return out;
    }
    if let Some(idx) = lower.find("<head>") {
        let insert_at = idx + "<head>".len();
        let mut out = String::with_capacity(html.len() + ARTIFACT_HOST_STYLE.len());
        out.push_str(&html[..insert_at]);
        out.push_str(ARTIFACT_HOST_STYLE);
        out.push_str(&html[insert_at..]);
        return out;
    }
    if let Some(idx) = lower.find("<body") {
        let mut out = String::with_capacity(html.len() + ARTIFACT_HOST_STYLE.len() + 32);
        out.push_str(&html[..idx]);
        out.push_str("<head>");
        out.push_str(ARTIFACT_HOST_STYLE);
        out.push_str("</head>");
        out.push_str(&html[idx..]);
        return out;
    }
    html.to_string()
}

fn wrap_html_document(html: &str) -> String {
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("<html") || lower.contains("<!doctype") {
        return inject_artifact_host_styles(trimmed);
    }
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'\">{ARTIFACT_HOST_STYLE}</head><body>{trimmed}</body></html>"
    )
}

fn normalize_presentation(presentation: &str) -> std::result::Result<String, String> {
    match presentation.trim().to_ascii_lowercase().as_str() {
        "inline" | "panel" | "fullscreen" => Ok(presentation.trim().to_ascii_lowercase()),
        other if other.is_empty() => Ok("inline".to_string()),
        other => Err(format!(
            "presentation must be inline, panel, or fullscreen (got {other})"
        )),
    }
}

pub fn find_artifact(session_id: &str, query: Option<&str>) -> Option<StoredArtifact> {
    let records = read_index_records();
    if records.is_empty() {
        return None;
    }

    let query = query.map(str::trim).unwrap_or("");
    let mut candidates: Vec<ArtifactRecord> = records
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| b.stored_at_utc.cmp(&a.stored_at_utc));

    let record = if query.is_empty() || query.eq_ignore_ascii_case("last") {
        candidates.into_iter().next()
    } else {
        candidates.into_iter().find(|record| {
            record.artifact_id.starts_with(query)
                || record.hash64.starts_with(query)
                || record.tool_name.contains(query)
        })
    }?;

    let payload = std::fs::read_to_string(&record.payload_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())?;

    Some(StoredArtifact { record, payload })
}

pub fn list_artifact_records(session_id: &str, limit: usize) -> Vec<ArtifactRecord> {
    let mut records: Vec<ArtifactRecord> = read_index_records()
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();
    records.sort_by(|a, b| b.stored_at_utc.cmp(&a.stored_at_utc));
    records.into_iter().take(limit.max(1)).collect()
}

pub fn artifact_index_stats(session_id: &str) -> ArtifactIndexStats {
    let records = list_artifact_records(session_id, usize::MAX);
    let mut hashes = HashSet::new();
    let mut total_bytes = 0usize;
    for record in &records {
        hashes.insert(record.hash64.clone());
        total_bytes = total_bytes.saturating_add(record.byte_size);
    }
    ArtifactIndexStats {
        records: records.len(),
        unique_hashes: hashes.len(),
        total_bytes,
    }
}

pub fn run_artifact_maintenance(
    max_per_session: usize,
    max_age_days: i64,
) -> std::result::Result<ArtifactMaintenanceReport, String> {
    let max_per_session = max_per_session.max(1);
    let max_age_days = max_age_days.max(1);

    let mut report = ArtifactMaintenanceReport::default();
    let now = Utc::now();
    let age_cutoff = now - Duration::days(max_age_days);

    let mut records = read_index_records();
    report.records_before = records.len();

    let before_missing = records.len();
    records.retain(|record| Path::new(&record.payload_path).exists());
    report.missing_payload_pruned = before_missing.saturating_sub(records.len());

    let before_dedupe = records.len();
    let mut deduped: HashMap<(String, String, String, String), ArtifactRecord> = HashMap::new();
    for record in records {
        let key = (
            record.session_id.clone(),
            record.tool_name.clone(),
            record.direction.clone(),
            record.hash64.clone(),
        );
        match deduped.get(&key) {
            Some(existing) if existing.stored_at_utc >= record.stored_at_utc => {}
            _ => {
                deduped.insert(key, record);
            }
        }
    }
    let records: Vec<ArtifactRecord> = deduped.into_values().collect();
    report.deduped_records_pruned = before_dedupe.saturating_sub(records.len());

    let mut by_session: HashMap<String, Vec<ArtifactRecord>> = HashMap::new();
    for record in records {
        by_session
            .entry(record.session_id.clone())
            .or_default()
            .push(record);
    }

    let mut kept_records = Vec::new();
    let mut pruned_records = Vec::new();
    for (_session_id, mut group) in by_session {
        group.sort_by(|a, b| b.stored_at_utc.cmp(&a.stored_at_utc));
        for (idx, record) in group.into_iter().enumerate() {
            let too_old = record.stored_at_utc < age_cutoff;
            let over_limit = idx >= max_per_session;
            if too_old || over_limit {
                pruned_records.push(record);
            } else {
                kept_records.push(record);
            }
        }
    }
    report.retention_pruned = pruned_records.len();

    let referenced_payloads: HashSet<String> = kept_records
        .iter()
        .map(|record| record.payload_path.clone())
        .collect();

    let mut payload_files_deleted = 0usize;
    for record in pruned_records {
        if !referenced_payloads.contains(&record.payload_path)
            && std::fs::remove_file(&record.payload_path).is_ok()
        {
            payload_files_deleted = payload_files_deleted.saturating_add(1);
        }
    }
    report.payload_files_deleted = payload_files_deleted;

    kept_records.sort_by(|a, b| a.stored_at_utc.cmp(&b.stored_at_utc));
    overwrite_index_records(&kept_records)?;

    report.records_after = kept_records.len();
    Ok(report)
}

fn append_index_record(record: &ArtifactRecord) -> std::result::Result<(), String> {
    artifact_index_store().append(record)?;
    if ARTIFACT_INDEX_USES_SURREAL.load(Ordering::Acquire) && record.direction == "ui" {
        let _ = file_append_index_record(record);
    }
    Ok(())
}

fn overwrite_index_records(records: &[ArtifactRecord]) -> std::result::Result<(), String> {
    artifact_index_store().overwrite_all(records)
}

fn read_index_records() -> Vec<ArtifactRecord> {
    let primary = artifact_index_store().read_all();
    if !ARTIFACT_INDEX_USES_SURREAL.load(Ordering::Acquire) {
        return primary;
    }
    merge_artifact_records(primary, file_read_index_records())
}

fn merge_artifact_records(
    primary: Vec<ArtifactRecord>,
    secondary: Vec<ArtifactRecord>,
) -> Vec<ArtifactRecord> {
    let mut by_id: HashMap<String, ArtifactRecord> = HashMap::new();
    for record in secondary {
        by_id.entry(record.artifact_id.clone()).or_insert(record);
    }
    for record in primary {
        by_id.insert(record.artifact_id.clone(), record);
    }
    let mut merged: Vec<ArtifactRecord> = by_id.into_values().collect();
    merged.sort_by(|left, right| left.stored_at_utc.cmp(&right.stored_at_utc));
    merged
}

fn artifact_index_store() -> Arc<dyn ArtifactIndexStore> {
    ARTIFACT_INDEX_STORE.read().unwrap().clone()
}

struct FileArtifactIndexStore;

impl ArtifactIndexStore for FileArtifactIndexStore {
    fn read_all(&self) -> Vec<ArtifactRecord> {
        file_read_index_records()
    }

    fn append(&self, record: &ArtifactRecord) -> std::result::Result<(), String> {
        file_append_index_record(record)
    }

    fn overwrite_all(&self, records: &[ArtifactRecord]) -> std::result::Result<(), String> {
        file_overwrite_index_records(records)
    }
}

struct SurrealArtifactIndexStore {
    db: Surreal<Any>,
}

impl SurrealArtifactIndexStore {
    fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
        for statement in ARTIFACT_SCHEMA_STATEMENTS {
            if let Err(err) = self.db.query(*statement).await {
                let text = err.to_string();
                if !(text.contains("already exists")
                    || text.contains("already defined")
                    || text.contains("Overwrite index"))
                {
                    return Err(err);
                }
            }
        }
        Ok(())
    }
}

impl ArtifactIndexStore for SurrealArtifactIndexStore {
    fn read_all(&self) -> Vec<ArtifactRecord> {
        let sql = "SELECT * FROM type::table($table) ORDER BY stored_at_utc ASC";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", ARTIFACT_INDEX_TABLE)),
        ) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("SurrealArtifactIndexStore::read_all query error: {err}");
                return Vec::new();
            }
        };

        response.take::<Vec<ArtifactRecord>>(0).unwrap_or_default()
    }

    fn append(&self, record: &ArtifactRecord) -> std::result::Result<(), String> {
        // Record ids must not contain `:` — artifact_id does (art:session:tool:dir:hash).
        let sql = "UPSERT type::record($table, $id) CONTENT $data";
        block_on(
            self.db
                .query(sql)
                .bind(("table", ARTIFACT_INDEX_TABLE))
                .bind(("id", record.hash64.clone()))
                .bind(("data", record.clone())),
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn overwrite_all(&self, records: &[ArtifactRecord]) -> std::result::Result<(), String> {
        block_on(self.db.query("DELETE type::table($table)").bind((
            "table",
            ARTIFACT_INDEX_TABLE,
        )))
        .map_err(|err| err.to_string())?;

        for record in records {
            self.append(record)?;
        }
        Ok(())
    }
}

fn block_on<F: IntoFuture>(f: F) -> F::Output {
    tokio::task::block_in_place(move || Handle::current().block_on(f.into_future()))
}

fn file_append_index_record(record: &ArtifactRecord) -> std::result::Result<(), String> {
    let index_path = artifacts_root().join("index.jsonl");
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .map_err(|err| err.to_string())?;
    let line = serde_json::to_string(record).map_err(|err| err.to_string())?;
    writeln!(file, "{line}").map_err(|err| err.to_string())?;
    Ok(())
}

fn file_overwrite_index_records(records: &[ArtifactRecord]) -> std::result::Result<(), String> {
    let index_path = artifacts_root().join("index.jsonl");
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let temp_path = index_path.with_extension("jsonl.tmp");
    let mut file = std::fs::File::create(&temp_path).map_err(|err| err.to_string())?;
    for record in records {
        let line = serde_json::to_string(record).map_err(|err| err.to_string())?;
        writeln!(file, "{line}").map_err(|err| err.to_string())?;
    }
    std::fs::rename(temp_path, index_path).map_err(|err| err.to_string())?;
    Ok(())
}

fn file_read_index_records() -> Vec<ArtifactRecord> {
    let index_path = artifacts_root().join("index.jsonl");
    let Ok(file) = std::fs::File::open(index_path) else {
        return Vec::new();
    };

    std::io::BufReader::new(file)
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<ArtifactRecord>(&line).ok())
        .collect()
}

fn artifacts_root() -> PathBuf {
    data_local_medousa_dir().join("artifacts")
}

fn data_local_medousa_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}

fn slugify_tool_name(tool_name: &str) -> String {
    let mut out = String::new();
    for ch in tool_name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_ui_artifact_stores_html_with_metadata() {
        let session_id = "test-ui-artifact-session";
        let record = persist_ui_artifact(
            session_id,
            "<p>Hello</p>",
            "Greeting",
            "inline",
            Some(360),
        )
        .expect("persist");

        assert_eq!(record.content_type, "text/html");
        assert_eq!(record.label.as_deref(), Some("Greeting"));
        assert_eq!(record.presentation.as_deref(), Some("inline"));
        assert_eq!(record.height_px, Some(360));
        assert!(record.payload_path.ends_with(".html"));

        let fetched = fetch_artifact(session_id, &record.artifact_id).expect("fetch");
        assert_eq!(fetched.mime, "text/html");
        assert!(fetched.body.contains("Hello"));
    }

    #[test]
    fn persist_ui_artifact_rejects_oversize_payload() {
        let session_id = "test-ui-artifact-oversize";
        let huge = "x".repeat(UI_ARTIFACT_MAX_BYTES + 1);
        let err = persist_ui_artifact(session_id, &huge, "Big", "inline", None)
            .expect_err("should fail");
        assert!(err.contains("exceeds"));
    }

    #[test]
    fn wrap_html_document_injects_host_scrollbar_styles() {
        let wrapped = wrap_html_document("<div>Chart</div>");
        assert!(wrapped.contains("medousa-artifact-host"));
        assert!(wrapped.contains("overflow:hidden"));
        assert!(wrapped.contains("--medousa-host-bg"));

        let full = wrap_html_document(
            "<!DOCTYPE html><html><head></head><body><p>Full doc</p></body></html>",
        );
        assert!(full.contains("medousa-artifact-host"));
        assert!(full.contains("medousa-fill"));
    }

    #[test]
    fn fetch_ui_artifact_falls_back_to_disk_without_index_entry() {
        let session_id = "test-ui-artifact-disk-fallback";
        let html = "<p>Disk fallback</p>";
        let wrapped = wrap_html_document(html);
        let hash64 = crate::payload_receipt::hash_text(&wrapped);
        let artifact_id = build_ui_artifact_id(session_id, &hash64);

        let payload_dir = artifacts_root()
            .join(session_id)
            .join("cognition_ui_present")
            .join("ui");
        std::fs::create_dir_all(&payload_dir).expect("mkdir");
        let payload_path = payload_dir.join(format!("{hash64}.html"));
        std::fs::write(&payload_path, wrapped.as_bytes()).expect("write html");

        let fetched = fetch_artifact(session_id, &artifact_id).expect("disk fallback fetch");
        assert_eq!(fetched.mime, "text/html");
        assert!(fetched.body.contains("Disk fallback"));

        let _ = std::fs::remove_dir_all(artifacts_root().join(session_id));
    }

    #[test]
    fn resolve_latest_artifact_id_follows_supersedes_chain() {
        let session_id = "test-ui-artifact-lineage";
        let first = persist_ui_artifact(
            session_id,
            "<p>v1</p>",
            "Lineage",
            "inline",
            None,
        )
        .expect("first");
        let second = persist_ui_artifact_revision(
            session_id,
            "<p>v2</p>",
            "Lineage",
            "inline",
            None,
            Some(&first.artifact_id),
        )
        .expect("second");
        assert_eq!(
            second.supersedes_artifact_id.as_deref(),
            Some(first.artifact_id.as_str())
        );
        assert_eq!(
            resolve_latest_artifact_id(session_id, &first.artifact_id).as_deref(),
            Some(second.artifact_id.as_str())
        );
        let fetched = fetch_artifact(session_id, &first.artifact_id).expect("latest fetch");
        assert!(fetched.body.contains("v2"));
        let _ = std::fs::remove_dir_all(artifacts_root().join(session_id));
    }

    #[test]
    fn grep_ui_artifact_finds_html_snippet() {
        let session_id = "test-ui-artifact-grep";
        let record = persist_ui_artifact(
            session_id,
            "<style>.badge{color:red}</style>",
            "Grep me",
            "inline",
            None,
        )
        .expect("persist");
        let result = grep_ui_artifact(session_id, &record.artifact_id, ".badge", 0, 10)
            .expect("grep");
        assert_eq!(result.match_count, 1);
        let _ = std::fs::remove_dir_all(artifacts_root().join(session_id));
    }
}
