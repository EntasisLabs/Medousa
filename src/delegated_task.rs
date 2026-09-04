//! Bounded daemon-to-daemon task context around Stasis agent envelopes.
//!
//! Stasis owns job, turn, correlation, and causation identity. Medousa owns
//! conversation authority and provenance. This module only binds those
//! existing contracts for authenticated transport; it is not another task
//! registry or scheduler.

use std::collections::HashSet;
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
use crate::workshop_contract::{
    ExecutionPlacementResolution, ExecutionResolutionReason, UNKNOWN_EXECUTION_RUNTIME_ID,
    default_unknown_runtime_id,
};

pub const DELEGATED_TASK_SCHEMA_VERSION: u32 = 1;
pub const WORKER_SPAWN_SPEC_SCHEMA_VERSION: u32 = 1;
pub const MAX_DELEGATED_CONTEXT_ENTRIES: usize = 128;
pub const MAX_DELEGATED_CONTEXT_BYTES: usize = 512 * 1024;
pub const MAX_DELEGATED_PROMPT_CHARS: usize = 64 * 1024;
pub const MAX_DELEGATED_CONTEXT_PROMPT_CHARS: usize = 32 * 1024;
pub const MAX_WORKER_SPAWN_SPEC_BYTES: usize = 128 * 1024;
pub const MAX_WORKER_REQUESTED_TOOLS: usize = 256;

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

/// Resolved, portable Specialist snapshot. The destination executes this
/// immutable copy instead of consulting a mutable manuscript with the same id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerManuscriptSpec {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_appendix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_appendix: Option<String>,
    #[serde(default)]
    pub max_tool_rounds: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools_allow: Vec<String>,
    #[serde(default)]
    pub openshell_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openshell_policy_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openshell_sandbox_from: Option<String>,
}

/// Bot identity and relationship context captured at parent-turn admission.
/// This is provenance and prompt context, never destination authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerBotSpec {
    pub bot_id: String,
    pub profile_revision: u64,
    pub memory_scope_id: String,
    pub prompt_appendix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerParentSpec {
    pub stream_turn_id: u64,
    pub turn_correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_mode: Option<String>,
    pub original_user_prompt: String,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_work_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<WorkerBotSpec>,
    #[serde(default)]
    pub supports_ui_artifacts: bool,
    #[serde(default)]
    pub supports_liquid_markdown: bool,
    #[serde(default)]
    pub supports_browser_host: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerToolRequest {
    /// Exact source-side worker allowlist before destination policy is applied.
    pub names: Vec<String>,
}

/// Canonical semantic input for both local and remote turn workers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerSpawnSpec {
    pub schema_version: u32,
    pub intent: String,
    pub task: String,
    pub user_ack: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manuscript_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manuscript: Option<WorkerManuscriptSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
    pub parent: WorkerParentSpec,
    pub execution_placement: ExecutionPlacementResolution,
    pub max_tool_rounds: usize,
    pub tools: WorkerToolRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRouteProvenance {
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
}

/// Authenticated mesh payload for one Stasis `TurnGranted` event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedTaskRequest {
    pub schema_version: u32,
    pub grant: AgentEnvelope,
    pub source_execution: ExecutionRef,
    #[serde(default = "default_unknown_runtime_id")]
    pub parent_runtime_id: String,
    #[serde(default)]
    pub execution_placement: ExecutionPlacementResolution,
    /// Absent only for the explicit v0.10 compatibility decoder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<WorkerSpawnSpec>,
    pub context: DelegatedContextGrant,
}

/// Result returned by the worker daemon and signed by its mesh identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedTaskResult {
    pub schema_version: u32,
    pub terminal: AgentEnvelope,
    pub execution: ExecutionRef,
    #[serde(default = "default_unknown_runtime_id")]
    pub parent_runtime_id: String,
    #[serde(default)]
    pub execution_placement: ExecutionPlacementResolution,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_execution_grant: Option<crate::peer_execution_policy::TaskExecutionGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_spec_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_route: Option<WorkerRouteProvenance>,
    pub derivation: SessionDerivation,
}

/// Whether this exchange created the canonical remote worker or observed the
/// one already admitted under the same Stasis identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegatedTaskAdmission {
    Accepted,
    Existing,
}

/// Current state of the canonical worker on the receiving daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegatedTaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegatedTaskControlAction {
    Cancel,
    Steer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedTaskControlRequest {
    pub schema_version: u32,
    /// Stable idempotency key for this exact control mutation.
    pub control_id: String,
    pub action: DelegatedTaskControlAction,
    pub work_id: String,
    pub source_execution: ExecutionRef,
    pub parent_runtime_id: String,
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedTaskControlObservation {
    pub schema_version: u32,
    pub action: DelegatedTaskControlAction,
    pub work_id: String,
    pub status: DelegatedTaskStatus,
    #[serde(default)]
    pub queued_steers: usize,
    pub destination_runtime_id: String,
}

impl DelegatedTaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Immediate, signed response for an idempotent submit-or-observe exchange.
/// Non-terminal responses carry canonical remote provenance but do not hold
/// the transport open while the worker runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegatedTaskObservation {
    pub schema_version: u32,
    pub work_id: String,
    pub admission: DelegatedTaskAdmission,
    pub status: DelegatedTaskStatus,
    pub execution: ExecutionRef,
    #[serde(default = "default_unknown_runtime_id")]
    pub parent_runtime_id: String,
    #[serde(default)]
    pub execution_placement: ExecutionPlacementResolution,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_execution_grant: Option<crate::peer_execution_policy::TaskExecutionGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_spec_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_route: Option<WorkerRouteProvenance>,
    pub derivation: SessionDerivation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<DelegatedTaskResult>,
}

/// Host integration port. Each exchange idempotently admits or observes the
/// same canonical remote worker. Implementations supply the existing pairing
/// bearer, signed mesh envelope, pinned peer verification, and LAN/Iroh
/// routing; they never own task identity, retry state, or context selection.
#[async_trait]
pub trait DelegatedTaskTransport: Send + Sync {
    async fn submit_or_observe(
        &self,
        target: &crate::delegation::DelegationTarget,
        request: DelegatedTaskRequest,
    ) -> Result<DelegatedTaskObservation, DelegatedTaskError>;

    async fn control(
        &self,
        _target: &crate::delegation::DelegationTarget,
        _request: DelegatedTaskControlRequest,
    ) -> Result<DelegatedTaskControlObservation, DelegatedTaskError> {
        Err(DelegatedTaskError::transport(
            "remote delegated worker control is unavailable",
        ))
    }
}

pub fn validate_task_control_request(
    request: &DelegatedTaskControlRequest,
) -> Result<(), DelegatedTaskError> {
    if request.schema_version != DELEGATED_TASK_SCHEMA_VERSION {
        return Err(DelegatedTaskError::invalid(
            "unsupported delegated task control schema version",
        ));
    }
    validate_worker_text("control work id", &request.work_id, 256)?;
    validate_worker_text("control id", &request.control_id, 256)?;
    validate_worker_text("control parent runtime", &request.parent_runtime_id, 512)?;
    validate_worker_text("control correlation id", &request.correlation_id, 512)?;
    match request.action {
        DelegatedTaskControlAction::Cancel if request.message.is_some() => {
            return Err(DelegatedTaskError::invalid(
                "delegated cancellation must not carry a steer message",
            ));
        }
        DelegatedTaskControlAction::Steer => {
            let message = request.message.as_deref().ok_or_else(|| {
                DelegatedTaskError::invalid("delegated steer message is required")
            })?;
            validate_worker_text("steer message", message, 16 * 1024)?;
        }
        DelegatedTaskControlAction::Cancel => {}
    }
    Ok(())
}

pub fn validate_task_control_observation(
    request: &DelegatedTaskControlRequest,
    observation: &DelegatedTaskControlObservation,
) -> Result<(), DelegatedTaskError> {
    if observation.schema_version != DELEGATED_TASK_SCHEMA_VERSION
        || observation.action != request.action
        || observation.work_id != request.work_id
        || observation.destination_runtime_id.trim().is_empty()
    {
        return Err(DelegatedTaskError::conflict(
            "delegated worker control observation does not match its request",
        ));
    }
    Ok(())
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

fn validate_worker_text(
    label: &str,
    value: &str,
    max_chars: usize,
) -> Result<(), DelegatedTaskError> {
    if value.trim().is_empty()
        || value != value.trim()
        || value.chars().count() > max_chars
        || value.chars().any(|character| character == '\0')
    {
        return Err(DelegatedTaskError::invalid(format!(
            "delegated worker {label} is missing or invalid"
        )));
    }
    Ok(())
}

pub fn worker_spec_digest(spec: &WorkerSpawnSpec) -> Result<String, DelegatedTaskError> {
    let bytes = serde_json::to_vec(spec)
        .map_err(|error| DelegatedTaskError::internal(error.to_string()))?;
    Ok(format!(
        "sha256:{}",
        versioned_hash(b"medousa/worker-spawn-spec/v1\0", &[&bytes])
    ))
}

pub fn validate_worker_spawn_spec(spec: &WorkerSpawnSpec) -> Result<(), DelegatedTaskError> {
    if spec.schema_version != WORKER_SPAWN_SPEC_SCHEMA_VERSION {
        return Err(DelegatedTaskError::invalid(format!(
            "unsupported worker spawn spec schema version {}",
            spec.schema_version
        )));
    }
    let intent = crate::agent_runtime::turn_worker::TurnWorkerIntent::parse(&spec.intent)
        .ok_or_else(|| DelegatedTaskError::invalid("delegated worker intent is unsupported"))?;
    validate_worker_text("task", &spec.task, MAX_DELEGATED_PROMPT_CHARS)?;
    validate_worker_text("acknowledgement", &spec.user_ack, 4_096)?;
    validate_worker_text(
        "parent correlation id",
        &spec.parent.turn_correlation_id,
        512,
    )?;
    validate_worker_text("parent provider", &spec.parent.provider, 256)?;
    validate_worker_text("parent model", &spec.parent.model, 512)?;
    validate_worker_text("response depth mode", &spec.parent.response_depth_mode, 128)?;
    if spec.parent.original_user_prompt.chars().count() > MAX_DELEGATED_PROMPT_CHARS {
        return Err(DelegatedTaskError::invalid(
            "delegated worker parent prompt is too large",
        ));
    }
    if !(1..=128).contains(&spec.max_tool_rounds) {
        return Err(DelegatedTaskError::invalid(
            "delegated worker tool-round budget must be between 1 and 128",
        ));
    }
    if spec.manuscript_ids.len() > 8 {
        return Err(DelegatedTaskError::invalid(
            "delegated worker carries too many manuscript ids",
        ));
    }
    let mut manuscript_ids = HashSet::new();
    for manuscript_id in &spec.manuscript_ids {
        validate_worker_text("manuscript id", manuscript_id, 256)?;
        if !manuscript_ids.insert(manuscript_id) {
            return Err(DelegatedTaskError::invalid(
                "delegated worker manuscript ids must be unique",
            ));
        }
    }
    if let Some(manuscript) = &spec.manuscript {
        validate_worker_text("manuscript id", &manuscript.id, 256)?;
        validate_worker_text("manuscript name", &manuscript.name, 256)?;
        if !spec.manuscript_ids.iter().any(|id| id == &manuscript.id) {
            return Err(DelegatedTaskError::invalid(
                "resolved worker manuscript is not named by manuscript_ids",
            ));
        }
        if manuscript
            .worker_intent
            .as_deref()
            .and_then(crate::agent_runtime::turn_worker::TurnWorkerIntent::parse)
            .is_some_and(|manuscript_intent| manuscript_intent != intent)
        {
            return Err(DelegatedTaskError::invalid(
                "resolved worker manuscript intent conflicts with the worker intent",
            ));
        }
    }
    if let Some(bot) = &spec.parent.bot {
        validate_worker_text("Bot id", &bot.bot_id, 256)?;
        validate_worker_text("Bot memory scope", &bot.memory_scope_id, 256)?;
        if bot.prompt_appendix.chars().count() > 4_096 {
            return Err(DelegatedTaskError::invalid(
                "delegated worker Bot prompt appendix is too large",
            ));
        }
    }
    if spec.tools.names.is_empty() || spec.tools.names.len() > MAX_WORKER_REQUESTED_TOOLS {
        return Err(DelegatedTaskError::invalid(format!(
            "delegated worker must request 1-{MAX_WORKER_REQUESTED_TOOLS} tools"
        )));
    }
    let manuscript_tools = spec
        .manuscript
        .as_ref()
        .map(|manuscript| manuscript.tools_allow.as_slice())
        .unwrap_or(&[]);
    let source_ceiling = crate::agent_runtime::turn_worker::worker_allowlist_for_intent_and_tools(
        intent,
        manuscript_tools,
    );
    let mut prior: Option<&str> = None;
    for tool in &spec.tools.names {
        validate_worker_text("tool name", tool, 256)?;
        if prior.is_some_and(|previous| previous >= tool.as_str()) {
            return Err(DelegatedTaskError::invalid(
                "delegated worker tool names must be sorted and unique",
            ));
        }
        if !crate::agent_runtime::turn_worker::tool_allowed(tool, &source_ceiling) {
            return Err(DelegatedTaskError::invalid(format!(
                "delegated worker tool '{tool}' is outside its intent and manuscript ceiling"
            )));
        }
        prior = Some(tool);
    }
    if !spec.tools.names.iter().any(|tool| tool == "cognition_turn") {
        return Err(DelegatedTaskError::invalid(
            "delegated worker must request the canonical turn tool",
        ));
    }
    if serialized_size(spec)? > MAX_WORKER_SPAWN_SPEC_BYTES {
        return Err(DelegatedTaskError::invalid(format!(
            "delegated worker spawn spec exceeds {MAX_WORKER_SPAWN_SPEC_BYTES} bytes"
        )));
    }
    Ok(())
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
    if request.execution_placement.resolution_reason != ExecutionResolutionReason::LegacyUnknown
        && (request.parent_runtime_id.trim().is_empty()
            || request.parent_runtime_id == UNKNOWN_EXECUTION_RUNTIME_ID
            || request
                .execution_placement
                .resolved_runtime_id
                .trim()
                .is_empty()
            || request.execution_placement.resolved_runtime_id == UNKNOWN_EXECUTION_RUNTIME_ID)
    {
        return Err(DelegatedTaskError::invalid(
            "delegated execution placement is missing parent or resolved runtime identity",
        ));
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
    if let Some(worker) = request.worker.as_ref() {
        validate_worker_spawn_spec(worker)?;
        if worker.task != prompt
            || worker.parent.turn_correlation_id != request.grant.correlation_id
            || worker.execution_placement != request.execution_placement
        {
            return Err(DelegatedTaskError::conflict(
                "delegated worker specification does not match its Stasis grant and placement",
            ));
        }
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
    if result.parent_runtime_id != request.parent_runtime_id
        || result.execution_placement != request.execution_placement
    {
        return Err(DelegatedTaskError::conflict(
            "delegated result execution placement does not match the request",
        ));
    }
    validate_worker_provenance(
        request,
        result.worker_spec_digest.as_deref(),
        result.worker_route.as_ref(),
    )?;
    if let Some(grant) = result.task_execution_grant.as_ref() {
        validate_task_execution_grant(request, result.execution.execution_id.as_str(), grant)?;
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

pub fn validate_task_observation(
    request: &DelegatedTaskRequest,
    observation: &DelegatedTaskObservation,
) -> Result<(), DelegatedTaskError> {
    if observation.schema_version != DELEGATED_TASK_SCHEMA_VERSION {
        return Err(DelegatedTaskError::invalid(format!(
            "unsupported delegated observation schema version {}",
            observation.schema_version
        )));
    }
    if observation.work_id.trim().is_empty()
        || observation.execution.execution_id.as_str() != observation.work_id
    {
        return Err(DelegatedTaskError::conflict(
            "delegated observation has invalid canonical work identity",
        ));
    }
    if observation.derivation.manifest != request.context.manifest
        || observation.derivation.caused_by.as_ref() != Some(&request.source_execution)
        || observation.derivation.intent != "mesh.task.request"
        || observation.execution.authority_id != observation.derivation.target_session.authority_id
        || observation.execution.session_id != observation.derivation.target_session.session_id
    {
        return Err(DelegatedTaskError::conflict(
            "delegated observation provenance does not match the granted context",
        ));
    }
    if observation.parent_runtime_id != request.parent_runtime_id
        || observation.execution_placement != request.execution_placement
    {
        return Err(DelegatedTaskError::conflict(
            "delegated observation execution placement does not match the request",
        ));
    }
    validate_worker_provenance(
        request,
        observation.worker_spec_digest.as_deref(),
        observation.worker_route.as_ref(),
    )?;
    if let Some(grant) = observation.task_execution_grant.as_ref() {
        validate_task_execution_grant(request, &observation.work_id, grant)?;
    }
    match (&observation.status, &observation.result) {
        (status, None) if !status.is_terminal() => Ok(()),
        (status, Some(result)) if status.is_terminal() => {
            validate_task_result(request, result)?;
            if result.execution != observation.execution
                || result.derivation != observation.derivation
                || result.task_execution_grant != observation.task_execution_grant
            {
                return Err(DelegatedTaskError::conflict(
                    "delegated terminal result does not match its observation provenance",
                ));
            }
            let expected_status = match result.terminal.kind {
                AgentEnvelopeKind::TurnCompleted => DelegatedTaskStatus::Completed,
                AgentEnvelopeKind::Failed => DelegatedTaskStatus::Failed,
                AgentEnvelopeKind::Cancelled => DelegatedTaskStatus::Cancelled,
                _ => unreachable!("validated terminal kind"),
            };
            if *status != expected_status {
                return Err(DelegatedTaskError::conflict(
                    "delegated observation status does not match its terminal envelope",
                ));
            }
            Ok(())
        }
        _ => Err(DelegatedTaskError::conflict(
            "delegated observation terminal payload does not match its status",
        )),
    }
}

fn validate_task_execution_grant(
    request: &DelegatedTaskRequest,
    work_id: &str,
    grant: &crate::peer_execution_policy::TaskExecutionGrant,
) -> Result<(), DelegatedTaskError> {
    if grant.schema_version != crate::peer_execution_policy::TASK_EXECUTION_GRANT_SCHEMA_VERSION
        || grant.grant_id.trim().is_empty()
        || grant.peer_device_id.trim().is_empty()
        || grant.work_id != work_id
        || grant.parent_session_id != request.grant.session_id
        || grant.origin_runtime_id != request.parent_runtime_id
        || grant.correlation_id != request.grant.correlation_id
        || grant.expires_at <= grant.issued_at
        || !grant
            .effective_tool_domains
            .iter()
            .any(|domain| domain == "turn")
        || grant
            .effective_tool_domains
            .iter()
            .any(|domain| !grant.requested_tool_domains.contains(domain))
        || grant
            .effective_tool_names
            .iter()
            .any(|name| !grant.requested_tool_names.contains(name))
        || grant.effective_tool_names.iter().any(|name| {
            !grant
                .effective_tool_domains
                .iter()
                .any(|domain| domain == crate::peer_execution_policy::execution_tool_domain(name))
        })
        || (grant.network_policy == crate::peer_execution_policy::PeerNetworkPolicy::Deny
            && grant
                .effective_tool_domains
                .iter()
                .any(|domain| domain == "web"))
    {
        return Err(DelegatedTaskError::conflict(
            "delegated task execution grant does not match the request",
        ));
    }
    if let Some(worker) = request.worker.as_ref()
        && (grant.worker_intent != worker.intent
            || grant.bot_id.as_deref() != worker.parent.bot.as_ref().map(|bot| bot.bot_id.as_str())
            || grant.requested_tool_names != worker.tools.names
            || grant
                .effective_tool_names
                .iter()
                .any(|name| !grant.requested_tool_names.contains(name)))
    {
        return Err(DelegatedTaskError::conflict(
            "delegated task execution grant does not match the canonical worker specification",
        ));
    }
    if request.execution_placement.resolution_reason != ExecutionResolutionReason::LegacyUnknown
        && grant.destination_runtime_id != request.execution_placement.resolved_runtime_id
    {
        return Err(DelegatedTaskError::conflict(
            "delegated task execution grant does not match the resolved runtime",
        ));
    }
    Ok(())
}

fn validate_worker_provenance(
    request: &DelegatedTaskRequest,
    observed_digest: Option<&str>,
    route: Option<&WorkerRouteProvenance>,
) -> Result<(), DelegatedTaskError> {
    let Some(worker) = request.worker.as_ref() else {
        return Ok(());
    };
    let expected_digest = worker_spec_digest(worker)?;
    if observed_digest != Some(expected_digest.as_str()) {
        return Err(DelegatedTaskError::conflict(
            "delegated worker result does not echo its canonical spawn specification",
        ));
    }
    let route = route.ok_or_else(|| {
        DelegatedTaskError::conflict("delegated worker result is missing route provenance")
    })?;
    validate_worker_text("resolved provider", &route.provider, 256)?;
    validate_worker_text("resolved model", &route.model, 512)?;
    if route.response_depth_mode != worker.parent.response_depth_mode
        || route.stage_role != worker.stage_role
        || route.model_hint != worker.model_hint
    {
        return Err(DelegatedTaskError::conflict(
            "delegated worker result route does not match its spawn specification",
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
    use crate::workshop_contract::ExecutionTargetSelection;
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
            parent_runtime_id: "runtime-source".to_string(),
            execution_placement: ExecutionPlacementResolution::resolved(
                ExecutionTargetSelection::Exact {
                    runtime_id: "remote-daemon".to_string(),
                },
                "remote-daemon",
                ExecutionResolutionReason::ExactTarget,
            ),
            worker: None,
            context,
        }
    }

    fn canonical_worker(request: &DelegatedTaskRequest) -> WorkerSpawnSpec {
        WorkerSpawnSpec {
            schema_version: WORKER_SPAWN_SPEC_SCHEMA_VERSION,
            intent: "research".to_string(),
            task: "do the heavy work".to_string(),
            user_ack: "I’m on it.".to_string(),
            manuscript_ids: Vec::new(),
            manuscript: None,
            stage_role: None,
            model_hint: None,
            parent: WorkerParentSpec {
                stream_turn_id: 7,
                turn_correlation_id: request.grant.correlation_id.clone(),
                agent_mode: Some("general".to_string()),
                original_user_prompt: "Please investigate this.".to_string(),
                provider: "openai".to_string(),
                model: "gpt-test".to_string(),
                response_depth_mode: "standard".to_string(),
                code_work_id: None,
                bot: None,
                supports_ui_artifacts: true,
                supports_liquid_markdown: true,
                supports_browser_host: false,
            },
            execution_placement: request.execution_placement.clone(),
            max_tool_rounds: 10,
            tools: WorkerToolRequest {
                names: vec![
                    "cognition_turn".to_string(),
                    "cognition_utility_time_now".to_string(),
                ],
            },
        }
    }

    #[test]
    fn canonical_request_reuses_stasis_and_medousa_identity() {
        let mut request = sample_request();
        request.worker = Some(canonical_worker(&request));
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
    fn delegated_control_requires_a_stable_id_and_action_payload() {
        let source = sample_request();
        let mut control = DelegatedTaskControlRequest {
            schema_version: DELEGATED_TASK_SCHEMA_VERSION,
            control_id: "control-1".to_string(),
            action: DelegatedTaskControlAction::Steer,
            work_id: "work-remote".to_string(),
            source_execution: source.source_execution,
            parent_runtime_id: source.parent_runtime_id,
            correlation_id: source.grant.correlation_id,
            message: Some("Focus on the failing integration test.".to_string()),
        };
        validate_task_control_request(&control).expect("valid steer");
        control.message = None;
        assert_eq!(
            validate_task_control_request(&control)
                .expect_err("steer text required")
                .kind,
            DelegatedTaskErrorKind::Invalid
        );
        control.action = DelegatedTaskControlAction::Cancel;
        validate_task_control_request(&control).expect("valid cancellation");
    }

    #[test]
    fn result_must_echo_stasis_job_and_bind_remote_execution_in_payload() {
        let mut request = sample_request();
        request.worker = Some(canonical_worker(&request));
        let spec_digest = worker_spec_digest(request.worker.as_ref().unwrap()).unwrap();
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
            parent_runtime_id: request.parent_runtime_id.clone(),
            execution_placement: request.execution_placement.clone(),
            task_execution_grant: None,
            worker_spec_digest: Some(spec_digest.clone()),
            worker_route: Some(WorkerRouteProvenance {
                provider: "openai".to_string(),
                model: "gpt-test".to_string(),
                response_depth_mode: "standard".to_string(),
                stage_role: None,
                model_hint: None,
            }),
            derivation,
        };
        validate_task_result(&request, &result).unwrap();
        let pending = DelegatedTaskObservation {
            schema_version: DELEGATED_TASK_SCHEMA_VERSION,
            work_id: execution.execution_id.to_string(),
            admission: DelegatedTaskAdmission::Accepted,
            status: DelegatedTaskStatus::Running,
            execution: execution.clone(),
            parent_runtime_id: request.parent_runtime_id.clone(),
            execution_placement: request.execution_placement.clone(),
            task_execution_grant: None,
            worker_spec_digest: Some(spec_digest),
            worker_route: result.worker_route.clone(),
            derivation: result.derivation.clone(),
            result: None,
        };
        validate_task_observation(&request, &pending).unwrap();
        let mut terminal = pending;
        terminal.admission = DelegatedTaskAdmission::Existing;
        terminal.status = DelegatedTaskStatus::Completed;
        terminal.result = Some(result.clone());
        validate_task_observation(&request, &terminal).unwrap();
        terminal.status = DelegatedTaskStatus::Running;
        assert_eq!(
            validate_task_observation(&request, &terminal)
                .unwrap_err()
                .kind,
            DelegatedTaskErrorKind::Conflict
        );
        result.terminal.job_id = Some("different-job".to_string());
        assert_eq!(
            validate_task_result(&request, &result).unwrap_err().kind,
            DelegatedTaskErrorKind::Conflict
        );
    }
}
