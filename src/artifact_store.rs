use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::prelude::RuntimeComposition;
use std::collections::{HashMap, HashSet};
use std::future::IntoFuture;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

use crate::store_root::StorePath;

const ARTIFACT_PAYLOAD_DOMAIN: &[u8] = b"artifact-payload";
const ARTIFACT_INDEX_FILE: &str = "index.jsonl";
const ARTIFACT_ALIASES_FILE: &str = "artifact_aliases.json";
const MAX_ARTIFACT_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;

#[cfg(test)]
static ARTIFACT_TEST_ROOT: Lazy<tempfile::TempDir> =
    Lazy::new(|| tempfile::tempdir().expect("artifact test store"));

static ARTIFACT_FILES: Lazy<crate::session_storage::SessionDirectoryStore> = Lazy::new(|| {
    #[cfg(test)]
    let root = ARTIFACT_TEST_ROOT.path().join("artifacts");
    #[cfg(not(test))]
    let root = crate::paths::medousa_data_dir().join("artifacts");
    crate::session_storage::SessionDirectoryStore::new(root)
});

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
    if let RuntimeComposition::Surreal(rt) = runtime {
        let store = SurrealArtifactIndexStore::new(rt.job_store.db());
        if let Err(err) = store.ensure_schema().await {
            eprintln!("Surreal artifact index schema init error: {err}; keeping file-backed index");
            return;
        }
        ARTIFACT_INDEX_USES_SURREAL.store(true, Ordering::Release);
        set_artifact_index_store(Arc::new(store));
        eprintln!("Surreal runtime detected; artifact index switched to SurrealDB backend");
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
    let (session_id, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
    let now = Utc::now();
    let tool_slug = slugify_tool_name(tool_name);
    let hash_short = hash64.chars().take(12).collect::<String>();
    let artifact_id = format!(
        "art:{}:{}:{}:{}",
        short_session(session_id.as_str()),
        tool_slug,
        direction,
        hash_short
    );

    let payload_path = artifact_payload_path(tool_name, direction, hash64, "json");
    if !ARTIFACT_FILES
        .is_file(&session_id, &payload_path)
        .map_err(|err| err.to_string())?
    {
        let raw = serde_json::to_vec_pretty(payload).map_err(|err| err.to_string())?;
        ARTIFACT_FILES
            .atomic_write(&session_id, &payload_path, &raw)
            .map_err(|err| err.to_string())?;
    }

    let record = ArtifactRecord {
        artifact_id,
        session_id: session_id.to_string(),
        tool_name: tool_name.to_string(),
        direction: direction.to_string(),
        hash64: hash64.to_string(),
        byte_size,
        stored_at_utc: now,
        payload_path: payload_path.file_name().to_string(),
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
    let (session_id, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
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
        short_session(session_id.as_str()),
        tool_slug,
        hash_short
    );

    let payload_path = artifact_payload_path(tool_name, "ui", &hash64, "html");
    if !ARTIFACT_FILES
        .is_file(&session_id, &payload_path)
        .map_err(|err| err.to_string())?
    {
        ARTIFACT_FILES
            .atomic_write(&session_id, &payload_path, wrapped.as_bytes())
            .map_err(|err| err.to_string())?;
    }

    let (supersedes, root_artifact_id) = if let Some(previous_id) = supersedes_artifact_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let previous = fetch_artifact_at_id(session_id.as_str(), previous_id).ok_or_else(|| {
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
        payload_path: payload_path.file_name().to_string(),
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

    let resolved = resolve_artifact_reference(session_id, query);
    let latest_id =
        resolve_latest_artifact_id(session_id, &resolved).unwrap_or_else(|| resolved.clone());
    fetch_artifact_at_id(session_id, &latest_id)
}

/// Resolve a presentation reference: canonical `art:…` ids, registered aliases, and hash suffixes.
pub fn resolve_artifact_reference(session_id: &str, artifact_ref: &str) -> String {
    let query = artifact_ref.trim();
    if query.is_empty() {
        return String::new();
    }
    if query.starts_with("art:") {
        return query.to_string();
    }
    resolve_artifact_alias(session_id, query).unwrap_or_else(|| query.to_string())
}

fn artifact_alias_path() -> StorePath {
    StorePath::parse(ARTIFACT_ALIASES_FILE).expect("static artifact alias path must be valid")
}

fn load_artifact_aliases(session_id: &str) -> HashMap<String, String> {
    let Ok(session_id) = crate::session_storage::SessionId::parse(session_id) else {
        return HashMap::new();
    };
    let Ok(raw) = ARTIFACT_FILES.read(&session_id, &artifact_alias_path()) else {
        return HashMap::new();
    };
    serde_json::from_slice(&raw).unwrap_or_default()
}

fn save_artifact_aliases(
    session_id: &str,
    aliases: &HashMap<String, String>,
) -> Result<(), String> {
    let (session_id, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
    let raw = serde_json::to_vec_pretty(aliases).map_err(|err| err.to_string())?;
    ARTIFACT_FILES
        .atomic_write(&session_id, &artifact_alias_path(), &raw)
        .map_err(|err| err.to_string())
}

/// Register a friendly alias (e.g. canvas component id) → canonical artifact id for a session.
pub fn register_artifact_alias(
    session_id: &str,
    alias: &str,
    artifact_id: &str,
) -> Result<(), String> {
    let alias = alias.trim();
    let artifact_id = artifact_id.trim();
    if alias.is_empty() || artifact_id.is_empty() {
        return Err("alias and artifact_id are required".to_string());
    }
    if !artifact_id.starts_with("art:") {
        return Err(format!(
            "artifact_id must be a canonical art:… id (got {artifact_id})"
        ));
    }
    let mut aliases = load_artifact_aliases(session_id);
    aliases.insert(alias.to_string(), artifact_id.to_string());
    save_artifact_aliases(session_id, &aliases)
}

/// Rebind aliases that pointed at `old_id` so they resolve to `new_id` after a revision.
pub fn rebind_artifact_aliases(session_id: &str, old_id: &str, new_id: &str) -> Result<(), String> {
    let old_id = old_id.trim();
    let new_id = new_id.trim();
    if old_id.is_empty() || new_id.is_empty() || old_id == new_id {
        return Ok(());
    }
    if !new_id.starts_with("art:") {
        return Err(format!(
            "artifact_id must be a canonical art:… id (got {new_id})"
        ));
    }
    let mut aliases = load_artifact_aliases(session_id);
    let mut changed = false;
    for target in aliases.values_mut() {
        if target.as_str() == old_id {
            *target = new_id.to_string();
            changed = true;
        }
    }
    if changed {
        save_artifact_aliases(session_id, &aliases)?;
    }
    Ok(())
}

pub fn resolve_artifact_alias(session_id: &str, alias: &str) -> Option<String> {
    let alias = alias.trim();
    if alias.is_empty() {
        return None;
    }
    load_artifact_aliases(session_id).get(alias).cloned()
}

pub fn presentation_artifact_exists(session_id: &str, artifact_ref: &str) -> bool {
    let resolved = resolve_artifact_reference(session_id, artifact_ref);
    if resolved.is_empty() {
        return false;
    }
    fetch_artifact_at_id(session_id, &resolved).is_some()
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
            records
                .iter()
                .find(|record| record.artifact_id == query || record.artifact_id.starts_with(query))
        })
        .cloned()?;

    load_fetched_from_record(record)
}

pub fn is_ui_html_record(record: &ArtifactRecord) -> bool {
    record.direction == "ui" && record.content_type == "text/html"
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
        .filter(is_ui_html_record)
        .filter(artifact_payload_exists)
        .filter(|record| session_id.is_none_or(|sid| record.session_id == sid))
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

    records.sort_by_key(|b| std::cmp::Reverse(b.stored_at_utc));
    dedupe_ui_artifacts_to_latest(records)
        .into_iter()
        .take(limit)
        .collect()
}

fn dedupe_ui_artifacts_to_latest(mut records: Vec<ArtifactRecord>) -> Vec<ArtifactRecord> {
    records.sort_by_key(|b| std::cmp::Reverse(b.stored_at_utc));
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
    kept.sort_by_key(|b| std::cmp::Reverse(b.stored_at_utc));
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
        .cloned()?;

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

/// Delete a UI HTML artifact revision chain (root + superseding revisions). Payload files are removed.
pub fn delete_ui_artifact(
    session_id: &str,
    artifact_ref: &str,
) -> std::result::Result<Vec<String>, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let (parsed_session_id, _mutation) =
        crate::session_deletion::acquire_mutation_for_str(session_id)?;
    let resolved = resolve_artifact_reference(session_id, artifact_ref);
    if resolved.is_empty() {
        return Err("artifact_id is required".to_string());
    }
    let latest = resolve_latest_artifact_id(session_id, &resolved).unwrap_or(resolved);
    let records = read_index_records();
    let Some(seed) = records
        .iter()
        .find(|record| record.session_id == session_id && record.artifact_id == latest)
        .cloned()
    else {
        return Err(format!("artifact not found: {latest}"));
    };
    if seed.direction != "ui" {
        return Err("only UI HTML presentation artifacts can be deleted from Home".to_string());
    }

    let root = seed
        .root_artifact_id
        .clone()
        .unwrap_or_else(|| seed.artifact_id.clone());
    let mut chain_ids: HashSet<String> = HashSet::new();
    chain_ids.insert(root.clone());
    for record in &records {
        if record.session_id != session_id {
            continue;
        }
        let record_root = record
            .root_artifact_id
            .clone()
            .unwrap_or_else(|| record.artifact_id.clone());
        if record_root == root {
            chain_ids.insert(record.artifact_id.clone());
        }
    }

    let to_delete: Vec<ArtifactRecord> = records
        .iter()
        .filter(|record| record.session_id == session_id && chain_ids.contains(&record.artifact_id))
        .cloned()
        .collect();
    if to_delete.is_empty() {
        return Err(format!("artifact not found: {latest}"));
    }

    let deleted_ids: Vec<String> = to_delete.iter().map(|r| r.artifact_id.clone()).collect();
    let remaining: Vec<ArtifactRecord> = records
        .into_iter()
        .filter(|record| !deleted_ids.contains(&record.artifact_id))
        .collect();
    overwrite_index_records(&remaining)?;

    let mut deleted_paths = HashSet::new();
    for record in &to_delete {
        let path = artifact_payload_path_for_record(record);
        if deleted_paths.insert(path.file_name().to_string()) {
            let exists = ARTIFACT_FILES
                .is_file(&parsed_session_id, &path)
                .map_err(|error| error.to_string())?;
            if exists {
                ARTIFACT_FILES
                    .remove_file(&parsed_session_id, &path)
                    .map_err(|error| error.to_string())?;
            }
        }
    }

    Ok(deleted_ids)
}

/// Delete the complete artifact satellite for one session.
pub fn delete_artifacts_for_session(session_id: &str) -> Result<(), String> {
    let session_id =
        crate::session_storage::SessionId::parse(session_id).map_err(|error| error.to_string())?;
    let remaining = artifact_index_store()
        .read_all()
        .into_iter()
        .filter(|record| record.session_id != session_id.as_str())
        .collect::<Vec<_>>();
    overwrite_index_records(&remaining)?;
    ARTIFACT_FILES
        .remove_session(&session_id)
        .map_err(|error| error.to_string())?;
    if artifact_index_store()
        .read_all()
        .iter()
        .any(|record| record.session_id == session_id.as_str())
        || ARTIFACT_FILES
            .contains_session(&session_id)
            .map_err(|error| error.to_string())?
    {
        return Err("artifact session data remains after deletion".to_string());
    }
    Ok(())
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
    load_fetched_from_store(&ARTIFACT_FILES, record)
}

fn load_fetched_from_store(
    files: &crate::session_storage::SessionDirectoryStore,
    mut record: ArtifactRecord,
) -> Option<FetchedArtifact> {
    let session_id = crate::session_storage::SessionId::parse(&record.session_id).ok()?;
    let path = artifact_payload_path_for_record(&record);
    let raw = match files.read_limited(&session_id, &path, MAX_ARTIFACT_PAYLOAD_BYTES) {
        Ok(raw) => raw,
        Err(_) => {
            let legacy_path = legacy_artifact_payload_path_for_record(&record)?;
            let raw = files
                .read_limited(&session_id, &legacy_path, MAX_ARTIFACT_PAYLOAD_BYTES)
                .ok()?;
            // The legacy location is derived exclusively from validated record metadata. Copying
            // it into the opaque object path restores old presentations without trusting the
            // persisted `payload_path`, which may contain an absolute or attacker-chosen path.
            let _ = files.atomic_write(&session_id, &path, &raw);
            raw
        }
    };
    record.payload_path = path.file_name().to_string();
    let mime = if record.content_type.is_empty() {
        if record.direction == "ui" {
            "text/html".to_string()
        } else {
            "application/json".to_string()
        }
    } else {
        record.content_type.clone()
    };

    let body = if mime == "text/html" {
        String::from_utf8(raw).ok()?
    } else {
        serde_json::from_slice::<Value>(&raw)
            .ok()
            .map(|value| serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()))
            .or_else(|| String::from_utf8(raw).ok())?
    };

    Some(FetchedArtifact { record, body, mime })
}

const ARTIFACT_HOST_STYLE: &str = concat!(
    "<style id=\"medousa-artifact-host\">",
    // Minimal host vars for chat/library exports; Home PresentationFrame injects full theme tokens live.
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
    if html.trim().is_empty() {
        return String::new();
    }
    let lower = html.to_ascii_lowercase();
    if lower.contains("<html") || lower.contains("<!doctype") {
        return inject_artifact_host_styles(html);
    }
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'\">{ARTIFACT_HOST_STYLE}</head><body>{html}</body></html>"
    )
}

fn normalize_presentation(presentation: &str) -> std::result::Result<String, String> {
    match presentation.trim().to_ascii_lowercase().as_str() {
        "inline" | "panel" | "fullscreen" => Ok(presentation.trim().to_ascii_lowercase()),
        "" => Ok("inline".to_string()),
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

    candidates.sort_by_key(|b| std::cmp::Reverse(b.stored_at_utc));

    let record = if query.is_empty() || query.eq_ignore_ascii_case("last") {
        candidates.into_iter().next()
    } else {
        candidates.into_iter().find(|record| {
            record.artifact_id.starts_with(query)
                || record.hash64.starts_with(query)
                || record.tool_name.contains(query)
        })
    }?;

    let session_id = crate::session_storage::SessionId::parse(&record.session_id).ok()?;
    let payload = ARTIFACT_FILES
        .read_limited(
            &session_id,
            &artifact_payload_path_for_record(&record),
            MAX_ARTIFACT_PAYLOAD_BYTES,
        )
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())?;

    Some(StoredArtifact { record, payload })
}

pub fn list_artifact_records(session_id: &str, limit: usize) -> Vec<ArtifactRecord> {
    let mut records: Vec<ArtifactRecord> = read_index_records()
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();
    records.sort_by_key(|b| std::cmp::Reverse(b.stored_at_utc));
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
    records.retain(artifact_payload_exists);
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
        group.sort_by_key(|b| std::cmp::Reverse(b.stored_at_utc));
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

    let referenced_payloads: HashSet<(String, String)> =
        kept_records.iter().map(artifact_payload_identity).collect();

    let mut payload_files_deleted = 0usize;
    for record in pruned_records {
        let identity = artifact_payload_identity(&record);
        if !referenced_payloads.contains(&identity)
            && let Ok(session_id) = crate::session_storage::SessionId::parse(&record.session_id)
            && ARTIFACT_FILES
                .remove_file(&session_id, &artifact_payload_path_for_record(&record))
                .is_ok()
        {
            payload_files_deleted += 1;
        }
    }
    report.payload_files_deleted = payload_files_deleted;

    kept_records.sort_by_key(|a| a.stored_at_utc);
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
    artifact_index_store().overwrite_all(records)?;
    if ARTIFACT_INDEX_USES_SURREAL.load(Ordering::Acquire) {
        let ui_records: Vec<ArtifactRecord> = records
            .iter()
            .filter(|record| is_ui_html_record(record))
            .cloned()
            .collect();
        file_overwrite_index_records(&ui_records)?;
    }
    Ok(())
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
    merged.sort_by_key(|left| left.stored_at_utc);
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
        let mut response = match block_on(self.db.query(sql).bind(("table", ARTIFACT_INDEX_TABLE)))
        {
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
        block_on(
            self.db
                .query("DELETE type::table($table)")
                .bind(("table", ARTIFACT_INDEX_TABLE)),
        )
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
    let mut line = serde_json::to_vec(record).map_err(|err| err.to_string())?;
    line.push(b'\n');
    ARTIFACT_FILES
        .append_root(&artifact_index_path(), &line)
        .map_err(|err| err.to_string())
}

fn file_overwrite_index_records(records: &[ArtifactRecord]) -> std::result::Result<(), String> {
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).map_err(|err| err.to_string())?;
        bytes.push(b'\n');
    }
    ARTIFACT_FILES
        .atomic_write_root(&artifact_index_path(), &bytes)
        .map_err(|err| err.to_string())
}

fn file_read_index_records() -> Vec<ArtifactRecord> {
    let Ok(bytes) = ARTIFACT_FILES.read_root(&artifact_index_path()) else {
        return Vec::new();
    };
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .filter_map(|line| serde_json::from_slice::<ArtifactRecord>(line).ok())
        .collect()
}

fn artifact_payload_path(
    tool_name: &str,
    direction: &str,
    hash64: &str,
    extension: &str,
) -> StorePath {
    let object_id = format!("{tool_name}\0{direction}\0{hash64}");
    crate::session_storage::session_object_path(ARTIFACT_PAYLOAD_DOMAIN, &object_id, extension)
}

fn artifact_payload_path_for_record(record: &ArtifactRecord) -> StorePath {
    let extension = if record.content_type == "text/html" || record.direction == "ui" {
        "html"
    } else {
        "json"
    };
    artifact_payload_path(
        &record.tool_name,
        &record.direction,
        &record.hash64,
        extension,
    )
}

fn legacy_artifact_payload_path_for_record(record: &ArtifactRecord) -> Option<StorePath> {
    let extension = if record.content_type == "text/html" || record.direction == "ui" {
        "html"
    } else {
        "json"
    };
    if record.hash64.is_empty() || !record.hash64.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    StorePath::parse(&format!(
        "{}/{}/{}.{}",
        slugify_tool_name(&record.tool_name),
        slugify_tool_name(&record.direction),
        record.hash64,
        extension,
    ))
    .ok()
}

fn artifact_payload_identity(record: &ArtifactRecord) -> (String, String) {
    (
        record.session_id.clone(),
        artifact_payload_path_for_record(record)
            .file_name()
            .to_string(),
    )
}

fn artifact_payload_exists(record: &ArtifactRecord) -> bool {
    let Ok(session_id) = crate::session_storage::SessionId::parse(&record.session_id) else {
        return false;
    };
    if ARTIFACT_FILES
        .is_file(&session_id, &artifact_payload_path_for_record(record))
        .unwrap_or(false)
    {
        return true;
    }
    legacy_artifact_payload_path_for_record(record)
        .is_some_and(|path| ARTIFACT_FILES.is_file(&session_id, &path).unwrap_or(false))
}

fn artifact_index_path() -> StorePath {
    StorePath::parse(ARTIFACT_INDEX_FILE).expect("static artifact index path must be valid")
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

    fn parsed_session_id(value: &str) -> crate::session_storage::SessionId {
        crate::session_storage::SessionId::parse(value).unwrap()
    }

    #[test]
    fn persist_ui_artifact_stores_html_with_metadata() {
        let session_id = "test-ui-artifact-session";
        let record =
            persist_ui_artifact(session_id, "<p>Hello</p>", "Greeting", "inline", Some(360))
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
        let err =
            persist_ui_artifact(session_id, &huge, "Big", "inline", None).expect_err("should fail");
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
    fn wrap_html_document_preserves_non_blank_content_boundaries() {
        let html = "  <div>Chart</div>  \n";
        let wrapped = wrap_html_document(html);
        assert!(wrapped.contains(html));
    }

    #[test]
    fn fetch_ignores_persisted_payload_path_authority() {
        let temp = tempfile::tempdir().expect("tempdir");
        let files = crate::session_storage::SessionDirectoryStore::new(temp.path().join("store"));
        let session_id = parsed_session_id("test-artifact-hostile-metadata");
        let payload = serde_json::json!({"source": "capability"});
        let raw = serde_json::to_vec(&payload).expect("serialize");
        let hash64 = crate::payload_receipt::hash_text(
            std::str::from_utf8(&raw).expect("JSON payload must be UTF-8"),
        );
        let path = artifact_payload_path("test_tool", "output", &hash64, "json");
        files
            .atomic_write(&session_id, &path, &raw)
            .expect("write derived payload");

        let outside = temp.path().join("outside.json");
        std::fs::write(&outside, br#"{"source":"outside"}"#).expect("write canary");
        let record = ArtifactRecord {
            artifact_id: "art:test:test_tool:output:hostile".to_string(),
            session_id: session_id.as_str().to_string(),
            tool_name: "test_tool".to_string(),
            direction: "output".to_string(),
            hash64,
            byte_size: raw.len(),
            stored_at_utc: Utc::now(),
            payload_path: outside.to_string_lossy().into_owned(),
            content_type: "application/json".to_string(),
            label: None,
            presentation: None,
            height_px: None,
            supersedes_artifact_id: None,
            root_artifact_id: None,
        };

        let fetched = load_fetched_from_store(&files, record).expect("fetch derived payload");
        assert!(fetched.body.contains("capability"));
        assert!(!fetched.body.contains("outside"));
        assert_eq!(fetched.record.payload_path, path.file_name());
        assert_eq!(
            std::fs::read(&outside).expect("read canary"),
            br#"{"source":"outside"}"#
        );
    }

    #[test]
    fn fetch_migrates_legacy_payload_from_metadata_derived_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let files = crate::session_storage::SessionDirectoryStore::new(temp.path().join("store"));
        let session_id = parsed_session_id("test-artifact-legacy-payload");
        let raw = b"<html><body>Legacy presentation</body></html>";
        let hash64 = crate::payload_receipt::hash_text(std::str::from_utf8(raw).expect("html"));
        let record = ArtifactRecord {
            artifact_id: format!("art:test:cognition_ui_present:ui:{hash64}"),
            session_id: session_id.as_str().to_string(),
            tool_name: "cognition_ui_present".to_string(),
            direction: "ui".to_string(),
            hash64,
            byte_size: raw.len(),
            stored_at_utc: Utc::now(),
            payload_path: "/ignored/legacy/path.html".to_string(),
            content_type: "text/html".to_string(),
            label: Some("Legacy".to_string()),
            presentation: Some("inline".to_string()),
            height_px: None,
            supersedes_artifact_id: None,
            root_artifact_id: None,
        };
        let legacy = legacy_artifact_payload_path_for_record(&record).expect("legacy path");
        let current = artifact_payload_path_for_record(&record);
        files
            .atomic_write(&session_id, &legacy, raw)
            .expect("write legacy payload");
        assert!(
            !files
                .is_file(&session_id, &current)
                .expect("current lookup")
        );

        let fetched = load_fetched_from_store(&files, record).expect("fetch legacy payload");

        assert!(fetched.body.contains("Legacy presentation"));
        assert!(
            files
                .is_file(&session_id, &current)
                .expect("migrated lookup")
        );
    }

    #[test]
    fn list_ui_artifacts_omits_records_without_payloads() {
        let session_id = "test-ui-artifact-missing-payload";
        let record = ArtifactRecord {
            artifact_id: "art:test:cognition_ui_present:ui:deadbeef".to_string(),
            session_id: session_id.to_string(),
            tool_name: "cognition_ui_present".to_string(),
            direction: "ui".to_string(),
            hash64: "deadbeef".to_string(),
            byte_size: 128,
            stored_at_utc: Utc::now(),
            payload_path: "/missing/presentation.html".to_string(),
            content_type: "text/html".to_string(),
            label: Some("Missing".to_string()),
            presentation: Some("inline".to_string()),
            height_px: None,
            supersedes_artifact_id: None,
            root_artifact_id: None,
        };
        append_index_record(&record).expect("index missing record");

        assert!(list_ui_artifacts(Some(session_id), 10, None).is_empty());
    }

    #[test]
    fn artifact_alias_resolves_friendly_component_ids() {
        let session_id = "test-ui-artifact-alias-session";
        let record = persist_ui_artifact(session_id, "<p>Alias test</p>", "Alias", "inline", None)
            .expect("persist");
        register_artifact_alias(session_id, "adhd-guide-index", &record.artifact_id)
            .expect("register alias");

        let fetched = fetch_artifact(session_id, "adhd-guide-index").expect("fetch by alias");
        assert_eq!(fetched.record.artifact_id, record.artifact_id);
        assert!(fetched.body.contains("Alias test"));
    }

    #[test]
    fn rebind_artifact_aliases_updates_targets() {
        let session_id = "test-ui-artifact-alias-rebind-v2";
        let first = persist_ui_artifact(
            session_id,
            "<p>rebind-v1-unique</p>",
            "Rebind",
            "inline",
            None,
        )
        .expect("first");
        register_artifact_alias(session_id, "widget-a", &first.artifact_id).expect("alias");
        let second = persist_ui_artifact(
            session_id,
            "<p>rebind-v2-unique</p>",
            "Rebind",
            "inline",
            None,
        )
        .expect("second");
        rebind_artifact_aliases(session_id, &first.artifact_id, &second.artifact_id)
            .expect("rebind");
        assert_eq!(
            resolve_artifact_alias(session_id, "widget-a").as_deref(),
            Some(second.artifact_id.as_str())
        );
        let _ = ARTIFACT_FILES.remove_session(&parsed_session_id(session_id));
    }

    #[test]
    fn resolve_latest_artifact_id_follows_supersedes_chain() {
        let session_id = "test-ui-artifact-lineage";
        let first =
            persist_ui_artifact(session_id, "<p>v1</p>", "Lineage", "inline", None).expect("first");
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
        let _ = ARTIFACT_FILES.remove_session(&parsed_session_id(session_id));
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
        let result =
            grep_ui_artifact(session_id, &record.artifact_id, ".badge", 0, 10).expect("grep");
        assert_eq!(result.match_count, 1);
        let _ = ARTIFACT_FILES.remove_session(&parsed_session_id(session_id));
    }
}
