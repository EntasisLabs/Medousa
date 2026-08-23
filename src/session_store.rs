use std::collections::HashMap;
use std::fmt;
use std::future::IntoFuture;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::{Arc, Mutex, RwLock};

use async_trait::async_trait;
use medousa_types::SessionId;
use medousa_types::session::{
    ConversationTurn, ExecutionRef, SessionDerivation, SessionHistorySummary, TranscriptEntry,
    TranscriptEntryId, TranscriptEntryRef,
};
use medousa_types::turn::{TurnPart, TurnSliceSummary};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

const TRANSCRIPT_ENTRY_TABLE: &str = "transcript_entry";
const SESSION_ENTRY_TABLE: &str = "session_entry";
pub const MAX_TRANSCRIPT_SEARCH_QUERY_CHARS: usize = 512;

static FILE_SESSION_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Configure the daemon-owned fallback transcript directory before the global
/// store is first accessed. Embedded deployments pass their sandbox root;
/// full daemon deployments retain the existing path resolver by default.
pub fn configure_file_session_root(root: PathBuf) -> Result<(), String> {
    if let Some(existing) = FILE_SESSION_ROOT.get() {
        return (existing == &root)
            .then_some(())
            .ok_or_else(|| "session store root was already configured".to_string());
    }
    FILE_SESSION_ROOT
        .set(root)
        .map_err(|_| "session store root configuration failed".to_string())
}

fn file_session_root() -> PathBuf {
    if let Some(root) = FILE_SESSION_ROOT.get() {
        return root.clone();
    }
    #[cfg(feature = "full-daemon")]
    {
        crate::paths::medousa_data_dir().join("history")
    }
    #[cfg(not(feature = "full-daemon"))]
    {
        panic!("embedded daemon must configure its session store root before boot")
    }
}

#[cfg(feature = "full-daemon")]
fn record_catalog_append(session_id: &SessionId, turn: &ConversationTurn) {
    crate::session_catalog::record_turn_appended_for_id(session_id, turn);
}

#[cfg(not(feature = "full-daemon"))]
fn record_catalog_append(_session_id: &SessionId, _turn: &ConversationTurn) {}

fn preview_from_turn(turn: &ConversationTurn) -> Option<String> {
    #[cfg(feature = "full-daemon")]
    {
        crate::session_catalog::preview_from_turn(turn)
    }
    #[cfg(not(feature = "full-daemon"))]
    {
        let text = turn.content.trim();
        if text.is_empty() {
            None
        } else {
            Some(text.lines().next().unwrap_or("").chars().take(72).collect())
        }
    }
}

const SESSION_SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE session_turn SCHEMAFULL",
    "DEFINE FIELD session_id ON TABLE session_turn TYPE string",
    "DEFINE FIELD role ON TABLE session_turn TYPE string",
    "DEFINE FIELD content ON TABLE session_turn TYPE string",
    "DEFINE FIELD timestamp ON TABLE session_turn TYPE datetime",
    "DEFINE FIELD tool_names ON TABLE session_turn TYPE array<string>",
    "DEFINE FIELD answer_state ON TABLE session_turn TYPE option<string>",
    // JSON-serialized TurnPart[] — kept as string so SCHEMAFULL does not reject nested arrays.
    "DEFINE FIELD parts ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD slice_summary ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD speaker_profile_id ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD search_text ON TABLE session_turn TYPE option<string>",
    "DEFINE INDEX idx_session_turn_session_id ON TABLE session_turn COLUMNS session_id",
    "DEFINE INDEX idx_session_turn_timestamp ON TABLE session_turn COLUMNS timestamp",
    "DEFINE ANALYZER medousa_transcript TOKENIZERS BLANK,CLASS,PUNCT FILTERS LOWERCASE",
    "DEFINE INDEX idx_session_turn_search ON TABLE session_turn FIELDS search_text FULLTEXT ANALYZER medousa_transcript BM25 HIGHLIGHTS",
    "DEFINE TABLE transcript_entry SCHEMAFULL",
    "DEFINE FIELD entry_id ON TABLE transcript_entry TYPE string",
    "DEFINE FIELD role ON TABLE transcript_entry TYPE string",
    "DEFINE FIELD content ON TABLE transcript_entry TYPE string",
    "DEFINE FIELD timestamp ON TABLE transcript_entry TYPE datetime",
    "DEFINE FIELD tool_names ON TABLE transcript_entry TYPE array<string>",
    "DEFINE FIELD answer_state ON TABLE transcript_entry TYPE option<string>",
    "DEFINE FIELD parts ON TABLE transcript_entry TYPE option<string>",
    "DEFINE FIELD slice_summary ON TABLE transcript_entry TYPE option<string>",
    "DEFINE FIELD speaker_profile_id ON TABLE transcript_entry TYPE option<string>",
    "DEFINE FIELD execution_authority_id ON TABLE transcript_entry TYPE option<string>",
    "DEFINE FIELD execution_session_id ON TABLE transcript_entry TYPE option<string>",
    "DEFINE FIELD execution_id ON TABLE transcript_entry TYPE option<string>",
    "DEFINE FIELD content_digest ON TABLE transcript_entry TYPE string",
    "DEFINE INDEX idx_transcript_entry_id ON TABLE transcript_entry COLUMNS entry_id UNIQUE",
    "DEFINE TABLE session_entry SCHEMAFULL",
    "DEFINE FIELD session_id ON TABLE session_entry TYPE string",
    "DEFINE FIELD entry_seq ON TABLE session_entry TYPE int",
    "DEFINE FIELD entry_id ON TABLE session_entry TYPE string",
    "DEFINE FIELD source_authority_id ON TABLE session_entry TYPE option<string>",
    "DEFINE FIELD source_session_id ON TABLE session_entry TYPE option<string>",
    "DEFINE FIELD source_entry_id ON TABLE session_entry TYPE option<string>",
    "DEFINE FIELD source_entry_seq ON TABLE session_entry TYPE option<int>",
    "DEFINE FIELD committed_at ON TABLE session_entry TYPE datetime",
    // Rebuildable search/catalog projection kept beside the binding.
    "DEFINE FIELD role ON TABLE session_entry TYPE string",
    "DEFINE FIELD search_text ON TABLE session_entry TYPE option<string>",
    "DEFINE FIELD timestamp ON TABLE session_entry TYPE datetime",
    "DEFINE INDEX idx_session_entry_position ON TABLE session_entry COLUMNS session_id, entry_seq UNIQUE",
    "DEFINE INDEX idx_session_entry_membership ON TABLE session_entry COLUMNS session_id, entry_id UNIQUE",
    "DEFINE INDEX idx_session_entry_entry_id ON TABLE session_entry COLUMNS entry_id",
    "DEFINE INDEX idx_session_entry_search ON TABLE session_entry FIELDS search_text FULLTEXT ANALYZER medousa_transcript BM25 HIGHLIGHTS",
    "DEFINE TABLE context_manifest SCHEMAFULL",
    "DEFINE FIELD manifest_id ON TABLE context_manifest TYPE string",
    "DEFINE FIELD manifest_json ON TABLE context_manifest TYPE string",
    "DEFINE FIELD created_by ON TABLE context_manifest TYPE string",
    "DEFINE FIELD created_at ON TABLE context_manifest TYPE datetime",
    "DEFINE INDEX idx_context_manifest_id ON TABLE context_manifest COLUMNS manifest_id UNIQUE",
    "DEFINE TABLE session_derivation SCHEMAFULL",
    "DEFINE FIELD derivation_id ON TABLE session_derivation TYPE string",
    "DEFINE FIELD target_session_id ON TABLE session_derivation TYPE string",
    "DEFINE FIELD manifest_id ON TABLE session_derivation TYPE string",
    "DEFINE FIELD idempotency_key_digest ON TABLE session_derivation TYPE string",
    "DEFINE FIELD request_digest ON TABLE session_derivation TYPE string",
    "DEFINE FIELD derivation_json ON TABLE session_derivation TYPE string",
    "DEFINE FIELD created_by ON TABLE session_derivation TYPE string",
    "DEFINE FIELD created_at ON TABLE session_derivation TYPE datetime",
    "DEFINE INDEX idx_session_derivation_id ON TABLE session_derivation COLUMNS derivation_id UNIQUE",
    "DEFINE INDEX idx_session_derivation_target ON TABLE session_derivation COLUMNS target_session_id UNIQUE",
    "DEFINE INDEX idx_session_derivation_idempotency ON TABLE session_derivation COLUMNS created_by, idempotency_key_digest UNIQUE",
];

const SESSION_SCHEMA_MIGRATIONS: &[&str] = &[
    "DEFINE FIELD OVERWRITE parts ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD OVERWRITE slice_summary ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD OVERWRITE speaker_profile_id ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD OVERWRITE search_text ON TABLE session_turn TYPE option<string>",
    "UPDATE session_turn SET search_text = content WHERE search_text = NONE OR search_text = NULL",
];

/// Initialize the session store based on the runtime composition.
/// When a Surreal runtime is active, swaps the file-backed store for a
/// SurrealDB-backed implementation.
pub async fn init_session_store_with_runtime(runtime: &RuntimeComposition) {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            let db = rt.job_store.db();
            let store = SurrealSessionStore::new(db);
            if let Err(err) = store.ensure_schema().await {
                eprintln!(
                    "Surreal session store schema init error: {err}; falling back to file-backed store"
                );
                return;
            }
            set_session_store(Arc::new(store));
            eprintln!("Surreal runtime detected; session store switched to SurrealDB backend");
        }
        _ => {
            // Keep file-backed store for in-memory runtimes.
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct SessionTurnRecord {
    session_id: String,
    role: String,
    content: String,
    timestamp: chrono::DateTime<chrono::Utc>,
    tool_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    answer_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parts: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slice_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    speaker_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct TranscriptEntryRecord {
    entry_id: String,
    role: String,
    content: String,
    timestamp: chrono::DateTime<chrono::Utc>,
    tool_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    answer_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parts: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slice_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    speaker_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_authority_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_id: Option<String>,
    content_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct SessionEntryRecord {
    session_id: String,
    entry_seq: i64,
    entry_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_authority_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_entry_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_entry_seq: Option<i64>,
    committed_at: chrono::DateTime<chrono::Utc>,
    role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_text: Option<String>,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct TranscriptAppend {
    pub turn: ConversationTurn,
    pub caused_by: Option<ExecutionRef>,
    pub existing_entry_id: Option<TranscriptEntryId>,
    pub source: Option<TranscriptEntryRef>,
    pub expected_digest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DerivationCommitRequest {
    pub derivation: SessionDerivation,
    pub idempotency_key_digest: String,
    pub request_digest: String,
    pub entries: Vec<TranscriptAppend>,
}

#[derive(Debug, Clone)]
pub struct DerivationCommitOutcome {
    pub derivation: SessionDerivation,
    pub reused: bool,
}

#[derive(Debug, Clone)]
pub struct DerivationLookup {
    pub derivation: SessionDerivation,
    pub idempotency_key_digest: String,
    pub request_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct StoredDerivationRecord {
    derivation_id: String,
    target_session_id: String,
    manifest_id: String,
    idempotency_key_digest: String,
    request_digest: String,
    derivation_json: String,
    created_by: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct StoredContextManifestRecord {
    manifest_id: String,
    manifest_json: String,
    created_by: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl TranscriptAppend {
    pub fn native(turn: ConversationTurn, caused_by: Option<ExecutionRef>) -> Self {
        Self {
            turn,
            caused_by,
            existing_entry_id: None,
            source: None,
            expected_digest: None,
        }
    }
}

#[derive(Default)]
struct SessionCommitLocks {
    by_session: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl SessionCommitLocks {
    fn for_session(&self, session_id: &SessionId) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.by_session.lock().unwrap();
        Arc::clone(
            locks
                .entry(session_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptSearchMatch {
    pub session_id: String,
    pub role: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub excerpt: String,
}

fn turn_search_text(turn: &ConversationTurn) -> Option<String> {
    if !matches!(turn.role.as_str(), "user" | "assistant" | "agent") {
        return None;
    }
    let content = turn.content.trim();
    if !content.is_empty() {
        return Some(content.to_string());
    }
    let text = turn
        .parts
        .as_deref()?
        .iter()
        .filter_map(|part| match part {
            TurnPart::Text { markdown } | TurnPart::Progress { markdown } => Some(markdown.trim()),
            TurnPart::Handoff { text, .. } => Some(text.trim()),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

fn transcript_excerpt(text: &str, query: &str) -> String {
    const EXCERPT_CHARS: usize = 420;
    const LEADING_CONTEXT_CHARS: usize = 90;

    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = collapsed.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    let start = lower
        .find(&needle)
        .map(|byte| {
            collapsed[..byte]
                .chars()
                .count()
                .saturating_sub(LEADING_CONTEXT_CHARS)
        })
        .unwrap_or(0);
    let excerpt = collapsed
        .chars()
        .skip(start)
        .take(EXCERPT_CHARS)
        .collect::<String>();
    if start > 0 {
        format!("…{excerpt}")
    } else {
        excerpt
    }
}

fn validate_transcript_search(query: &str) -> Result<(), StoreError> {
    if query.trim().is_empty() {
        return Err(StoreError::InvalidInput(
            "transcript search query cannot be empty".to_string(),
        ));
    }
    if query.chars().count() > MAX_TRANSCRIPT_SEARCH_QUERY_CHARS {
        return Err(StoreError::InvalidInput(format!(
            "transcript search query exceeds {MAX_TRANSCRIPT_SEARCH_QUERY_CHARS} characters"
        )));
    }
    Ok(())
}

fn parts_to_json(parts: Option<&[TurnPart]>) -> Option<String> {
    parts.and_then(|items| serde_json::to_string(items).ok())
}

fn parts_from_json(value: Option<String>) -> Option<Vec<TurnPart>> {
    let raw = value?;
    if let Ok(parts) = serde_json::from_str::<Vec<TurnPart>>(&raw) {
        return (!parts.is_empty()).then_some(parts);
    }
    // Tolerant reload: keep recognized parts if the array mixed in an unknown kind.
    let items: Vec<serde_json::Value> = serde_json::from_str(&raw).ok()?;
    let parts = items
        .into_iter()
        .filter_map(|item| serde_json::from_value::<TurnPart>(item).ok())
        .collect::<Vec<_>>();
    (!parts.is_empty()).then_some(parts)
}

fn slice_summary_from_json(value: Option<String>) -> Option<TurnSliceSummary> {
    value.and_then(|raw| serde_json::from_str(&raw).ok())
}

fn slice_summary_to_json(value: Option<&TurnSliceSummary>) -> Option<String> {
    value.and_then(|summary| serde_json::to_string(summary).ok())
}

impl From<SessionTurnRecord> for ConversationTurn {
    fn from(record: SessionTurnRecord) -> Self {
        ConversationTurn {
            role: record.role,
            content: record.content,
            timestamp: record.timestamp,
            tool_names: record.tool_names,
            answer_state: record.answer_state,
            parts: parts_from_json(record.parts),
            slice_summary: slice_summary_from_json(record.slice_summary),
            speaker_profile_id: record.speaker_profile_id,
        }
    }
}

impl From<&ConversationTurn> for SessionTurnRecord {
    fn from(turn: &ConversationTurn) -> Self {
        SessionTurnRecord {
            session_id: String::new(), // filled in by caller
            role: turn.role.clone(),
            content: turn.content.clone(),
            timestamp: turn.timestamp,
            tool_names: turn.tool_names.clone(),
            answer_state: turn.answer_state.clone(),
            parts: parts_to_json(turn.parts.as_deref()),
            slice_summary: slice_summary_to_json(turn.slice_summary.as_ref()),
            speaker_profile_id: turn.speaker_profile_id.clone(),
            search_text: turn_search_text(turn),
        }
    }
}

fn content_digest(turn: &ConversationTurn) -> Result<String, StoreError> {
    use sha2::{Digest as _, Sha256};

    let bytes =
        serde_json::to_vec(turn).map_err(|error| StoreError::Serialization(error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(b"medousa/transcript-entry/v1\0");
    digest.update(bytes);
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn new_entry_id() -> TranscriptEntryId {
    TranscriptEntryId::parse(format!("ent_{}", uuid::Uuid::new_v4().simple()))
        .expect("daemon-generated transcript entry id must be valid")
}

fn legacy_entry_id(session_id: &SessionId, entry_seq: u64, digest: &str) -> TranscriptEntryId {
    use sha2::{Digest as _, Sha256};

    let mut hash = Sha256::new();
    hash.update(b"medousa/legacy-transcript-entry/v1\0");
    hash.update(session_id.as_str().as_bytes());
    hash.update(entry_seq.to_be_bytes());
    hash.update(digest.as_bytes());
    let encoded = format!("{:x}", hash.finalize());
    TranscriptEntryId::parse(format!("ent_{}", &encoded[..32]))
        .expect("derived legacy transcript entry id must be valid")
}

fn materialize_append(
    entry_seq: u64,
    append: &TranscriptAppend,
) -> Result<TranscriptEntry, StoreError> {
    let digest = content_digest(&append.turn)?;
    if append
        .expected_digest
        .as_deref()
        .is_some_and(|expected| expected != digest)
    {
        return Err(StoreError::InvalidInput(
            "transcript entry digest does not match immutable payload".to_string(),
        ));
    }
    Ok(TranscriptEntry {
        entry_id: append
            .existing_entry_id
            .clone()
            .unwrap_or_else(new_entry_id),
        entry_seq,
        caused_by: append.caused_by.clone(),
        source: append.source.clone(),
        content_digest: digest,
        turn: append.turn.clone(),
    })
}

fn materialize_legacy_turn(
    session_id: &SessionId,
    entry_seq: u64,
    turn: ConversationTurn,
) -> Result<TranscriptEntry, StoreError> {
    let digest = content_digest(&turn)?;
    Ok(TranscriptEntry {
        entry_id: legacy_entry_id(session_id, entry_seq, &digest),
        entry_seq,
        caused_by: None,
        source: None,
        content_digest: digest,
        turn,
    })
}

fn entry_record(entry: &TranscriptEntry) -> TranscriptEntryRecord {
    TranscriptEntryRecord {
        entry_id: entry.entry_id.to_string(),
        role: entry.turn.role.clone(),
        content: entry.turn.content.clone(),
        timestamp: entry.turn.timestamp,
        tool_names: entry.turn.tool_names.clone(),
        answer_state: entry.turn.answer_state.clone(),
        parts: parts_to_json(entry.turn.parts.as_deref()),
        slice_summary: slice_summary_to_json(entry.turn.slice_summary.as_ref()),
        speaker_profile_id: entry.turn.speaker_profile_id.clone(),
        execution_authority_id: entry
            .caused_by
            .as_ref()
            .map(|value| value.authority_id.to_string()),
        execution_session_id: entry
            .caused_by
            .as_ref()
            .map(|value| value.session_id.to_string()),
        execution_id: entry
            .caused_by
            .as_ref()
            .map(|value| value.execution_id.to_string()),
        content_digest: entry.content_digest.clone(),
    }
}

fn binding_record(session_id: &SessionId, entry: &TranscriptEntry) -> SessionEntryRecord {
    SessionEntryRecord {
        session_id: session_id.to_string(),
        entry_seq: i64::try_from(entry.entry_seq)
            .expect("transcript entry sequence must fit SurrealDB int"),
        entry_id: entry.entry_id.to_string(),
        source_authority_id: entry
            .source
            .as_ref()
            .map(|value| value.session.authority_id.to_string()),
        source_session_id: entry
            .source
            .as_ref()
            .map(|value| value.session.session_id.to_string()),
        source_entry_id: entry
            .source
            .as_ref()
            .map(|value| value.entry_id.to_string()),
        source_entry_seq: entry.source.as_ref().map(|value| {
            i64::try_from(value.entry_seq).expect("source entry sequence must fit SurrealDB int")
        }),
        committed_at: chrono::Utc::now(),
        role: entry.turn.role.clone(),
        search_text: turn_search_text(&entry.turn),
        timestamp: entry.turn.timestamp,
    }
}

fn transcript_entry_from_records(
    entry: TranscriptEntryRecord,
    binding: SessionEntryRecord,
) -> Option<TranscriptEntry> {
    use medousa_types::session::{AuthorityId, ExecutionId, SessionRef};

    let caused_by = match (
        entry.execution_authority_id,
        entry.execution_session_id,
        entry.execution_id,
    ) {
        (Some(authority), Some(session), Some(execution)) => Some(ExecutionRef {
            authority_id: AuthorityId::parse(authority).ok()?,
            session_id: SessionId::parse(session).ok()?,
            execution_id: ExecutionId::parse(execution).ok()?,
        }),
        _ => None,
    };
    let source = match (
        binding.source_authority_id,
        binding.source_session_id,
        binding.source_entry_id,
        binding.source_entry_seq,
    ) {
        (Some(authority), Some(session), Some(entry_id), Some(entry_seq)) => {
            Some(TranscriptEntryRef {
                session: SessionRef {
                    authority_id: AuthorityId::parse(authority).ok()?,
                    session_id: SessionId::parse(session).ok()?,
                },
                entry_id: TranscriptEntryId::parse(entry_id).ok()?,
                entry_seq: u64::try_from(entry_seq).ok()?,
            })
        }
        _ => None,
    };
    Some(TranscriptEntry {
        entry_id: TranscriptEntryId::parse(entry.entry_id).ok()?,
        entry_seq: u64::try_from(binding.entry_seq).ok()?,
        caused_by,
        source,
        content_digest: entry.content_digest,
        turn: ConversationTurn {
            role: entry.role,
            content: entry.content,
            timestamp: entry.timestamp,
            tool_names: entry.tool_names,
            answer_state: entry.answer_state,
            parts: parts_from_json(entry.parts),
            slice_summary: slice_summary_from_json(entry.slice_summary),
            speaker_profile_id: entry.speaker_profile_id,
        },
    })
}

fn stored_derivation_record(
    request: &DerivationCommitRequest,
) -> Result<StoredDerivationRecord, StoreError> {
    let derivation = &request.derivation;
    Ok(StoredDerivationRecord {
        derivation_id: derivation.derivation_id.to_string(),
        target_session_id: derivation.target_session.session_id.to_string(),
        manifest_id: derivation.manifest.manifest_id.to_string(),
        idempotency_key_digest: request.idempotency_key_digest.clone(),
        request_digest: request.request_digest.clone(),
        derivation_json: serde_json::to_string(derivation)
            .map_err(|error| StoreError::Serialization(error.to_string()))?,
        created_by: derivation.created_by.clone(),
        created_at: derivation.created_at,
    })
}

fn decode_stored_derivation(
    record: &StoredDerivationRecord,
) -> Result<SessionDerivation, StoreError> {
    serde_json::from_str(&record.derivation_json)
        .map_err(|error| StoreError::Serialization(error.to_string()))
}

fn derivation_lookup(record: &StoredDerivationRecord) -> Result<DerivationLookup, StoreError> {
    Ok(DerivationLookup {
        derivation: decode_stored_derivation(record)?,
        idempotency_key_digest: record.idempotency_key_digest.clone(),
        request_digest: record.request_digest.clone(),
    })
}

fn reused_derivation(
    record: &StoredDerivationRecord,
    request: &DerivationCommitRequest,
) -> Result<DerivationCommitOutcome, StoreError> {
    if record.idempotency_key_digest != request.idempotency_key_digest
        || record.request_digest != request.request_digest
    {
        return Err(StoreError::InvalidInput(
            "idempotency key was already used for another derivation request".to_string(),
        ));
    }
    Ok(DerivationCommitOutcome {
        derivation: decode_stored_derivation(record)?,
        reused: true,
    })
}

fn derivation_entries_match(existing: &[TranscriptEntry], expected: &[TranscriptEntry]) -> bool {
    existing.len() == expected.len()
        && existing.iter().zip(expected).all(|(left, right)| {
            left.entry_id == right.entry_id
                && left.entry_seq == right.entry_seq
                && left.content_digest == right.content_digest
                && left.source == right.source
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitDurability {
    /// The bytes were accepted by the capability-owned filesystem handle.
    FilesystemWrite,
    /// The database accepted the complete batch as one statement.
    DatabaseCommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitReceipt {
    pub turns: usize,
    pub bytes: usize,
    pub durability: CommitDurability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    InvalidInput(String),
    Serialization(String),
    Backend(String),
    Worker(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::Serialization(message) => write!(formatter, "serialization failed: {message}"),
            Self::Backend(message) => write!(formatter, "store backend failed: {message}"),
            Self::Worker(message) => write!(formatter, "store worker failed: {message}"),
        }
    }
}

impl std::error::Error for StoreError {}

#[async_trait]
pub trait SessionStore: Send + Sync + 'static {
    fn load_transcript_entries(&self, session_id: &SessionId) -> Vec<TranscriptEntry>;

    fn load_history(&self, session_id: &SessionId) -> Vec<ConversationTurn> {
        self.load_transcript_entries(session_id)
            .into_iter()
            .map(|entry| entry.turn)
            .collect()
    }

    async fn append_transcript_batch(
        &self,
        session_id: &SessionId,
        entries: &[TranscriptAppend],
    ) -> Result<CommitReceipt, StoreError>;

    async fn materialize_derivation(
        &self,
        request: &DerivationCommitRequest,
    ) -> Result<DerivationCommitOutcome, StoreError>;

    fn load_derivation(
        &self,
        target_session_id: &SessionId,
    ) -> Result<Option<DerivationLookup>, StoreError>;

    async fn append_turn_batch(
        &self,
        session_id: &SessionId,
        turns: &[ConversationTurn],
    ) -> Result<CommitReceipt, StoreError> {
        let entries = turns
            .iter()
            .cloned()
            .map(|turn| TranscriptAppend::native(turn, None))
            .collect::<Vec<_>>();
        self.append_transcript_batch(session_id, &entries).await
    }

    async fn append_turn(
        &self,
        session_id: &SessionId,
        turn: &ConversationTurn,
    ) -> Result<CommitReceipt, StoreError> {
        self.append_turn_batch(session_id, std::slice::from_ref(turn))
            .await
    }
    fn delete_session(&self, session_id: &SessionId) -> Result<(), String>;
    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary>;
    fn search_transcripts(
        &self,
        session_ids: &[String],
        query: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptSearchMatch>, StoreError>;
    fn build_backfill_summaries(&self, limit: usize) -> Vec<SessionHistorySummary>;
    fn has_persisted_sessions(&self) -> bool;
}

/// Helper: run an `IntoFuture` on the current Tokio runtime from a sync context.
/// SAFETY: must be called from within a Tokio runtime (daemon, TUI, or test).
fn block_on<F: IntoFuture>(f: F) -> F::Output {
    tokio::task::block_in_place(move || Handle::current().block_on(f.into_future()))
}

// ---------------------------------------------------------------------------
// File-backed store (original)
// ---------------------------------------------------------------------------

struct FileSessionStore {
    files: Arc<crate::session_storage::SessionFileStore>,
    derivations: Arc<crate::session_storage::SessionFileStore>,
    commit_locks: Arc<SessionCommitLocks>,
    derivation_lock: Arc<tokio::sync::Mutex<()>>,
}

impl FileSessionStore {
    fn new() -> Self {
        Self::at(file_session_root())
    }

    fn at(root: std::path::PathBuf) -> Self {
        Self {
            derivations: Arc::new(crate::session_storage::SessionFileStore::new(
                root.join("derivations"),
                "json",
            )),
            files: Arc::new(crate::session_storage::SessionFileStore::new(root, "jsonl")),
            commit_locks: Arc::new(SessionCommitLocks::default()),
            derivation_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn read_entries(
        files: &crate::session_storage::SessionFileStore,
        session_id: &SessionId,
    ) -> Result<(Vec<TranscriptEntry>, bool), StoreError> {
        let bytes = match files.read(session_id) {
            Ok(bytes) => bytes,
            Err(error) if error.is_not_found() => return Ok((Vec::new(), false)),
            Err(error) => return Err(StoreError::Backend(error.to_string())),
        };
        let mut entries = Vec::new();
        let mut migrated_legacy = false;
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        {
            if let Ok(mut entry) = serde_json::from_slice::<TranscriptEntry>(line) {
                entry.entry_seq = entries.len() as u64 + 1;
                entries.push(entry);
                continue;
            }
            let turn = serde_json::from_slice::<ConversationTurn>(line)
                .map_err(|error| StoreError::Serialization(error.to_string()))?;
            migrated_legacy = true;
            entries.push(materialize_legacy_turn(
                session_id,
                entries.len() as u64 + 1,
                turn,
            )?);
        }
        Ok((entries, migrated_legacy))
    }

    fn encode_entries(entries: &[TranscriptEntry]) -> Result<Vec<u8>, StoreError> {
        let mut bytes = Vec::new();
        for entry in entries {
            serde_json::to_writer(&mut bytes, entry)
                .map_err(|error| StoreError::Serialization(error.to_string()))?;
            bytes.push(b'\n');
        }
        Ok(bytes)
    }
}

#[async_trait]
impl SessionStore for FileSessionStore {
    fn load_transcript_entries(&self, session_id: &SessionId) -> Vec<TranscriptEntry> {
        let Ok((entries, migrated_legacy)) = Self::read_entries(&self.files, session_id) else {
            return Vec::new();
        };
        if migrated_legacy
            && let Ok(bytes) = Self::encode_entries(&entries)
            && let Err(error) = self.files.atomic_write(session_id, &bytes)
        {
            tracing::warn!(%session_id, %error, "legacy transcript coordinate backfill failed");
        }
        entries
    }

    async fn append_transcript_batch(
        &self,
        session_id: &SessionId,
        appends: &[TranscriptAppend],
    ) -> Result<CommitReceipt, StoreError> {
        if appends.is_empty() {
            return Ok(CommitReceipt {
                turns: 0,
                bytes: 0,
                durability: CommitDurability::FilesystemWrite,
            });
        }

        let commit_lock = self.commit_locks.for_session(session_id);
        let _commit = commit_lock.lock().await;
        let files = Arc::clone(&self.files);
        let session_id = session_id.clone();
        let appends = appends.to_vec();
        tokio::task::spawn_blocking(move || {
            let (mut entries, migrated_legacy) = Self::read_entries(&files, &session_id)?;
            let first_seq = entries.len() as u64 + 1;
            let mut appended = Vec::with_capacity(appends.len());
            for (offset, append) in appends.iter().enumerate() {
                appended.push(materialize_append(first_seq + offset as u64, append)?);
            }
            let appended_bytes = Self::encode_entries(&appended)?;
            if migrated_legacy {
                entries.extend(appended);
                let all_bytes = Self::encode_entries(&entries)?;
                files
                    .atomic_write(&session_id, &all_bytes)
                    .map_err(|error| StoreError::Backend(error.to_string()))?;
            } else {
                files
                    .append(&session_id, &appended_bytes)
                    .map_err(|error| StoreError::Backend(error.to_string()))?;
            }
            for append in &appends {
                record_catalog_append(&session_id, &append.turn);
            }
            Ok(CommitReceipt {
                turns: appends.len(),
                bytes: appended_bytes.len(),
                durability: CommitDurability::FilesystemWrite,
            })
        })
        .await
        .map_err(|error| StoreError::Worker(error.to_string()))?
    }

    async fn materialize_derivation(
        &self,
        request: &DerivationCommitRequest,
    ) -> Result<DerivationCommitOutcome, StoreError> {
        let _derivation = self.derivation_lock.lock().await;
        let files = Arc::clone(&self.files);
        let derivations = Arc::clone(&self.derivations);
        let request = request.clone();
        tokio::task::spawn_blocking(move || {
            let target = &request.derivation.target_session.session_id;
            match derivations.read(target) {
                Ok(bytes) => {
                    let record = serde_json::from_slice::<StoredDerivationRecord>(&bytes)
                        .map_err(|error| StoreError::Serialization(error.to_string()))?;
                    return reused_derivation(&record, &request);
                }
                Err(error) if error.is_not_found() => {}
                Err(error) => return Err(StoreError::Backend(error.to_string())),
            }

            let mut seen = std::collections::HashSet::new();
            let entries = request
                .entries
                .iter()
                .enumerate()
                .map(|(offset, append)| {
                    if append.existing_entry_id.is_none() || append.source.is_none() {
                        return Err(StoreError::InvalidInput(
                            "derived entries require immutable ids and source coordinates"
                                .to_string(),
                        ));
                    }
                    let entry = materialize_append(offset as u64 + 1, append)?;
                    if !seen.insert(entry.entry_id.clone()) {
                        return Err(StoreError::InvalidInput(
                            "a derivation cannot bind the same transcript entry twice".to_string(),
                        ));
                    }
                    Ok(entry)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if entries.is_empty() {
                return Err(StoreError::InvalidInput(
                    "a derivation requires at least one committed entry".to_string(),
                ));
            }

            let (existing, _) = Self::read_entries(&files, target)?;
            if existing.is_empty() {
                files
                    .atomic_write(target, &Self::encode_entries(&entries)?)
                    .map_err(|error| StoreError::Backend(error.to_string()))?;
            } else if !derivation_entries_match(&existing, &entries) {
                return Err(StoreError::InvalidInput(
                    "derived target session already contains different history".to_string(),
                ));
            }

            let record = stored_derivation_record(&request)?;
            let bytes = serde_json::to_vec(&record)
                .map_err(|error| StoreError::Serialization(error.to_string()))?;
            derivations
                .atomic_write(target, &bytes)
                .map_err(|error| StoreError::Backend(error.to_string()))?;
            Ok(DerivationCommitOutcome {
                derivation: request.derivation,
                reused: false,
            })
        })
        .await
        .map_err(|error| StoreError::Worker(error.to_string()))?
    }

    fn load_derivation(
        &self,
        target_session_id: &SessionId,
    ) -> Result<Option<DerivationLookup>, StoreError> {
        match self.derivations.read(target_session_id) {
            Ok(bytes) => {
                let record = serde_json::from_slice::<StoredDerivationRecord>(&bytes)
                    .map_err(|error| StoreError::Serialization(error.to_string()))?;
                derivation_lookup(&record).map(Some)
            }
            Err(error) if error.is_not_found() => Ok(None),
            Err(error) => Err(StoreError::Backend(error.to_string())),
        }
    }

    fn delete_session(&self, session_id: &SessionId) -> Result<(), String> {
        self.files
            .remove(session_id)
            .map_err(|error| error.to_string())?;
        self.derivations
            .remove(session_id)
            .map_err(|error| error.to_string())?;
        if self
            .files
            .contains(session_id)
            .map_err(|error| error.to_string())?
        {
            return Err("session transcript remains after deletion".to_string());
        }
        Ok(())
    }

    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary> {
        #[cfg(feature = "full-daemon")]
        {
            crate::session_catalog::list_sessions(limit)
        }
        #[cfg(not(feature = "full-daemon"))]
        {
            self.build_backfill_summaries(limit)
        }
    }

    fn search_transcripts(
        &self,
        session_ids: &[String],
        query: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptSearchMatch>, StoreError> {
        validate_transcript_search(query)?;
        let needle = query.to_ascii_lowercase();
        let mut hits = session_ids
            .iter()
            .flat_map(|session_id| {
                let needle = needle.clone();
                let parsed = SessionId::parse(session_id).ok();
                parsed
                    .map(|id| self.load_history(&id))
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(move |turn| {
                        let text = turn_search_text(&turn)?;
                        text.to_ascii_lowercase()
                            .contains(&needle)
                            .then(|| TranscriptSearchMatch {
                                session_id: session_id.clone(),
                                role: if turn.role == "agent" {
                                    "assistant".to_string()
                                } else {
                                    turn.role
                                },
                                timestamp: turn.timestamp,
                                excerpt: transcript_excerpt(&text, query),
                            })
                    })
            })
            .collect::<Vec<_>>();
        hits.sort_by_key(|hit| std::cmp::Reverse(hit.timestamp));
        hits.truncate(limit);
        Ok(hits)
    }

    fn build_backfill_summaries(&self, limit: usize) -> Vec<SessionHistorySummary> {
        #[cfg(feature = "full-daemon")]
        {
            crate::session::file_build_history_summaries_from_files(&self.files, limit)
        }
        #[cfg(not(feature = "full-daemon"))]
        {
            let _ = limit;
            Vec::new()
        }
    }

    fn has_persisted_sessions(&self) -> bool {
        self.files.list().is_ok_and(|entries| !entries.is_empty())
    }
}

// ---------------------------------------------------------------------------
// SurrealDB-backed store
// ---------------------------------------------------------------------------

pub struct SurrealSessionStore {
    db: Surreal<Any>,
    commit_locks: Arc<SessionCommitLocks>,
    derivation_lock: Arc<tokio::sync::Mutex<()>>,
}

impl SurrealSessionStore {
    pub fn new(db: Surreal<Any>) -> Self {
        Self {
            db,
            commit_locks: Arc::new(SessionCommitLocks::default()),
            derivation_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
        Self::ensure_schema_for_db(&self.db).await
    }

    pub async fn ensure_schema_for_db(db: &Surreal<Any>) -> Result<(), surrealdb::Error> {
        for statement in SESSION_SCHEMA_STATEMENTS {
            if let Err(err) = db.query(*statement).await {
                let text = err.to_string();
                if !(text.contains("already exists")
                    || text.contains("already defined")
                    || text.contains("Overwrite index"))
                {
                    return Err(err);
                }
            }
        }
        for statement in SESSION_SCHEMA_MIGRATIONS {
            db.query(*statement).await?;
        }
        Self::backfill_legacy_transcripts(db).await?;
        Ok(())
    }

    async fn backfill_legacy_transcripts(db: &Surreal<Any>) -> Result<(), surrealdb::Error> {
        #[derive(Debug, Deserialize, SurrealValue)]
        struct LegacySession {
            session_id: String,
        }
        #[derive(Debug, Deserialize, SurrealValue)]
        struct ExistingBinding {
            #[allow(dead_code)]
            entry_id: String,
        }

        let mut response = db
            .query("SELECT session_id FROM session_turn GROUP BY session_id")
            .await?;
        let sessions = response.take::<Vec<LegacySession>>(0)?;
        for legacy in sessions {
            let session_id = match SessionId::parse(&legacy.session_id) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let mut existing = db
                .query("SELECT entry_id FROM session_entry WHERE session_id = $session_id LIMIT 1")
                .bind(("session_id", session_id.to_string()))
                .await?;
            if !existing
                .take::<Vec<ExistingBinding>>(0)
                .unwrap_or_default()
                .is_empty()
            {
                continue;
            }
            let mut legacy_rows = db
                .query(
                    "SELECT id, session_id, role, content, timestamp, tool_names, answer_state, parts, \
                     slice_summary, speaker_profile_id, search_text \
                     FROM session_turn WHERE session_id = $session_id \
                     ORDER BY timestamp ASC, id ASC",
                )
                .bind(("session_id", session_id.to_string()))
                .await?;
            let rows = legacy_rows.take::<Vec<SessionTurnRecord>>(0)?;
            if rows.is_empty() {
                continue;
            }
            let entries = rows
                .into_iter()
                .enumerate()
                .map(|(index, row)| {
                    materialize_legacy_turn(&session_id, index as u64 + 1, row.into())
                        .expect("legacy transcript rows must serialize")
                })
                .collect::<Vec<_>>();
            let entry_records = entries.iter().map(entry_record).collect::<Vec<_>>();
            let binding_records = entries
                .iter()
                .map(|entry| binding_record(&session_id, entry))
                .collect::<Vec<_>>();
            db.query(
                "BEGIN TRANSACTION; \
                 INSERT INTO transcript_entry $entries; \
                 INSERT INTO session_entry $bindings; \
                 COMMIT TRANSACTION;",
            )
            .bind(("entries", entry_records))
            .bind(("bindings", binding_records))
            .await?
            .check()?;
        }
        Ok(())
    }
}

#[async_trait]
impl SessionStore for SurrealSessionStore {
    fn load_transcript_entries(&self, session_id: &SessionId) -> Vec<TranscriptEntry> {
        let binding_sql = "SELECT session_id, entry_seq, entry_id, source_authority_id, \
                           source_session_id, source_entry_id, source_entry_seq, committed_at, \
                           role, search_text, timestamp \
                           FROM type::table($table) \
                           WHERE session_id = $session_id \
                           ORDER BY entry_seq ASC";
        let session_id_owned = session_id.to_string();
        let mut response = match block_on(
            self.db
                .query(binding_sql)
                .bind(("table", SESSION_ENTRY_TABLE))
                .bind(("session_id", session_id_owned.clone())),
        ) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("SurrealSessionStore::load_transcript_entries query error: {err}");
                return Vec::new();
            }
        };
        let bindings = match response.take::<Vec<SessionEntryRecord>>(0) {
            Ok(records) => records,
            Err(err) => {
                eprintln!("SurrealSessionStore::load_transcript_entries deserialize error: {err}");
                return Vec::new();
            }
        };
        if bindings.is_empty() {
            return Vec::new();
        }
        let entry_ids = bindings
            .iter()
            .map(|binding| binding.entry_id.clone())
            .collect::<Vec<_>>();
        let entry_sql = "SELECT entry_id, role, content, timestamp, tool_names, answer_state, \
                         parts, slice_summary, speaker_profile_id, execution_authority_id, \
                         execution_session_id, execution_id, content_digest \
                         FROM type::table($table) WHERE entry_id IN $entry_ids";
        let mut entry_response = match block_on(
            self.db
                .query(entry_sql)
                .bind(("table", TRANSCRIPT_ENTRY_TABLE))
                .bind(("entry_ids", entry_ids)),
        ) {
            Ok(response) => response,
            Err(error) => {
                eprintln!("SurrealSessionStore::load_transcript_entries payload error: {error}");
                return Vec::new();
            }
        };
        let records = match entry_response.take::<Vec<TranscriptEntryRecord>>(0) {
            Ok(records) => records,
            Err(error) => {
                eprintln!("SurrealSessionStore::load_transcript_entries payload decode: {error}");
                return Vec::new();
            }
        };
        let mut by_id = records
            .into_iter()
            .map(|record| (record.entry_id.clone(), record))
            .collect::<HashMap<_, _>>();
        bindings
            .into_iter()
            .filter_map(|binding| {
                let record = by_id.remove(&binding.entry_id)?;
                transcript_entry_from_records(record, binding)
            })
            .collect()
    }

    async fn append_transcript_batch(
        &self,
        session_id: &SessionId,
        appends: &[TranscriptAppend],
    ) -> Result<CommitReceipt, StoreError> {
        if appends.is_empty() {
            return Ok(CommitReceipt {
                turns: 0,
                bytes: 0,
                durability: CommitDurability::DatabaseCommit,
            });
        }

        let commit_lock = self.commit_locks.for_session(session_id);
        let _commit = commit_lock.lock().await;
        #[derive(Debug, Deserialize, SurrealValue)]
        struct HeadSequence {
            entry_seq: i64,
        }
        let mut head_response = self
            .db
            .query("SELECT entry_seq FROM session_entry WHERE session_id = $session_id ORDER BY entry_seq DESC LIMIT 1")
            .bind(("session_id", session_id.to_string()))
            .await
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        let head = head_response
            .take::<Vec<HeadSequence>>(0)
            .map_err(|error| StoreError::Serialization(error.to_string()))?
            .into_iter()
            .next()
            .map(|row| row.entry_seq)
            .unwrap_or(0);
        let head = u64::try_from(head).map_err(|_| {
            StoreError::Serialization("negative transcript entry sequence in storage".to_string())
        })?;
        let entries = appends
            .iter()
            .enumerate()
            .map(|(offset, append)| materialize_append(head + offset as u64 + 1, append))
            .collect::<Result<Vec<_>, _>>()?;
        let entry_records = entries.iter().map(entry_record).collect::<Vec<_>>();
        let binding_records = entries
            .iter()
            .map(|entry| binding_record(session_id, entry))
            .collect::<Vec<_>>();
        let bytes = entry_records
            .iter()
            .map(|record| serde_json::to_vec(record).map(|value| value.len()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| StoreError::Serialization(error.to_string()))?
            .into_iter()
            .sum();

        let response = self
            .db
            .query(
                "BEGIN TRANSACTION; \
                 INSERT INTO transcript_entry $entries; \
                 INSERT INTO session_entry $bindings; \
                 COMMIT TRANSACTION;",
            )
            .bind(("entries", entry_records))
            .bind(("bindings", binding_records))
            .await
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        response
            .check()
            .map_err(|error| StoreError::Backend(error.to_string()))?;

        for append in appends {
            record_catalog_append(session_id, &append.turn);
        }
        Ok(CommitReceipt {
            turns: appends.len(),
            bytes,
            durability: CommitDurability::DatabaseCommit,
        })
    }

    async fn materialize_derivation(
        &self,
        request: &DerivationCommitRequest,
    ) -> Result<DerivationCommitOutcome, StoreError> {
        let _derivation = self.derivation_lock.lock().await;
        let target = &request.derivation.target_session.session_id;
        let mut existing_response = self
            .db
            .query(
                "SELECT derivation_id, target_session_id, manifest_id, \
                 idempotency_key_digest, request_digest, derivation_json, created_by, created_at \
                 FROM session_derivation WHERE target_session_id = $target_session_id LIMIT 1",
            )
            .bind(("target_session_id", target.to_string()))
            .await
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        if let Some(record) = existing_response
            .take::<Vec<StoredDerivationRecord>>(0)
            .map_err(|error| StoreError::Serialization(error.to_string()))?
            .into_iter()
            .next()
        {
            return reused_derivation(&record, request);
        }

        let mut seen = std::collections::HashSet::new();
        let entries = request
            .entries
            .iter()
            .enumerate()
            .map(|(offset, append)| {
                if append.existing_entry_id.is_none() || append.source.is_none() {
                    return Err(StoreError::InvalidInput(
                        "derived entries require immutable ids and source coordinates".to_string(),
                    ));
                }
                let entry = materialize_append(offset as u64 + 1, append)?;
                if !seen.insert(entry.entry_id.clone()) {
                    return Err(StoreError::InvalidInput(
                        "a derivation cannot bind the same transcript entry twice".to_string(),
                    ));
                }
                Ok(entry)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if entries.is_empty() {
            return Err(StoreError::InvalidInput(
                "a derivation requires at least one committed entry".to_string(),
            ));
        }

        #[derive(Debug, Deserialize, SurrealValue)]
        struct EntryIdRow {
            entry_id: String,
        }
        let entry_ids = entries
            .iter()
            .map(|entry| entry.entry_id.to_string())
            .collect::<Vec<_>>();
        let mut payload_response = self
            .db
            .query("SELECT entry_id FROM transcript_entry WHERE entry_id IN $entry_ids")
            .bind(("entry_ids", entry_ids.clone()))
            .await
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        let available = payload_response
            .take::<Vec<EntryIdRow>>(0)
            .map_err(|error| StoreError::Serialization(error.to_string()))?
            .into_iter()
            .map(|row| row.entry_id)
            .collect::<std::collections::HashSet<_>>();
        if entry_ids
            .iter()
            .any(|entry_id| !available.contains(entry_id))
        {
            return Err(StoreError::InvalidInput(
                "a selected transcript payload is no longer available".to_string(),
            ));
        }

        let record = stored_derivation_record(request)?;
        let manifest = &request.derivation.manifest;
        let manifest_record = StoredContextManifestRecord {
            manifest_id: manifest.manifest_id.to_string(),
            manifest_json: serde_json::to_string(manifest)
                .map_err(|error| StoreError::Serialization(error.to_string()))?,
            created_by: manifest.created_by.clone(),
            created_at: manifest.created_at,
        };
        let bindings = entries
            .iter()
            .map(|entry| binding_record(target, entry))
            .collect::<Vec<_>>();
        let response = self
            .db
            .query(
                "BEGIN TRANSACTION; \
                 INSERT INTO context_manifest $manifests; \
                 INSERT INTO session_derivation $derivations; \
                 INSERT INTO session_entry $bindings; \
                 COMMIT TRANSACTION;",
            )
            .bind(("manifests", vec![manifest_record]))
            .bind(("derivations", vec![record]))
            .bind(("bindings", bindings))
            .await
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        response
            .check()
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        Ok(DerivationCommitOutcome {
            derivation: request.derivation.clone(),
            reused: false,
        })
    }

    fn load_derivation(
        &self,
        target_session_id: &SessionId,
    ) -> Result<Option<DerivationLookup>, StoreError> {
        let mut response = block_on(
            self.db
                .query(
                    "SELECT derivation_id, target_session_id, manifest_id, \
                     idempotency_key_digest, request_digest, derivation_json, created_by, created_at \
                     FROM session_derivation WHERE target_session_id = $target_session_id LIMIT 1",
                )
                .bind(("target_session_id", target_session_id.to_string())),
        )
        .map_err(|error| StoreError::Backend(error.to_string()))?;
        response
            .take::<Vec<StoredDerivationRecord>>(0)
            .map_err(|error| StoreError::Serialization(error.to_string()))?
            .into_iter()
            .next()
            .as_ref()
            .map(derivation_lookup)
            .transpose()
    }

    fn delete_session(&self, session_id: &SessionId) -> Result<(), String> {
        #[derive(Debug, Deserialize, SurrealValue)]
        struct EntryIdRow {
            entry_id: String,
        }
        #[derive(Debug, Deserialize, SurrealValue)]
        struct ManifestIdRow {
            manifest_id: String,
        }
        let mut inventory = block_on(
            self.db
                .query("SELECT entry_id FROM session_entry WHERE session_id = $session_id")
                .bind(("session_id", session_id.to_string())),
        )
        .map_err(|error| error.to_string())?;
        let entry_ids = inventory
            .take::<Vec<EntryIdRow>>(0)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|row| row.entry_id)
            .collect::<Vec<_>>();
        let mut derivation_inventory = block_on(
            self.db
                .query(
                    "SELECT manifest_id FROM session_derivation WHERE target_session_id = $session_id",
                )
                .bind(("session_id", session_id.to_string())),
        )
        .map_err(|error| error.to_string())?;
        let manifest_ids = derivation_inventory
            .take::<Vec<ManifestIdRow>>(0)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|row| row.manifest_id)
            .collect::<Vec<_>>();
        let sql = "BEGIN TRANSACTION; \
                   DELETE session_entry WHERE session_id = $session_id; \
                   DELETE session_turn WHERE session_id = $session_id; \
                   DELETE session_derivation WHERE target_session_id = $session_id; \
                   DELETE context_manifest WHERE manifest_id IN $manifest_ids; \
                   COMMIT TRANSACTION;";
        let response = block_on(
            self.db
                .query(sql)
                .bind(("session_id", session_id.to_string()))
                .bind(("manifest_ids", manifest_ids)),
        )
        .map_err(|error| error.to_string())?;
        response.check().map_err(|error| error.to_string())?;
        for entry_id in entry_ids {
            let mut references = block_on(
                self.db
                    .query("SELECT entry_id FROM session_entry WHERE entry_id = $entry_id LIMIT 1")
                    .bind(("entry_id", entry_id.clone())),
            )
            .map_err(|error| error.to_string())?;
            if references
                .take::<Vec<EntryIdRow>>(0)
                .map_err(|error| error.to_string())?
                .is_empty()
            {
                block_on(
                    self.db
                        .query("DELETE transcript_entry WHERE entry_id = $entry_id")
                        .bind(("entry_id", entry_id)),
                )
                .map_err(|error| error.to_string())?
                .check()
                .map_err(|error| error.to_string())?;
            }
        }
        let mut verify = block_on(
            self.db
                .query("SELECT entry_id FROM session_entry WHERE session_id = $session_id LIMIT 1")
                .bind(("session_id", session_id.to_string())),
        )
        .map_err(|error| error.to_string())?;
        if !verify
            .take::<Vec<EntryIdRow>>(0)
            .map_err(|error| error.to_string())?
            .is_empty()
        {
            return Err("session transcript rows remain after deletion".to_string());
        }
        Ok(())
    }

    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary> {
        #[cfg(feature = "full-daemon")]
        {
            crate::session_catalog::list_sessions(limit)
        }
        #[cfg(not(feature = "full-daemon"))]
        {
            self.build_backfill_summaries(limit)
        }
    }

    fn search_transcripts(
        &self,
        session_ids: &[String],
        query: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptSearchMatch>, StoreError> {
        validate_transcript_search(query)?;
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }

        #[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
        struct SearchRow {
            session_id: String,
            role: String,
            search_text: Option<String>,
            timestamp: chrono::DateTime<chrono::Utc>,
        }

        let sql = "SELECT session_id, role, search_text, timestamp \
                   FROM type::table($table) \
                   WHERE session_id IN $session_ids \
                     AND role IN ['user', 'assistant', 'agent'] \
                     AND search_text @@ $query \
                   ORDER BY timestamp DESC \
                   LIMIT $limit";
        let mut response = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_ENTRY_TABLE))
                .bind(("session_ids", session_ids.to_vec()))
                .bind(("query", query.to_string()))
                .bind(("limit", limit as i64)),
        )
        .map_err(|error| StoreError::Backend(error.to_string()))?;
        let rows = response
            .take::<Vec<SearchRow>>(0)
            .map_err(|error| StoreError::Serialization(error.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let text = row.search_text?;
                Some(TranscriptSearchMatch {
                    session_id: row.session_id,
                    role: if row.role == "agent" {
                        "assistant".to_string()
                    } else {
                        row.role
                    },
                    timestamp: row.timestamp,
                    excerpt: transcript_excerpt(&text, query),
                })
            })
            .collect())
    }

    fn build_backfill_summaries(&self, limit: usize) -> Vec<SessionHistorySummary> {
        let sql = "SELECT session_id, \
                           count() AS turns, \
                           type::datetime(time::max(timestamp)) AS last_timestamp \
                    FROM type::table($table) \
                    GROUP BY session_id \
                    ORDER BY last_timestamp DESC \
                    LIMIT $limit";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_ENTRY_TABLE))
                .bind(("limit", limit.max(1) as i64)),
        ) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("SurrealSessionStore::build_backfill_summaries query error: {err}");
                return Vec::new();
            }
        };

        #[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
        struct SessionAggregate {
            session_id: String,
            turns: usize,
            last_timestamp: Option<chrono::DateTime<chrono::Utc>>,
        }

        let aggregates: Vec<SessionAggregate> = match response.take(0) {
            Ok(rows) => rows,
            Err(err) => {
                eprintln!("SurrealSessionStore::build_backfill_summaries deserialize error: {err}");
                return Vec::new();
            }
        };

        aggregates
            .into_iter()
            .map(|agg| {
                let preview = self
                    .preview_for_session(&agg.session_id)
                    .unwrap_or_else(|| "(empty session)".to_string());
                SessionHistorySummary {
                    session_id: agg.session_id,
                    display_name: None,
                    turns: agg.turns,
                    verification_runs: 0,
                    last_timestamp: agg.last_timestamp,
                    last_verification_timestamp: None,
                    last_verification_confidence: None,
                    last_verification_coverage: None,
                    last_verification_verified: None,
                    preview,
                    catalog: None,
                    origin_surface: None,
                    has_code_work: false,
                }
            })
            .collect()
    }

    fn has_persisted_sessions(&self) -> bool {
        let sql = "SELECT count() AS total FROM type::table($table) GROUP ALL";
        let mut response = match block_on(self.db.query(sql).bind(("table", SESSION_ENTRY_TABLE))) {
            Ok(response) => response,
            Err(_) => return false,
        };

        #[derive(Debug, Deserialize, SurrealValue)]
        struct CountRow {
            total: usize,
        }

        response
            .take::<Vec<CountRow>>(0)
            .ok()
            .and_then(|rows| rows.into_iter().next())
            .is_some_and(|row| row.total > 0)
    }
}

impl SurrealSessionStore {
    fn preview_for_session(&self, session_id: &str) -> Option<String> {
        let session_id = SessionId::parse(session_id).ok()?;
        for entry in self
            .load_transcript_entries(&session_id)
            .into_iter()
            .rev()
            .take(8)
        {
            if let Some(preview) = preview_from_turn(&entry.turn) {
                return Some(preview);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Global singleton & public helpers
// ---------------------------------------------------------------------------

static SESSION_STORE: Lazy<RwLock<Arc<dyn SessionStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(FileSessionStore::new())));

pub fn set_session_store(store: Arc<dyn SessionStore>) {
    let mut guard = SESSION_STORE.write().unwrap();
    *guard = store;
}

pub fn get_session_store() -> Arc<dyn SessionStore> {
    SESSION_STORE.read().unwrap().clone()
}

/// Release a test deployment's database-backed global handle so a same-process
/// reboot can model the handle teardown that happens at process exit.
#[cfg(all(
    test,
    feature = "embedded-daemon",
    not(feature = "full-daemon")
))]
pub(crate) fn reset_session_store_for_test() {
    set_session_store(Arc::new(FileSessionStore::new()));
}

pub fn build_backfill_summaries(limit: usize) -> Vec<SessionHistorySummary> {
    get_session_store().build_backfill_summaries(limit)
}

pub fn has_persisted_sessions() -> bool {
    get_session_store().has_persisted_sessions()
}

pub fn delete_session_transcript(session_id: &SessionId) -> Result<(), String> {
    get_session_store().delete_session(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medousa_types::session::{
        AuthorityId, ContextManifest, ContextManifestId, ConversationRangeSelection, DerivationId,
        ExecutionId, ResolvedConversationRange, SessionDerivation, SessionRef,
    };

    fn turn(content: &str) -> ConversationTurn {
        ConversationTurn {
            role: "assistant".to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_names: Vec::new(),
            answer_state: None,
            parts: None,
            slice_summary: None,
            speaker_profile_id: None,
        }
    }

    fn test_authority(hex: char) -> AuthorityId {
        AuthorityId::parse(format!("auth_{}", hex.to_string().repeat(64))).unwrap()
    }

    fn derivation_request(
        authority: &AuthorityId,
        source_session_id: &SessionId,
        target_session_id: &SessionId,
        source_entry: &TranscriptEntry,
    ) -> DerivationCommitRequest {
        let source_session = SessionRef {
            authority_id: authority.clone(),
            session_id: source_session_id.clone(),
        };
        let source = TranscriptEntryRef {
            session: source_session.clone(),
            entry_id: source_entry.entry_id.clone(),
            entry_seq: source_entry.entry_seq,
        };
        let created_at = Utc::now();
        let manifest = ContextManifest {
            manifest_id: ContextManifestId::parse(format!("ctx_{}", "c".repeat(32))).unwrap(),
            sources: vec![ResolvedConversationRange {
                selection: ConversationRangeSelection {
                    session: source_session,
                    after_entry_seq: None,
                    through_entry_seq: source_entry.entry_seq,
                },
                selection_digest: "sha256:selection".to_string(),
            }],
            created_by: "profile:user:test".to_string(),
            created_at,
        };
        DerivationCommitRequest {
            derivation: SessionDerivation {
                derivation_id: DerivationId::parse(format!("drv_{}", "d".repeat(32))).unwrap(),
                target_session: SessionRef {
                    authority_id: authority.clone(),
                    session_id: target_session_id.clone(),
                },
                manifest,
                intent: "fork".to_string(),
                caused_by: None,
                created_by: "profile:user:test".to_string(),
                created_at,
            },
            idempotency_key_digest: "sha256:idempotency".to_string(),
            request_digest: "sha256:request".to_string(),
            entries: vec![TranscriptAppend {
                turn: source_entry.turn.clone(),
                caused_by: source_entry.caused_by.clone(),
                existing_entry_id: Some(source_entry.entry_id.clone()),
                source: Some(source),
                expected_digest: Some(source_entry.content_digest.clone()),
            }],
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn file_receipt_is_visible_to_a_fresh_store_instance() {
        let root = std::env::temp_dir().join(format!(
            "medousa-session-receipt-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let session_id = SessionId::parse("receipt-reload").unwrap();
        let store = FileSessionStore::at(root.clone());
        let receipt = store
            .append_turn_batch(&session_id, &[turn("one"), turn("two")])
            .await
            .unwrap();
        assert_eq!(receipt.turns, 2);
        assert!(receipt.bytes > 0);

        let reopened = FileSessionStore::at(root.clone());
        let entries = reopened.load_transcript_entries(&session_id);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].entry_seq, 1);
        assert_eq!(entries[1].entry_seq, 2);
        assert_ne!(entries[0].entry_id, entries[1].entry_id);
        assert_eq!(entries[1].turn.content, "two");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn file_entries_preserve_execution_causation_after_restart() {
        let root = std::env::temp_dir().join(format!(
            "medousa-session-causation-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let session_id = SessionId::parse("causation-reload").unwrap();
        let caused_by = ExecutionRef {
            authority_id: AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap(),
            session_id: session_id.clone(),
            execution_id: ExecutionId::parse("turn-42").unwrap(),
        };
        let store = FileSessionStore::at(root.clone());
        store
            .append_transcript_batch(
                &session_id,
                &[TranscriptAppend::native(
                    turn("caused output"),
                    Some(caused_by.clone()),
                )],
            )
            .await
            .unwrap();

        let reopened = FileSessionStore::at(root.clone());
        let entries = reopened.load_transcript_entries(&session_id);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].caused_by, Some(caused_by));
        assert!(entries[0].content_digest.starts_with("sha256:"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn file_legacy_rows_receive_restart_stable_coordinates() {
        let root = std::env::temp_dir().join(format!(
            "medousa-session-legacy-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let session_id = SessionId::parse("legacy-reload").unwrap();
        let store = FileSessionStore::at(root.clone());
        let mut bytes = serde_json::to_vec(&turn("legacy one")).unwrap();
        bytes.push(b'\n');
        bytes.extend(serde_json::to_vec(&turn("legacy two")).unwrap());
        bytes.push(b'\n');
        store.files.atomic_write(&session_id, &bytes).unwrap();

        let first = store.load_transcript_entries(&session_id);
        let reopened = FileSessionStore::at(root.clone());
        let second = reopened.load_transcript_entries(&session_id);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].entry_seq, 1);
        assert_eq!(first[1].entry_seq, 2);
        assert_eq!(first[0].entry_id, second[0].entry_id);
        assert_eq!(first[1].entry_id, second[1].entry_id);

        let stored = store.files.read(&session_id).unwrap();
        let first_line = stored.split(|byte| *byte == b'\n').next().unwrap();
        let migrated: TranscriptEntry = serde_json::from_slice(first_line).unwrap();
        assert_eq!(migrated.entry_id, first[0].entry_id);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn file_concurrent_batches_allocate_contiguous_sequences() {
        let root = std::env::temp_dir().join(format!(
            "medousa-session-concurrent-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let session_id = SessionId::parse("concurrent-sequences").unwrap();
        let store = Arc::new(FileSessionStore::at(root.clone()));
        let mut tasks = Vec::new();
        for index in 0..12 {
            let store = Arc::clone(&store);
            let session_id = session_id.clone();
            tasks.push(tokio::spawn(async move {
                store
                    .append_turn(&session_id, &turn(&format!("turn {index}")))
                    .await
                    .unwrap();
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }

        let entries = store.load_transcript_entries(&session_id);
        assert_eq!(entries.len(), 12);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.entry_seq)
                .collect::<Vec<_>>(),
            (1..=12).collect::<Vec<_>>()
        );
        let ids = entries
            .iter()
            .map(|entry| entry.entry_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), 12);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn file_derivation_is_idempotent_and_survives_source_deletion() {
        let root = std::env::temp_dir().join(format!(
            "medousa-session-derive-file-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let store = FileSessionStore::at(root.clone());
        let source_id = SessionId::parse("derive-file-source").unwrap();
        let target_id = SessionId::parse("derive-file-target").unwrap();
        store
            .append_turn(&source_id, &turn("selected source"))
            .await
            .unwrap();
        let source = store.load_transcript_entries(&source_id).remove(0);
        let request = derivation_request(&test_authority('a'), &source_id, &target_id, &source);

        let bad_target_id = SessionId::parse("derive-file-digest-conflict").unwrap();
        let mut digest_conflict = request.clone();
        digest_conflict.derivation.target_session.session_id = bad_target_id.clone();
        digest_conflict.entries[0].expected_digest = Some("sha256:not-the-payload".to_string());
        assert!(matches!(
            store.materialize_derivation(&digest_conflict).await,
            Err(StoreError::InvalidInput(_))
        ));
        assert!(store.load_transcript_entries(&bad_target_id).is_empty());

        let first = store.materialize_derivation(&request).await.unwrap();
        assert!(!first.reused);
        let target = store.load_transcript_entries(&target_id);
        assert_eq!(target.len(), 1);
        assert_eq!(target[0].entry_id, source.entry_id);
        assert_eq!(target[0].source.as_ref().unwrap().entry_seq, 1);

        let replay = store.materialize_derivation(&request).await.unwrap();
        assert!(replay.reused);
        let mut conflict = request.clone();
        conflict.request_digest = "sha256:different".to_string();
        assert!(matches!(
            store.materialize_derivation(&conflict).await,
            Err(StoreError::InvalidInput(_))
        ));

        store.delete_session(&source_id).unwrap();
        assert_eq!(store.load_transcript_entries(&target_id).len(), 1);
        assert!(store.load_derivation(&target_id).unwrap().is_some());
        store.delete_session(&target_id).unwrap();
        assert!(store.load_derivation(&target_id).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn file_backend_failure_never_produces_a_receipt() {
        let parent = std::env::temp_dir().join(format!(
            "medousa-session-failure-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&parent, b"not a directory").unwrap();
        let store = FileSessionStore::at(parent.join("history"));
        let session_id = SessionId::parse("receipt-failure").unwrap();
        let error = store
            .append_turn(&session_id, &turn("must fail"))
            .await
            .unwrap_err();
        assert!(matches!(error, StoreError::Backend(_)));
        std::fs::remove_file(parent).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn file_search_returns_visible_matching_turns_newest_first() {
        let root = std::env::temp_dir().join(format!(
            "medousa-session-search-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let first_id = SessionId::parse("search-first").unwrap();
        let second_id = SessionId::parse("search-second").unwrap();
        let store = FileSessionStore::at(root.clone());
        let mut older = turn("The phoenix project ships tomorrow");
        older.timestamp = Utc::now() - chrono::Duration::minutes(1);
        let newer = turn("A newer Phoenix project update");
        store.append_turn(&first_id, &older).await.unwrap();
        store.append_turn(&second_id, &newer).await.unwrap();

        let hits = store
            .search_transcripts(
                &[first_id.to_string(), second_id.to_string()],
                "phoenix project",
                10,
            )
            .unwrap();

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].session_id, second_id.as_str());
        assert_eq!(hits[0].role, "assistant");
        assert!(hits[0].excerpt.contains("Phoenix project"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transcript_search_rejects_unbounded_queries() {
        let query = "x".repeat(MAX_TRANSCRIPT_SEARCH_QUERY_CHARS + 1);
        assert!(matches!(
            validate_transcript_search(&query),
            Err(StoreError::InvalidInput(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn surreal_search_uses_full_text_index_and_session_scope() {
        let db = surrealdb::engine::any::connect("mem://").await.unwrap();
        db.use_ns("session-search-tests")
            .use_db(uuid::Uuid::new_v4().simple().to_string())
            .await
            .unwrap();
        SurrealSessionStore::ensure_schema_for_db(&db)
            .await
            .unwrap();
        SurrealSessionStore::ensure_schema_for_db(&db)
            .await
            .unwrap();
        let store = SurrealSessionStore::new(db);
        let visible_id = SessionId::parse("visible-search").unwrap();
        let hidden_id = SessionId::parse("hidden-search").unwrap();
        store
            .append_turn(&visible_id, &turn("Phoenix transcript sentinel"))
            .await
            .unwrap();
        store
            .append_turn(&hidden_id, &turn("Phoenix hidden sentinel"))
            .await
            .unwrap();

        let hits = store
            .search_transcripts(&[visible_id.to_string()], "phoenix", 10)
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, visible_id.as_str());
        assert!(hits[0].excerpt.contains("Phoenix transcript sentinel"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn surreal_entries_round_trip_coordinates_and_causation() {
        let db = surrealdb::engine::any::connect("mem://").await.unwrap();
        db.use_ns("session-coordinate-tests")
            .use_db(uuid::Uuid::new_v4().simple().to_string())
            .await
            .unwrap();
        SurrealSessionStore::ensure_schema_for_db(&db)
            .await
            .unwrap();
        let store = SurrealSessionStore::new(db);
        let session_id = SessionId::parse("surreal-coordinates").unwrap();
        let caused_by = ExecutionRef {
            authority_id: AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap(),
            session_id: session_id.clone(),
            execution_id: ExecutionId::parse("surreal-turn-1").unwrap(),
        };
        store
            .append_transcript_batch(
                &session_id,
                &[
                    TranscriptAppend::native(turn("one"), Some(caused_by.clone())),
                    TranscriptAppend::native(turn("two"), None),
                ],
            )
            .await
            .unwrap();

        let entries = store.load_transcript_entries(&session_id);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].entry_seq, 1);
        assert_eq!(entries[1].entry_seq, 2);
        assert_eq!(entries[0].caused_by, Some(caused_by));
        assert_eq!(entries[1].turn.content, "two");
        assert_ne!(entries[0].entry_id, entries[1].entry_id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn surreal_legacy_rows_backfill_once_with_stable_coordinates() {
        let db = surrealdb::engine::any::connect("mem://").await.unwrap();
        db.use_ns("session-legacy-tests")
            .use_db(uuid::Uuid::new_v4().simple().to_string())
            .await
            .unwrap();
        SurrealSessionStore::ensure_schema_for_db(&db)
            .await
            .unwrap();
        let session_id = SessionId::parse("surreal-legacy").unwrap();
        let mut legacy = SessionTurnRecord::from(&turn("legacy surreal turn"));
        legacy.session_id = session_id.to_string();
        db.query("INSERT INTO session_turn $turn")
            .bind(("turn", legacy))
            .await
            .unwrap()
            .check()
            .unwrap();

        SurrealSessionStore::ensure_schema_for_db(&db)
            .await
            .unwrap();
        let store = SurrealSessionStore::new(db.clone());
        let first = store.load_transcript_entries(&session_id);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].entry_seq, 1);
        assert_eq!(first[0].turn.content, "legacy surreal turn");

        SurrealSessionStore::ensure_schema_for_db(&db)
            .await
            .unwrap();
        let second = store.load_transcript_entries(&session_id);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].entry_id, first[0].entry_id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn surreal_derivation_reuses_payload_and_retains_target_after_source_delete() {
        let db = surrealdb::engine::any::connect("mem://").await.unwrap();
        db.use_ns("session-derivation-tests")
            .use_db(uuid::Uuid::new_v4().simple().to_string())
            .await
            .unwrap();
        SurrealSessionStore::ensure_schema_for_db(&db)
            .await
            .unwrap();
        let store = SurrealSessionStore::new(db);
        let source_id = SessionId::parse("derive-surreal-source").unwrap();
        let target_id = SessionId::parse("derive-surreal-target").unwrap();
        store
            .append_turn(&source_id, &turn("shared immutable payload"))
            .await
            .unwrap();
        let source = store.load_transcript_entries(&source_id).remove(0);
        let request = derivation_request(&test_authority('b'), &source_id, &target_id, &source);

        assert!(!store.materialize_derivation(&request).await.unwrap().reused);
        assert!(store.materialize_derivation(&request).await.unwrap().reused);
        let target = store.load_transcript_entries(&target_id);
        assert_eq!(target.len(), 1);
        assert_eq!(target[0].entry_id, source.entry_id);
        assert_eq!(
            target[0].source.as_ref().unwrap().session.session_id,
            source_id
        );

        store.delete_session(&source_id).unwrap();
        let retained = store.load_transcript_entries(&target_id);
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].turn.content, "shared immutable payload");
        store.delete_session(&target_id).unwrap();
        assert!(store.load_derivation(&target_id).unwrap().is_none());
    }
}
