//! Durable turn work records (host/worker bus).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::agent_runtime::turn_context::WorkerHandoffCapsule;
use crate::session;
use crate::turn_continuation::StoredDeliveryTarget;
use crate::workshop_contract::{
    ExecutionPlacementResolution, default_unknown_runtime_id,
};

const TURN_WORKERS_FILE: &str = "workspace/turn_workers.json";
const LEGACY_TURN_WORKERS_FILE: &str = "turn_workers.json";
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
    /// Requested and resolved execution target captured before enqueue.
    #[serde(default)]
    pub execution_placement: ExecutionPlacementResolution,
    pub intent: String,
    pub task_prompt: String,
    pub status: TurnWorkStatus,
    pub result_text: Option<String>,
    pub tool_names: Vec<String>,
    pub termination_reason: Option<String>,
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
    /// Snapshotted host agent mode (`general` / `teacher` / `instant` / `coder`) so resume stays in-lane.
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
    ) -> Self {
        let now = Utc::now();
        Self {
            work_id,
            session_id,
            identity_user_id: Some(identity_user_id),
            parent_turn_correlation_id: Some(parent_turn_correlation_id),
            parent_stream_turn_id: 0,
            parent_runtime_id,
            execution_placement,
            intent: "research".to_string(),
            task_prompt,
            status: TurnWorkStatus::Pending,
            result_text: None,
            tool_names: Vec::new(),
            termination_reason: None,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegatedWorkAdmissionError {
    SessionDeleting,
    ConflictingIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundWorkshopMutationError {
    SessionDeleting,
    MissingGeneration,
    StaleGeneration { active_work_id: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnWorkerMutationError {
    SessionDeleting,
    MissingWork,
    ForeignSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerExecutionRegistrationError {
    MissingWork,
    NotActive,
    AlreadyRunning,
    AtCapacity { limit: usize },
}

impl Default for TurnWorkerStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnWorkerStore {
    pub fn new() -> Self {
        let store = Self {
            records: Mutex::new(HashMap::new()),
            live_cancellations: Mutex::new(HashMap::new()),
        };
        store.reload_from_disk();
        store
    }

    #[cfg(test)]
    pub(crate) fn empty_for_tests() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            live_cancellations: Mutex::new(HashMap::new()),
        }
    }

    fn path() -> PathBuf {
        Self::path_in(&session::medousa_data_dir())
    }

    fn legacy_path() -> PathBuf {
        Self::legacy_path_in(&session::medousa_data_dir())
    }

    fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(TURN_WORKERS_FILE)
    }

    fn legacy_path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(LEGACY_TURN_WORKERS_FILE)
    }

    fn reload_from_disk(&self) {
        let _ = fs::create_dir_all(session::medousa_data_dir().join("workspace"));
        if let Some(projection) = crate::workspace::persist::startup_projection() {
            *self.records.lock().expect("turn worker records") = projection.turn_workers;
            return;
        }
        self.reload_from_paths(&Self::path(), &Self::legacy_path());
    }

    fn reload_from_paths(&self, canonical_path: &Path, legacy_path: &Path) {
        let (raw, migrated_legacy) = match fs::read_to_string(canonical_path) {
            Ok(raw) => (raw, false),
            Err(_) => match fs::read_to_string(legacy_path) {
                Ok(raw) => (raw, true),
                Err(_) => return,
            },
        };
        let Ok(map) = serde_json::from_str::<HashMap<String, TurnWorkRecord>>(&raw) else {
            return;
        };
        *self.records.lock().expect("turn worker records") = map;
        if migrated_legacy {
            if let Some(parent) = canonical_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(err) = fs::write(canonical_path, raw) {
                eprintln!(
                    "turn_worker_store: legacy snapshot migration failed path={} error={err}",
                    canonical_path.display()
                );
            }
        }
    }

    fn persist(&self, work_id: &str, stasis_job_id: Option<&str>) {
        let mut guard = self.records.lock().expect("turn worker records");
        let mut changed = Self::prune_map(&mut guard);
        if let Some(record) = guard.get(work_id).cloned() {
            changed.push(record);
        }
        let retained = guard.keys().cloned().collect::<Vec<_>>();
        drop(guard);
        changed.sort_by(|left, right| left.work_id.cmp(&right.work_id));
        changed.dedup_by(|left, right| left.work_id == right.work_id);
        for record in changed {
            let _ = crate::workspace::persist::queue_mutation(
                crate::workspace::persist::WorkspaceMutation::UpsertTurnWorker {
                    record: Box::new(record),
                },
            );
        }
        let _ = crate::workspace::persist::queue_mutation(
            crate::workspace::persist::WorkspaceMutation::RetainTurnWorkers { work_ids: retained },
        );
        Self::notify_turn_worker_changed(work_id, stasis_job_id);
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
        let Ok((_session, _mutation)) =
            crate::session_deletion::acquire_mutation_for_str(&record.session_id)
        else {
            tracing::warn!(session_id = %record.session_id, "rejected turn-worker insert for deleting session");
            return;
        };
        let work_id = record.work_id.clone();
        let stasis_job_id = record.stasis_job_id.clone();
        let mut guard = self.records.lock().expect("turn worker records");
        guard.insert(work_id.clone(), record);
        drop(guard);
        self.persist(&work_id, stasis_job_id.as_deref());
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
        if let Some(active) = records.values().find(|candidate| {
            is_active_bound(candidate) && candidate.session_id == record.session_id
        }) {
            return Err(BoundWorkshopAdmissionError::ActiveGeneration {
                work_id: active.work_id.clone(),
            });
        }
        records.insert(work_id.clone(), record);
        drop(records);
        self.persist(&work_id, stasis_job_id.as_deref());
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
                && existing.task_prompt == record.task_prompt;
            return if matches {
                Ok(false)
            } else {
                Err(DelegatedWorkAdmissionError::ConflictingIdentity)
            };
        }
        records.insert(work_id.clone(), record);
        drop(records);
        self.persist(&work_id, stasis_job_id.as_deref());
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
    ) -> Vec<TurnWorkRecord> {
        let mut records = self
            .records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| {
                !record.archived
                    && record.disposition == TurnWorkDisposition::Parallel
                    && record.session_id == session_id
                    && record.parent_stream_turn_id == parent_stream_turn_id
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.work_id.cmp(&right.work_id))
        });
        records
    }

    /// Atomically claim a finished parallel spawn cohort for one host-resume turn.
    ///
    /// Returns `None` while any sibling is still pending/running, when the cohort
    /// is empty, or when every member is already marked delivered.
    pub fn try_claim_parallel_cohort_intake(
        &self,
        session_id: &str,
        parent_stream_turn_id: u64,
    ) -> Option<Vec<TurnWorkRecord>> {
        let mut guard = self.records.lock().expect("turn worker records");
        let ids: Vec<String> = guard
            .values()
            .filter(|record| {
                !record.archived
                    && record.disposition == TurnWorkDisposition::Parallel
                    && record.session_id == session_id
                    && record.parent_stream_turn_id == parent_stream_turn_id
            })
            .map(|record| record.work_id.clone())
            .collect();
        if ids.is_empty() {
            return None;
        }
        let terminal = |status: TurnWorkStatus| {
            matches!(
                status,
                TurnWorkStatus::Completed | TurnWorkStatus::Failed | TurnWorkStatus::Cancelled
            )
        };
        if ids
            .iter()
            .any(|id| guard.get(id).is_none_or(|record| !terminal(record.status)))
        {
            return None;
        }
        if ids.iter().all(|id| {
            guard
                .get(id)
                .is_some_and(|record| record.synthesis_delivered)
        }) {
            return None;
        }
        let now = Utc::now();
        let mut claimed = Vec::new();
        for id in &ids {
            let Some(record) = guard.get_mut(id) else {
                continue;
            };
            record.synthesis_delivered = true;
            record.updated_at = now;
            claimed.push(record.clone());
        }
        drop(guard);
        claimed.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.work_id.cmp(&right.work_id))
        });
        for record in &claimed {
            self.persist(&record.work_id, record.stasis_job_id.as_deref());
        }
        Some(claimed)
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
        self.persist(&cloned.work_id, cloned.stasis_job_id.as_deref());
        Some(cloned)
    }

    pub fn archive(&self, work_id: &str, purge_body: bool) -> Option<TurnWorkRecord> {
        let session_id = self
            .records
            .lock()
            .expect("turn worker records")
            .get(work_id)?
            .session_id
            .clone();
        let (_session, _mutation) =
            crate::session_deletion::acquire_mutation_for_str(&session_id).ok()?;
        let now = Utc::now();
        let mut guard = self.records.lock().expect("turn worker records");
        let record = guard.get_mut(work_id)?;
        record.archived = true;
        record.updated_at = now;
        if purge_body {
            record.result_text = None;
            record.worker_scratch = None;
        }
        let snapshot = record.clone();
        drop(guard);
        self.persist(&snapshot.work_id, snapshot.stasis_job_id.as_deref());
        Some(snapshot)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<(), String> {
        let mut guard = self.records.lock().map_err(|error| error.to_string())?;
        let removed_work_ids: Vec<_> = guard
            .values()
            .filter(|record| record.session_id == session_id)
            .map(|record| record.work_id.clone())
            .collect();
        guard.retain(|_, record| record.session_id != session_id);
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
        let retained = guard.keys().cloned().collect::<Vec<_>>();
        drop(guard);
        crate::workspace::persist::queue_mutation(
            crate::workspace::persist::WorkspaceMutation::RetainTurnWorkers { work_ids: retained },
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn session_absent_on_disk(session_id: &str) -> Result<bool, String> {
        if let Ok(projection) = crate::workspace::persist::persisted_projection() {
            return Ok(!projection
                .turn_workers
                .values()
                .any(|record| record.session_id == session_id));
        }
        for path in [Self::path(), Self::legacy_path()] {
            let raw = match fs::read_to_string(path) {
                Ok(raw) => raw,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            let records = serde_json::from_str::<HashMap<String, TurnWorkRecord>>(&raw)
                .map_err(|_| "turn-worker snapshot is corrupt".to_string())?;
            if records
                .values()
                .any(|record| record.session_id == session_id)
            {
                return Ok(false);
            }
        }
        Ok(true)
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
        let record = records
            .get_mut(work_id)
            .ok_or(BoundWorkshopMutationError::MissingGeneration)?;
        record.steer_messages.push(WorkshopSteerMessage {
            text,
            at: Utc::now(),
            speaker_profile_id,
        });
        record.updated_at = Utc::now();
        let updated = record.clone();
        drop(records);
        self.persist(&updated.work_id, updated.stasis_job_id.as_deref());
        Ok(updated)
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
        let record = records
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
        if let Some(cancellation) = self
            .live_cancellations
            .lock()
            .expect("turn worker cancellations")
            .get(work_id)
            .cloned()
        {
            cancellation.cancel();
        }
        drop(records);
        self.persist(&updated.work_id, updated.stasis_job_id.as_deref());
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
    fn canonical_snapshot_lives_in_workspace_directory() {
        let root = Path::new("/tmp/medousa-test-data");
        assert_eq!(
            TurnWorkerStore::path_in(root),
            root.join("workspace/turn_workers.json")
        );
        assert_eq!(
            TurnWorkerStore::legacy_path_in(root),
            root.join("turn_workers.json")
        );
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
    fn legacy_snapshot_is_migrated_without_deleting_source() {
        let temp = tempfile::tempdir().expect("temp data directory");
        let canonical = TurnWorkerStore::path_in(temp.path());
        let legacy = TurnWorkerStore::legacy_path_in(temp.path());
        fs::write(&legacy, "{}").expect("write legacy snapshot");
        let store = TurnWorkerStore {
            records: Mutex::new(HashMap::new()),
            live_cancellations: Mutex::new(HashMap::new()),
        };

        store.reload_from_paths(&canonical, &legacy);

        assert_eq!(fs::read_to_string(&canonical).unwrap(), "{}");
        assert!(legacy.exists());
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
            execution_placement: Default::default(),
            intent: "research".to_string(),
            task_prompt: "task".to_string(),
            status,
            result_text: Some("done".to_string()),
            tool_names: Vec::new(),
            termination_reason: None,
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
                .try_claim_parallel_cohort_intake("sess-c", 7)
                .is_none()
        );
        assert_eq!(store.parallel_cohort("sess-c", 7).len(), 2);
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
            .try_claim_parallel_cohort_intake("sess-c", 9)
            .expect("claim");
        assert_eq!(claimed.len(), 2);
        assert!(claimed.iter().all(|record| record.synthesis_delivered));
        assert!(
            store
                .try_claim_parallel_cohort_intake("sess-c", 9)
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
            .try_claim_parallel_cohort_intake("sess-c", 3)
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
}
