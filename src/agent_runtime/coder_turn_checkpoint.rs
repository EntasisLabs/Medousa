//! Durable, protocol-safe checkpoints for unfinished Coder turns.
//!
//! Semantic memory explains the work. This store preserves the normalized
//! provider transcript and loop counters needed to continue an exact turn.
//! It only accepts snapshots produced after a complete model-only boundary or
//! after every tool call in a batch has a matching result.
//!
//! H06.10 persists logical checkpoint generations through a durable journal and
//! references the H03 turn journal via [`medousa_engine::TranscriptCursor`]
//! rather than duplicating the canonical event stream.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use genai::chat::{ChatMessage, ContentPart, MessageContent, ToolCall, ToolResponse};
use medousa_forge::forge::Forge;
use medousa_forge::model::{
    AttemptId, AttemptState, ExecutionLease, GovernedEnv, RecoveryDisposition, WorkId,
};
use medousa_forge::observation::ObservationCompleteness;
use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::persistence::{
    CommitReceipt, DurabilityLevel, FileTransaction, StoreKind, TransactionFaultPoint,
};
use crate::store_root::{StoreEntryKind, StoreRoot};

use super::coder_activity::{CoderActivityKind, CoderActivityStore};
use super::coder_mode::CoderEntryContext;
use super::coder_tools::CoderBoundToolRegistry;
use super::turn_budget::TurnOrchestrationState;
use super::turn_context::TurnScratchpad;

pub const ACTIVE_TURN_CHECKPOINT_SCHEMA_VERSION: u32 = 1;
pub const LOGICAL_CHECKPOINT_JOURNAL_SCHEMA_VERSION: u32 = 1;
pub use medousa_runtime::checkpoint::{
    ActiveTurnCheckpointSink, ActiveTurnCheckpointStatus, ActiveTurnCounters,
    ActiveTurnResumeState, ActiveTurnTranscript, CheckpointToolInvocation, OutstandingTurnBoundary,
    SafeCheckpointBoundary, TOOL_ROUND_BUDGET_EXHAUSTED_REASON, ToolLoopCheckpointState,
};

const CHECKPOINT_DIR: &str = "coder_turn_checkpoints";
const CHECKPOINT_OBJECT_DOMAIN: &[u8] = b"coder-turn-checkpoint";
const CHECKPOINT_JOURNAL_DOMAIN: &[u8] = b"coder-turn-checkpoint-journal";
const MAX_CHECKPOINT_BYTES: u64 = 512 * 1024;
const MAX_TRANSCRIPT_BYTES: usize = 224 * 1024;
const MAX_TRANSCRIPT_MESSAGES: usize = 192;
const MAX_MESSAGE_PART_CHARS: usize = 24_000;
const MAX_INVOCATIONS: usize = 160;
const MAX_INVOCATION_BYTES: usize = 96 * 1024;
const MAX_CHANGED_PATHS: usize = 160;
const MAX_VISIBLE_TOOLS: usize = 160;
const MAX_FIELD_CHARS: usize = 16_000;
const MAX_JSON_VALUE_BYTES: usize = 24_000;
const MAX_JOURNAL_RECORD_BYTES: usize = 768 * 1024;
const MAX_CHECKPOINT_JOURNAL_BYTES: u64 = MAX_CHECKPOINT_BYTES * 8;

static CODER_TURN_CHECKPOINT_STORE: Lazy<Arc<CoderTurnCheckpointStore>> = Lazy::new(|| {
    Arc::new(CoderTurnCheckpointStore::open(
        crate::session::medousa_data_dir().join(CHECKPOINT_DIR),
    ))
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedToolBoundary {
    pub batch_index: usize,
    pub model_round: usize,
    pub tool_names: Vec<String>,
    pub provider_call_ids: Vec<String>,
    pub activity_cursor: u64,
    pub environment_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointForgeState {
    pub work_id: String,
    pub attempt_id: String,
    pub resumed_from_attempt_id: Option<String>,
    pub worktree: String,
    pub branch: String,
    pub environment_generation: u32,
    pub head_oid: String,
    pub dirty: bool,
    pub dirty_digest: String,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTurnCheckpoint {
    pub schema_version: u32,
    pub session_id: String,
    pub daemon_turn_id: String,
    pub resumed_from_turn_id: Option<String>,
    pub agent_id: String,
    pub agent_mode: String,
    pub contract_revision: String,
    pub provider: String,
    pub model: String,
    pub authoritative_prompt: String,
    pub current_goal: String,
    pub status: ActiveTurnCheckpointStatus,
    pub safe_boundary: SafeCheckpointBoundary,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub counters: ActiveTurnCounters,
    pub transcript: ActiveTurnTranscript,
    pub invocations: Vec<CheckpointToolInvocation>,
    pub pack_hold_fragments: Vec<String>,
    pub scratch: TurnScratchpad,
    pub forge: CheckpointForgeState,
    pub activity_cursor: u64,
    pub locus_cursor: Option<String>,
    pub visible_tools: Vec<String>,
    pub outstanding_boundary: Option<OutstandingTurnBoundary>,
    pub last_completed_tool_boundary: Option<CompletedToolBoundary>,
    pub termination_reason: Option<String>,
    #[serde(default)]
    pub transcript_cursor: Option<medousa_engine::TranscriptCursor>,
    #[serde(default)]
    pub checkpoint_generation: u64,
    #[serde(default)]
    pub required_workspace_generation: u64,
}

impl ActiveTurnCheckpoint {
    pub fn into_resume_state(self) -> ActiveTurnResumeState {
        let reset_turn_budget = !self.status.restores_interrupted_budget();
        let mut counters = if reset_turn_budget {
            ActiveTurnCounters::default()
        } else {
            self.counters.clone()
        };
        counters.max_tool_rounds = counters
            .max_tool_rounds
            .min(super::turn_loop_settings::DEFAULT_CODER_MAX_TOOL_ROUNDS);
        ActiveTurnResumeState {
            source_daemon_turn_id: self.daemon_turn_id.clone(),
            restore_turn_budget: !reset_turn_budget,
            append_current_user_message: true,
            counters,
            transcript: self.transcript.clone(),
            invocations: if reset_turn_budget {
                Vec::new()
            } else {
                self.invocations.clone()
            },
            pack_hold_fragments: if reset_turn_budget {
                Vec::new()
            } else {
                self.pack_hold_fragments.clone()
            },
            scratch: self.scratch.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogicalCheckpointRecord {
    schema_version: u32,
    generation: u64,
    turn_id: String,
    session_id: String,
    work_id: String,
    published_at_utc: DateTime<Utc>,
    body_digest: String,
    body: LogicalCheckpointBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LogicalCheckpointBody {
    /// Full logical state at a protocol-safe boundary (replace semantics).
    Boundary { checkpoint: ActiveTurnCheckpoint },
}

pub struct CoderTurnCheckpointStore {
    root_path: PathBuf,
    files: crate::session_storage::SessionDirectoryStore,
    transaction: OnceCell<FileTransaction>,
    lock: Mutex<()>,
}

impl std::fmt::Debug for CoderTurnCheckpointStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CoderTurnCheckpointStore")
            .field("root_path", &self.root_path)
            .field("transaction_ready", &self.transaction.get().is_some())
            .finish_non_exhaustive()
    }
}

impl CoderTurnCheckpointStore {
    pub fn open(root: impl Into<PathBuf>) -> Self {
        let root_path = root.into();
        Self {
            files: crate::session_storage::SessionDirectoryStore::new_with_legacy_directory(
                root_path.clone(),
                checkpoint_legacy_session_directory,
            ),
            root_path,
            transaction: OnceCell::new(),
            lock: Mutex::new(()),
        }
    }

    fn transaction(&self) -> Result<&FileTransaction, String> {
        self.transaction
            .get_or_try_init(|| {
                let root = Arc::new(
                    StoreRoot::open_or_create(&self.root_path)
                        .map_err(|error| format!("cannot open Coder checkpoint store: {error}"))?,
                );
                Ok(FileTransaction::new(root))
            })
            .map_err(|error: String| error)
    }

    pub fn load_latest_resume_candidate(
        &self,
        session_id: &str,
        work_id: &str,
    ) -> Result<Option<ActiveTurnCheckpoint>, String> {
        let session_id = crate::session_storage::SessionId::parse(session_id)
            .map_err(|error| error.to_string())?;
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let entries = match self.files.list(&session_id) {
            Ok(entries) => entries,
            Err(error) if error.is_not_found() => return Ok(None),
            Err(err) => return Err(format!("cannot scan Coder turn checkpoints: {err}")),
        };
        let mut latest_by_turn = HashMap::<String, ActiveTurnCheckpoint>::new();
        for entry in entries {
            if entry.kind != StoreEntryKind::File || entry.size == 0 {
                continue;
            }
            let name = entry.path.file_name();
            if name.ends_with(".jsonl") {
                // Prefer the published head; journals are folded only when the
                // head is missing for a turn discovered via the journal name.
                continue;
            }
            if entry.size > MAX_CHECKPOINT_BYTES {
                continue;
            }
            let raw = match self
                .files
                .read_limited(&session_id, &entry.path, MAX_CHECKPOINT_BYTES)
            {
                Ok(raw) => raw,
                Err(_) => continue,
            };
            let checkpoint = match serde_json::from_slice::<ActiveTurnCheckpoint>(&raw) {
                Ok(checkpoint) => checkpoint,
                Err(err) => {
                    tracing::warn!(error = %err, entry = name, "ignoring malformed Coder turn checkpoint");
                    continue;
                }
            };
            if validate_checkpoint(&checkpoint, session_id.as_str(), work_id).is_err() {
                continue;
            }
            let turn_id = checkpoint.daemon_turn_id.clone();
            if latest_by_turn.get(&turn_id).is_none_or(|current| {
                checkpoint.checkpoint_generation > current.checkpoint_generation
                    || (checkpoint.checkpoint_generation == current.checkpoint_generation
                        && checkpoint.updated_at_utc >= current.updated_at_utc)
            }) {
                latest_by_turn.insert(turn_id, checkpoint);
            }
        }
        Ok(latest_by_turn
            .into_values()
            .filter(|checkpoint| checkpoint.status.is_resume_candidate())
            .max_by_key(|checkpoint| {
                (
                    checkpoint.checkpoint_generation,
                    checkpoint.updated_at_utc.timestamp_millis(),
                )
            }))
    }

    /// Fold the durable logical journal for a turn after a crash or missing head.
    pub fn recover_from_journal(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Option<ActiveTurnCheckpoint>, String> {
        let session_id = crate::session_storage::SessionId::parse(session_id)
            .map_err(|error| error.to_string())?;
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        self.fold_journal_unlocked(&session_id, turn_id)
    }

    pub fn save(&self, checkpoint: &ActiveTurnCheckpoint) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        self.save_unlocked(checkpoint)
    }

    pub fn mark_superseded(
        &self,
        checkpoint: &ActiveTurnCheckpoint,
        reason: &str,
    ) -> Result<(), String> {
        let mut superseded = checkpoint.clone();
        superseded.status = ActiveTurnCheckpointStatus::Superseded;
        superseded.safe_boundary = SafeCheckpointBoundary::Terminal;
        superseded.updated_at_utc = Utc::now();
        superseded.termination_reason = Some(truncate(reason, 2_000));
        superseded.checkpoint_generation = superseded.checkpoint_generation.saturating_add(1);
        self.save(&superseded)
    }

    fn save_unlocked(&self, checkpoint: &ActiveTurnCheckpoint) -> Result<(), String> {
        let session_id = crate::session_storage::SessionId::parse(&checkpoint.session_id)
            .map_err(|error| error.to_string())?;
        let _mutation = crate::session_deletion::acquire_mutation(&session_id)?;
        validate_checkpoint(
            checkpoint,
            &checkpoint.session_id,
            &checkpoint.forge.work_id,
        )?;
        let mut bounded = checkpoint.clone();
        bound_checkpoint(&mut bounded);
        evict_prefix_to_budget(&mut bounded);
        let head_bytes = serde_json::to_vec(&bounded)
            .map_err(|err| format!("cannot serialize Coder turn checkpoint: {err}"))?;
        if head_bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
            return Err(format!(
                "Coder turn checkpoint exceeds {} bytes after bounding",
                MAX_CHECKPOINT_BYTES
            ));
        }

        let existing = self.fold_journal_unlocked(&session_id, &bounded.daemon_turn_id)?;
        if let Some(existing) = existing.as_ref() {
            if bounded.checkpoint_generation < existing.checkpoint_generation {
                return Err(format!(
                    "stale checkpoint generation {} rejected; durable generation is {}",
                    bounded.checkpoint_generation, existing.checkpoint_generation
                ));
            }
            if bounded.checkpoint_generation == existing.checkpoint_generation {
                if checkpoint_body_digest(existing) == checkpoint_body_digest(&bounded) {
                    return Ok(());
                }
                return Err(format!(
                    "stale checkpoint writer collided at generation {}",
                    bounded.checkpoint_generation
                ));
            }
        }

        let body = LogicalCheckpointBody::Boundary {
            checkpoint: bounded.clone(),
        };
        let body_digest = logical_body_digest(&body);
        let record = LogicalCheckpointRecord {
            schema_version: LOGICAL_CHECKPOINT_JOURNAL_SCHEMA_VERSION,
            generation: bounded.checkpoint_generation,
            turn_id: bounded.daemon_turn_id.clone(),
            session_id: bounded.session_id.clone(),
            work_id: bounded.forge.work_id.clone(),
            published_at_utc: Utc::now(),
            body_digest: body_digest.clone(),
            body,
        };
        let record_bytes = serde_json::to_vec(&record)
            .map_err(|err| format!("cannot serialize logical checkpoint delta: {err}"))?;
        if record_bytes.len() > MAX_JOURNAL_RECORD_BYTES {
            return Err(format!(
                "logical checkpoint delta exceeds {MAX_JOURNAL_RECORD_BYTES} bytes"
            ));
        }

        let journal_rel = checkpoint_journal_path(&bounded.daemon_turn_id);
        let head_rel = checkpoint_path(&bounded.daemon_turn_id);
        let journal_path = self
            .files
            .resolve_write_path(&session_id, &journal_rel)
            .map_err(|err| format!("cannot resolve Coder checkpoint journal path: {err}"))?;
        let head_path = self
            .files
            .resolve_write_path(&session_id, &head_rel)
            .map_err(|err| format!("cannot resolve Coder checkpoint head path: {err}"))?;
        let tx = self.transaction()?;
        let current_journal_bytes = match tx.root().metadata(&journal_path) {
            Ok(metadata) => metadata.size,
            Err(error) if error.is_not_found() => 0,
            Err(error) => return Err(format!("cannot inspect Coder checkpoint journal: {error}")),
        };
        if current_journal_bytes
            .saturating_add(record_bytes.len() as u64)
            .saturating_add(1)
            > MAX_CHECKPOINT_JOURNAL_BYTES
        {
            let compacted = existing.as_ref().ok_or_else(|| {
                "checkpoint journal exceeded its bound without a recoverable prefix".to_string()
            })?;
            let compacted_body = LogicalCheckpointBody::Boundary {
                checkpoint: compacted.clone(),
            };
            let compacted_record = LogicalCheckpointRecord {
                schema_version: LOGICAL_CHECKPOINT_JOURNAL_SCHEMA_VERSION,
                generation: compacted.checkpoint_generation,
                turn_id: compacted.daemon_turn_id.clone(),
                session_id: compacted.session_id.clone(),
                work_id: compacted.forge.work_id.clone(),
                published_at_utc: Utc::now(),
                body_digest: logical_body_digest(&compacted_body),
                body: compacted_body,
            };
            let mut compacted_bytes = serde_json::to_vec(&compacted_record)
                .map_err(|err| format!("cannot serialize compacted checkpoint journal: {err}"))?;
            compacted_bytes.push(b'\n');
            tx.replace_snapshot(&journal_path, &compacted_bytes, DurabilityLevel::Synced)
                .map_err(|err| format!("cannot compact Coder checkpoint journal: {err}"))?;
        }
        tx.check(TransactionFaultPoint::BeforeCheckpointDelta)
            .map_err(|err| format!("checkpoint journal fault before delta: {err}"))?;
        tx.append_record(&journal_path, &record_bytes, DurabilityLevel::Synced)
            .map_err(|err| format!("cannot append Coder checkpoint journal: {err}"))?;
        tx.check(TransactionFaultPoint::AfterCheckpointDelta)
            .map_err(|err| format!("checkpoint journal fault after delta: {err}"))?;
        tx.replace_snapshot(&head_path, &head_bytes, DurabilityLevel::Synced)
            .map_err(|err| format!("cannot publish Coder checkpoint head: {err}"))?;
        let _ = CommitReceipt::new(
            StoreKind::CoderCheckpoint,
            &bounded.daemon_turn_id,
            bounded.checkpoint_generation,
            DurabilityLevel::Synced,
            head_bytes.len(),
        );
        Ok(())
    }

    fn fold_journal_unlocked(
        &self,
        session_id: &crate::session_storage::SessionId,
        turn_id: &str,
    ) -> Result<Option<ActiveTurnCheckpoint>, String> {
        let journal_rel = checkpoint_journal_path(turn_id);
        let path = match self.files.resolve_read_path(session_id, &journal_rel) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let tx = self.transaction()?;
        let raw = match tx
            .root()
            .read_limited(&path, MAX_CHECKPOINT_BYTES.saturating_mul(8))
        {
            Ok(raw) => raw,
            Err(error) if error.is_not_found() => return Ok(None),
            Err(error) => {
                return Err(format!("cannot read Coder checkpoint journal: {error}"));
            }
        };
        if raw.is_empty() {
            return Ok(None);
        }
        let mut latest: Option<ActiveTurnCheckpoint> = None;
        let mut last_generation: Option<u64> = None;
        for (index, line) in raw.split(|byte| *byte == b'\n').enumerate() {
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let record: LogicalCheckpointRecord = match serde_json::from_slice(line) {
                Ok(record) => record,
                Err(error) => {
                    let is_final = !raw.ends_with(b"\n")
                        && raw
                            .rsplit(|byte| *byte == b'\n')
                            .next()
                            .is_some_and(|tail| tail == line);
                    if is_final {
                        // Incomplete final journal record only — keep prior prefix.
                        break;
                    }
                    return Err(format!(
                        "Coder checkpoint journal corruption at record {index}: {error}"
                    ));
                }
            };
            if record.schema_version != LOGICAL_CHECKPOINT_JOURNAL_SCHEMA_VERSION {
                return Err(format!(
                    "unsupported Coder checkpoint journal schema {}",
                    record.schema_version
                ));
            }
            if record.turn_id != turn_id {
                return Err("Coder checkpoint journal turn_id mismatch".into());
            }
            let LogicalCheckpointBody::Boundary { checkpoint } = &record.body;
            let digest = logical_body_digest(&record.body);
            if digest != record.body_digest {
                return Err(format!(
                    "Coder checkpoint journal digest mismatch at generation {}",
                    record.generation
                ));
            }
            match last_generation {
                Some(previous) if record.generation < previous => {
                    return Err(format!(
                        "Coder checkpoint journal generation {} is not monotonic",
                        record.generation
                    ));
                }
                Some(previous) if record.generation == previous => {
                    if latest.as_ref().is_some_and(|current| {
                        checkpoint_body_digest(current) == checkpoint_body_digest(checkpoint)
                    }) {
                        continue;
                    }
                    return Err(format!(
                        "Coder checkpoint journal generation {} collided with a different body",
                        record.generation
                    ));
                }
                _ => {
                    last_generation = Some(record.generation);
                    latest = Some(checkpoint.clone());
                }
            }
        }
        Ok(latest)
    }

    pub fn delete_session(
        &self,
        session_id: &crate::session_storage::SessionId,
    ) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        self.files
            .remove_session(session_id)
            .map_err(|error| format!("cannot delete Coder checkpoint directory: {error}"))?;
        if self
            .files
            .contains_session(session_id)
            .map_err(|error| format!("cannot verify Coder checkpoint deletion: {error}"))?
        {
            return Err("Coder checkpoint directory remains after deletion".to_string());
        }
        Ok(())
    }
}

pub fn coder_turn_checkpoint_store() -> Arc<CoderTurnCheckpointStore> {
    CODER_TURN_CHECKPOINT_STORE.clone()
}

#[derive(Debug, Clone)]
pub enum CoderRecoveryPlan {
    Fresh,
    Exact(ActiveTurnCheckpoint),
    Semantic {
        checkpoint: ActiveTurnCheckpoint,
        reason: String,
    },
}

impl CoderRecoveryPlan {
    pub fn checkpoint(&self) -> Option<&ActiveTurnCheckpoint> {
        match self {
            Self::Exact(checkpoint) | Self::Semantic { checkpoint, .. } => Some(checkpoint),
            Self::Fresh => None,
        }
    }

    pub fn exact_checkpoint(&self) -> Option<&ActiveTurnCheckpoint> {
        match self {
            Self::Exact(checkpoint) => Some(checkpoint),
            _ => None,
        }
    }

    pub fn prompt_note(&self) -> Option<String> {
        match self {
            Self::Fresh => None,
            Self::Exact(checkpoint) => Some(format!(
                "[MEDOUSA_CODER_RECOVERY]\nmode=exact\nsource_turn={}\nsource_attempt={}\nsafe_boundary={:?}\nrounds_completed={}\ninstruction=Continue from the restored transcript and completed tool boundary. Do not repeat completed tool effects unless new evidence requires it.",
                checkpoint.daemon_turn_id,
                checkpoint.forge.attempt_id,
                checkpoint.safe_boundary,
                checkpoint.counters.model_rounds_executed,
            )),
            Self::Semantic { checkpoint, reason } => Some(format!(
                "[MEDOUSA_CODER_RECOVERY]\nmode=semantic\nsource_turn={}\nsource_attempt={}\nreason={}\ninstruction=The exact provider boundary could not be proven. Preserve the governed worktree, inspect current Forge/activity/Locus evidence, and never replay a possibly completed side effect.",
                checkpoint.daemon_turn_id,
                checkpoint.forge.attempt_id,
                truncate(reason, 800),
            )),
        }
    }
}

pub fn plan_coder_recovery(
    store: &CoderTurnCheckpointStore,
    forge: &Forge,
    activity: &CoderActivityStore,
    session_id: &str,
    work_id: &WorkId,
) -> Result<CoderRecoveryPlan, String> {
    let Some(checkpoint) = store.load_latest_resume_candidate(session_id, work_id.as_str())? else {
        return Ok(CoderRecoveryPlan::Fresh);
    };
    let semantic = |reason: String| CoderRecoveryPlan::Semantic {
        checkpoint: checkpoint.clone(),
        reason,
    };
    let item = match forge.load(work_id) {
        Ok(item) => item,
        Err(err) => return Ok(semantic(format!("Forge undertaking is unavailable: {err}"))),
    };
    if item.has_active_attempts() {
        return Ok(semantic(
            "another active Forge attempt prevents exact environment rebinding".into(),
        ));
    }
    let source_attempt_id = AttemptId::from(checkpoint.forge.attempt_id.clone());
    let Some(attempt) = item.attempt(&source_attempt_id) else {
        return Ok(semantic("checkpoint Forge attempt no longer exists".into()));
    };
    if attempt.state != AttemptState::Interrupted
        || !matches!(
            attempt.recovery,
            Some(RecoveryDisposition::RestartAllowed)
                | Some(RecoveryDisposition::ResumeSupported { .. })
        )
    {
        return Ok(semantic(format!(
            "checkpoint attempt is not restartable (state={:?})",
            attempt.state
        )));
    }
    let Some(environment) = attempt.environment.as_ref() else {
        return Ok(semantic(
            "checkpoint attempt has no preserved Forge environment".into(),
        ));
    };
    let observed = match observe_environment(forge, environment, work_id, &source_attempt_id, None)
    {
        Ok(observed) => observed,
        Err(err) => return Ok(semantic(err)),
    };
    if let Some(reason) = forge_drift_reason(&checkpoint.forge, &observed) {
        return Ok(semantic(reason));
    }

    let events = activity
        .events_for_work(work_id.as_str())
        .map_err(|err| format!("cannot reconcile Coder activity cursor: {err}"))?;
    let current_revision = events.last().map(|event| event.revision).unwrap_or(0);
    if current_revision < checkpoint.activity_cursor {
        return Ok(semantic(format!(
            "activity ledger regressed from {} to {current_revision}",
            checkpoint.activity_cursor
        )));
    }
    let uncertain_completion = events.iter().any(|event| {
        event.revision > checkpoint.activity_cursor
            && event.agent_id == checkpoint.agent_id
            && matches!(
                event.kind,
                CoderActivityKind::ToolPlanned
                    | CoderActivityKind::ToolCompleted
                    | CoderActivityKind::ToolFailed
            )
    });
    if uncertain_completion {
        return Ok(semantic(
            "the activity ledger contains a tool start or completion after the last durable protocol boundary"
                .into(),
        ));
    }
    Ok(CoderRecoveryPlan::Exact(checkpoint))
}

pub struct CoderTurnCheckpointController {
    store: Arc<CoderTurnCheckpointStore>,
    checkpoint: Mutex<ActiveTurnCheckpoint>,
    forge: Arc<Forge>,
    entry: Arc<CoderEntryContext>,
    registry: Arc<CoderBoundToolRegistry>,
    event_log: Option<Arc<medousa_engine::TurnEventLog>>,
}

pub struct CoderTurnCheckpointControllerParams {
    pub store: Arc<CoderTurnCheckpointStore>,
    pub session_id: String,
    pub daemon_turn_id: String,
    pub agent_mode: String,
    pub contract_revision: String,
    pub provider: String,
    pub model: String,
    pub authoritative_prompt: String,
    pub forge: Arc<Forge>,
    pub lease: ExecutionLease,
    pub entry: Arc<CoderEntryContext>,
    pub registry: Arc<CoderBoundToolRegistry>,
    pub resume_from: Option<ActiveTurnCheckpoint>,
    pub event_log: Option<Arc<medousa_engine::TurnEventLog>>,
}

impl CoderTurnCheckpointController {
    pub fn new(params: CoderTurnCheckpointControllerParams) -> Result<Arc<Self>, String> {
        let now = Utc::now();
        let resume_state = params
            .resume_from
            .clone()
            .map(ActiveTurnCheckpoint::into_resume_state);
        let forge_state = observe_entry_environment(
            &params.forge,
            &params.entry,
            &params.lease,
            params
                .resume_from
                .as_ref()
                .map(|checkpoint| checkpoint.forge.attempt_id.clone()),
        )?;
        let visible_tools = params
            .registry
            .checkpoint_visible_tools()
            .map_err(|err| err.to_string())?;
        let activity_cursor = params
            .registry
            .checkpoint_activity_cursor()
            .map_err(|err| err.to_string())?;
        let agent_id = params
            .registry
            .checkpoint_agent_id()
            .map_err(|err| err.to_string())?;
        let locus_cursor = params
            .registry
            .checkpoint_memory_cursor()
            .map_err(|err| err.to_string())?;
        let checkpoint = ActiveTurnCheckpoint {
            schema_version: ACTIVE_TURN_CHECKPOINT_SCHEMA_VERSION,
            session_id: params.session_id,
            daemon_turn_id: params.daemon_turn_id,
            resumed_from_turn_id: params
                .resume_from
                .as_ref()
                .map(|checkpoint| checkpoint.daemon_turn_id.clone()),
            agent_id,
            agent_mode: params.agent_mode,
            contract_revision: params.contract_revision,
            provider: params.provider,
            model: params.model,
            authoritative_prompt: truncate(&params.authoritative_prompt, MAX_FIELD_CHARS),
            current_goal: resume_state
                .as_ref()
                .map(|state| state.scratch.goal.clone())
                .unwrap_or_else(|| params.authoritative_prompt.clone()),
            status: ActiveTurnCheckpointStatus::Active,
            safe_boundary: SafeCheckpointBoundary::TurnStarted,
            created_at_utc: now,
            updated_at_utc: now,
            counters: resume_state
                .as_ref()
                .map(|state| state.counters.clone())
                .unwrap_or_default(),
            transcript: resume_state
                .as_ref()
                .map(|state| state.transcript.clone())
                .unwrap_or_default(),
            invocations: resume_state
                .as_ref()
                .map(|state| state.invocations.clone())
                .unwrap_or_default(),
            pack_hold_fragments: resume_state
                .as_ref()
                .map(|state| state.pack_hold_fragments.clone())
                .unwrap_or_default(),
            scratch: resume_state
                .as_ref()
                .map(|state| state.scratch.clone())
                .unwrap_or_else(|| TurnScratchpad::from_user_prompt(&params.authoritative_prompt)),
            forge: forge_state,
            activity_cursor,
            locus_cursor,
            visible_tools,
            outstanding_boundary: None,
            last_completed_tool_boundary: params
                .resume_from
                .as_ref()
                .and_then(|checkpoint| checkpoint.last_completed_tool_boundary.clone()),
            termination_reason: None,
            transcript_cursor: None,
            checkpoint_generation: 0,
            required_workspace_generation: 0,
        };
        let controller = Arc::new(Self {
            store: params.store,
            checkpoint: Mutex::new(checkpoint),
            forge: params.forge,
            entry: params.entry,
            registry: params.registry,
            event_log: params.event_log,
        });
        controller.persist_current()?;
        if let Some(source) = params.resume_from.as_ref()
            && source.daemon_turn_id != controller.current_turn_id()?
            && let Err(err) = controller
                .store
                .mark_superseded(source, "continued by a newer fenced Coder turn")
        {
            tracing::warn!(error = %err, "failed to supersede resumed Coder checkpoint");
        }
        Ok(controller)
    }

    pub fn initial_resume_state(
        checkpoint: Option<ActiveTurnCheckpoint>,
    ) -> Option<ActiveTurnResumeState> {
        checkpoint.map(ActiveTurnCheckpoint::into_resume_state)
    }

    fn current_turn_id(&self) -> Result<String, String> {
        self.checkpoint
            .lock()
            .map(|checkpoint| checkpoint.daemon_turn_id.clone())
            .map_err(|err| err.to_string())
    }

    fn attach_transcript_cursor(
        &self,
        checkpoint: &mut ActiveTurnCheckpoint,
    ) -> Result<(), String> {
        let Some(log) = self.event_log.as_ref() else {
            return Ok(());
        };
        let cursor = medousa_engine::TranscriptCursor::from_log(log);
        if cursor.fence > 0 {
            log.ensure_synced_through(cursor.fence).map_err(|error| {
                format!("cannot sync H03 transcript before checkpoint publication: {error}")
            })?;
            cursor
                .verify(log)
                .map_err(|error| format!("H03 transcript cursor failed verification: {error}"))?;
        }
        checkpoint.transcript_cursor = Some(cursor);
        Ok(())
    }

    fn publish_logical_checkpoint(
        &self,
        checkpoint: &mut ActiveTurnCheckpoint,
    ) -> Result<(), String> {
        self.attach_transcript_cursor(checkpoint)?;
        checkpoint.checkpoint_generation = checkpoint.checkpoint_generation.saturating_add(1);
        checkpoint.updated_at_utc = Utc::now();
        self.store.save(checkpoint)
    }

    fn refresh_runtime_metadata(
        &self,
        checkpoint: &mut ActiveTurnCheckpoint,
    ) -> Result<(), String> {
        let lease_attempt = AttemptId::from(checkpoint.forge.attempt_id.clone());
        let work_id = WorkId::from(self.entry.work_id.clone());
        let item = self
            .forge
            .load(&work_id)
            .map_err(|err| format!("cannot refresh checkpoint undertaking: {err}"))?;
        let environment = item
            .environment_for_attempt(&lease_attempt)
            .cloned()
            .ok_or_else(|| "checkpoint attempt environment is no longer active".to_string())?;
        checkpoint.forge = observe_environment(
            &self.forge,
            &environment,
            &work_id,
            &lease_attempt,
            checkpoint.forge.resumed_from_attempt_id.clone(),
        )?;
        checkpoint.activity_cursor = self
            .registry
            .checkpoint_activity_cursor()
            .map_err(|err| err.to_string())?;
        checkpoint.agent_id = self
            .registry
            .checkpoint_agent_id()
            .map_err(|err| err.to_string())?;
        checkpoint.locus_cursor = self
            .registry
            .checkpoint_memory_cursor()
            .map_err(|err| err.to_string())?;
        checkpoint.visible_tools = self
            .registry
            .checkpoint_visible_tools()
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn persist_current(&self) -> Result<(), String> {
        let mut checkpoint = self.checkpoint.lock().map_err(|err| err.to_string())?;
        self.refresh_runtime_metadata(&mut checkpoint)?;
        self.publish_logical_checkpoint(&mut checkpoint)
    }
}

impl ActiveTurnCheckpointSink for CoderTurnCheckpointController {
    fn persist_boundary(&self, state: ToolLoopCheckpointState) -> Result<(), String> {
        let mut checkpoint = self.checkpoint.lock().map_err(|err| err.to_string())?;
        checkpoint.status = state.status;
        checkpoint.safe_boundary = state.boundary;
        checkpoint.counters = state.counters;
        checkpoint.transcript = ActiveTurnTranscript {
            user_lane_prefix: state.user_lane_prefix,
            tool_lane_messages: state.tool_lane_messages,
        };
        checkpoint.invocations = state.invocations;
        checkpoint.pack_hold_fragments = state.pack_hold_fragments;
        checkpoint.current_goal = state.scratch.goal.clone();
        checkpoint.scratch = state.scratch;
        checkpoint.outstanding_boundary = state.outstanding_boundary;
        checkpoint.termination_reason = state.termination_reason;
        if state.boundary == SafeCheckpointBoundary::ToolBatchCompleted {
            checkpoint.last_completed_tool_boundary = Some(CompletedToolBoundary {
                batch_index: checkpoint.counters.tool_batches_completed,
                model_round: checkpoint.counters.model_rounds_executed,
                tool_names: state.tool_names,
                provider_call_ids: state.provider_call_ids,
                activity_cursor: checkpoint.activity_cursor,
                environment_digest: checkpoint.forge.dirty_digest.clone(),
            });
        }
        self.publish_logical_checkpoint(&mut checkpoint)
    }

    fn mark_status(
        &self,
        status: ActiveTurnCheckpointStatus,
        boundary: SafeCheckpointBoundary,
        reason: Option<&str>,
        orchestration: Option<&TurnOrchestrationState>,
    ) -> Result<(), String> {
        let mut checkpoint = self.checkpoint.lock().map_err(|err| err.to_string())?;
        checkpoint.status = status;
        checkpoint.safe_boundary = boundary;
        checkpoint.termination_reason = reason.map(|reason| truncate(reason, 2_000));
        checkpoint.counters.orchestration = orchestration.cloned();
        checkpoint.counters.retry_count = orchestration.map(|state| state.retries).unwrap_or(0);
        self.publish_logical_checkpoint(&mut checkpoint)
    }

    fn latest_safe_resume(&self) -> Result<Option<ActiveTurnResumeState>, String> {
        let checkpoint = self
            .checkpoint
            .lock()
            .map_err(|err| err.to_string())?
            .clone();
        if !checkpoint.status.is_resume_candidate() {
            return Ok(None);
        }
        let mut observed = checkpoint.clone();
        self.refresh_runtime_metadata(&mut observed)?;
        if let Some(reason) = forge_drift_reason(&checkpoint.forge, &observed.forge) {
            return Err(format!("unsafe checkpoint divergence: {reason}"));
        }
        if observed.activity_cursor > checkpoint.activity_cursor {
            let events = self
                .registry
                .engineering_events()
                .map_err(|err| err.to_string())?;
            if events.iter().any(|event| {
                event.revision > checkpoint.activity_cursor
                    && event.agent_id == checkpoint.agent_id
                    && matches!(
                        event.kind,
                        CoderActivityKind::ToolPlanned
                            | CoderActivityKind::ToolCompleted
                            | CoderActivityKind::ToolFailed
                    )
            }) {
                return Err(
                    "unsafe checkpoint divergence: a tool began after the durable boundary".into(),
                );
            }
        }
        let mut resume = checkpoint.into_resume_state();
        // Provider fallback and same-target retry happen inside the same daemon
        // request; the current principal message is already in the checkpoint.
        resume.append_current_user_message = false;
        Ok(Some(resume))
    }

    fn set_model_route(&self, provider: &str, model: &str) -> Result<(), String> {
        let mut checkpoint = self.checkpoint.lock().map_err(|err| err.to_string())?;
        checkpoint.provider = truncate(provider, 200);
        checkpoint.model = truncate(model, 300);
        self.publish_logical_checkpoint(&mut checkpoint)
    }
}

fn observe_entry_environment(
    forge: &Forge,
    entry: &CoderEntryContext,
    lease: &ExecutionLease,
    resumed_from_attempt_id: Option<String>,
) -> Result<CheckpointForgeState, String> {
    let item = forge
        .load(&lease.work_id)
        .map_err(|err| format!("cannot load Coder undertaking for checkpoint: {err}"))?;
    let environment = item
        .environment_for_attempt(&lease.attempt_id)
        .ok_or_else(|| "Coder checkpoint attempt has no governed environment".to_string())?;
    if environment.worktree != entry.worktree {
        let expected = std::fs::canonicalize(&environment.worktree)
            .map_err(|err| format!("cannot canonicalize checkpoint environment: {err}"))?;
        if expected != entry.worktree {
            return Err(
                "Coder checkpoint entry is not bound to the active Forge environment".into(),
            );
        }
    }
    observe_environment(
        forge,
        environment,
        &lease.work_id,
        &lease.attempt_id,
        resumed_from_attempt_id,
    )
}

fn observe_environment(
    forge: &Forge,
    environment: &GovernedEnv,
    work_id: &WorkId,
    attempt_id: &AttemptId,
    resumed_from_attempt_id: Option<String>,
) -> Result<CheckpointForgeState, String> {
    let worktree = std::fs::canonicalize(&environment.worktree)
        .map_err(|err| format!("cannot resolve checkpoint worktree: {err}"))?;
    let actual_root = forge
        .git()
        .worktree_root(&worktree)
        .and_then(|root| std::fs::canonicalize(root).map_err(medousa_forge::ForgeError::Io))
        .map_err(|err| format!("cannot verify checkpoint worktree: {err}"))?;
    if actual_root != worktree {
        return Err("checkpoint worktree root no longer matches Forge authority".into());
    }
    let capture_source = forge.watcher_fence().bind(
        u64::from(environment.generation),
        u64::from(environment.generation),
    );
    let observation = forge
        .observer()
        .observe_exact(forge.git(), work_id, &worktree, &capture_source, true)
        .map_err(|err| format!("cannot observe checkpoint worktree: {err}"))?;
    if observation.completeness != ObservationCompleteness::Exact {
        return Err(format!(
            "workspace observation is {:?}: {}",
            observation.completeness,
            observation.limits_hit.join(", ")
        ));
    }
    let branch = observation
        .branch
        .ok_or_else(|| "checkpoint worktree is detached".to_string())?;
    let head_oid = observation
        .head_oid
        .ok_or_else(|| "cannot read checkpoint HEAD".to_string())?;
    Ok(CheckpointForgeState {
        work_id: work_id.to_string(),
        attempt_id: attempt_id.to_string(),
        resumed_from_attempt_id,
        worktree: worktree.to_string_lossy().into_owned(),
        branch,
        environment_generation: environment.generation,
        head_oid,
        dirty: !observation.changed_paths.is_empty(),
        dirty_digest: observation.dirty_digest,
        changed_paths: observation
            .changed_paths
            .into_iter()
            .map(|path| truncate(&path, 512))
            .take(MAX_CHANGED_PATHS)
            .collect(),
    })
}

fn forge_drift_reason(
    expected: &CheckpointForgeState,
    observed: &CheckpointForgeState,
) -> Option<String> {
    let comparisons = [
        (
            "worktree",
            expected.worktree.as_str(),
            observed.worktree.as_str(),
        ),
        ("branch", expected.branch.as_str(), observed.branch.as_str()),
        (
            "HEAD",
            expected.head_oid.as_str(),
            observed.head_oid.as_str(),
        ),
        (
            "dirty fingerprint",
            expected.dirty_digest.as_str(),
            observed.dirty_digest.as_str(),
        ),
    ];
    for (label, expected, observed) in comparisons {
        if expected != observed {
            return Some(format!("{label} changed after the last safe boundary"));
        }
    }
    (expected.environment_generation != observed.environment_generation)
        .then(|| "Forge environment generation changed after the last safe boundary".to_string())
}

fn validate_checkpoint(
    checkpoint: &ActiveTurnCheckpoint,
    session_id: &str,
    work_id: &str,
) -> Result<(), String> {
    if checkpoint.schema_version != ACTIVE_TURN_CHECKPOINT_SCHEMA_VERSION {
        return Err("unsupported Coder turn checkpoint schema".into());
    }
    if checkpoint.session_id != session_id || checkpoint.forge.work_id != work_id {
        return Err("Coder turn checkpoint scope mismatch".into());
    }
    if checkpoint.daemon_turn_id.trim().is_empty()
        || checkpoint.forge.attempt_id.trim().is_empty()
        || checkpoint.forge.worktree.trim().is_empty()
    {
        return Err("Coder turn checkpoint is missing required identity".into());
    }
    Ok(())
}

fn bound_checkpoint(checkpoint: &mut ActiveTurnCheckpoint) {
    checkpoint.counters.max_tool_rounds = checkpoint
        .counters
        .max_tool_rounds
        .min(super::turn_loop_settings::DEFAULT_CODER_MAX_TOOL_ROUNDS);
    checkpoint.authoritative_prompt =
        bounded_checkpoint_text(&checkpoint.authoritative_prompt, MAX_FIELD_CHARS);
    checkpoint.current_goal = bounded_checkpoint_text(&checkpoint.current_goal, 4_000);
    checkpoint.counters.last_response_preview = checkpoint
        .counters
        .last_response_preview
        .as_deref()
        .map(|preview| bounded_checkpoint_text(preview, 2_000));
    if let Some(state) = checkpoint.counters.orchestration.as_mut() {
        state.final_mode = bounded_checkpoint_text(&state.final_mode, 256);
    }
    checkpoint.termination_reason = checkpoint
        .termination_reason
        .as_deref()
        .map(|reason| bounded_checkpoint_text(reason, 2_000));
    checkpoint.visible_tools.sort();
    checkpoint.visible_tools.dedup();
    checkpoint.visible_tools = checkpoint
        .visible_tools
        .iter()
        .map(|name| truncate(name, 256))
        .collect();
    checkpoint.visible_tools.truncate(MAX_VISIBLE_TOOLS);
    checkpoint.forge.changed_paths = checkpoint
        .forge
        .changed_paths
        .iter()
        .map(|path| truncate(path, 4_000))
        .take(MAX_CHANGED_PATHS)
        .collect();
    checkpoint.pack_hold_fragments = checkpoint
        .pack_hold_fragments
        .iter()
        .map(|fragment| bounded_checkpoint_text(fragment, MAX_MESSAGE_PART_CHARS))
        .take(4)
        .collect();
    checkpoint.invocations.iter_mut().for_each(|invocation| {
        invocation.tool_name = truncate(&invocation.tool_name, 256);
        invocation.tool_input = bounded_json_value(&invocation.tool_input);
        invocation.tool_output = bounded_json_value(&invocation.tool_output);
    });
    checkpoint.invocations = checkpoint
        .invocations
        .iter()
        .rev()
        .take(MAX_INVOCATIONS)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    evict_front_to_exact_budget(&mut checkpoint.invocations, MAX_INVOCATION_BYTES);
    checkpoint.transcript.user_lane_prefix = bounded_messages(
        &checkpoint.transcript.user_lane_prefix,
        MAX_TRANSCRIPT_MESSAGES / 2,
        MAX_TRANSCRIPT_BYTES / 3,
    );
    checkpoint.transcript.tool_lane_messages = bounded_messages(
        &checkpoint.transcript.tool_lane_messages,
        MAX_TRANSCRIPT_MESSAGES,
        MAX_TRANSCRIPT_BYTES,
    );
    redact_scratch(&mut checkpoint.scratch);
    if let Some(boundary) = checkpoint.outstanding_boundary.as_mut() {
        match boundary {
            OutstandingTurnBoundary::UserInput { reason } => {
                *reason = bounded_checkpoint_text(reason, 2_000);
            }
            OutstandingTurnBoundary::BudgetApproval { request_id, .. } => {
                *request_id = truncate(request_id, 512);
            }
        }
    }
    if let Some(boundary) = checkpoint.last_completed_tool_boundary.as_mut() {
        boundary.tool_names = boundary
            .tool_names
            .iter()
            .map(|name| truncate(name, 256))
            .take(64)
            .collect();
        boundary.provider_call_ids = boundary
            .provider_call_ids
            .iter()
            .map(|id| truncate(id, 512))
            .take(64)
            .collect();
    }
}

fn bounded_messages(
    messages: &[ChatMessage],
    max_messages: usize,
    max_bytes: usize,
) -> Vec<ChatMessage> {
    let mut bounded = messages
        .iter()
        .map(bounded_chat_message)
        .collect::<Vec<_>>();
    if bounded.len() > max_messages {
        bounded.drain(0..bounded.len() - max_messages);
    }
    evict_front_to_exact_budget(&mut bounded, max_bytes);
    strip_orphaned_tool_responses(&mut bounded);
    bounded
}

fn strip_orphaned_tool_responses(messages: &mut Vec<ChatMessage>) {
    while messages
        .first()
        .is_some_and(|message| message.role == genai::chat::ChatRole::Tool)
    {
        messages.remove(0);
    }
}

fn bounded_chat_message(message: &ChatMessage) -> ChatMessage {
    let keep_tool_turn_reasoning = message
        .content
        .parts()
        .iter()
        .any(ContentPart::is_tool_call);
    let parts = message
        .content
        .parts()
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text(text) => Some(ContentPart::Text(bounded_checkpoint_text(
                text,
                MAX_MESSAGE_PART_CHARS,
            ))),
            ContentPart::ToolCall(call) => Some(ContentPart::ToolCall(ToolCall {
                call_id: truncate(&call.call_id, 512),
                fn_name: truncate(&call.fn_name, 256),
                fn_arguments: bounded_json_value(&call.fn_arguments),
                thought_signatures: call.thought_signatures.as_ref().map(|signatures| {
                    signatures
                        .iter()
                        .map(|signature| truncate(signature, 8_000))
                        .take(8)
                        .collect()
                }),
            })),
            ContentPart::ToolResponse(response) => {
                let content = serde_json::from_str::<Value>(&response.content)
                    .map(|value| bounded_json_value(&value).to_string())
                    .unwrap_or_else(|_| {
                        bounded_checkpoint_text(&response.content, MAX_MESSAGE_PART_CHARS)
                    });
                Some(ContentPart::ToolResponse(ToolResponse {
                    call_id: truncate(&response.call_id, 512),
                    fn_name: response.fn_name.as_deref().map(|name| truncate(name, 256)),
                    content,
                }))
            }
            ContentPart::ThoughtSignature(signature) => {
                Some(ContentPart::ThoughtSignature(truncate(signature, 8_000)))
            }
            // Tool-call turns must keep verbatim thinking-mode CoT so DeepSeek
            // resume does not 400. Reasoning-only assistant turns are not
            // required on later requests and stay omitted from durable storage.
            ContentPart::ReasoningContent(text) if keep_tool_turn_reasoning => {
                Some(ContentPart::ReasoningContent(text.clone()))
            }
            ContentPart::ReasoningContent(_) => None,
            ContentPart::Binary(_) => Some(ContentPart::Text(
                "[binary content omitted from durable Coder checkpoint]".into(),
            )),
            ContentPart::Custom(_) => Some(ContentPart::Text(
                "[custom provider content omitted from durable Coder checkpoint]".into(),
            )),
        })
        .collect::<Vec<_>>();
    ChatMessage {
        role: message.role.clone(),
        content: MessageContent::from_parts(parts),
        options: message.options.clone(),
    }
}

fn bounded_json_value(value: &Value) -> Value {
    let redacted = redact_checkpoint_json_value(&crate::settings_guard::redact_json_value(value));
    if serialized_size(&redacted) <= MAX_JSON_VALUE_BYTES {
        return redacted;
    }
    json!({
        "checkpoint_truncated": true,
        "sha256": full_digest(&redacted.to_string()),
        "logical_bytes": serialized_size(&redacted),
    })
}

fn redact_checkpoint_json_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), redact_checkpoint_json_value(value)))
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.iter().map(redact_checkpoint_json_value).collect())
        }
        Value::String(value) => Value::String(super::coder_evidence::redact_evidence_text(value)),
        _ => value.clone(),
    }
}

fn redact_scratch(scratch: &mut TurnScratchpad) {
    scratch.goal = bounded_checkpoint_text(&scratch.goal, MAX_FIELD_CHARS);
    scratch.last_tools = scratch
        .last_tools
        .iter()
        .map(|name| truncate(name, 256))
        .take(MAX_VISIBLE_TOOLS)
        .collect();
    scratch.last_error = scratch
        .last_error
        .as_deref()
        .map(|error| bounded_checkpoint_text(error, 2_000));
    scratch.open_gaps = scratch
        .open_gaps
        .iter()
        .map(|gap| bounded_checkpoint_text(gap, 2_000))
        .take(32)
        .collect();
    scratch.round_digests = scratch
        .round_digests
        .iter()
        .map(|digest| bounded_checkpoint_text(digest, 2_000))
        .take(12)
        .collect();
    scratch.tools_this_turn = scratch
        .tools_this_turn
        .iter()
        .map(|name| truncate(name, 256))
        .take(MAX_VISIBLE_TOOLS)
        .collect();
    scratch.working_notes = scratch
        .working_notes
        .iter()
        .map(|note| bounded_checkpoint_text(note, 2_000))
        .take(5)
        .collect();
    if let Some(delegate) = scratch.delegate.as_mut() {
        delegate.work_id = truncate(&delegate.work_id, 512);
        delegate.intent = bounded_checkpoint_text(&delegate.intent, 2_000);
    }
}

fn bounded_checkpoint_text(value: &str, max_chars: usize) -> String {
    truncate(
        &super::coder_evidence::redact_evidence_text(value),
        max_chars,
    )
}

fn serialized_size(value: &impl Serialize) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

fn evict_prefix_to_budget(checkpoint: &mut ActiveTurnCheckpoint) {
    evict_front_to_exact_budget(
        &mut checkpoint.transcript.tool_lane_messages,
        MAX_TRANSCRIPT_BYTES,
    );
    strip_orphaned_tool_responses(&mut checkpoint.transcript.tool_lane_messages);
    evict_front_to_exact_budget(
        &mut checkpoint.transcript.user_lane_prefix,
        MAX_TRANSCRIPT_BYTES / 3,
    );
}

fn evict_front_to_exact_budget<T: Serialize>(items: &mut Vec<T>, max_bytes: usize) {
    if items.is_empty() {
        return;
    }
    let mut sizes: Vec<usize> = items.iter().map(serialized_size).collect();
    let mut total: usize = sizes.iter().copied().sum();
    let mut drop_count = 0;
    while total > max_bytes && drop_count + 1 < items.len() {
        total = total.saturating_sub(sizes[drop_count]);
        drop_count += 1;
    }
    if drop_count > 0 {
        items.drain(0..drop_count);
        sizes.drain(0..drop_count);
        let _ = sizes;
    }
}

fn checkpoint_body_digest(checkpoint: &ActiveTurnCheckpoint) -> String {
    format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(checkpoint).unwrap_or_default())
    )
}

fn logical_body_digest(body: &LogicalCheckpointBody) -> String {
    format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(body).unwrap_or_default())
    )
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect()
    }
}

fn checkpoint_path(turn_id: &str) -> crate::store_root::StorePath {
    crate::session_storage::session_object_path(CHECKPOINT_OBJECT_DOMAIN, turn_id, "json")
}

fn checkpoint_journal_path(turn_id: &str) -> crate::store_root::StorePath {
    crate::session_storage::session_object_path(CHECKPOINT_JOURNAL_DOMAIN, turn_id, "jsonl")
}

fn checkpoint_legacy_session_directory(
    session_id: &crate::session_storage::SessionId,
) -> crate::store_root::StorePath {
    crate::store_root::StorePath::parse(&short_digest(session_id.as_str()))
        .expect("legacy checkpoint digest must be a valid store path")
}

fn short_digest(value: &str) -> String {
    full_digest(value)[..24].to_string()
}

fn full_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use medousa_forge::model::{ExecutorDescriptor, RecoveryDisposition};
    use tempfile::tempdir;

    use super::*;

    fn session_id(value: &str) -> crate::session_storage::SessionId {
        crate::session_storage::SessionId::parse(value).unwrap()
    }

    fn checkpoint(session: &str, turn: &str, work: &str) -> ActiveTurnCheckpoint {
        let now = Utc::now();
        ActiveTurnCheckpoint {
            schema_version: ACTIVE_TURN_CHECKPOINT_SCHEMA_VERSION,
            session_id: session.into(),
            daemon_turn_id: turn.into(),
            resumed_from_turn_id: None,
            agent_id: format!("coder:{session}:{turn}"),
            agent_mode: "coder".into(),
            contract_revision: "coder-v3".into(),
            provider: "test".into(),
            model: "test-model".into(),
            authoritative_prompt: "fix it".into(),
            current_goal: "fix it".into(),
            status: ActiveTurnCheckpointStatus::Active,
            safe_boundary: SafeCheckpointBoundary::ToolBatchCompleted,
            created_at_utc: now,
            updated_at_utc: now,
            counters: ActiveTurnCounters {
                model_rounds_executed: 4,
                max_tool_rounds: 100,
                ..Default::default()
            },
            transcript: ActiveTurnTranscript {
                user_lane_prefix: vec![ChatMessage::user("fix it")],
                tool_lane_messages: vec![ChatMessage::assistant("working")],
            },
            invocations: Vec::new(),
            pack_hold_fragments: Vec::new(),
            scratch: TurnScratchpad::from_user_prompt("fix it"),
            forge: CheckpointForgeState {
                work_id: work.into(),
                attempt_id: "att-test".into(),
                resumed_from_attempt_id: None,
                worktree: "/tmp/worktree".into(),
                branch: "worktree/test".into(),
                environment_generation: 1,
                head_oid: "abc".into(),
                dirty: true,
                dirty_digest: "digest".into(),
                changed_paths: vec!["src/lib.rs".into()],
            },
            activity_cursor: 7,
            locus_cursor: Some("node-7".into()),
            visible_tools: vec![crate::public_api::COGNITION_STORE_READ.into()],
            outstanding_boundary: None,
            last_completed_tool_boundary: None,
            termination_reason: None,
            transcript_cursor: None,
            checkpoint_generation: 0,
            required_workspace_generation: 0,
        }
    }

    fn interrupted_attempt_checkpoint(
        session: &str,
    ) -> (
        tempfile::TempDir,
        tempfile::TempDir,
        Forge,
        WorkId,
        ActiveTurnCheckpoint,
    ) {
        let repo = tempdir().expect("repo");
        let forge_root = tempdir().expect("forge root");
        let initialized = std::process::Command::new("git")
            .args(["init", "-b", "main", "--template="])
            .current_dir(repo.path())
            .status()
            .expect("git init");
        assert!(initialized.success());
        std::fs::write(repo.path().join("README.md"), "initial\n").expect("seed repo");
        let staged = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo.path())
            .status()
            .expect("git add");
        assert!(staged.success());
        GitEngine::detect()
            .expect("git")
            .commit_checkpoint(repo.path(), "initial", &CheckpointAuthor::default())
            .expect("initial commit");

        let forge = Forge::open(forge_root.path()).expect("forge");
        let item = forge
            .register(
                "Recover exact turn",
                "Preserve a live Coder attempt",
                repo.path(),
                "main",
                "user-test",
                &Forge::system_actor(),
            )
            .expect("register");
        let item = forge
            .provision(&item.id, &Forge::system_actor())
            .expect("provision");
        let work_id = item.id.clone();
        let (item, lease) = forge
            .begin_isolated_attempt(
                &work_id,
                ExecutorDescriptor {
                    kind: "medousa-coder".into(),
                    detail: Value::Null,
                },
                None,
                &Forge::system_actor(),
            )
            .expect("begin attempt");
        let environment = item
            .environment_for_attempt(&lease.attempt_id)
            .expect("attempt environment");
        let forge_state =
            observe_environment(&forge, environment, &work_id, &lease.attempt_id, None)
                .expect("observe environment");
        let mut checkpoint = checkpoint(session, "turn-recover", work_id.as_str());
        checkpoint.forge = forge_state;
        checkpoint.agent_id = format!("coder:{session}:turn-recover");
        checkpoint.activity_cursor = 0;
        forge
            .interrupt_attempt(
                &lease,
                RecoveryDisposition::RestartAllowed,
                &Forge::system_actor(),
            )
            .expect("interrupt attempt");
        (repo, forge_root, forge, work_id, checkpoint)
    }

    #[test]
    fn persists_and_loads_newest_nonterminal_checkpoint() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        let mut older = checkpoint("session-a", "turn-1", "work-a");
        older.updated_at_utc = Utc::now() - chrono::Duration::seconds(2);
        store.save(&older).unwrap();
        let newer = checkpoint("session-a", "turn-2", "work-a");
        store.save(&newer).unwrap();

        let loaded = store
            .load_latest_resume_candidate("session-a", "work-a")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.daemon_turn_id, "turn-2");
        assert_eq!(loaded.counters.model_rounds_executed, 4);
    }

    #[test]
    fn completed_and_superseded_checkpoints_are_not_resumed() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        let mut completed = checkpoint("session-a", "turn-1", "work-a");
        completed.status = ActiveTurnCheckpointStatus::Completed;
        store.save(&completed).unwrap();
        assert!(
            store
                .load_latest_resume_candidate("session-a", "work-a")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn terminal_history_cannot_crowd_out_a_resumable_checkpoint() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        let mut resumable = checkpoint("session-crowded", "turn-live", "work-crowded");
        resumable.updated_at_utc = Utc::now() - chrono::Duration::hours(1);
        store.save(&resumable).unwrap();

        for index in 0..300 {
            let mut terminal = checkpoint(
                "session-crowded",
                &format!("turn-terminal-{index}"),
                "work-crowded",
            );
            terminal.status = ActiveTurnCheckpointStatus::Completed;
            terminal.safe_boundary = SafeCheckpointBoundary::Terminal;
            store.save(&terminal).unwrap();
        }

        let loaded = store
            .load_latest_resume_candidate("session-crowded", "work-crowded")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.daemon_turn_id, "turn-live");
    }

    #[test]
    fn user_boundary_keeps_context_but_resets_turn_counters_and_invocations() {
        let mut source = checkpoint("session-a", "turn-1", "work-a");
        source.status = ActiveTurnCheckpointStatus::AwaitingUser;
        source.invocations.push(CheckpointToolInvocation {
            tool_name: "cognition_turn_checkpoint".into(),
            tool_input: Value::Null,
            tool_output: Value::Null,
        });
        let resumed = source.into_resume_state();
        assert!(!resumed.restore_turn_budget);
        assert_eq!(resumed.counters.model_rounds_executed, 0);
        assert!(resumed.invocations.is_empty());
        assert_eq!(resumed.transcript.tool_lane_messages.len(), 1);
    }

    #[test]
    fn interrupted_active_turn_restores_remaining_budget() {
        let source = checkpoint("session-a", "turn-1", "work-a");
        let resumed = source.into_resume_state();
        assert!(resumed.restore_turn_budget);
        assert_eq!(resumed.counters.model_rounds_executed, 4);
        assert_eq!(resumed.counters.max_tool_rounds, 100);
    }

    #[test]
    fn restored_checkpoint_cannot_exceed_the_coder_hard_ceiling() {
        let mut source = checkpoint("session-a", "turn-1", "work-a");
        source.counters.max_tool_rounds = 500;
        let resumed = source.into_resume_state();
        assert_eq!(resumed.counters.max_tool_rounds, 100);
    }

    #[test]
    fn transcript_bounding_drops_reasoning_and_large_binary_like_payloads() {
        let message = ChatMessage::assistant(MessageContent::from_parts(vec![
            ContentPart::ReasoningContent("private".into()),
            ContentPart::Text("x".repeat(MAX_MESSAGE_PART_CHARS + 100)),
        ]));
        let bounded = bounded_chat_message(&message);
        assert!(
            bounded
                .content
                .parts()
                .iter()
                .all(|part| !matches!(part, ContentPart::ReasoningContent(_)))
        );
        assert!(bounded.content.size() <= MAX_MESSAGE_PART_CHARS);
    }

    #[test]
    fn transcript_bounding_keeps_tool_call_reasoning_content() {
        let message = ChatMessage::assistant(MessageContent::from_parts(vec![
            ContentPart::ReasoningContent("I should inspect the file first.".into()),
            ContentPart::ToolCall(ToolCall {
                call_id: "call_keep".into(),
                fn_name: "cognition_workshop_query".into(),
                fn_arguments: json!({ "action": "workshop.status" }),
                thought_signatures: None,
            }),
        ]));
        let bounded = bounded_chat_message(&message);
        assert_eq!(
            bounded.content.joined_reasoning_content().as_deref(),
            Some("I should inspect the file first.")
        );
        assert!(bounded.content.contains_tool_call());
    }

    #[test]
    fn durable_checkpoint_redacts_secret_markers_across_text_and_json() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        let mut durable = checkpoint("session-redact", "turn-redact", "work-redact");
        durable.authoritative_prompt = "api_key=prompt-secret continue".into();
        durable.current_goal = "password=goal-secret finish".into();
        durable.counters.last_response_preview = Some("token=preview-secret done".into());
        durable.pack_hold_fragments = vec!["secret=pack-secret wait".into()];
        durable.scratch.goal = "auth_token=scratch-secret resume".into();
        durable.scratch.working_notes = vec!["access_token=note-secret later".into()];
        durable.transcript.user_lane_prefix = vec![ChatMessage::user(
            "Authorization: Bearer transcript-secret\ncontinue",
        )];
        durable.transcript.tool_lane_messages = vec![
            ChatMessage::assistant(MessageContent::from_parts(vec![ContentPart::ToolCall(
                ToolCall {
                    call_id: "call-redact".into(),
                    fn_name: "cognition_shell_run".into(),
                    fn_arguments: json!({}),
                    thought_signatures: None,
                },
            )])),
            ChatMessage::tool(MessageContent::from_parts(vec![ContentPart::ToolResponse(
                ToolResponse {
                    call_id: "call-redact".into(),
                    fn_name: Some("cognition_shell_run".into()),
                    content: "x-api-key: response-secret\nok=true".into(),
                },
            )])),
        ];
        durable.invocations = vec![CheckpointToolInvocation {
            tool_name: "cognition_shell_run".into(),
            tool_input: json!({"password": "input-secret"}),
            tool_output: json!({"message": "token=output-secret done"}),
        }];

        store.save(&durable).unwrap();
        let raw = store
            .files
            .read(
                &session_id("session-redact"),
                &checkpoint_path("turn-redact"),
            )
            .map(|bytes| String::from_utf8(bytes).unwrap())
            .unwrap();
        for secret in [
            "prompt-secret",
            "goal-secret",
            "preview-secret",
            "pack-secret",
            "scratch-secret",
            "note-secret",
            "transcript-secret",
            "response-secret",
            "input-secret",
            "output-secret",
        ] {
            assert!(!raw.contains(secret), "checkpoint leaked {secret}");
        }
        assert!(raw.contains("[REDACTED]"));
    }

    #[test]
    fn scope_mismatch_cannot_load_another_worktree_checkpoint() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        store
            .save(&checkpoint("session-a", "turn-1", "work-a"))
            .unwrap();
        assert!(
            store
                .load_latest_resume_candidate("session-a", "work-b")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn checkpoint_files_require_typed_sessions_and_hide_turn_ids() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        assert!(
            store
                .save(&checkpoint("private/session", "turn:secret", "work-secret"))
                .is_err()
        );
        let id = session_id("private-session");
        let path = checkpoint_path("turn:secret");
        assert!(!path.file_name().contains(id.as_str()));
        assert!(!path.file_name().contains("turn:secret"));
        assert!(path.file_name().starts_with("o1-"));
        assert!(path.file_name().ends_with(".json"));
    }

    #[test]
    fn checkpoint_write_migrates_the_legacy_truncated_digest_directory() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        let id = session_id("session-migrate");
        let legacy = temp
            .path()
            .join(checkpoint_legacy_session_directory(&id).file_name());
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("canary.json"), b"{}").unwrap();

        store
            .save(&checkpoint(id.as_str(), "turn-migrate", "work-migrate"))
            .unwrap();

        let current = crate::session_storage::session_dir(temp.path(), &id);
        assert!(current.join("canary.json").is_file());
        assert!(!legacy.exists());
    }

    #[test]
    fn full_key_terminal_snapshot_shadows_the_same_legacy_active_turn() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        let id = session_id("session-shadow");
        let legacy_dir = temp
            .path()
            .join(checkpoint_legacy_session_directory(&id).file_name());
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let active = checkpoint(id.as_str(), "turn-shadow", "work-shadow");
        std::fs::write(
            legacy_dir.join(format!("{}.json", short_digest(&active.daemon_turn_id))),
            serde_json::to_vec_pretty(&active).unwrap(),
        )
        .unwrap();

        assert!(
            store
                .load_latest_resume_candidate(id.as_str(), "work-shadow")
                .unwrap()
                .is_some()
        );
        store
            .mark_superseded(&active, "continued by a newer turn")
            .unwrap();
        assert!(
            store
                .load_latest_resume_candidate(id.as_str(), "work-shadow")
                .unwrap()
                .is_none()
        );
    }

    #[cfg(unix)]
    #[test]
    fn checkpoint_store_rejects_link_backed_session_directory() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let root = temp.path().join("checkpoints");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("canary"), b"safe").unwrap();
        let id = session_id("session-linked");
        symlink(
            &outside,
            root.join(crate::session_storage::StorageKey::for_session(&id).as_str()),
        )
        .unwrap();
        let store = CoderTurnCheckpointStore::open(root);

        assert!(
            store
                .save(&checkpoint(id.as_str(), "turn-linked", "work-linked"))
                .is_err()
        );
        assert!(store.delete_session(&id).is_err());
        assert_eq!(std::fs::read(outside.join("canary")).unwrap(), b"safe");
    }

    #[test]
    fn checkpoint_delete_removes_current_and_legacy_layouts() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        let id = session_id("session-delete");
        let current = crate::session_storage::session_dir(temp.path(), &id);
        let legacy = temp
            .path()
            .join(checkpoint_legacy_session_directory(&id).file_name());
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&legacy).unwrap();

        store.delete_session(&id).unwrap();

        assert!(!current.exists());
        assert!(!legacy.exists());
    }

    #[test]
    fn deep_turn_checkpoint_is_bounded_before_atomic_write() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        let mut deep = checkpoint("session-deep", "turn-deep", "work-deep");
        deep.transcript.tool_lane_messages = (0..300)
            .map(|index| ChatMessage::system(format!("round={index} {}", "x".repeat(8_000))))
            .collect();
        deep.invocations = (0..200)
            .map(|index| CheckpointToolInvocation {
                tool_name: format!("tool-{index}"),
                tool_input: json!({"payload": "i".repeat(20_000)}),
                tool_output: json!({"payload": "o".repeat(20_000)}),
            })
            .collect();
        store.save(&deep).unwrap();
        let raw = store
            .files
            .read(&session_id("session-deep"), &checkpoint_path("turn-deep"))
            .unwrap();
        assert!(raw.len() as u64 <= MAX_CHECKPOINT_BYTES);
        assert!(
            store
                .load_latest_resume_candidate("session-deep", "work-deep")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn budget_exhaustion_is_a_typed_resume_boundary() {
        assert!(ActiveTurnCheckpointStatus::BudgetExhausted.is_resume_candidate());
        assert!(!ActiveTurnCheckpointStatus::BudgetExhausted.restores_interrupted_budget());
        assert_eq!(
            TOOL_ROUND_BUDGET_EXHAUSTED_REASON,
            "tool_round_budget_exhausted"
        );
    }

    #[test]
    fn recovery_plan_reopens_an_exact_interrupted_environment() {
        let (_repo, _forge_root, forge, work_id, checkpoint) =
            interrupted_attempt_checkpoint("session-exact");
        let checkpoint_root = tempdir().unwrap();
        let activity_root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(checkpoint_root.path());
        let activity = CoderActivityStore::open(activity_root.path().join("activity.json"));
        store.save(&checkpoint).unwrap();

        let plan =
            plan_coder_recovery(&store, &forge, &activity, "session-exact", &work_id).unwrap();
        assert!(matches!(plan, CoderRecoveryPlan::Exact(_)));
    }

    #[test]
    fn worktree_drift_downgrades_exact_recovery_without_deleting_changes() {
        let (_repo, _forge_root, forge, work_id, checkpoint) =
            interrupted_attempt_checkpoint("session-drift");
        std::fs::write(
            PathBuf::from(&checkpoint.forge.worktree).join("unfinished.rs"),
            "pub fn unfinished() {}\n",
        )
        .unwrap();
        let checkpoint_root = tempdir().unwrap();
        let activity_root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(checkpoint_root.path());
        let activity = CoderActivityStore::open(activity_root.path().join("activity.json"));
        store.save(&checkpoint).unwrap();

        let plan =
            plan_coder_recovery(&store, &forge, &activity, "session-drift", &work_id).unwrap();
        assert!(matches!(
            plan,
            CoderRecoveryPlan::Semantic { ref reason, .. }
                if reason.contains("dirty fingerprint")
        ));
        assert!(
            PathBuf::from(&checkpoint.forge.worktree)
                .join("unfinished.rs")
                .exists()
        );
    }

    #[test]
    fn content_drift_inside_an_already_dirty_path_downgrades_exact_recovery() {
        let (_repo, _forge_root, forge, work_id, mut checkpoint) =
            interrupted_attempt_checkpoint("session-content-drift");
        let path = PathBuf::from(&checkpoint.forge.worktree).join("README.md");
        std::fs::write(&path, "first dirty state\n").unwrap();
        let source_attempt = AttemptId::from(checkpoint.forge.attempt_id.clone());
        let item = forge.load(&work_id).unwrap();
        let environment = item
            .environment_for_attempt(&source_attempt)
            .expect("preserved environment");
        checkpoint.forge =
            observe_environment(&forge, environment, &work_id, &source_attempt, None).unwrap();
        std::fs::write(&path, "other dirty state\n").unwrap();

        let checkpoint_root = tempdir().unwrap();
        let activity_root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(checkpoint_root.path());
        let activity = CoderActivityStore::open(activity_root.path().join("activity.json"));
        store.save(&checkpoint).unwrap();

        let plan =
            plan_coder_recovery(&store, &forge, &activity, "session-content-drift", &work_id)
                .unwrap();
        assert!(matches!(
            plan,
            CoderRecoveryPlan::Semantic { ref reason, .. }
                if reason.contains("dirty fingerprint")
        ));
    }

    #[test]
    fn post_checkpoint_tool_start_never_replays_as_exact() {
        let (_repo, _forge_root, forge, work_id, mut checkpoint) =
            interrupted_attempt_checkpoint("session-uncertain");
        let checkpoint_root = tempdir().unwrap();
        let activity_root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(checkpoint_root.path());
        let activity = CoderActivityStore::open(activity_root.path().join("activity.json"));
        let identity = super::super::coder_activity::CoderAgentIdentity::for_turn(
            "session-uncertain",
            "turn-recover",
            &checkpoint.forge.attempt_id,
        );
        checkpoint.agent_id = identity.agent_id.clone();
        activity
            .register_agent(work_id.as_str(), &identity)
            .expect("register activity");
        checkpoint.activity_cursor = activity
            .snapshot(work_id.as_str(), &identity.agent_id)
            .unwrap()
            .revision;
        store.save(&checkpoint).unwrap();
        activity
            .begin_tool(
                work_id.as_str(),
                &identity,
                "cognition_code_apply_patch",
                "apply the fix",
                vec!["file://src/lib.rs".into()],
                Vec::new(),
            )
            .expect("plan tool");

        let plan =
            plan_coder_recovery(&store, &forge, &activity, "session-uncertain", &work_id).unwrap();
        assert!(matches!(
            plan,
            CoderRecoveryPlan::Semantic { ref reason, .. }
                if reason.contains("tool start")
        ));
    }

    #[test]
    fn cm011_checkpoint_generations_are_monotonic() {
        let root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(root.path());
        let mut first = checkpoint("session-cm011", "turn-cm011", "work-cm011");
        first.checkpoint_generation = 1;
        first.status = ActiveTurnCheckpointStatus::Active;
        store.save(&first).unwrap();
        let mut second = first.clone();
        second.checkpoint_generation = 2;
        second.updated_at_utc = first.updated_at_utc + chrono::Duration::seconds(1);
        store.save(&second).unwrap();
        let loaded = store
            .load_latest_resume_candidate("session-cm011", "work-cm011")
            .unwrap()
            .expect("checkpoint");
        assert_eq!(loaded.checkpoint_generation, 2);
        assert!(loaded.checkpoint_generation > first.checkpoint_generation);
    }

    #[test]
    fn restart_recovers_logical_checkpoint_from_journal_when_head_missing() {
        let root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(root.path());
        let mut first = checkpoint("session-restart", "turn-restart", "work-restart");
        first.checkpoint_generation = 1;
        first.transcript_cursor = Some(medousa_engine::TranscriptCursor {
            turn_id: "turn-restart".into(),
            journal_seq: 2,
            fence: 2,
            digest: "sha256:abc".into(),
        });
        store.save(&first).unwrap();
        let session = session_id("session-restart");
        let head = checkpoint_path("turn-restart");
        store.files.remove_file(&session, &head).unwrap();

        let recovered = store
            .recover_from_journal("session-restart", "turn-restart")
            .unwrap()
            .expect("journal fold");
        assert_eq!(recovered.checkpoint_generation, 1);
        assert_eq!(
            recovered.transcript_cursor.as_ref().map(|c| c.fence),
            Some(2)
        );
    }

    #[test]
    fn advanced_log_verification_survives_later_h03_events() {
        let root = tempdir().unwrap();
        let log_root = root.path().join("turn_log");
        std::fs::create_dir_all(&log_root).unwrap();
        let envelope = medousa_engine::TurnEnvelope::new(
            "turn-advanced-cursor",
            medousa_engine::Principal::operator(),
        );
        let log = medousa_engine::TurnEventLog::open_in(&log_root, envelope).unwrap();
        log.append(medousa_engine::TurnEvent::Notice {
            message: "boundary".into(),
        })
        .unwrap();
        let cursor = medousa_engine::TranscriptCursor::from_log(&log);
        log.append(medousa_engine::TurnEvent::Notice {
            message: "after-checkpoint".into(),
        })
        .unwrap();
        assert!(cursor.verify(&log).is_ok());
        let prefix = medousa_engine::reconstruct_from_journal(&log, &cursor).unwrap();
        assert_eq!(prefix.len(), 1);

        let store = CoderTurnCheckpointStore::open(root.path().join("checkpoints"));
        let mut durable = checkpoint("session-advanced", "turn-advanced", "work-advanced");
        durable.checkpoint_generation = 3;
        durable.transcript_cursor = Some(cursor);
        store.save(&durable).unwrap();
        let loaded = store
            .load_latest_resume_candidate("session-advanced", "work-advanced")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.transcript_cursor.as_ref().map(|c| c.fence), Some(1));
    }

    #[test]
    fn journal_corruption_in_the_middle_fails_closed() {
        let root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(root.path());
        let mut first = checkpoint("session-corrupt", "turn-corrupt", "work-corrupt");
        first.checkpoint_generation = 1;
        store.save(&first).unwrap();
        let session = session_id("session-corrupt");
        let journal = checkpoint_journal_path("turn-corrupt");
        let path = store.files.resolve_write_path(&session, &journal).unwrap();
        let tx = store.transaction().unwrap();
        let mut raw = tx.root().read(&path).unwrap();
        raw.extend_from_slice(b"{\"not\":\"a record\"}\n");
        raw.extend_from_slice(br#"{"schema_version":1,"generation":2,"turn_id":"turn-corrupt","session_id":"session-corrupt","work_id":"work-corrupt","published_at_utc":"2026-01-01T00:00:00Z","body_digest":"sha256:dead","body":{"kind":"boundary","checkpoint":{}}}"#);
        raw.push(b'\n');
        tx.root().atomic_write(&path, &raw).unwrap();
        let error = store
            .recover_from_journal("session-corrupt", "turn-corrupt")
            .unwrap_err();
        assert!(
            error.contains("corruption") || error.contains("digest"),
            "{error}"
        );
    }

    #[test]
    fn stale_writer_cannot_replace_newer_generation() {
        let root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(root.path());
        let mut newer = checkpoint("session-stale", "turn-stale", "work-stale");
        newer.checkpoint_generation = 5;
        store.save(&newer).unwrap();
        let mut stale = newer.clone();
        stale.checkpoint_generation = 4;
        stale.current_goal = "stale overwrite".into();
        let error = store.save(&stale).unwrap_err();
        assert!(error.contains("stale checkpoint generation"), "{error}");
        let loaded = store
            .load_latest_resume_candidate("session-stale", "work-stale")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.checkpoint_generation, 5);
        assert_ne!(loaded.current_goal, "stale overwrite");
    }

    #[test]
    fn idempotent_recovery_replays_the_same_generation() {
        let root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(root.path());
        let mut first = checkpoint("session-idem", "turn-idem", "work-idem");
        first.checkpoint_generation = 2;
        store.save(&first).unwrap();
        store.save(&first).unwrap();
        let loaded = store
            .load_latest_resume_candidate("session-idem", "work-idem")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.checkpoint_generation, 2);
    }

    #[test]
    fn exact_serialized_byte_budgets_bound_retained_memory() {
        let root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(root.path());
        let mut oversized = checkpoint("session-budget", "turn-budget", "work-budget");
        oversized.checkpoint_generation = 1;
        oversized.transcript.tool_lane_messages = (0..80)
            .map(|index| ChatMessage::assistant("x".repeat(8_000) + &index.to_string()))
            .collect();
        store.save(&oversized).unwrap();
        let loaded = store
            .load_latest_resume_candidate("session-budget", "work-budget")
            .unwrap()
            .unwrap();
        let encoded = serde_json::to_vec(&loaded.transcript.tool_lane_messages).unwrap();
        assert!(encoded.len() <= MAX_TRANSCRIPT_BYTES);
        assert!(serde_json::to_vec(&loaded).unwrap().len() as u64 <= MAX_CHECKPOINT_BYTES);
    }

    #[test]
    fn journal_replay_preserves_complete_prefix_after_partial_tail() {
        let root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(root.path());
        let mut first = checkpoint("session-replay", "turn-replay", "work-replay");
        first.checkpoint_generation = 1;
        first.current_goal = "first boundary".into();
        store.save(&first).unwrap();
        let mut second = first.clone();
        second.checkpoint_generation = 2;
        second.current_goal = "second boundary".into();
        store.save(&second).unwrap();

        let session = session_id("session-replay");
        let journal = checkpoint_journal_path("turn-replay");
        let path = store.files.resolve_write_path(&session, &journal).unwrap();
        let tx = store.transaction().unwrap();
        let mut raw = tx.root().read(&path).unwrap();
        raw.extend_from_slice(b"{\"generation\":3,\"partial\":");
        tx.root().atomic_write(&path, &raw).unwrap();

        let recovered = store
            .recover_from_journal("session-replay", "turn-replay")
            .unwrap()
            .unwrap();
        assert_eq!(recovered.checkpoint_generation, 2);
        assert_eq!(recovered.current_goal, "second boundary");
    }

    #[test]
    fn long_turn_compacts_checkpoint_journal_before_the_read_bound() {
        let root = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(root.path());
        let mut value = checkpoint("session-long", "turn-long", "work-long");
        value.transcript.tool_lane_messages = vec![ChatMessage::assistant("x".repeat(180_000))];

        for generation in 1..=40 {
            value.checkpoint_generation = generation;
            value.current_goal = format!("boundary-{generation}");
            store.save(&value).unwrap();
        }

        let session = session_id("session-long");
        let journal = checkpoint_journal_path("turn-long");
        let path = store.files.resolve_read_path(&session, &journal).unwrap();
        let metadata = store.transaction().unwrap().root().metadata(&path).unwrap();
        assert!(metadata.size <= MAX_CHECKPOINT_JOURNAL_BYTES);
        let recovered = store
            .recover_from_journal("session-long", "turn-long")
            .unwrap()
            .unwrap();
        assert_eq!(recovered.checkpoint_generation, 40);
        assert_eq!(recovered.current_goal, "boundary-40");
    }

    #[test]
    fn store_root_can_be_any_path() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("nested");
        let store = CoderTurnCheckpointStore::open(&root);
        store
            .save(&checkpoint("session-a", "turn-1", "work-a"))
            .unwrap();
        assert!(root.exists());
    }
}
