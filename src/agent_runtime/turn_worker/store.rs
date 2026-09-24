//! Durable turn work records (host/worker bus).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::agent_runtime::turn_context::WorkerHandoffCapsule;
use crate::peer_execution_policy::TaskExecutionGrant;
use crate::stage_routing::StageRoute;
use crate::turn_continuation::StoredDeliveryTarget;
use crate::workshop_contract::{ExecutionPlacementResolution, default_unknown_runtime_id};

const MAX_ACTIVE_TURN_WORKERS: usize = 500;
use crate::workspace::retention::WorkspaceRetentionConfig;

static STORE: Lazy<Arc<TurnWorkerStore>> = Lazy::new(|| Arc::new(TurnWorkerStore::new()));

pub fn turn_worker_store() -> Arc<TurnWorkerStore> {
    STORE.clone()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnWorkStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TurnWorkDisposition {
    #[default]
    Parallel,
    Bound,
    /// Authenticated daemon-to-daemon work. It executes in the normal worker
    /// loop but never resumes or synthesizes into a receiver-local host turn.
    Delegated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopSteerMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_id: Option<String>,
    pub text: String,
    pub at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_profile_id: Option<String>,
}

/// One live tool run for the worker transcript, correlated start-to-finish by
/// `run_id` so the UI can render a single `tool(params) → result` row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerToolActivity {
    #[serde(default)]
    pub run_id: String,
    pub name: String,
    #[serde(default)]
    pub round: usize,
    /// running | succeeded | failed
    #[serde(default)]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_params: Vec<medousa_types::daemon_api::ToolInputParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_summary: Option<String>,
    /// `alias` keeps pre-correlation `turn_workers.json` files loadable.
    #[serde(default = "Utc::now", alias = "at")]
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}

fn default_worker_max_tool_rounds() -> usize {
    10
}

fn default_parent_stream_turn_id() -> u64 {
    0
}

/// The parent's user-facing route, captured before the worker can resolve its
/// own execution model. Versioned so future routing policy can evolve without
/// reinterpreting durable work records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParentContinuationRoute {
    pub schema_version: u16,
    pub stage_role: String,
    pub policy_profile: String,
    pub provider: String,
    pub model: String,
    /// Route policy labels selected by the parent StageRoutingMatrix.
    #[serde(default)]
    pub fallback_chain: Vec<String>,
}

impl ParentContinuationRoute {
    pub const SCHEMA_VERSION: u16 = 1;

    pub fn from_stage_route(route: &StageRoute) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            stage_role: route.role.clone(),
            policy_profile: route.policy_profile.clone(),
            provider: route.provider.clone(),
            model: route.model.clone(),
            fallback_chain: route.fallback_chain.clone(),
        }
    }

    pub fn stage_route(&self) -> Option<StageRoute> {
        if self.schema_version != Self::SCHEMA_VERSION
            || self.stage_role.trim() != "final_response"
            || self.policy_profile.trim().is_empty()
            || self.provider.trim().is_empty()
            || self.model.trim().is_empty()
        {
            return None;
        }
        Some(StageRoute {
            role: self.stage_role.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            policy_profile: self.policy_profile.clone(),
            fallback_chain: self.fallback_chain.clone(),
        })
    }
}

/// Resolve old records through an explicit legacy policy. Parallel records
/// can still recover the parent provider/model from their spawn contract;
/// bound records use the currently configured workshop final-response route.
/// The worker's resolved provider/model is intentionally never consulted.
pub fn continuation_route_for_record(
    record: &TurnWorkRecord,
    legacy_route: &StageRoute,
) -> Option<StageRoute> {
    if let Some(contract) = record.parent_continuation_route.as_ref() {
        // An unknown or malformed future contract must not silently degrade
        // to today's defaults or a worker model.
        return contract.stage_route();
    }

    if let Some(parent) = record.worker_spawn_spec.as_ref().map(|spec| &spec.parent)
        && !parent.provider.trim().is_empty()
        && !parent.model.trim().is_empty()
    {
        return Some(
            crate::stage_routing::StageRoutingMatrix::default_for(&parent.provider, &parent.model)
                .final_response,
        );
    }

    // The caller supplies its already-admitted host fallback. Resolving a
    // legacy record must not perform synchronous settings I/O on the turn task.
    ParentContinuationRoute::from_stage_route(legacy_route).stage_route()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnWorkRecord {
    pub work_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_user_id: Option<String>,
    pub parent_turn_correlation_id: Option<String>,
    #[serde(default = "default_parent_stream_turn_id")]
    pub parent_stream_turn_id: u64,
    /// Runtime that admitted the parent turn. Legacy records retain an
    /// explicit unknown value rather than pretending they ran locally.
    #[serde(default = "default_unknown_runtime_id")]
    pub parent_runtime_id: String,
    /// Parent final-response route. Missing on legacy records, which use the
    /// explicit compatibility resolver above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_continuation_route: Option<ParentContinuationRoute>,
    /// Requested and resolved execution target captured before enqueue.
    #[serde(default)]
    pub execution_placement: ExecutionPlacementResolution,
    /// Immutable destination-owned authority compiled when a remote peer's
    /// request was admitted. Local and pre-policy records leave this empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_execution_grant: Option<TaskExecutionGrant>,
    /// Canonical semantic worker contract. Legacy and bound-workshop records
    /// may omit it; new local and remote parallel workers persist it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_spawn_spec: Option<crate::delegated_task::WorkerSpawnSpec>,
    pub intent: String,
    pub task_prompt: String,
    pub status: TurnWorkStatus,
    pub result_text: Option<String>,
    pub tool_names: Vec<String>,
    pub termination_reason: Option<String>,
    /// Explicit worker handback decision. `false` means the worker result is
    /// already principal-facing and must be delivered without another model call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_synthesis: Option<bool>,
    pub error: Option<String>,
    pub user_ack: String,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    /// Tool-round budget snapshotted from the host turn's operator settings at spawn.
    #[serde(default = "default_worker_max_tool_rounds")]
    pub max_tool_rounds: usize,
    pub delivery_target: Option<StoredDeliveryTarget>,
    pub parent_user_prompt: Option<String>,
    /// Snapshotted host agent mode (`general` / `assistant` / `teacher` / `instant` / `coder`) so resume stays in-lane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_agent_mode: Option<String>,
    /// Snapshotted Forge work id when the host spawned from Coder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_code_work_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_capsule: Option<WorkerHandoffCapsule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_scratch: Option<crate::agent_runtime::turn_context::TurnScratchpad>,
    /// Host synthesis delivered the worker result to the parent turn.
    #[serde(default)]
    pub synthesis_delivered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stasis_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manuscript_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_group_id: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub disposition: TurnWorkDisposition,
    #[serde(default)]
    pub steer_messages: Vec<WorkshopSteerMessage>,
    /// Durable idempotency fence for remote steering. Messages may be drained
    /// while their control ids remain so a transport retry cannot enqueue the
    /// same instruction twice.
    #[serde(default)]
    pub processed_steer_control_ids: Vec<String>,
    /// Snapshotted from host client when work was delegated (Home canvas lane).
    #[serde(default)]
    pub supports_ui_artifacts: bool,
    #[serde(default)]
    pub supports_liquid_markdown: bool,
    #[serde(default)]
    pub supports_browser_host: bool,
    /// Live tool/assistant activity for chat-adjacent transcript (rolling, capped).
    #[serde(default)]
    pub live_tool_activity: Vec<crate::agent_runtime::turn_worker::WorkerToolActivity>,
    /// Live reasoning transcript — joined chunks, capped tail. Prose, not deltas.
    #[serde(default)]
    pub live_thinking: String,
    /// Live assistant output preview — joined chunks, capped tail.
    #[serde(default)]
    pub live_output: String,
    /// First/last reasoning chunk, so chat can render "Thought for Ns".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TurnWorkRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn delegated(
        work_id: String,
        session_id: String,
        identity_user_id: String,
        parent_turn_correlation_id: String,
        task_prompt: String,
        provider: String,
        model: String,
        response_depth_mode: String,
        max_tool_rounds: usize,
        handoff_capsule: WorkerHandoffCapsule,
        parent_runtime_id: String,
        execution_placement: ExecutionPlacementResolution,
        task_execution_grant: TaskExecutionGrant,
    ) -> Self {
        let now = Utc::now();
        Self {
            work_id,
            session_id,
            identity_user_id: Some(identity_user_id),
            parent_turn_correlation_id: Some(parent_turn_correlation_id),
            parent_stream_turn_id: 0,
            parent_runtime_id,
            parent_continuation_route: None,
            execution_placement,
            task_execution_grant: Some(task_execution_grant),
            worker_spawn_spec: None,
            intent: "research".to_string(),
            task_prompt,
            status: TurnWorkStatus::Pending,
            result_text: None,
            tool_names: Vec::new(),
            termination_reason: None,
            needs_synthesis: None,
            error: None,
            user_ack: String::new(),
            provider,
            model,
            response_depth_mode,
            max_tool_rounds: max_tool_rounds.max(1),
            delivery_target: None,
            parent_user_prompt: None,
            parent_agent_mode: None,
            parent_code_work_id: None,
            handoff_capsule: Some(handoff_capsule),
            worker_scratch: None,
            synthesis_delivered: false,
            stasis_job_id: None,
            thread_id: None,
            stage_role: None,
            model_hint: None,
            manuscript_id: None,
            branch_group_id: None,
            archived: false,
            disposition: TurnWorkDisposition::Delegated,
            steer_messages: Vec::new(),
            processed_steer_control_ids: Vec::new(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            live_tool_activity: Vec::new(),
            live_thinking: String::new(),
            live_output: String::new(),
            thinking_started_at: None,
            thinking_finished_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

pub struct TurnWorkerStore {
    records: Mutex<HashMap<String, TurnWorkRecord>>,
    live_cancellations: Mutex<HashMap<String, Arc<CancellationToken>>>,
    parallel_intakes: Mutex<HashSet<ParallelCohortKey>>,
    admit_mutations: MutationAdmitter,
}

type MutationAdmitter = Arc<
    dyn Fn(
            Vec<crate::workspace::persist::WorkspaceMutation>,
        ) -> Result<(), crate::persistence::PersistenceError>
        + Send
        + Sync,
>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ParallelCohortKey {
    session_id: String,
    parent_turn_id: Option<String>,
    legacy_stream_turn_id: u64,
}

impl ParallelCohortKey {
    fn new(session_id: &str, stream_turn_id: u64, parent_turn_id: Option<&str>) -> Self {
        let parent_turn_id = parent_turn_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        Self {
            session_id: session_id.into(),
            legacy_stream_turn_id: if parent_turn_id.is_some() {
                0
            } else {
                stream_turn_id
            },
            parent_turn_id,
        }
    }

    fn matches(&self, record: &TurnWorkRecord) -> bool {
        !record.archived
            && record.disposition == TurnWorkDisposition::Parallel
            && *self
                == Self::new(
                    &record.session_id,
                    record.parent_stream_turn_id,
                    record.parent_turn_correlation_id.as_deref(),
                )
    }
}

/// Process-local exclusion is separate from durable delivery acknowledgement.
/// Dropping an unfinished intake (including cancellation) leaves results pending;
/// after restart there are no stale in-flight claims to suppress recovery.
pub struct ParallelCohortIntake<'a> {
    store: &'a TurnWorkerStore,
    key: ParallelCohortKey,
    records: Vec<TurnWorkRecord>,
}

impl std::ops::Deref for ParallelCohortIntake<'_> {
    type Target = [TurnWorkRecord];
    fn deref(&self) -> &Self::Target {
        &self.records
    }
}

impl ParallelCohortIntake<'_> {
    pub fn acknowledge(self) {
        for record in &self.records {
            self.store
                .update(&record.work_id, |worker| worker.synthesis_delivered = true);
        }
    }
}

impl Drop for ParallelCohortIntake<'_> {
    fn drop(&mut self) {
        self.store
            .parallel_intakes
            .lock()
            .expect("parallel intakes")
            .remove(&self.key);
    }
}

pub struct WorkerExecutionLease {
    store: Arc<TurnWorkerStore>,
    work_id: String,
    cancellation: Arc<CancellationToken>,
}

impl WorkerExecutionLease {
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
}

impl Drop for WorkerExecutionLease {
    fn drop(&mut self) {
        let mut live = self
            .store
            .live_cancellations
            .lock()
            .expect("turn worker cancellations");
        if live
            .get(&self.work_id)
            .is_some_and(|current| Arc::ptr_eq(current, &self.cancellation))
        {
            live.remove(&self.work_id);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundWorkshopAdmissionError {
    SessionDeleting,
    ActiveGeneration { work_id: String },
    Persistence(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegatedWorkAdmissionError {
    SessionDeleting,
    ConflictingIdentity,
    Persistence(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundWorkshopMutationError {
    SessionDeleting,
    MissingGeneration,
    StaleGeneration { active_work_id: Option<String> },
    Persistence(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnWorkerMutationError {
    SessionDeleting,
    MissingWork,
    ForeignSession,
    Persistence(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegatedWorkControlError {
    SessionDeleting,
    MissingWork,
    ForeignIdentity,
    WrongDisposition,
    NotActive,
    Persistence(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerExecutionRegistrationError {
    MissingWork,
    NotActive,
    AlreadyRunning,
    AtCapacity { limit: usize },
    Persistence(String),
}

impl Default for TurnWorkerStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnWorkerStore {
    pub fn new() -> Self {
        let store = Self::with_admitter(Arc::new(crate::workspace::persist::queue_mutations));
        store.reload_from_disk();
        store
    }

    fn with_admitter(admit_mutations: MutationAdmitter) -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            live_cancellations: Mutex::new(HashMap::new()),
            parallel_intakes: Mutex::new(HashSet::new()),
            admit_mutations,
        }
    }

    #[cfg(test)]
    pub(crate) fn empty_for_tests() -> Self {
        Self::with_admitter(Arc::new(|_| Ok(())))
    }

    fn reload_from_disk(&self) {
        if let Some(projection) = crate::workspace::persist::startup_projection() {
            *self.records.lock().expect("turn worker records") = projection.turn_workers;
        } else {
            tracing::error!(
                "turn_worker_store: workspace persistence projection unavailable; refusing ambient snapshot fallback"
            );
        }
    }

    fn persist(
        &self,
        work_id: &str,
        stasis_job_id: Option<&str>,
    ) -> Result<(), crate::persistence::PersistenceError> {
        let mut guard = self.records.lock().expect("turn worker records");
        let mut changed = Self::prune_map(&mut guard);
        if let Some(record) = guard.get(work_id).cloned() {
            changed.push(record);
        }
        let retained = guard.keys().cloned().collect::<Vec<_>>();
        changed.sort_by(|left, right| left.work_id.cmp(&right.work_id));
        changed.dedup_by(|left, right| left.work_id == right.work_id);
        let mutations = Self::mutations_for_records(changed, retained);
        (self.admit_mutations)(mutations)?;
        drop(guard);
        Self::notify_turn_worker_changed(work_id, stasis_job_id);
        Ok(())
    }

    fn persist_candidate(
        &self,
        candidate: &mut HashMap<String, TurnWorkRecord>,
        work_id: &str,
    ) -> Result<(), crate::persistence::PersistenceError> {
        let mut changed = Self::prune_map(candidate);
        if let Some(record) = candidate.get(work_id).cloned() {
            changed.push(record);
        }
        let retained = candidate.keys().cloned().collect::<Vec<_>>();
        changed.sort_by(|left, right| left.work_id.cmp(&right.work_id));
        changed.dedup_by(|left, right| left.work_id == right.work_id);
        (self.admit_mutations)(Self::mutations_for_records(changed, retained))
    }

    fn mutations_for_records(
        changed: Vec<TurnWorkRecord>,
        retained: Vec<String>,
    ) -> Vec<crate::workspace::persist::WorkspaceMutation> {
        let mut mutations = changed
            .into_iter()
            .map(
                |record| crate::workspace::persist::WorkspaceMutation::UpsertTurnWorker {
                    record: Box::new(record),
                },
            )
            .collect::<Vec<_>>();
        mutations.push(
            crate::workspace::persist::WorkspaceMutation::RetainTurnWorkers { work_ids: retained },
        );
        mutations
    }

    fn report_persistence_error(
        action: &str,
        work_id: &str,
        error: &crate::persistence::PersistenceError,
    ) {
        tracing::error!(work_id, action, error = %error, "turn_worker_store persistence admission failed");
    }

    fn notify_turn_worker_changed(work_id: &str, stasis_job_id: Option<&str>) {
        crate::workspace::domain_event::notify_workspace_event(
            crate::workspace::domain_event::WorkspaceDomainEvent::TurnWorkerChanged {
                work_id: work_id.to_string(),
            },
        );
        if let Some(job_id) = stasis_job_id.filter(|value| !value.is_empty()) {
            crate::workspace::domain_event::notify_workspace_event(
                crate::workspace::domain_event::WorkspaceDomainEvent::StasisJobChanged {
                    job_id: job_id.to_string(),
                },
            );
        }
    }

    fn prune_map(map: &mut HashMap<String, TurnWorkRecord>) -> Vec<TurnWorkRecord> {
        let mut changed = Vec::new();
        let retention = WorkspaceRetentionConfig::load();
        let cutoff = retention.wipe_cutoff(Utc::now());
        map.retain(|_, record| {
            if record.archived {
                return record.updated_at >= cutoff;
            }
            true
        });

        let active: Vec<_> = map
            .values()
            .filter(|record| !record.archived)
            .map(|record| record.work_id.clone())
            .collect();
        if active.len() > MAX_ACTIVE_TURN_WORKERS {
            let overflow = active.len().saturating_sub(MAX_ACTIVE_TURN_WORKERS);
            let mut stale_ids: Vec<_> = map
                .values()
                .filter(|record| {
                    !record.archived
                        && matches!(
                            record.status,
                            TurnWorkStatus::Completed
                                | TurnWorkStatus::Failed
                                | TurnWorkStatus::Cancelled
                        )
                })
                .map(|record| (record.updated_at, record.work_id.clone()))
                .collect();
            stale_ids.sort_by_key(|(updated, _)| *updated);
            for (_, work_id) in stale_ids.into_iter().take(overflow) {
                if let Some(entry) = map.get_mut(&work_id) {
                    entry.archived = true;
                    entry.result_text = None;
                    entry.worker_scratch = None;
                    entry.updated_at = Utc::now();
                    changed.push(entry.clone());
                }
            }
        }
        changed
    }

    pub fn insert(&self, record: TurnWorkRecord) {
        if let Err(error) = self.try_insert(record) {
            Self::report_persistence_error("insert", "unknown", &error);
        }
    }

    pub fn try_insert(
        &self,
        record: TurnWorkRecord,
    ) -> Result<(), crate::persistence::PersistenceError> {
        let Ok((_session, _mutation)) =
            crate::session_deletion::acquire_mutation_for_str(&record.session_id)
        else {
            return Err(crate::persistence::PersistenceError::new(
                crate::persistence::PersistenceErrorKind::Conflict,
                "session is deleting",
            ));
        };
        let work_id = record.work_id.clone();
        let stasis_job_id = record.stasis_job_id.clone();
        let mut guard = self.records.lock().expect("turn worker records");
        let mut candidate = guard.clone();
        candidate.insert(work_id.clone(), record);
        self.persist_candidate(&mut candidate, &work_id)?;
        *guard = candidate;
        drop(guard);
        Self::notify_turn_worker_changed(&work_id, stasis_job_id.as_deref());
        Ok(())
    }

    pub fn try_insert_bound(
        &self,
        record: TurnWorkRecord,
    ) -> Result<(), BoundWorkshopAdmissionError> {
        debug_assert_eq!(record.disposition, TurnWorkDisposition::Bound);
        let Ok((_session, _mutation)) =
            crate::session_deletion::acquire_mutation_for_str(&record.session_id)
        else {
            return Err(BoundWorkshopAdmissionError::SessionDeleting);
        };
        let work_id = record.work_id.clone();
        let stasis_job_id = record.stasis_job_id.clone();
        let mut records = self.records.lock().expect("turn worker records");
        let mut candidate_records = records.clone();
        if let Some(active) = candidate_records.values().find(|candidate| {
            is_active_bound(candidate) && candidate.session_id == record.session_id
        }) {
            return Err(BoundWorkshopAdmissionError::ActiveGeneration {
                work_id: active.work_id.clone(),
            });
        }
        candidate_records.insert(work_id.clone(), record);
        self.persist_candidate(&mut candidate_records, &work_id)
            .map_err(|error| BoundWorkshopAdmissionError::Persistence(error.to_string()))?;
        *records = candidate_records;
        drop(records);
        Self::notify_turn_worker_changed(&work_id, stasis_job_id.as_deref());
        Ok(())
    }

    /// Admit one worker whose identity is derived from a signed Stasis grant.
    /// Exact retries reuse the existing record; changed work under the same
    /// identity fails closed.
    pub fn try_insert_delegated(
        &self,
        record: TurnWorkRecord,
    ) -> Result<bool, DelegatedWorkAdmissionError> {
        debug_assert_eq!(record.disposition, TurnWorkDisposition::Delegated);
        let Ok((_session, _mutation)) =
            crate::session_deletion::acquire_mutation_for_str(&record.session_id)
        else {
            return Err(DelegatedWorkAdmissionError::SessionDeleting);
        };
        let work_id = record.work_id.clone();
        let stasis_job_id = record.stasis_job_id.clone();
        let mut records = self.records.lock().expect("turn worker records");
        if let Some(existing) = records.get(&work_id) {
            let matches = existing.disposition == TurnWorkDisposition::Delegated
                && existing.session_id == record.session_id
                && existing.identity_user_id == record.identity_user_id
                && existing.parent_turn_correlation_id == record.parent_turn_correlation_id
                && existing.parent_runtime_id == record.parent_runtime_id
                && existing.execution_placement == record.execution_placement
                && existing.worker_spawn_spec == record.worker_spawn_spec
                && existing.task_prompt == record.task_prompt;
            return if matches {
                Ok(false)
            } else {
                Err(DelegatedWorkAdmissionError::ConflictingIdentity)
            };
        }
        let mut candidate_records = records.clone();
        candidate_records.insert(work_id.clone(), record);
        self.persist_candidate(&mut candidate_records, &work_id)
            .map_err(|error| DelegatedWorkAdmissionError::Persistence(error.to_string()))?;
        *records = candidate_records;
        drop(records);
        Self::notify_turn_worker_changed(&work_id, stasis_job_id.as_deref());
        Ok(true)
    }

    pub fn get(&self, work_id: &str) -> Option<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .get(work_id)
            .cloned()
    }

    pub fn list_for_session(&self, session_id: &str) -> Vec<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| record.session_id == session_id && !record.archived)
            .cloned()
            .collect()
    }

    pub fn list_all(&self, limit: usize) -> Vec<TurnWorkRecord> {
        let mut records = self
            .records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| !record.archived)
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|right| std::cmp::Reverse(right.updated_at));
        records.truncate(limit);
        records
    }

    pub fn list_all_unbounded(&self) -> Vec<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| !record.archived)
            .cloned()
            .collect()
    }

    pub fn parallel_cohort(
        &self,
        session_id: &str,
        parent_stream_turn_id: u64,
        parent_turn_id: Option<&str>,
    ) -> Vec<TurnWorkRecord> {
        let key = ParallelCohortKey::new(session_id, parent_stream_turn_id, parent_turn_id);
        let mut records = self
            .records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| key.matches(record))
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.work_id.cmp(&right.work_id))
        });
        records
    }

    /// Claim only a terminal, undelivered cohort. Success must be acknowledged
    /// after host intake; claiming alone never changes durable delivery state.
    pub fn try_claim_parallel_cohort_intake(
        &self,
        session_id: &str,
        parent_stream_turn_id: u64,
        parent_turn_id: Option<&str>,
    ) -> Option<ParallelCohortIntake<'_>> {
        let key = ParallelCohortKey::new(session_id, parent_stream_turn_id, parent_turn_id);
        let guard = self.records.lock().expect("turn worker records");
        let mut records: Vec<_> = guard
            .values()
            .filter(|record| key.matches(record))
            .cloned()
            .collect();
        if records.is_empty()
            || records.iter().any(|record| {
                matches!(
                    record.status,
                    TurnWorkStatus::Pending | TurnWorkStatus::Running
                )
            })
            || records.iter().all(|record| record.synthesis_delivered)
        {
            return None;
        }
        if !self
            .parallel_intakes
            .lock()
            .expect("parallel intakes")
            .insert(key.clone())
        {
            return None;
        }
        records.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.work_id.cmp(&right.work_id))
        });
        Some(ParallelCohortIntake {
            store: self,
            key,
            records,
        })
    }

    /// Only the job responsible for an available intake needs a retry. Siblings
    /// still executing, or the job already running intake, will drive completion.
    pub fn parallel_intake_needs_retry(&self, record: &TurnWorkRecord) -> bool {
        let key = ParallelCohortKey::new(
            &record.session_id,
            record.parent_stream_turn_id,
            record.parent_turn_correlation_id.as_deref(),
        );
        let guard = self.records.lock().expect("turn worker records");
        let cohort: Vec<_> = guard
            .values()
            .filter(|worker| key.matches(worker))
            .collect();
        !cohort.is_empty()
            && cohort.iter().any(|worker| !worker.synthesis_delivered)
            && cohort.iter().all(|worker| {
                !matches!(
                    worker.status,
                    TurnWorkStatus::Pending | TurnWorkStatus::Running
                )
            })
            && !self
                .parallel_intakes
                .lock()
                .expect("parallel intakes")
                .contains(&key)
    }

    pub fn list_incomplete(&self) -> Vec<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| {
                !record.archived
                    && (matches!(
                        record.status,
                        TurnWorkStatus::Pending | TurnWorkStatus::Running
                    ) || (!record.synthesis_delivered
                        && (record.status == TurnWorkStatus::Completed
                            || (record.disposition == TurnWorkDisposition::Parallel
                                && matches!(
                                    record.status,
                                    TurnWorkStatus::Failed | TurnWorkStatus::Cancelled
                                )))))
            })
            .cloned()
            .collect()
    }

    pub fn update<F>(&self, work_id: &str, update: F) -> Option<TurnWorkRecord>
    where
        F: FnOnce(&mut TurnWorkRecord),
    {
        let session_id = self
            .records
            .lock()
            .expect("turn worker records")
            .get(work_id)?
            .session_id
            .clone();
        let (_session, _mutation) =
            crate::session_deletion::acquire_mutation_for_str(&session_id).ok()?;
        let mut guard = self.records.lock().expect("turn worker records");
        let record = guard.get_mut(work_id)?;
        update(record);
        record.updated_at = Utc::now();
        let cloned = record.clone();
        drop(guard);
        if let Err(error) = self.persist(&cloned.work_id, cloned.stasis_job_id.as_deref()) {
            Self::report_persistence_error("update", &cloned.work_id, &error);
        }
        Some(cloned)
    }

    pub fn try_archive(
        &self,
        work_id: &str,
        purge_body: bool,
    ) -> Result<Option<TurnWorkRecord>, crate::persistence::PersistenceError> {
        let Some(current) = self.get(work_id) else {
            return Ok(None);
        };
        let (_session, _mutation) = crate::session_deletion::acquire_mutation_for_str(
            &current.session_id,
        )
        .map_err(|error| {
            crate::persistence::PersistenceError::new(
                crate::persistence::PersistenceErrorKind::Cancelled,
                error,
            )
        })?;
        let mut guard = self.records.lock().expect("turn worker records");
        let mut candidate = guard.clone();
        let Some(record) = candidate.get_mut(work_id) else {
            return Ok(None);
        };
        record.archived = true;
        record.updated_at = Utc::now();
        if purge_body {
            record.result_text = None;
            record.worker_scratch = None;
        }
        let snapshot = record.clone();
        self.persist_candidate(&mut candidate, work_id)?;
        *guard = candidate;
        drop(guard);
        Self::notify_turn_worker_changed(work_id, snapshot.stasis_job_id.as_deref());
        Ok(Some(snapshot))
    }

    pub fn archive(&self, work_id: &str, purge_body: bool) -> Option<TurnWorkRecord> {
        match self.try_archive(work_id, purge_body) {
            Ok(record) => record,
            Err(error) => {
                Self::report_persistence_error("archive", work_id, &error);
                None
            }
        }
    }

    pub fn delete_session(&self, session_id: &str) -> Result<(), String> {
        let mut guard = self.records.lock().map_err(|error| error.to_string())?;
        let removed_work_ids: Vec<_> = guard
            .values()
            .filter(|record| record.session_id == session_id)
            .map(|record| record.work_id.clone())
            .collect();
        let mut candidate = guard.clone();
        candidate.retain(|_, record| record.session_id != session_id);
        let retained = candidate.keys().cloned().collect::<Vec<_>>();
        (self.admit_mutations)(vec![
            crate::workspace::persist::WorkspaceMutation::RetainTurnWorkers { work_ids: retained },
        ])
        .map_err(|error| error.to_string())?;
        *guard = candidate;
        drop(guard);
        let mut live = self
            .live_cancellations
            .lock()
            .map_err(|error| error.to_string())?;
        for work_id in &removed_work_ids {
            if let Some(cancellation) = live.remove(work_id) {
                cancellation.cancel();
            }
        }
        drop(live);
        Ok(())
    }

    pub fn session_absent_on_disk(session_id: &str) -> Result<bool, String> {
        let projection =
            crate::workspace::persist::persisted_projection().map_err(|error| error.to_string())?;
        Ok(!projection
            .turn_workers
            .values()
            .any(|record| record.session_id == session_id))
    }

    pub fn active_bound_workshop(&self, session_id: &str) -> Option<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| record.session_id == session_id && is_active_bound(record))
            .max_by_key(|record| record.updated_at)
            .cloned()
    }

    pub fn push_steer_exact(
        &self,
        session_id: &str,
        work_id: &str,
        text: String,
        speaker_profile_id: Option<String>,
    ) -> Result<TurnWorkRecord, BoundWorkshopMutationError> {
        let Ok((_session, _mutation)) =
            crate::session_deletion::acquire_mutation_for_str(session_id)
        else {
            return Err(BoundWorkshopMutationError::SessionDeleting);
        };
        let mut records = self.records.lock().expect("turn worker records");
        let active_work_id = records
            .values()
            .find(|record| record.session_id == session_id && is_active_bound(record))
            .map(|record| record.work_id.clone());
        if active_work_id.as_deref() != Some(work_id) {
            return Err(BoundWorkshopMutationError::StaleGeneration { active_work_id });
        }
        let mut candidate = records.clone();
        let record = candidate
            .get_mut(work_id)
            .ok_or(BoundWorkshopMutationError::MissingGeneration)?;
        record.steer_messages.push(WorkshopSteerMessage {
            control_id: None,
            text,
            at: Utc::now(),
            speaker_profile_id,
        });
        record.updated_at = Utc::now();
        let updated = record.clone();
        self.persist_candidate(&mut candidate, &updated.work_id)
            .map_err(|error| BoundWorkshopMutationError::Persistence(error.to_string()))?;
        *records = candidate;
        drop(records);
        Self::notify_turn_worker_changed(&updated.work_id, updated.stasis_job_id.as_deref());
        Ok(updated)
    }

    pub fn push_delegated_steer_exact(
        &self,
        work_id: &str,
        identity_user_id: &str,
        control_id: &str,
        text: String,
        speaker_profile_id: Option<String>,
    ) -> Result<TurnWorkRecord, DelegatedWorkControlError> {
        let session_id = self
            .get(work_id)
            .ok_or(DelegatedWorkControlError::MissingWork)?
            .session_id;
        let Ok((_session, _mutation)) =
            crate::session_deletion::acquire_mutation_for_str(&session_id)
        else {
            return Err(DelegatedWorkControlError::SessionDeleting);
        };
        let mut records = self.records.lock().expect("turn worker records");
        let mut candidate = records.clone();
        let record = candidate
            .get_mut(work_id)
            .ok_or(DelegatedWorkControlError::MissingWork)?;
        if record.disposition != TurnWorkDisposition::Delegated {
            return Err(DelegatedWorkControlError::WrongDisposition);
        }
        if record.identity_user_id.as_deref() != Some(identity_user_id) {
            return Err(DelegatedWorkControlError::ForeignIdentity);
        }
        if !matches!(
            record.status,
            TurnWorkStatus::Pending | TurnWorkStatus::Running
        ) {
            return Err(DelegatedWorkControlError::NotActive);
        }
        if record
            .processed_steer_control_ids
            .iter()
            .any(|existing| existing == control_id)
        {
            return Ok(record.clone());
        }
        record.steer_messages.push(WorkshopSteerMessage {
            control_id: Some(control_id.to_string()),
            text,
            at: Utc::now(),
            speaker_profile_id,
        });
        record
            .processed_steer_control_ids
            .push(control_id.to_string());
        if record.processed_steer_control_ids.len() > 128 {
            let overflow = record.processed_steer_control_ids.len() - 128;
            record.processed_steer_control_ids.drain(..overflow);
        }
        record.updated_at = Utc::now();
        let updated = record.clone();
        self.persist_candidate(&mut candidate, &updated.work_id)
            .map_err(|error| DelegatedWorkControlError::Persistence(error.to_string()))?;
        *records = candidate;
        drop(records);
        Self::notify_turn_worker_changed(&updated.work_id, updated.stasis_job_id.as_deref());
        Ok(updated)
    }

    pub fn cancel_delegated_exact(
        &self,
        work_id: &str,
        identity_user_id: &str,
    ) -> Result<TurnWorkRecord, DelegatedWorkControlError> {
        let current = self
            .get(work_id)
            .ok_or(DelegatedWorkControlError::MissingWork)?;
        if current.disposition != TurnWorkDisposition::Delegated {
            return Err(DelegatedWorkControlError::WrongDisposition);
        }
        if current.identity_user_id.as_deref() != Some(identity_user_id) {
            return Err(DelegatedWorkControlError::ForeignIdentity);
        }
        self.cancel_exact(&current.session_id, work_id)
            .map_err(|error| match error {
                TurnWorkerMutationError::SessionDeleting => {
                    DelegatedWorkControlError::SessionDeleting
                }
                TurnWorkerMutationError::MissingWork => DelegatedWorkControlError::MissingWork,
                TurnWorkerMutationError::ForeignSession => {
                    DelegatedWorkControlError::ForeignIdentity
                }
                TurnWorkerMutationError::Persistence(error) => {
                    DelegatedWorkControlError::Persistence(error)
                }
            })
    }

    pub fn cancel_exact(
        &self,
        session_id: &str,
        work_id: &str,
    ) -> Result<TurnWorkRecord, TurnWorkerMutationError> {
        let Ok((_session, _mutation)) =
            crate::session_deletion::acquire_mutation_for_str(session_id)
        else {
            return Err(TurnWorkerMutationError::SessionDeleting);
        };
        let mut records = self.records.lock().expect("turn worker records");
        let mut candidate = records.clone();
        let record = candidate
            .get_mut(work_id)
            .ok_or(TurnWorkerMutationError::MissingWork)?;
        if record.session_id != session_id {
            return Err(TurnWorkerMutationError::ForeignSession);
        }
        if matches!(
            record.status,
            TurnWorkStatus::Pending | TurnWorkStatus::Running
        ) {
            record.status = TurnWorkStatus::Cancelled;
            record.updated_at = Utc::now();
        }
        let updated = record.clone();
        self.persist_candidate(&mut candidate, &updated.work_id)
            .map_err(|error| TurnWorkerMutationError::Persistence(error.to_string()))?;
        *records = candidate;
        drop(records);
        if let Some(cancellation) = self
            .live_cancellations
            .lock()
            .expect("turn worker cancellations")
            .get(work_id)
            .cloned()
        {
            cancellation.cancel();
        }
        Self::notify_turn_worker_changed(&updated.work_id, updated.stasis_job_id.as_deref());
        Ok(updated)
    }

    pub fn register_execution(
        self: &Arc<Self>,
        work_id: &str,
    ) -> Result<WorkerExecutionLease, WorkerExecutionRegistrationError> {
        let records = self.records.lock().expect("turn worker records");
        let record = records
            .get(work_id)
            .ok_or(WorkerExecutionRegistrationError::MissingWork)?;
        if !matches!(
            record.status,
            TurnWorkStatus::Pending | TurnWorkStatus::Running
        ) {
            return Err(WorkerExecutionRegistrationError::NotActive);
        }
        let mut live = self
            .live_cancellations
            .lock()
            .expect("turn worker cancellations");
        if live.contains_key(work_id) {
            return Err(WorkerExecutionRegistrationError::AlreadyRunning);
        }
        if live.len() >= MAX_ACTIVE_TURN_WORKERS {
            return Err(WorkerExecutionRegistrationError::AtCapacity {
                limit: MAX_ACTIVE_TURN_WORKERS,
            });
        }
        let cancellation = Arc::new(CancellationToken::new());
        live.insert(work_id.to_string(), cancellation.clone());
        drop(live);
        drop(records);
        Ok(WorkerExecutionLease {
            store: self.clone(),
            work_id: work_id.to_string(),
            cancellation,
        })
    }

    #[cfg(test)]
    pub(crate) fn live_execution_count(&self) -> usize {
        self.live_cancellations
            .lock()
            .expect("turn worker cancellations")
            .len()
    }

    pub fn drain_steer_messages(&self, work_id: &str) -> Vec<WorkshopSteerMessage> {
        let mut drained = Vec::new();
        self.update(work_id, |record| {
            drained = std::mem::take(&mut record.steer_messages);
        });
        drained
    }

    pub fn is_work_cancelled(&self, work_id: &str) -> bool {
        self.get(work_id)
            .is_some_and(|record| record.status == TurnWorkStatus::Cancelled)
    }
}

impl medousa_runtime::DelegationControlPort for TurnWorkerStore {
    fn is_cancelled(&self, work_id: &str) -> bool {
        TurnWorkerStore::is_work_cancelled(self, work_id)
    }

    fn drain_steer_messages(&self, work_id: &str) -> Vec<medousa_runtime::TurnSteerMessage> {
        TurnWorkerStore::drain_steer_messages(self, work_id)
            .into_iter()
            .map(|message| medousa_runtime::TurnSteerMessage {
                text: message.text,
                speaker_profile_id: message.speaker_profile_id,
            })
            .collect()
    }
}

fn is_active_bound(record: &TurnWorkRecord) -> bool {
    !record.archived
        && record.disposition == TurnWorkDisposition::Bound
        && matches!(
            record.status,
            TurnWorkStatus::Pending | TurnWorkStatus::Running
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_continuation_route_wins_over_worker_model_and_round_trips() {
        let mut record = test_record("work-route", "session-route", 1, TurnWorkStatus::Completed);
        record.provider = "worker-provider".to_string();
        record.model = "worker-model".to_string();
        record.parent_continuation_route =
            Some(ParentContinuationRoute::from_stage_route(&StageRoute {
                role: "final_response".to_string(),
                provider: "host-provider".to_string(),
                model: "host-model".to_string(),
                policy_profile: "careful".to_string(),
                fallback_chain: vec!["safe-default".to_string()],
            }));

        let serialized = serde_json::to_vec(&record).expect("serialize durable route");
        let restored: TurnWorkRecord =
            serde_json::from_slice(&serialized).expect("restore durable route");
        let route = continuation_route_for_record(
            &restored,
            &crate::stage_routing::StageRoutingMatrix::default_for("legacy", "default")
                .final_response,
        )
        .expect("parent route");
        assert_eq!(route.provider, "host-provider");
        assert_eq!(route.model, "host-model");
        assert_eq!(route.policy_profile, "careful");
        assert_eq!(route.fallback_chain, ["safe-default"]);
    }

    #[test]
    fn legacy_parallel_route_uses_parent_spawn_contract_not_worker_target() {
        let mut record = test_record(
            "work-legacy-route",
            "session-legacy",
            1,
            TurnWorkStatus::Completed,
        );
        record.provider = "worker-provider".to_string();
        record.model = "worker-model".to_string();
        record.worker_spawn_spec = Some(crate::delegated_task::WorkerSpawnSpec {
            schema_version: crate::delegated_task::WORKER_SPAWN_SPEC_SCHEMA_VERSION,
            intent: "research".to_string(),
            task: "task".to_string(),
            user_ack: "On it".to_string(),
            manuscript_ids: Vec::new(),
            manuscript: None,
            stage_role: None,
            model_hint: None,
            parent: crate::delegated_task::WorkerParentSpec {
                stream_turn_id: 1,
                turn_correlation_id: "turn-parent".to_string(),
                agent_mode: None,
                original_user_prompt: "question".to_string(),
                provider: "host-provider".to_string(),
                model: "host-model".to_string(),
                response_depth_mode: "normal".to_string(),
                code_work_id: None,
                bot: None,
                supports_ui_artifacts: false,
                supports_liquid_markdown: false,
                supports_browser_host: false,
            },
            code_project: None,
            execution_placement: Default::default(),
            world_ids: Vec::new(),
            max_tool_rounds: 8,
            tools: crate::delegated_task::WorkerToolRequest { names: Vec::new() },
        });

        let route = continuation_route_for_record(
            &record,
            &crate::stage_routing::StageRoutingMatrix::default_for("legacy", "default")
                .final_response,
        )
        .expect("legacy parent route");
        assert_eq!(route.provider, "host-provider");
        assert_eq!(route.model, "host-model");
    }

    #[test]
    fn legacy_bound_route_is_explicit_and_malformed_contract_never_falls_back() {
        let mut record = test_record("legacy-bound", "owner", 1, TurnWorkStatus::Completed);
        record.provider = "worker-provider".into();
        record.model = "worker-small".into();
        let fallback =
            crate::stage_routing::StageRoutingMatrix::default_for("host-provider", "host-fallback")
                .final_response;
        assert_eq!(
            continuation_route_for_record(&record, &fallback),
            Some(fallback.clone())
        );
        let mut contract = ParentContinuationRoute::from_stage_route(&fallback);
        contract.schema_version = 999;
        record.parent_continuation_route = Some(contract);
        assert!(continuation_route_for_record(&record, &fallback).is_none());
        record
            .parent_continuation_route
            .as_mut()
            .unwrap()
            .schema_version = 1;
        record
            .parent_continuation_route
            .as_mut()
            .unwrap()
            .stage_role = "worker".into();
        assert!(continuation_route_for_record(&record, &fallback).is_none());
    }

    #[test]
    fn parent_continuation_route_round_trips_as_durable_record_data() {
        let mut record = test_record(
            "work-route-roundtrip",
            "session-route-roundtrip",
            7,
            TurnWorkStatus::Completed,
        );
        record.provider = "worker-provider".into();
        record.model = "worker-model".into();
        record.parent_continuation_route = Some(ParentContinuationRoute {
            schema_version: ParentContinuationRoute::SCHEMA_VERSION,
            stage_role: "final_response".into(),
            policy_profile: "host-careful".into(),
            provider: "host-provider".into(),
            model: "host-model".into(),
            fallback_chain: vec!["host-fallback".into()],
        });
        let reopened: TurnWorkRecord =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        let fallback =
            crate::stage_routing::StageRoutingMatrix::default_for("changed", "changed-model")
                .final_response;
        let route = continuation_route_for_record(&reopened, &fallback)
            .expect("captured parent route survives serialization");
        assert_eq!(route.provider, "host-provider");
        assert_eq!(route.model, "host-model");
        assert_eq!(route.policy_profile, "host-careful");
        assert_eq!(route.fallback_chain, ["host-fallback"]);
    }

    #[test]
    fn stale_execution_lease_cannot_remove_replacement_token() {
        let store = Arc::new(TurnWorkerStore::empty_for_tests());
        let stale_token = Arc::new(CancellationToken::new());
        store
            .live_cancellations
            .lock()
            .unwrap()
            .insert("work-1".to_string(), stale_token.clone());
        let stale = WorkerExecutionLease {
            store: store.clone(),
            work_id: "work-1".to_string(),
            cancellation: stale_token,
        };
        let replacement = Arc::new(CancellationToken::new());
        store
            .live_cancellations
            .lock()
            .unwrap()
            .insert("work-1".to_string(), replacement.clone());

        drop(stale);

        let current = store
            .live_cancellations
            .lock()
            .unwrap()
            .get("work-1")
            .cloned()
            .unwrap();
        assert!(Arc::ptr_eq(&current, &replacement));
    }

    #[test]
    fn worker_admission_is_not_published_when_persistence_queue_rejects_it() {
        let store = TurnWorkerStore::with_admitter(Arc::new(|_| {
            Err(crate::persistence::PersistenceError::new(
                crate::persistence::PersistenceErrorKind::Overloaded,
                "test queue full",
            ))
        }));
        let record = test_record(
            "work-not-durable",
            "session-admission-test",
            1,
            TurnWorkStatus::Pending,
        );

        let error = store.try_insert(record).expect_err("admission must fail");

        assert_eq!(error.to_string(), "test queue full");
        assert!(store.get("work-not-durable").is_none());
    }

    fn test_record(
        work_id: &str,
        session_id: &str,
        parent_stream_turn_id: u64,
        status: TurnWorkStatus,
    ) -> TurnWorkRecord {
        TurnWorkRecord {
            work_id: work_id.to_string(),
            session_id: session_id.to_string(),
            identity_user_id: None,
            parent_turn_correlation_id: None,
            parent_stream_turn_id,
            parent_runtime_id: "runtime-test".to_string(),
            parent_continuation_route: None,
            execution_placement: Default::default(),
            task_execution_grant: None,
            worker_spawn_spec: None,
            intent: "research".to_string(),
            task_prompt: "task".to_string(),
            status,
            result_text: Some("done".to_string()),
            tool_names: Vec::new(),
            termination_reason: None,
            needs_synthesis: None,
            error: None,
            user_ack: "On it".to_string(),
            provider: "openai".to_string(),
            model: "gpt".to_string(),
            response_depth_mode: "normal".to_string(),
            max_tool_rounds: 8,
            delivery_target: None,
            parent_user_prompt: Some("user prompt".to_string()),
            parent_agent_mode: Some("general".to_string()),
            parent_code_work_id: None,
            handoff_capsule: None,
            worker_scratch: None,
            synthesis_delivered: false,
            stasis_job_id: None,
            thread_id: None,
            stage_role: None,
            model_hint: None,
            manuscript_id: None,
            branch_group_id: None,
            archived: false,
            disposition: TurnWorkDisposition::Parallel,
            steer_messages: Vec::new(),
            processed_steer_control_ids: Vec::new(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            live_tool_activity: Vec::new(),
            live_thinking: String::new(),
            live_output: String::new(),
            thinking_started_at: None,
            thinking_finished_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn legacy_record_defaults_to_unknown_execution_provenance() {
        let mut value = serde_json::to_value(test_record(
            "work-legacy",
            "sess-legacy",
            0,
            TurnWorkStatus::Completed,
        ))
        .expect("serialize record");
        let object = value.as_object_mut().expect("record object");
        object.remove("parent_runtime_id");
        object.remove("execution_placement");

        let record: TurnWorkRecord = serde_json::from_value(value).expect("legacy record");
        assert_eq!(
            record.parent_runtime_id,
            crate::workshop_contract::UNKNOWN_EXECUTION_RUNTIME_ID
        );
        assert_eq!(
            record.execution_placement.resolution_reason,
            crate::workshop_contract::ExecutionResolutionReason::LegacyUnknown
        );
    }

    fn seed(store: &TurnWorkerStore, record: TurnWorkRecord) {
        store
            .records
            .lock()
            .expect("turn worker records")
            .insert(record.work_id.clone(), record);
    }

    #[test]
    fn parallel_cohort_waits_while_a_sibling_is_running() {
        let store = TurnWorkerStore::empty_for_tests();
        seed(
            &store,
            test_record("work-a", "sess-c", 7, TurnWorkStatus::Completed),
        );
        seed(
            &store,
            test_record("work-b", "sess-c", 7, TurnWorkStatus::Running),
        );
        assert!(
            store
                .try_claim_parallel_cohort_intake("sess-c", 7, None)
                .is_none()
        );
        assert_eq!(store.parallel_cohort("sess-c", 7, None).len(), 2);
        assert!(!store.parallel_intake_needs_retry(&store.get("work-a").unwrap()));
    }

    #[test]
    fn parallel_cohort_claims_once_when_all_terminal() {
        let store = TurnWorkerStore::empty_for_tests();
        let mut failed = test_record("work-a", "sess-c", 9, TurnWorkStatus::Failed);
        failed.result_text = None;
        failed.error = Some("boom".to_string());
        seed(&store, failed);
        seed(
            &store,
            test_record("work-b", "sess-c", 9, TurnWorkStatus::Completed),
        );
        let claimed = store
            .try_claim_parallel_cohort_intake("sess-c", 9, None)
            .expect("claim");
        assert_eq!(claimed.len(), 2);
        assert!(claimed.iter().all(|record| !record.synthesis_delivered));
        assert!(
            store
                .try_claim_parallel_cohort_intake("sess-c", 9, None)
                .is_none()
        );
        claimed.acknowledge();
        assert!(store.get("work-a").unwrap().synthesis_delivered);
        assert!(
            store
                .try_claim_parallel_cohort_intake("sess-c", 9, None)
                .is_none()
        );
    }

    #[test]
    fn failed_or_cancelled_intake_can_be_claimed_again_without_reexecuting_peer() {
        let store = TurnWorkerStore::empty_for_tests();
        seed(
            &store,
            test_record("peer", "sess-c", 1, TurnWorkStatus::Completed),
        );
        let claim = store
            .try_claim_parallel_cohort_intake("sess-c", 1, None)
            .unwrap();
        assert!(
            store
                .try_claim_parallel_cohort_intake("sess-c", 1, None)
                .is_none()
        );
        assert!(!store.get("peer").unwrap().synthesis_delivered);
        assert!(!store.parallel_intake_needs_retry(&store.get("peer").unwrap()));
        drop(claim); // includes cancellation/unwind of the host-resume future
        assert!(store.parallel_intake_needs_retry(&store.get("peer").unwrap()));
        let retry = store
            .try_claim_parallel_cohort_intake("sess-c", 1, None)
            .unwrap();
        assert_eq!(retry[0].status, TurnWorkStatus::Completed);
        assert!(!retry[0].synthesis_delivered);
        drop(retry);
        // Durable state survives a restart; process-local claims do not.
        let reopened = TurnWorkerStore::empty_for_tests();
        seed(&reopened, store.get("peer").unwrap());
        assert!(
            reopened
                .try_claim_parallel_cohort_intake("sess-c", 1, None)
                .is_some()
        );
    }

    #[test]
    fn correlated_parents_do_not_share_intake_when_stream_ids_repeat() {
        let store = TurnWorkerStore::empty_for_tests();
        let mut first = test_record("peer-a", "sess-c", 1, TurnWorkStatus::Completed);
        first.parent_turn_correlation_id = Some("parent-a".into());
        seed(&store, first);
        let mut next = test_record("peer-b", "sess-c", 1, TurnWorkStatus::Running);
        next.parent_turn_correlation_id = Some("parent-b".into());
        seed(&store, next);
        seed(
            &store,
            test_record("legacy", "sess-c", 1, TurnWorkStatus::Running),
        );
        let intake = store
            .try_claim_parallel_cohort_intake("sess-c", 1, Some("parent-a"))
            .unwrap();
        assert_eq!(intake.len(), 1);
        assert_eq!(intake[0].work_id, "peer-a");
        assert!(
            store
                .try_claim_parallel_cohort_intake("sess-c", 1, Some("parent-b"))
                .is_none()
        );
        assert!(
            store
                .try_claim_parallel_cohort_intake("sess-c", 1, None)
                .is_none()
        );
    }

    #[test]
    fn bound_workers_are_not_part_of_a_parallel_cohort() {
        let store = TurnWorkerStore::empty_for_tests();
        let mut bound = test_record("work-bound", "sess-c", 3, TurnWorkStatus::Completed);
        bound.disposition = TurnWorkDisposition::Bound;
        seed(&store, bound);
        seed(
            &store,
            test_record("work-p", "sess-c", 3, TurnWorkStatus::Completed),
        );
        let claimed = store
            .try_claim_parallel_cohort_intake("sess-c", 3, None)
            .expect("claim");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].work_id, "work-p");
    }

    #[test]
    fn list_incomplete_retries_undelivered_failed_parallel_not_bound() {
        let store = TurnWorkerStore::empty_for_tests();
        seed(
            &store,
            test_record("work-f", "sess-c", 1, TurnWorkStatus::Failed),
        );
        let mut bound_failed = test_record("work-bf", "sess-c", 1, TurnWorkStatus::Failed);
        bound_failed.disposition = TurnWorkDisposition::Bound;
        seed(&store, bound_failed);
        let mut delivered = test_record("work-d", "sess-c", 2, TurnWorkStatus::Failed);
        delivered.synthesis_delivered = true;
        seed(&store, delivered);
        let ids: Vec<String> = store
            .list_incomplete()
            .into_iter()
            .map(|record| record.work_id)
            .collect();
        assert!(ids.contains(&"work-f".to_string()));
        assert!(!ids.contains(&"work-bf".to_string()));
        assert!(!ids.contains(&"work-d".to_string()));
    }

    #[test]
    fn delegated_steering_is_owned_and_idempotent_after_delivery() {
        let store = TurnWorkerStore::empty_for_tests();
        let mut record = test_record("work-remote", "sess-derived", 4, TurnWorkStatus::Running);
        record.disposition = TurnWorkDisposition::Delegated;
        record.identity_user_id = Some("peer:phone-a".to_string());
        seed(&store, record);

        assert!(matches!(
            store.push_delegated_steer_exact(
                "work-remote",
                "peer:phone-b",
                "control-1",
                "wrong peer".to_string(),
                None,
            ),
            Err(DelegatedWorkControlError::ForeignIdentity)
        ));
        let updated = store
            .push_delegated_steer_exact(
                "work-remote",
                "peer:phone-a",
                "control-1",
                "inspect the parser".to_string(),
                Some("peer:phone-a".to_string()),
            )
            .expect("queue steer");
        assert_eq!(updated.steer_messages.len(), 1);
        assert_eq!(store.drain_steer_messages("work-remote").len(), 1);

        let retried = store
            .push_delegated_steer_exact(
                "work-remote",
                "peer:phone-a",
                "control-1",
                "inspect the parser".to_string(),
                Some("peer:phone-a".to_string()),
            )
            .expect("idempotent retry");
        assert!(retried.steer_messages.is_empty());
    }

    #[test]
    fn delegated_cancellation_cannot_cross_peer_identity() {
        let store = TurnWorkerStore::empty_for_tests();
        let mut record = test_record("work-remote", "sess-derived", 4, TurnWorkStatus::Running);
        record.disposition = TurnWorkDisposition::Delegated;
        record.identity_user_id = Some("peer:phone-a".to_string());
        seed(&store, record);

        assert!(matches!(
            store.cancel_delegated_exact("work-remote", "peer:phone-b"),
            Err(DelegatedWorkControlError::ForeignIdentity)
        ));
        assert_eq!(
            store
                .cancel_delegated_exact("work-remote", "peer:phone-a")
                .expect("owner cancellation")
                .status,
            TurnWorkStatus::Cancelled
        );
    }
}
