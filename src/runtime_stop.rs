//! One owner-facing stop operation for durable workers, jobs and follow-ups.
//! The request ledger freezes membership before effects, so a retry never sweeps
//! in newly admitted work. It is intentionally not a permanent workshop pause.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::Utc;
use medousa_store::store_root::{StorePath, StoreRoot};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use stasis::domain::runtime::job::JobState;
use stasis::prelude::{RuntimeComposition, StasisError};
use stasis::sdk::runtime_sdk::RuntimeSdk;
use tokio::sync::Mutex;

use crate::agent_runtime::execution_context::active_turn_execution_context;
use crate::agent_runtime::turn_worker::{TurnWorkRecord, TurnWorkStatus, turn_worker_store};
use crate::request_principal::{Capability, PrincipalKind};
use crate::runtime_composition_ext::RuntimeCompositionExt;
use crate::turn_continuation::{TurnContinuationStatus, turn_continuation_store};

static STOP_LOCK: Mutex<()> = Mutex::const_new(());

#[cfg(test)]
#[path = "runtime_stop_tests.rs"]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StopScope {
    Chat,
    Project,
    Workshop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeStop {
    /// Stable key for retries. The first call freezes the target set.
    pub request_key: String,
    /// Chat means this admitted chat. Project/workshop require operator authority.
    pub scope: StopScope,
    /// Required only for project scope; never inferred from a model-supplied chat id.
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkerTarget {
    work_id: String,
    session_id: String,
    job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StopBatch {
    input: RuntimeStop,
    owner: String,
    session_id: String,
    created_at: chrono::DateTime<Utc>,
    workers: Vec<WorkerTarget>,
    jobs: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StopTargetReceipt {
    pub kind: String,
    pub id: String,
    /// stop_requested, already_finished, or failed.
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StopReceipt {
    pub request_key: String,
    pub scope: StopScope,
    pub snapshot_at: chrono::DateTime<Utc>,
    pub caller_excluded: bool,
    pub targets: Vec<StopTargetReceipt>,
    pub failed: usize,
    /// Cancellation is cooperative. Acknowledgement is not proof of process exit.
    pub cancellation_is_cooperative: bool,
}

fn error(message: impl std::fmt::Display) -> StasisError {
    StasisError::PortFailure(message.to_string())
}

fn matches_worker(
    input: &RuntimeStop,
    record: &TurnWorkRecord,
    session: &str,
    owner: &str,
    admin: bool,
) -> bool {
    let in_scope = match input.scope {
        StopScope::Chat => record.session_id == session,
        StopScope::Project => record.parent_code_work_id.as_deref() == input.project_id.as_deref(),
        StopScope::Workshop => true,
    };
    in_scope && (admin || record.identity_user_id.as_deref() == Some(owner))
}

fn excludes_caller(record: &TurnWorkRecord, turn_id: &str) -> bool {
    turn_id == record.work_id
        || turn_id == format!("work-{}", record.work_id)
        || turn_id == format!("{}-synthesis", record.work_id)
        || turn_id == format!("{}-host-resume", record.work_id)
}

fn select_workers(
    mut records: Vec<TurnWorkRecord>,
    input: &RuntimeStop,
    session: &str,
    owner: &str,
    admin: bool,
    turn_id: &str,
) -> (Vec<WorkerTarget>, BTreeSet<String>) {
    let caller_cohort = records
        .iter()
        .find(|record| excludes_caller(record, turn_id))
        .filter(|_| turn_id.ends_with("-host-resume"))
        .map(|record| {
            (
                record.session_id.clone(),
                record.parent_stream_turn_id,
                record.parent_turn_correlation_id.clone(),
            )
        });
    let excluded = |record: &TurnWorkRecord| {
        excludes_caller(record, turn_id)
            || caller_cohort
                .as_ref()
                .is_some_and(|(session, stream, parent)| {
                    record.session_id == *session
                        && record.parent_stream_turn_id == *stream
                        && record.parent_turn_correlation_id == *parent
                })
    };
    let excluded_jobs = records
        .iter()
        .filter(|record| excluded(record))
        .filter_map(|record| record.stasis_job_id.clone())
        .collect();
    records.retain(|record| {
        matches_worker(input, record, session, owner, admin)
            && !excluded(record)
            && !record.archived
            && (matches!(
                record.status,
                TurnWorkStatus::Pending | TurnWorkStatus::Running
            ) || !record.synthesis_delivered)
    });
    records.sort_by(|a, b| a.work_id.cmp(&b.work_id));
    (
        records
            .into_iter()
            .map(|record| WorkerTarget {
                work_id: record.work_id,
                session_id: record.session_id,
                job_id: record.stasis_job_id,
            })
            .collect(),
        excluded_jobs,
    )
}

// File I/O runs off the async executor and is confined to a daemon-owned root.
async fn ledger_read(name: String) -> stasis::prelude::Result<Option<StopBatch>> {
    tokio::task::spawn_blocking(move || {
        let root = StoreRoot::open_or_create_nofollow(
            &crate::paths::medousa_data_dir().join("runtime-stops"),
        )
        .map_err(error)?;
        let path = StorePath::parse(&name).map_err(error)?;
        match root.read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(error),
            Err(err) if err.is_not_found() => Ok(None),
            Err(err) => Err(error(err)),
        }
    })
    .await
    .map_err(error)?
}

async fn ledger_write(name: String, batch: StopBatch) -> stasis::prelude::Result<()> {
    tokio::task::spawn_blocking(move || {
        let root = StoreRoot::open_or_create_nofollow(
            &crate::paths::medousa_data_dir().join("runtime-stops"),
        )
        .map_err(error)?;
        let path = StorePath::parse(&name).map_err(error)?;
        crate::persistence::FileTransaction::new(Arc::new(root))
            .replace_snapshot_json(&path, &batch)
            .map_err(error)
    })
    .await
    .map_err(error)?
}

pub async fn execute(
    input: RuntimeStop,
    runtime: Arc<RuntimeComposition>,
) -> stasis::prelude::Result<StopReceipt> {
    let context = active_turn_execution_context()
        .ok_or_else(|| error("runtime.stop requires an admitted owner turn"))?;
    let principal = context.principal();
    if matches!(
        principal.kind(),
        PrincipalKind::Anonymous
            | PrincipalKind::Peer
            | PrincipalKind::Worker
            | PrincipalKind::ExternalAgent
    ) {
        return Err(error("runtime.stop requires owner authority"));
    }
    let owner = context
        .legacy_scope()
        .identity_user_id
        .clone()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| error("runtime.stop requires an authenticated profile"))?;
    let admin = principal.capabilities().contains(Capability::AdminRuntime);
    if input.scope != StopScope::Chat && !admin {
        return Err(error("project/workshop stop requires admin.runtime"));
    }
    if input.request_key.trim().is_empty() || input.request_key.len() > 200 {
        return Err(error("request_key must contain 1–200 bytes"));
    }
    if match input.scope {
        StopScope::Project => input
            .project_id
            .as_ref()
            .is_none_or(|id| id.trim().is_empty()),
        _ => input.project_id.is_some(),
    } {
        return Err(error("project_id is required only for project scope"));
    }
    let session = context.session_id().as_str().to_string();
    let ledger_name = format!(
        "{:x}.json",
        Sha256::digest(serde_json::to_vec(&(&owner, &session, &input.request_key)).map_err(error)?)
    );
    let _serial = STOP_LOCK.lock().await;
    let _resume_guard = crate::turn_continuation::RESUME_ADMISSION.lock().await;
    let live_resumes = crate::turn_continuation::live_resumes();
    let store = turn_worker_store();
    let continuations = turn_continuation_store();
    let batch = if let Some(batch) = ledger_read(ledger_name.clone()).await? {
        if batch.input != input || batch.owner != owner || batch.session_id != session {
            return Err(error(
                "request_key already belongs to a different stop request",
            ));
        }
        batch
    } else {
        let (workers, excluded_jobs) = select_workers(
            store.list_all_unbounded(),
            &input,
            &session,
            &owner,
            admin,
            context.turn_id(),
        );
        let mut jobs = workers
            .iter()
            .filter_map(|target| target.job_id.clone())
            .collect::<BTreeSet<_>>();
        let mut candidates = BTreeMap::new();
        for state in [
            JobState::Enqueued,
            JobState::Leased,
            JobState::Running,
            JobState::Succeeded,
            JobState::Failed,
            JobState::DeadLetter,
            JobState::Canceled,
        ] {
            for job in runtime.list_jobs_by_state(state).await? {
                candidates.insert(job.id.clone(), job);
            }
        }
        for job in candidates.values() {
            if live_resumes
                .get(&job.id)
                .is_some_and(|execution| execution.turn_id() == context.turn_id())
            {
                continue;
            }
            if excluded_jobs.contains(&job.id) {
                continue;
            }
            if let Some(record) = continuations.get(&job.id).await {
                let in_scope = match input.scope {
                    StopScope::Chat => {
                        record.session_id == session
                            && (admin || record.identity_user_id.as_deref() == Some(&owner))
                    }
                    StopScope::Project => false, // project membership comes from durable worker records, never inferred
                    StopScope::Workshop => true,
                };
                // The continuation's correlation is the *parent* turn, not the
                // job's execution identity. Children of the caller must stop too.
                if in_scope
                    && (record.status == TurnContinuationStatus::Pending
                        || live_resumes.contains_key(&job.id)
                        || matches!(
                            job.state,
                            JobState::Enqueued | JobState::Leased | JobState::Running
                        ))
                {
                    jobs.insert(job.id.clone());
                }
            } else if input.scope == StopScope::Workshop
                && job.id != context.turn_id()
                && matches!(
                    job.state,
                    JobState::Enqueued | JobState::Leased | JobState::Running
                )
            {
                jobs.insert(job.id.clone());
            }
        }
        let batch = StopBatch {
            input: input.clone(),
            owner,
            session_id: session,
            created_at: Utc::now(),
            workers,
            jobs: jobs.into_iter().collect(),
        };
        ledger_write(ledger_name, batch.clone()).await?;
        batch
    };
    let mut targets = Vec::new();
    // Fence every worker before queue cancellation, including completed workers
    // with pending host intake. Never set synthesis_delivered as part of stop.
    for target in &batch.workers {
        let result = store.stop_exact(&target.session_id, &target.work_id, &input.request_key);
        let (status, err) = match result {
            Ok(_) => ("stop_requested", None),
            Err(err) => ("failed", Some(format!("{err:?}"))),
        };
        targets.push(StopTargetReceipt {
            kind: "worker".into(),
            id: target.work_id.clone(),
            status: status.into(),
            error: err,
        });
    }
    // Admission is not a durability acknowledgement. Await the workspace
    // writer before reporting worker fences as durable, including on retries.
    if !batch.workers.is_empty() {
        if let Err(error) = crate::workspace::persist::flush_persist_writer().await {
            for target in &mut targets {
                if target.status != "failed" {
                    target.status = "failed".into();
                    target.error = Some(format!("stop fence durability unconfirmed: {error}"));
                }
            }
        }
    }
    let sdk = RuntimeSdk::new(runtime.as_ref().clone());
    for id in &batch.jobs {
        // Suppress replay before requesting cancellation. Continue other targets
        // on failure, but never report a failed suppression as successful.
        let result = async {
            let mut followup_stopped = false;
            if continuations.get(id).await.is_some_and(|record| {
                matches!(
                    record.status,
                    TurnContinuationStatus::Pending | TurnContinuationStatus::Resumed
                )
            }) {
                continuations.mark_abandoned(id).await.map_err(error)?;
                followup_stopped = true;
            }
            let resumed = if let Some(execution) = live_resumes.get(id) {
                execution.cancellation().cancel();
                true
            } else {
                false
            };
            sdk.cancel(id)
                .await
                .map(|cancelled| cancelled || resumed || followup_stopped)
        }
        .await;
        let (status, err) = match result {
            Ok(true) => ("stop_requested", None),
            Ok(false) => ("already_finished", None),
            Err(err) => ("failed", Some(err.to_string())),
        };
        targets.push(StopTargetReceipt {
            kind: "job".into(),
            id: id.clone(),
            status: status.into(),
            error: err,
        });
    }
    let failed = targets
        .iter()
        .filter(|target| target.status == "failed")
        .count();
    Ok(StopReceipt {
        request_key: input.request_key,
        scope: input.scope,
        snapshot_at: batch.created_at,
        caller_excluded: true,
        targets,
        failed,
        cancellation_is_cooperative: true,
    })
}
