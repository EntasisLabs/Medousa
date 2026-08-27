//! Bounded daemon-to-daemon task context around Stasis agent envelopes.
//!
//! Stasis owns job, turn, correlation, and causation identity. Medousa owns
//! conversation authority and provenance. This module only binds those
//! existing contracts for authenticated transport; it is not another task
//! registry or scheduler.

use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use medousa_types::session::{
    AuthorityId, ContextManifest, ContextManifestId, ConversationRangeSelection, ConversationTurn,
    DerivationId, ExecutionId, ExecutionRef, ResolvedConversationRange, SessionDerivation,
    SessionId, SessionRef, TranscriptEntryRef,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use stasis::domain::agent::envelope::{
    AGENT_ENVELOPE_SCHEMA_VERSION_V1, AgentEnvelope, AgentEnvelopeKind,
};

use crate::session_store::{
    DerivationCommitOutcome, DerivationCommitRequest, SessionStore, StoreError, TranscriptAppend,
    transcript_content_digest,
};

pub const DELEGATED_TASK_SCHEMA_VERSION: u32 = 1;
pub const MAX_DELEGATED_CONTEXT_ENTRIES: usize = 128;
pub const MAX_DELEGATED_CONTEXT_BYTES: usize = 512 * 1024;
pub const MAX_DELEGATED_PROMPT_CHARS: usize = 64 * 1024;
pub const MAX_DELEGATED_CONTEXT_PROMPT_CHARS: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegatedTaskErrorKind {
    Invalid,
    Conflict,
    Transport,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatedTaskError {
    pub kind: DelegatedTaskErrorKind,
    pub message: String,
}

impl DelegatedTaskError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: DelegatedTaskErrorKind::Invalid,
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            kind: DelegatedTaskErrorKind::Conflict,
            message: message.into(),
        }
    }

    pub fn transport(message: impl Into<String>) -> Self {
        Self {
            kind: DelegatedTaskErrorKind::Transport,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: DelegatedTaskErrorKind::Internal,
            message: message.into(),
        }
    }
}

impl fmt::Display for DelegatedTaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DelegatedTaskError {}

impl From<StoreError> for DelegatedTaskError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::InvalidInput(message) => Self::conflict(message),
            StoreError::Serialization(message)
            | StoreError::Backend(message)
            | StoreError::Worker(message) => Self::internal(message),
        }
    }
}

/// One immutable payload plus its authoritative source coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedContextEntry {
    pub source: TranscriptEntryRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caused_by: Option<ExecutionRef>,
    pub content_digest: String,
    pub turn: ConversationTurn,
}

/// A bounded, digest-checked slice of committed conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedContextGrant {
    pub manifest: ContextManifest,
    pub entries: Vec<DelegatedContextEntry>,
}

/// Authenticated mesh payload for one Stasis `TurnGranted` event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedTaskRequest {
    pub schema_version: u32,
    pub grant: AgentEnvelope,
    pub source_execution: ExecutionRef,
    pub context: DelegatedContextGrant,
}

/// Result returned by the worker daemon and signed by its mesh identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedTaskResult {
    pub schema_version: u32,
    pub terminal: AgentEnvelope,
    pub execution: ExecutionRef,
    pub derivation: SessionDerivation,
}

/// Host integration port. Implementations supply the existing pairing bearer,
/// signed mesh envelope, pinned peer verification, and LAN/Iroh routing. They
/// must bind the returned terminal participant to that authenticated peer and
/// never own task identity or context selection.
#[async_trait]
pub trait DelegatedTaskTransport: Send + Sync {
    async fn dispatch(
        &self,
        target: &crate::delegation::DelegationTarget,
        request: DelegatedTaskRequest,
    ) -> Result<DelegatedTaskResult, DelegatedTaskError>;
}

fn versioned_hash(domain: &[u8], chunks: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for chunk in chunks {
        digest.update((chunk.len() as u64).to_be_bytes());
        digest.update(chunk);
    }
    format!("{:x}", digest.finalize())
}

fn deterministic_id(prefix: &str, domain: &[u8], chunks: &[&[u8]]) -> String {
    let digest = versioned_hash(domain, chunks);
    format!("{prefix}{}", &digest[..32])
}

fn range_digest(session: &SessionRef, entries: &[DelegatedContextEntry]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"medousa/conversation-range/v1\0");
    digest.update(session.authority_id.as_str().as_bytes());
    digest.update(session.session_id.as_str().as_bytes());
    for entry in entries {
        digest.update(entry.source.entry_seq.to_be_bytes());
        digest.update(entry.source.entry_id.as_str().as_bytes());
        digest.update(entry.content_digest.as_bytes());
    }
    format!("sha256:{:x}", digest.finalize())
}

fn serialized_size<T: Serialize>(value: &T) -> Result<usize, DelegatedTaskError> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|error| DelegatedTaskError::internal(error.to_string()))
}

/// Select the newest contiguous committed entries from one daemon-owned
/// session and describe them using the existing context-manifest model.
pub fn build_bounded_context_grant(
    store: &dyn SessionStore,
    authority_id: &AuthorityId,
    session_id: &SessionId,
    created_by: &str,
    correlation_key: &str,
    created_at: DateTime<Utc>,
) -> Result<DelegatedContextGrant, DelegatedTaskError> {
    let entries = store.load_transcript_entries(session_id);
    if entries.is_empty() {
        return Err(DelegatedTaskError::invalid(
            "delegated work requires at least one committed transcript entry",
        ));
    }
    let start = entries.len().saturating_sub(MAX_DELEGATED_CONTEXT_ENTRIES);
    let source_session = SessionRef {
        authority_id: authority_id.clone(),
        session_id: session_id.clone(),
    };
    let selected = entries[start..]
        .iter()
        .map(|entry| DelegatedContextEntry {
            source: TranscriptEntryRef {
                session: source_session.clone(),
                entry_id: entry.entry_id.clone(),
                entry_seq: entry.entry_seq,
            },
            caused_by: entry.caused_by.clone(),
            content_digest: entry.content_digest.clone(),
            turn: entry.turn.clone(),
        })
        .collect::<Vec<_>>();
    let first = selected
        .first()
        .expect("non-empty delegated context selection");
    let last = selected
        .last()
        .expect("non-empty delegated context selection");
    let selection = ConversationRangeSelection {
        session: source_session.clone(),
        after_entry_seq: (first.source.entry_seq > 1).then_some(first.source.entry_seq - 1),
        through_entry_seq: last.source.entry_seq,
    };
    let selection_digest = range_digest(&source_session, &selected);
    let manifest_id = ContextManifestId::parse(deterministic_id(
        "ctx_",
        b"medousa/delegated-context-manifest/v1\0",
        &[
            authority_id.as_str().as_bytes(),
            session_id.as_str().as_bytes(),
            correlation_key.as_bytes(),
            selection_digest.as_bytes(),
        ],
    ))
    .map_err(|error| DelegatedTaskError::internal(error.to_string()))?;
    let grant = DelegatedContextGrant {
        manifest: ContextManifest {
            manifest_id,
            sources: vec![ResolvedConversationRange {
                selection,
                selection_digest,
            }],
            created_by: created_by.trim().to_string(),
            created_at,
        },
        entries: selected,
    };
    validate_context_grant(&grant)?;
    Ok(grant)
}

pub fn validate_context_grant(context: &DelegatedContextGrant) -> Result<(), DelegatedTaskError> {
    if context.manifest.sources.len() != 1 {
        return Err(DelegatedTaskError::invalid(
            "delegated context must contain exactly one resolved source range",
        ));
    }
    if context.entries.is_empty() || context.entries.len() > MAX_DELEGATED_CONTEXT_ENTRIES {
        return Err(DelegatedTaskError::invalid(format!(
            "delegated context must contain 1-{MAX_DELEGATED_CONTEXT_ENTRIES} entries"
        )));
    }
    let resolved = &context.manifest.sources[0];
    let selection = &resolved.selection;
    let after = selection.after_entry_seq.unwrap_or(0);
    if after >= selection.through_entry_seq {
        return Err(DelegatedTaskError::invalid(
            "delegated context range must end after its exclusive lower bound",
        ));
    }
    let expected_len = selection
        .through_entry_seq
        .checked_sub(after)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| DelegatedTaskError::invalid("delegated context range is too large"))?;
    if expected_len != context.entries.len() {
        return Err(DelegatedTaskError::invalid(
            "delegated context range does not match its immutable entries",
        ));
    }
    for (offset, entry) in context.entries.iter().enumerate() {
        let expected_seq = after + offset as u64 + 1;
        if entry.source.session != selection.session || entry.source.entry_seq != expected_seq {
            return Err(DelegatedTaskError::invalid(
                "delegated context entries are not contiguous source coordinates",
            ));
        }
        let digest = transcript_content_digest(&entry.turn)?;
        if digest != entry.content_digest {
            return Err(DelegatedTaskError::conflict(
                "delegated transcript entry digest does not match its immutable payload",
            ));
        }
    }
    if range_digest(&selection.session, &context.entries) != resolved.selection_digest {
        return Err(DelegatedTaskError::conflict(
            "delegated context range digest does not match its entries",
        ));
    }
    if serialized_size(context)? > MAX_DELEGATED_CONTEXT_BYTES {
        return Err(DelegatedTaskError::invalid(format!(
            "delegated context exceeds {MAX_DELEGATED_CONTEXT_BYTES} bytes"
        )));
    }
    Ok(())
}

pub fn validate_task_request(request: &DelegatedTaskRequest) -> Result<(), DelegatedTaskError> {
    if request.schema_version != DELEGATED_TASK_SCHEMA_VERSION {
        return Err(DelegatedTaskError::invalid(format!(
            "unsupported delegated task schema version {}",
            request.schema_version
        )));
    }
    request
        .grant
        .validate_schema_version()
        .map_err(DelegatedTaskError::invalid)?;
    if request.grant.kind != AgentEnvelopeKind::TurnGranted {
        return Err(DelegatedTaskError::invalid(
            "delegated task requires a Stasis turn_granted envelope",
        ));
    }
    let turn_id = request
        .grant
        .turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DelegatedTaskError::invalid("delegated grant turn_id is required"))?;
    if request.grant.job_id.as_deref().is_none_or(str::is_empty)
        || request.grant.correlation_id.trim().is_empty()
        || request.grant.causation_id.trim().is_empty()
        || request
            .grant
            .participant_id
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err(DelegatedTaskError::invalid(
            "delegated grant is missing Stasis job/correlation/causation/participant identity",
        ));
    }
    let prompt = request
        .grant
        .payload
        .get("user_prompt")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DelegatedTaskError::invalid("delegated task prompt is required"))?;
    if prompt.chars().count() > MAX_DELEGATED_PROMPT_CHARS {
        return Err(DelegatedTaskError::invalid(format!(
            "delegated task prompt exceeds {MAX_DELEGATED_PROMPT_CHARS} characters"
        )));
    }
    validate_context_grant(&request.context)?;
    let source = &request.context.manifest.sources[0].selection.session;
    if request.source_execution.authority_id != source.authority_id
        || request.source_execution.session_id != source.session_id
        || request.grant.session_id != source.session_id.as_str()
    {
        return Err(DelegatedTaskError::invalid(
            "delegated source execution, grant session, and context authority do not match",
        ));
    }
    if request.source_execution.execution_id.as_str() != request.grant.causation_id {
        return Err(DelegatedTaskError::invalid(
            "delegated source execution must match Stasis causation identity",
        ));
    }
    if turn_id.len() > 128 {
        return Err(DelegatedTaskError::invalid(
            "delegated grant turn_id exceeds 128 characters",
        ));
    }
    Ok(())
}

pub fn delegated_work_id(sender_device_id: &str, turn_id: &str) -> String {
    deterministic_id(
        "work-",
        b"medousa/delegated-worker/v1\0",
        &[sender_device_id.as_bytes(), turn_id.as_bytes()],
    )
}

fn delegated_target_session_id(
    target_authority: &AuthorityId,
    sender_device_id: &str,
    turn_id: &str,
) -> Result<SessionId, DelegatedTaskError> {
    SessionId::parse(deterministic_id(
        "ses_",
        b"medousa/delegated-worker-session/v1\0",
        &[
            target_authority.as_str().as_bytes(),
            sender_device_id.as_bytes(),
            turn_id.as_bytes(),
        ],
    ))
    .map_err(|error| DelegatedTaskError::internal(error.to_string()))
}

/// Materialize the transferred immutable payloads as a derived session owned
/// by the receiving daemon. Retries reuse the same target and fail closed if a
/// turn or context is changed under the same Stasis identity.
pub async fn materialize_delegated_context(
    store: &dyn SessionStore,
    target_authority: &AuthorityId,
    sender_device_id: &str,
    request: &DelegatedTaskRequest,
) -> Result<DerivationCommitOutcome, DelegatedTaskError> {
    validate_task_request(request)?;
    let turn_id = request
        .grant
        .turn_id
        .as_deref()
        .expect("validated delegated turn id");
    let target_session_id =
        delegated_target_session_id(target_authority, sender_device_id, turn_id)?;
    let key_digest = format!(
        "sha256:{}",
        versioned_hash(
            b"medousa/delegated-worker-idempotency/v1\0",
            &[
                target_authority.as_str().as_bytes(),
                sender_device_id.as_bytes(),
                turn_id.as_bytes(),
            ],
        )
    );
    let request_bytes = serde_json::to_vec(request)
        .map_err(|error| DelegatedTaskError::internal(error.to_string()))?;
    let request_digest = format!(
        "sha256:{}",
        versioned_hash(b"medousa/delegated-worker-request/v1\0", &[&request_bytes],)
    );
    let derivation_id = DerivationId::parse(deterministic_id(
        "drv_",
        b"medousa/delegated-worker-derivation/v1\0",
        &[key_digest.as_bytes()],
    ))
    .map_err(|error| DelegatedTaskError::internal(error.to_string()))?;
    let actor = format!("peer:{}", sender_device_id.trim());
    let derivation = SessionDerivation {
        derivation_id,
        target_session: SessionRef {
            authority_id: target_authority.clone(),
            session_id: target_session_id,
        },
        manifest: request.context.manifest.clone(),
        intent: "mesh.task.request".to_string(),
        caused_by: Some(request.source_execution.clone()),
        created_by: actor,
        created_at: Utc::now(),
    };
    let entries = request
        .context
        .entries
        .iter()
        .map(|entry| TranscriptAppend {
            turn: entry.turn.clone(),
            caused_by: entry.caused_by.clone(),
            existing_entry_id: Some(entry.source.entry_id.clone()),
            source: Some(entry.source.clone()),
            expected_digest: Some(entry.content_digest.clone()),
        })
        .collect();
    store
        .materialize_derivation(&DerivationCommitRequest {
            derivation,
            idempotency_key_digest: key_digest,
            request_digest,
            entries,
        })
        .await
        .map_err(Into::into)
}

pub fn validate_task_result(
    request: &DelegatedTaskRequest,
    result: &DelegatedTaskResult,
) -> Result<(), DelegatedTaskError> {
    if result.schema_version != DELEGATED_TASK_SCHEMA_VERSION {
        return Err(DelegatedTaskError::invalid(format!(
            "unsupported delegated result schema version {}",
            result.schema_version
        )));
    }
    result
        .terminal
        .validate_schema_version()
        .map_err(DelegatedTaskError::invalid)?;
    if !matches!(
        result.terminal.kind,
        AgentEnvelopeKind::TurnCompleted | AgentEnvelopeKind::Failed | AgentEnvelopeKind::Cancelled
    ) {
        return Err(DelegatedTaskError::invalid(
            "delegated result must carry a terminal Stasis agent envelope",
        ));
    }
    if result.terminal.session_id != request.grant.session_id
        || result.terminal.thread_id != request.grant.thread_id
        || result.terminal.turn_id != request.grant.turn_id
        || result.terminal.correlation_id != request.grant.correlation_id
        || result.terminal.causation_id != request.grant.envelope_id
    {
        return Err(DelegatedTaskError::invalid(
            "delegated result does not match the pending Stasis turn",
        ));
    }
    if result.terminal.job_id != request.grant.job_id
        || result
            .terminal
            .participant_id
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err(DelegatedTaskError::conflict(
            "delegated terminal does not echo its canonical Stasis job and participant",
        ));
    }
    if result.derivation.manifest != request.context.manifest
        || result.derivation.caused_by.as_ref() != Some(&request.source_execution)
        || result.derivation.intent != "mesh.task.request"
        || result.execution.authority_id != result.derivation.target_session.authority_id
        || result.execution.session_id != result.derivation.target_session.session_id
    {
        return Err(DelegatedTaskError::conflict(
            "delegated result provenance does not match the granted context",
        ));
    }
    let payload_execution = result
        .terminal
        .payload
        .get("execution")
        .cloned()
        .ok_or_else(|| {
            DelegatedTaskError::conflict(
                "delegated terminal payload is missing its remote execution reference",
            )
        })
        .and_then(|value| {
            serde_json::from_value::<ExecutionRef>(value).map_err(|error| {
                DelegatedTaskError::conflict(format!(
                    "delegated terminal execution reference is invalid: {error}"
                ))
            })
        })?;
    let payload_derivation = result
        .terminal
        .payload
        .get("derivation")
        .cloned()
        .ok_or_else(|| {
            DelegatedTaskError::conflict(
                "delegated terminal payload is missing its session derivation",
            )
        })
        .and_then(|value| {
            serde_json::from_value::<SessionDerivation>(value).map_err(|error| {
                DelegatedTaskError::conflict(format!(
                    "delegated terminal session derivation is invalid: {error}"
                ))
            })
        })?;
    if payload_execution != result.execution || payload_derivation != result.derivation {
        return Err(DelegatedTaskError::conflict(
            "delegated terminal payload does not match its signed result provenance",
        ));
    }
    Ok(())
}

pub fn delegated_context_prompt(context: &DelegatedContextGrant) -> String {
    let mut output = String::from(
        "[MEDOUSA_DELEGATED_CONTEXT]\nThe following immutable transcript range was granted by the source daemon.\n",
    );
    for entry in &context.entries {
        let block = format!(
            "\n[{} seq={} entry={} digest={}]\n{}\n",
            entry.turn.role,
            entry.source.entry_seq,
            entry.source.entry_id,
            entry.content_digest,
            entry.turn.content.trim(),
        );
        if output.chars().count() + block.chars().count() > MAX_DELEGATED_CONTEXT_PROMPT_CHARS {
            output.push_str("\n[context prompt truncated at daemon policy boundary]\n");
            break;
        }
        output.push_str(&block);
    }
    output
}

pub fn source_execution_from_grant(
    authority_id: &AuthorityId,
    grant: &AgentEnvelope,
) -> Result<ExecutionRef, DelegatedTaskError> {
    let session_id = SessionId::parse(&grant.session_id)
        .map_err(|error| DelegatedTaskError::invalid(error.to_string()))?;
    let execution_id = ExecutionId::parse(&grant.causation_id)
        .map_err(|error| DelegatedTaskError::invalid(error.to_string()))?;
    Ok(ExecutionRef {
        authority_id: authority_id.clone(),
        session_id,
        execution_id,
    })
}

pub fn canonical_agent_schema_version() -> u32 {
    AGENT_ENVELOPE_SCHEMA_VERSION_V1
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medousa_types::session::TranscriptEntryId;
    use serde_json::json;

    fn sample_turn(content: &str) -> ConversationTurn {
        ConversationTurn {
            role: "user".to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_names: Vec::new(),
            answer_state: None,
            parts: None,
            slice_summary: None,
            speaker_profile_id: None,
        }
    }

    fn sample_context() -> DelegatedContextGrant {
        let authority = AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap();
        let session_id = SessionId::parse("ses_source").unwrap();
        let turn = sample_turn("bounded context");
        let digest = transcript_content_digest(&turn).unwrap();
        let source = SessionRef {
            authority_id: authority,
            session_id,
        };
        let entry = DelegatedContextEntry {
            source: TranscriptEntryRef {
                session: source.clone(),
                entry_id: TranscriptEntryId::parse("ent_0123456789abcdef0123456789abcdef").unwrap(),
                entry_seq: 1,
            },
            caused_by: None,
            content_digest: digest,
            turn,
        };
        DelegatedContextGrant {
            manifest: ContextManifest {
                manifest_id: ContextManifestId::parse("ctx_0123456789abcdef0123456789abcdef")
                    .unwrap(),
                sources: vec![ResolvedConversationRange {
                    selection: ConversationRangeSelection {
                        session: source.clone(),
                        after_entry_seq: None,
                        through_entry_seq: 1,
                    },
                    selection_digest: range_digest(&source, std::slice::from_ref(&entry)),
                }],
                created_by: "daemon:test".to_string(),
                created_at: Utc::now(),
            },
            entries: vec![entry],
        }
    }

    fn sample_request() -> DelegatedTaskRequest {
        let context = sample_context();
        let source = &context.manifest.sources[0].selection.session;
        DelegatedTaskRequest {
            schema_version: DELEGATED_TASK_SCHEMA_VERSION,
            grant: AgentEnvelope {
                schema_version: AGENT_ENVELOPE_SCHEMA_VERSION_V1,
                kind: AgentEnvelopeKind::TurnGranted,
                envelope_id: "grant-turn-1".to_string(),
                session_id: source.session_id.to_string(),
                thread_id: Some("thread-1".to_string()),
                turn_id: Some("turn-1".to_string()),
                job_id: Some("job-1".to_string()),
                correlation_id: "corr-1".to_string(),
                causation_id: "source-exec-1".to_string(),
                participant_id: Some("remote-worker".to_string()),
                occurred_at: Utc::now(),
                payload: json!({ "user_prompt": "do the heavy work" }),
            },
            source_execution: ExecutionRef {
                authority_id: source.authority_id.clone(),
                session_id: source.session_id.clone(),
                execution_id: ExecutionId::parse("source-exec-1").unwrap(),
            },
            context,
        }
    }

    #[test]
    fn canonical_request_reuses_stasis_and_medousa_identity() {
        let request = sample_request();
        validate_task_request(&request).expect("valid request");
        assert_eq!(
            request.source_execution.execution_id.as_str(),
            request.grant.causation_id
        );
    }

    #[test]
    fn changed_immutable_payload_fails_closed() {
        let mut request = sample_request();
        request.context.entries[0].turn.content = "tampered".to_string();
        let error = validate_task_request(&request).expect_err("digest conflict");
        assert_eq!(error.kind, DelegatedTaskErrorKind::Conflict);
    }

    #[test]
    fn work_identity_is_deterministic_without_a_parallel_registry() {
        assert_eq!(
            delegated_work_id("phone-a", "turn-1"),
            delegated_work_id("phone-a", "turn-1")
        );
        assert_ne!(
            delegated_work_id("phone-a", "turn-1"),
            delegated_work_id("phone-b", "turn-1")
        );
    }

    #[test]
    fn result_must_echo_stasis_job_and_bind_remote_execution_in_payload() {
        let request = sample_request();
        let authority = AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
        let session_id = SessionId::parse("ses_remote").unwrap();
        let execution = ExecutionRef {
            authority_id: authority.clone(),
            session_id: session_id.clone(),
            execution_id: ExecutionId::parse("work-remote").unwrap(),
        };
        let derivation = SessionDerivation {
            derivation_id: DerivationId::parse(format!("drv_{}", "b".repeat(32))).unwrap(),
            target_session: SessionRef {
                authority_id: authority,
                session_id,
            },
            manifest: request.context.manifest.clone(),
            intent: "mesh.task.request".to_string(),
            caused_by: Some(request.source_execution.clone()),
            created_by: "peer:source".to_string(),
            created_at: Utc::now(),
        };
        let mut result = DelegatedTaskResult {
            schema_version: DELEGATED_TASK_SCHEMA_VERSION,
            terminal: AgentEnvelope {
                schema_version: AGENT_ENVELOPE_SCHEMA_VERSION_V1,
                kind: AgentEnvelopeKind::TurnCompleted,
                envelope_id: "result-turn-1".to_string(),
                session_id: request.grant.session_id.clone(),
                thread_id: request.grant.thread_id.clone(),
                turn_id: request.grant.turn_id.clone(),
                job_id: request.grant.job_id.clone(),
                correlation_id: request.grant.correlation_id.clone(),
                causation_id: request.grant.envelope_id.clone(),
                participant_id: Some("remote-daemon".to_string()),
                occurred_at: Utc::now(),
                payload: json!({
                    "text": "done",
                    "execution": execution,
                    "derivation": derivation,
                }),
            },
            execution: execution.clone(),
            derivation,
        };
        validate_task_result(&request, &result).unwrap();
        result.terminal.job_id = Some("different-job".to_string());
        assert_eq!(
            validate_task_result(&request, &result).unwrap_err().kind,
            DelegatedTaskErrorKind::Conflict
        );
    }
}
