//! Durable, protocol-safe checkpoints for unfinished Coder turns.
//!
//! Semantic memory explains the work. This store preserves the normalized
//! provider transcript and loop counters needed to continue an exact turn.
//! It only accepts snapshots produced after a complete model-only boundary or
//! after every tool call in a batch has a matching result.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use genai::chat::{ChatMessage, ContentPart, MessageContent, ToolCall, ToolResponse};
use medousa_forge::forge::Forge;
use medousa_forge::git::PorcelainKind;
use medousa_forge::model::{
    AttemptId, AttemptState, ExecutionLease, GovernedEnv, RecoveryDisposition, WorkId,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use super::coder_activity::{CoderActivityKind, CoderActivityStore};
use super::coder_mode::CoderEntryContext;
use super::coder_tools::CoderBoundToolRegistry;
use super::turn_budget::TurnOrchestrationState;
use super::turn_context::TurnScratchpad;

pub const ACTIVE_TURN_CHECKPOINT_SCHEMA_VERSION: u32 = 1;
pub const TOOL_ROUND_BUDGET_EXHAUSTED_REASON: &str = "tool_round_budget_exhausted";

const CHECKPOINT_DIR: &str = "coder_turn_checkpoints";
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

static CODER_TURN_CHECKPOINT_STORE: Lazy<Arc<CoderTurnCheckpointStore>> = Lazy::new(|| {
    Arc::new(CoderTurnCheckpointStore::open(
        crate::session::medousa_data_dir().join(CHECKPOINT_DIR),
    ))
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveTurnCheckpointStatus {
    Active,
    AwaitingUser,
    BudgetExhausted,
    RecoverableFailure,
    Completed,
    Superseded,
}

impl ActiveTurnCheckpointStatus {
    pub fn is_resume_candidate(self) -> bool {
        matches!(
            self,
            Self::Active | Self::AwaitingUser | Self::BudgetExhausted | Self::RecoverableFailure
        )
    }

    pub fn restores_interrupted_budget(self) -> bool {
        matches!(self, Self::Active | Self::RecoverableFailure)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeCheckpointBoundary {
    TurnStarted,
    ModelResponseCompleted,
    ToolBatchCompleted,
    PackHold,
    AwaitingApproval,
    AwaitingUser,
    BudgetExhausted,
    RecoverableFailure,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutstandingTurnBoundary {
    UserInput {
        reason: String,
    },
    BudgetApproval {
        request_id: String,
        requested_rounds: usize,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveTurnCounters {
    pub model_rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub tool_batches_completed: usize,
    pub interim_continues_used: usize,
    pub empty_after_tools_continues_used: usize,
    pub text_only_continues_without_new_tools: usize,
    pub invocations_at_last_text_continue: usize,
    pub user_responses_sent: usize,
    pub last_response_preview: Option<String>,
    pub pending_final_answer: bool,
    pub retry_count: usize,
    pub orchestration: Option<TurnOrchestrationState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveTurnTranscript {
    pub user_lane_prefix: Vec<ChatMessage>,
    pub tool_lane_messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointToolInvocation {
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_output: Value,
}

impl CheckpointToolInvocation {
    pub fn from_runtime(invocation: &ToolInvocation) -> Self {
        Self {
            tool_name: truncate(&invocation.tool_name, 256),
            tool_input: bounded_json_value(&invocation.tool_input),
            tool_output: bounded_json_value(&invocation.tool_output),
        }
    }

    pub fn into_runtime(self) -> ToolInvocation {
        ToolInvocation {
            tool_name: self.tool_name,
            tool_input: self.tool_input,
            tool_output: self.tool_output,
        }
    }
}

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

#[derive(Debug, Clone)]
pub struct ActiveTurnResumeState {
    pub source_daemon_turn_id: String,
    pub restore_turn_budget: bool,
    pub append_current_user_message: bool,
    pub counters: ActiveTurnCounters,
    pub transcript: ActiveTurnTranscript,
    pub invocations: Vec<CheckpointToolInvocation>,
    pub pack_hold_fragments: Vec<String>,
    pub scratch: TurnScratchpad,
}

#[derive(Debug, Clone)]
pub struct ToolLoopCheckpointState {
    pub boundary: SafeCheckpointBoundary,
    pub status: ActiveTurnCheckpointStatus,
    pub counters: ActiveTurnCounters,
    pub user_lane_prefix: Vec<ChatMessage>,
    pub tool_lane_messages: Vec<ChatMessage>,
    pub invocations: Vec<CheckpointToolInvocation>,
    pub pack_hold_fragments: Vec<String>,
    pub scratch: TurnScratchpad,
    pub outstanding_boundary: Option<OutstandingTurnBoundary>,
    pub tool_names: Vec<String>,
    pub provider_call_ids: Vec<String>,
    pub termination_reason: Option<String>,
}

pub trait ActiveTurnCheckpointSink: Send + Sync {
    fn persist_boundary(&self, state: ToolLoopCheckpointState) -> Result<(), String>;
    fn mark_status(
        &self,
        status: ActiveTurnCheckpointStatus,
        boundary: SafeCheckpointBoundary,
        reason: Option<&str>,
        orchestration: Option<&TurnOrchestrationState>,
    ) -> Result<(), String>;
    fn latest_safe_resume(&self) -> Result<Option<ActiveTurnResumeState>, String>;
    fn set_model_route(&self, provider: &str, model: &str) -> Result<(), String>;
}

#[derive(Debug)]
pub struct CoderTurnCheckpointStore {
    root: PathBuf,
    lock: Mutex<()>,
}

impl CoderTurnCheckpointStore {
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            lock: Mutex::new(()),
        }
    }

    pub fn load_latest_resume_candidate(
        &self,
        session_id: &str,
        work_id: &str,
    ) -> Result<Option<ActiveTurnCheckpoint>, String> {
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let dir = self.session_dir(session_id);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(format!("cannot scan Coder turn checkpoints: {err}")),
        };
        let mut latest: Option<ActiveTurnCheckpoint> = None;
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let path = entry.path();
            let metadata = match entry.metadata() {
                Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_CHECKPOINT_BYTES => {
                    metadata
                }
                _ => continue,
            };
            if metadata.len() == 0 {
                continue;
            }
            let raw = match std::fs::read(&path) {
                Ok(raw) => raw,
                Err(_) => continue,
            };
            let checkpoint = match serde_json::from_slice::<ActiveTurnCheckpoint>(&raw) {
                Ok(checkpoint) => checkpoint,
                Err(err) => {
                    tracing::warn!(error = %err, path = %path.display(), "ignoring malformed Coder turn checkpoint");
                    continue;
                }
            };
            if validate_checkpoint(&checkpoint, session_id, work_id).is_err()
                || !checkpoint.status.is_resume_candidate()
            {
                continue;
            }
            if latest
                .as_ref()
                .is_none_or(|current| checkpoint.updated_at_utc > current.updated_at_utc)
            {
                latest = Some(checkpoint);
            }
        }
        Ok(latest)
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
        self.save(&superseded)
    }

    fn save_unlocked(&self, checkpoint: &ActiveTurnCheckpoint) -> Result<(), String> {
        validate_checkpoint(
            checkpoint,
            &checkpoint.session_id,
            &checkpoint.forge.work_id,
        )?;
        let mut bounded = checkpoint.clone();
        bound_checkpoint(&mut bounded);
        let mut bytes = serde_json::to_vec_pretty(&bounded)
            .map_err(|err| format!("cannot serialize Coder turn checkpoint: {err}"))?;
        while bytes.len() as u64 > MAX_CHECKPOINT_BYTES
            && bounded.transcript.tool_lane_messages.len() > 1
        {
            bounded.transcript.tool_lane_messages.remove(0);
            strip_orphaned_tool_responses(&mut bounded.transcript.tool_lane_messages);
            bytes = serde_json::to_vec_pretty(&bounded)
                .map_err(|err| format!("cannot serialize Coder turn checkpoint: {err}"))?;
        }
        while bytes.len() as u64 > MAX_CHECKPOINT_BYTES
            && bounded.transcript.user_lane_prefix.len() > 1
        {
            bounded.transcript.user_lane_prefix.remove(0);
            bytes = serde_json::to_vec_pretty(&bounded)
                .map_err(|err| format!("cannot serialize Coder turn checkpoint: {err}"))?;
        }
        while bytes.len() as u64 > MAX_CHECKPOINT_BYTES && bounded.invocations.len() > 1 {
            bounded.invocations.remove(0);
            bytes = serde_json::to_vec_pretty(&bounded)
                .map_err(|err| format!("cannot serialize Coder turn checkpoint: {err}"))?;
        }
        if bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
            return Err(format!(
                "Coder turn checkpoint exceeds {} bytes after bounding",
                MAX_CHECKPOINT_BYTES
            ));
        }
        crate::session::atomic_write(
            &self.turn_path(&bounded.session_id, &bounded.daemon_turn_id),
            &bytes,
        )
        .map_err(|err| format!("cannot persist Coder turn checkpoint: {err}"))
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.root.join(short_digest(session_id))
    }

    fn turn_path(&self, session_id: &str, turn_id: &str) -> PathBuf {
        self.session_dir(session_id)
            .join(format!("{}.json", short_digest(turn_id)))
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
        };
        let controller = Arc::new(Self {
            store: params.store,
            checkpoint: Mutex::new(checkpoint),
            forge: params.forge,
            entry: params.entry,
            registry: params.registry,
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
        checkpoint.updated_at_utc = Utc::now();
        self.store.save(&checkpoint)
    }
}

impl ActiveTurnCheckpointSink for CoderTurnCheckpointController {
    fn persist_boundary(&self, state: ToolLoopCheckpointState) -> Result<(), String> {
        let mut checkpoint = self.checkpoint.lock().map_err(|err| err.to_string())?;
        self.refresh_runtime_metadata(&mut checkpoint)?;
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
        checkpoint.updated_at_utc = Utc::now();
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
        self.store.save(&checkpoint)
    }

    fn mark_status(
        &self,
        status: ActiveTurnCheckpointStatus,
        boundary: SafeCheckpointBoundary,
        reason: Option<&str>,
        orchestration: Option<&TurnOrchestrationState>,
    ) -> Result<(), String> {
        let mut checkpoint = self.checkpoint.lock().map_err(|err| err.to_string())?;
        self.refresh_runtime_metadata(&mut checkpoint)?;
        checkpoint.status = status;
        checkpoint.safe_boundary = boundary;
        checkpoint.termination_reason = reason.map(|reason| truncate(reason, 2_000));
        checkpoint.counters.orchestration = orchestration.cloned();
        checkpoint.counters.retry_count = orchestration.map(|state| state.retries).unwrap_or(0);
        checkpoint.updated_at_utc = Utc::now();
        self.store.save(&checkpoint)
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
        checkpoint.updated_at_utc = Utc::now();
        self.store.save(&checkpoint)
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
    let branch = forge
        .git()
        .current_branch(&worktree)
        .map_err(|err| format!("cannot read checkpoint branch: {err}"))?
        .ok_or_else(|| "checkpoint worktree is detached".to_string())?;
    let head_oid = forge
        .git()
        .head_oid(&worktree)
        .map_err(|err| format!("cannot read checkpoint HEAD: {err}"))?
        .to_string();
    let mut status = forge
        .git()
        .status_porcelain(&worktree)
        .map_err(|err| format!("cannot read checkpoint dirty state: {err}"))?;
    status.sort_by(|left, right| left.path.cmp(&right.path));
    let dirty_material = status
        .iter()
        .map(|entry| {
            format!(
                "{:?}|{}|{}|{}",
                entry.kind,
                entry.xy.as_deref().unwrap_or(""),
                entry.path,
                entry.orig_path.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tracked_patch = forge
        .git()
        .diff_binary_worktree(
            &worktree,
            &medousa_forge::model::GitOid::new(head_oid.clone()),
        )
        .map_err(|err| format!("cannot fingerprint checkpoint worktree patch: {err}"))?;
    let mut dirty_hasher = Sha256::new();
    dirty_hasher.update(dirty_material.as_bytes());
    dirty_hasher.update(b"\0tracked-patch\0");
    dirty_hasher.update(&tracked_patch);
    for entry in status
        .iter()
        .filter(|entry| entry.kind == PorcelainKind::Untracked)
    {
        hash_untracked_path(&mut dirty_hasher, &worktree, &entry.path)?;
    }
    let dirty_digest = format!("{:x}", dirty_hasher.finalize());
    Ok(CheckpointForgeState {
        work_id: work_id.to_string(),
        attempt_id: attempt_id.to_string(),
        resumed_from_attempt_id,
        worktree: worktree.to_string_lossy().into_owned(),
        branch,
        environment_generation: environment.generation,
        head_oid,
        dirty: !status.is_empty(),
        dirty_digest,
        changed_paths: status
            .into_iter()
            .map(|entry| truncate(&entry.path, 512))
            .take(MAX_CHANGED_PATHS)
            .collect(),
    })
}

fn hash_untracked_path(hasher: &mut Sha256, root: &Path, relative: &str) -> Result<(), String> {
    let relative_path = Path::new(relative);
    if relative.is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "cannot fingerprint unsafe untracked checkpoint path: {relative}"
        ));
    }
    let path = root.join(relative_path);
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|err| format!("cannot inspect untracked checkpoint path {relative}: {err}"))?;
    hasher.update(b"\0untracked\0");
    hasher.update(relative.as_bytes());
    hasher.update(metadata.len().to_le_bytes());
    hasher.update([u8::from(metadata.permissions().readonly())]);
    if metadata.file_type().is_symlink() {
        hasher.update(b"\0symlink\0");
        let target = std::fs::read_link(&path)
            .map_err(|err| format!("cannot read untracked checkpoint symlink {relative}: {err}"))?;
        hasher.update(target.to_string_lossy().as_bytes());
        return Ok(());
    }
    if !metadata.is_file() {
        return Err(format!(
            "cannot exactly fingerprint non-file untracked checkpoint path: {relative}"
        ));
    }
    let canonical = std::fs::canonicalize(&path)
        .map_err(|err| format!("cannot resolve untracked checkpoint path {relative}: {err}"))?;
    if !canonical.starts_with(root) {
        return Err(format!(
            "untracked checkpoint path escaped the governed worktree: {relative}"
        ));
    }
    let mut file = std::fs::File::open(&canonical)
        .map_err(|err| format!("cannot open untracked checkpoint path {relative}: {err}"))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("cannot hash untracked checkpoint path {relative}: {err}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
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
    while serialized_size(&checkpoint.invocations) > MAX_INVOCATION_BYTES
        && checkpoint.invocations.len() > 1
    {
        checkpoint.invocations.remove(0);
    }
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
    while serialized_size(&bounded) > max_bytes && bounded.len() > 1 {
        bounded.remove(0);
    }
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
            // Durable checkpoints never retain private reasoning or binary/custom
            // payloads. The normalized placeholder keeps message ordering stable.
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

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect()
    }
}

fn short_digest(value: &str) -> String {
    full_digest(value)[..24].to_string()
}

fn full_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use medousa_forge::model::{ExecutorDescriptor, RecoveryDisposition};
    use tempfile::tempdir;

    use super::*;

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
            visible_tools: vec!["cognition_code_read".into()],
            outstanding_boundary: None,
            last_completed_tool_boundary: None,
            termination_reason: None,
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
        let raw =
            std::fs::read_to_string(store.turn_path("session-redact", "turn-redact")).unwrap();
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
    fn checkpoint_files_are_keyed_without_raw_session_or_turn_ids() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path());
        let path = store.turn_path("private/session", "turn:secret");
        assert!(!path.to_string_lossy().contains("private/session"));
        assert!(!path.to_string_lossy().contains("turn:secret"));
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("json")
        );
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
        let path = store.turn_path("session-deep", "turn-deep");
        assert!(std::fs::metadata(path).unwrap().len() <= MAX_CHECKPOINT_BYTES);
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
    fn store_root_can_be_any_path() {
        let temp = tempdir().unwrap();
        let store = CoderTurnCheckpointStore::open(temp.path().join("nested"));
        store
            .save(&checkpoint("session-a", "turn-1", "work-a"))
            .unwrap();
        assert!(Path::new(&store.root).exists());
    }
}
