//! Durable fan-out and reconciliation contracts for parallel work environments.
//!
//! Children remain ordinary `workflow.medousa.work_environment` jobs. Their
//! immutable checkpoints become one portable reconciliation checkpoint; no
//! workspace path or container handle crosses this boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use medousa_runtime::{
    WorkEnvironmentArtifact, WorkEnvironmentCheckpoint, WorkEnvironmentCheckpointManifest,
    WorkEnvironmentPublication,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::application::runtime::job_context::JobContext;
use stasis::domain::runtime::blob_descriptor::BlobDescriptor;
use stasis::domain::runtime::job::{BackoffPolicy, Job, JobState, NewJob};
use stasis::domain::runtime::placement::PlacementConstraints;
use stasis::ports::outbound::runtime::blob_transfer::BlobTransferPort;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::work_environment_federation::{
    RemoteWorkEnvironmentDispatcher, build_remote_work_environment_envelope,
    decode_remote_terminal_result, load_recorded_terminal_result,
    stage_remote_work_environment_payload,
};
use crate::work_environment_job::{
    WorkEnvironmentJobPayload, WorkEnvironmentJobProgress, WorkEnvironmentWorkflowPhase,
};

pub const PARALLEL_WORK_COORDINATOR_JOB_TYPE: &str = "workflow.medousa.parallel_work_environment";
pub const REMOTE_WORK_ENVIRONMENT_PROXY_JOB_TYPE: &str =
    "workflow.medousa.remote_work_environment_proxy";
pub const PARALLEL_RECONCILIATION_JOB_TYPE: &str =
    "workflow.medousa.work_environment_reconciliation";
pub const PARALLEL_WORK_PLAN_SCHEMA_VERSION: u32 = 1;
pub const RECONCILIATION_INPUT_SCHEMA_VERSION: u32 = 1;
pub const RECONCILIATION_INPUT_MEDIA_TYPE: &str =
    "application/vnd.medousa.work-environment-reconciliation-input+json";
pub const RECONCILIATION_CHECKPOINT_MEDIA_TYPE: &str =
    "application/vnd.medousa.work-environment-checkpoint+json";
const MIN_PARALLEL_CHILDREN: usize = 2;
const MAX_PARALLEL_CHILDREN: usize = 16;
const RECONCILIATION_ROOT: &str = ".medousa/reconciliation";
const COORDINATOR_PROGRESS_SCHEMA_VERSION: u32 = 1;
const RECONCILIATION_PAYLOAD_SCHEMA_VERSION: u32 = 1;
const COORDINATOR_DELAY_MILLIS: i64 = 10;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParallelWorkChild {
    pub child_id: String,
    pub work: WorkEnvironmentJobPayload,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParallelWorkPlan {
    pub schema_version: u32,
    pub plan_id: String,
    pub base_commit: String,
    pub children: Vec<ParallelWorkChild>,
    /// Ordinary work-environment job used to combine, verify, and publish the
    /// child results. Its checkpoint is filled by the coordinator.
    pub reconciliation: WorkEnvironmentJobPayload,
}

impl ParallelWorkPlan {
    pub fn validate(&self, now: DateTime<Utc>) -> StasisResult<()> {
        if self.schema_version != PARALLEL_WORK_PLAN_SCHEMA_VERSION {
            return Err(invalid(format!(
                "unsupported parallel plan schema_version={}",
                self.schema_version
            )));
        }
        validate_id("plan_id", &self.plan_id)?;
        if !(MIN_PARALLEL_CHILDREN..=MAX_PARALLEL_CHILDREN).contains(&self.children.len()) {
            return Err(invalid(format!(
                "parallel plan must contain {MIN_PARALLEL_CHILDREN}..={MAX_PARALLEL_CHILDREN} children"
            )));
        }
        self.reconciliation
            .validate(now)
            .map_err(|error| invalid(error.to_string()))?;
        if self.reconciliation.spec.base_commit != self.base_commit {
            return Err(invalid(
                "reconciliation base_commit does not match the parallel plan",
            ));
        }
        if self.reconciliation.spec.checkpoint_ref.is_some() {
            return Err(invalid(
                "reconciliation checkpoint is coordinator-owned and must be unset",
            ));
        }
        if self.reconciliation.federation.is_some() {
            return Err(invalid(
                "reconciliation federation context is destination-owned",
            ));
        }
        require_expected_base(
            self.reconciliation.spec.publication.as_ref(),
            &self.base_commit,
        )?;

        let repository = &self.reconciliation.spec.repository;
        let mut child_ids = BTreeSet::new();
        let mut environment_ids = BTreeSet::new();
        let mut workspace_ids = BTreeSet::new();
        environment_ids.insert(self.reconciliation.spec.environment_id.as_str().to_string());
        workspace_ids.insert(self.reconciliation.spec.workspace_id.as_str().to_string());
        for child in &self.children {
            validate_id("child_id", &child.child_id)?;
            if !child_ids.insert(child.child_id.as_str()) {
                return Err(invalid(format!(
                    "duplicate parallel child_id: {}",
                    child.child_id
                )));
            }
            child
                .work
                .validate(now)
                .map_err(|error| invalid(error.to_string()))?;
            if child.work.spec.base_commit != self.base_commit
                || child.work.spec.repository != *repository
            {
                return Err(invalid(format!(
                    "parallel child {} does not share the exact repository and base",
                    child.child_id
                )));
            }
            if child.work.spec.publication.is_some() {
                return Err(invalid(format!(
                    "parallel child {} must preserve a checkpoint instead of publishing",
                    child.child_id
                )));
            }
            if child.work.federation.is_some() {
                return Err(invalid(format!(
                    "parallel child {} contains destination-owned federation context",
                    child.child_id
                )));
            }
            if !environment_ids.insert(child.work.spec.environment_id.as_str().to_string()) {
                return Err(invalid("parallel environments must be distinct"));
            }
            if !workspace_ids.insert(child.work.spec.workspace_id.as_str().to_string()) {
                return Err(invalid("parallel workspaces must be distinct"));
            }
        }
        Ok(())
    }

    pub fn into_job(
        self,
        job_id: impl Into<String>,
        queue: impl Into<String>,
        causation_id: impl Into<String>,
        scheduled_at: DateTime<Utc>,
    ) -> StasisResult<NewJob> {
        self.validate(scheduled_at)?;
        let job_id = job_id.into();
        let payload_ref = serde_json::to_string(&self)
            .map_err(|error| invalid(format!("encode parallel work plan: {error}")))?;
        Ok(NewJob {
            id: job_id.clone(),
            queue: queue.into(),
            job_type: PARALLEL_WORK_COORDINATOR_JOB_TYPE.to_string(),
            payload_ref,
            priority: 100,
            max_attempts: 12,
            idempotency_key: format!("idem-{job_id}"),
            correlation_id: job_id.clone(),
            causation_id: causation_id.into(),
            trace_id: job_id,
            input_provenance: None,
            placement: PlacementConstraints::unrestricted(),
            scheduled_at,
            backoff_policy: BackoffPolicy::default(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParallelWorkChildResult {
    pub child_id: String,
    pub job_id: String,
    pub checkpoint: WorkEnvironmentCheckpoint,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconciliationArtifactInput {
    pub source_path: String,
    pub materialized_path: String,
    pub blob: BlobDescriptor,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconciliationChildInput {
    pub child_id: String,
    pub job_id: String,
    pub checkpoint_commit: String,
    pub source_bundle: BlobDescriptor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialized_bundle_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ReconciliationArtifactInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconciliationInputManifest {
    pub schema_version: u32,
    pub plan_id: String,
    pub base_commit: String,
    pub primary_child_id: String,
    pub children: Vec<ReconciliationChildInput>,
    pub created_at: DateTime<Utc>,
}

pub struct PreparedReconciliationInput {
    pub checkpoint: WorkEnvironmentCheckpoint,
    pub manifest: ReconciliationInputManifest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParallelChildObservation {
    pub child_id: String,
    pub job_id: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<WorkEnvironmentCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<medousa_runtime::WorkEnvironmentPublicationResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParallelCoordinatorProgress {
    pub schema_version: u32,
    pub children: Vec<ParallelChildObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciliation_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciliation_state: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ParallelReconciliationPayload {
    schema_version: u32,
    parent_job_id: String,
    plan: ParallelWorkPlan,
    children: Vec<ParallelWorkChildResult>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ParallelReconciliationProgress {
    schema_version: u32,
    work_job_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checkpoint: Option<WorkEnvironmentCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publication: Option<medousa_runtime::WorkEnvironmentPublicationResult>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RemoteWorkEnvironmentProxyPayload {
    schema_version: u32,
    target_runtime_id: String,
    envelope_id: String,
    deadline: DateTime<Utc>,
    work: WorkEnvironmentJobPayload,
}

struct ParallelWorkCoordinatorJobHandler {
    jobs: Arc<dyn JobStore>,
    dispatcher: Option<Arc<dyn RemoteWorkEnvironmentDispatcher>>,
}

struct RemoteWorkEnvironmentProxyJobHandler {
    runtime: RuntimeComposition,
    blobs: Arc<dyn BlobTransferPort>,
    dispatcher: Option<Arc<dyn RemoteWorkEnvironmentDispatcher>>,
}

struct ParallelReconciliationJobHandler {
    jobs: Arc<dyn JobStore>,
    blobs: Arc<dyn BlobTransferPort>,
}

pub async fn register_parallel_work_environment_job_handlers(
    composition: &RuntimeComposition,
    blobs: Arc<dyn BlobTransferPort>,
    dispatcher: Option<Arc<dyn RemoteWorkEnvironmentDispatcher>>,
) -> anyhow::Result<()> {
    match composition {
        RuntimeComposition::InMemory(runtime) => {
            let jobs: Arc<dyn JobStore> = Arc::new(runtime.job_store.clone());
            runtime.register_handler(ParallelWorkCoordinatorJobHandler {
                jobs: Arc::clone(&jobs),
                dispatcher: dispatcher.clone(),
            })?;
            runtime.register_handler(RemoteWorkEnvironmentProxyJobHandler {
                runtime: composition.clone(),
                blobs: Arc::clone(&blobs),
                dispatcher: dispatcher.clone(),
            })?;
            runtime.register_handler(ParallelReconciliationJobHandler { jobs, blobs })?;
        }
        RuntimeComposition::Surreal(runtime) => {
            let jobs: Arc<dyn JobStore> = Arc::new(runtime.job_store.clone());
            runtime.register_handler(ParallelWorkCoordinatorJobHandler {
                jobs: Arc::clone(&jobs),
                dispatcher: dispatcher.clone(),
            })?;
            runtime.register_handler(RemoteWorkEnvironmentProxyJobHandler {
                runtime: composition.clone(),
                blobs: Arc::clone(&blobs),
                dispatcher: dispatcher.clone(),
            })?;
            runtime.register_handler(ParallelReconciliationJobHandler { jobs, blobs })?;
        }
    }
    Ok(())
}

pub fn parallel_child_job_id(parent_job_id: &str, child_id: &str) -> StasisResult<String> {
    validate_id("child_id", child_id)?;
    if parent_job_id.trim().is_empty() {
        return Err(invalid("parallel parent job id is required"));
    }
    Ok(format!("{}:parallel:{child_id}", parent_job_id.trim()))
}

pub fn parallel_reconciliation_job_id(parent_job_id: &str) -> StasisResult<String> {
    if parent_job_id.trim().is_empty() {
        return Err(invalid("parallel parent job id is required"));
    }
    Ok(format!("{}:reconcile", parent_job_id.trim()))
}

pub fn remote_child_envelope_id(child_job_id: &str) -> StasisResult<String> {
    if child_job_id.trim().is_empty() {
        return Err(invalid("remote child job id is required"));
    }
    Ok(format!("{}:federated", child_job_id.trim()))
}

fn remote_proxy_job(
    child: &ParallelWorkChild,
    target_runtime_id: &str,
    job_id: &str,
    queue: &str,
    parent: &Job,
) -> StasisResult<NewJob> {
    validate_id("target_runtime_id", target_runtime_id)?;
    let scheduled_at = parent.started_at.unwrap_or(parent.scheduled_at);
    let deadline = child
        .work
        .deadline_at
        .unwrap_or(scheduled_at + chrono::Duration::hours(24));
    let mut work = child.work.clone();
    work.spec.requirements.placement.target_node = Some(target_runtime_id.to_string());
    let payload = RemoteWorkEnvironmentProxyPayload {
        schema_version: 1,
        target_runtime_id: target_runtime_id.to_string(),
        envelope_id: remote_child_envelope_id(job_id)?,
        deadline,
        work: work.clone(),
    };
    let payload_ref = serde_json::to_string(&payload)
        .map_err(|error| invalid(format!("encode remote work-environment proxy: {error}")))?;
    let mut candidate = work.into_job(
        job_id.to_string(),
        queue.to_string(),
        parent.id.clone(),
        Utc::now(),
    )?;
    candidate.job_type = REMOTE_WORK_ENVIRONMENT_PROXY_JOB_TYPE.to_string();
    candidate.payload_ref = payload_ref;
    // This durable proxy executes on the origin. Only its signed remote
    // envelope carries the destination's OCI placement constraints.
    candidate.placement = PlacementConstraints::unrestricted();
    Ok(candidate)
}

/// Assemble every child result into one portable checkpoint. The first child
/// is restored as the reconciliation worktree; every other Git bundle and all
/// declared artifacts are materialized under `.medousa/reconciliation/`.
pub async fn prepare_reconciliation_input(
    blobs: &dyn BlobTransferPort,
    plan: &ParallelWorkPlan,
    results: &[ParallelWorkChildResult],
    now: DateTime<Utc>,
) -> StasisResult<PreparedReconciliationInput> {
    plan.validate(now)?;
    let results_by_child = exact_results(plan, results)?;
    let mut inputs = Vec::with_capacity(plan.children.len());
    let mut checkpoint_artifacts = Vec::new();
    let mut primary_manifest = None;

    for (index, child) in plan.children.iter().enumerate() {
        let result = results_by_child
            .get(child.child_id.as_str())
            .expect("exact result set was validated");
        result
            .checkpoint
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        let bytes = blobs.get(&result.checkpoint.manifest).await?;
        let manifest: WorkEnvironmentCheckpointManifest = serde_json::from_slice(&bytes)
            .map_err(|error| invalid(format!("decode child checkpoint manifest: {error}")))?;
        manifest
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        if manifest.base_commit != plan.base_commit
            || manifest.environment_id != child.work.spec.environment_id
            || manifest.workspace_id != child.work.spec.workspace_id
        {
            return Err(invalid(format!(
                "parallel child {} checkpoint does not match its exact base, environment, and workspace",
                child.child_id
            )));
        }
        for descriptor in std::iter::once(&manifest.source_bundle)
            .chain(manifest.artifacts.iter().map(|artifact| &artifact.blob))
        {
            if !blobs.exists(descriptor).await? {
                return Err(invalid(format!(
                    "parallel child {} checkpoint content is missing: {}:{}",
                    child.child_id, descriptor.digest.algorithm, descriptor.digest.hex
                )));
            }
        }

        let materialized_bundle_path = (index != 0)
            .then(|| format!("{RECONCILIATION_ROOT}/children/{}.bundle", child.child_id));
        if let Some(path) = materialized_bundle_path.as_ref() {
            checkpoint_artifacts.push(WorkEnvironmentArtifact {
                path: path.clone(),
                blob: manifest.source_bundle.clone(),
            });
        } else {
            primary_manifest = Some(manifest.clone());
        }

        let mut artifacts = Vec::with_capacity(manifest.artifacts.len());
        for artifact in &manifest.artifacts {
            let materialized_path = format!(
                "{RECONCILIATION_ROOT}/children/{}/artifacts/{}",
                child.child_id, artifact.path
            );
            checkpoint_artifacts.push(WorkEnvironmentArtifact {
                path: materialized_path.clone(),
                blob: artifact.blob.clone(),
            });
            artifacts.push(ReconciliationArtifactInput {
                source_path: artifact.path.clone(),
                materialized_path,
                blob: artifact.blob.clone(),
            });
        }
        inputs.push(ReconciliationChildInput {
            child_id: child.child_id.clone(),
            job_id: result.job_id.clone(),
            checkpoint_commit: manifest.checkpoint_commit,
            source_bundle: manifest.source_bundle,
            materialized_bundle_path,
            artifacts,
        });
    }

    let primary = primary_manifest.expect("parallel plan always has a first child");
    let manifest = ReconciliationInputManifest {
        schema_version: RECONCILIATION_INPUT_SCHEMA_VERSION,
        plan_id: plan.plan_id.clone(),
        base_commit: plan.base_commit.clone(),
        primary_child_id: plan.children[0].child_id.clone(),
        children: inputs,
        created_at: now,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|error| invalid(format!("encode reconciliation input: {error}")))?;
    let manifest_blob = blobs
        .put(&manifest_bytes, Some(RECONCILIATION_INPUT_MEDIA_TYPE))
        .await?;
    checkpoint_artifacts.push(WorkEnvironmentArtifact {
        path: format!("{RECONCILIATION_ROOT}/manifest.json"),
        blob: manifest_blob,
    });

    let reconciliation_manifest = WorkEnvironmentCheckpointManifest {
        schema_version: medousa_runtime::WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION,
        environment_id: plan.reconciliation.spec.environment_id.clone(),
        workspace_id: plan.reconciliation.spec.workspace_id.clone(),
        base_commit: plan.base_commit.clone(),
        checkpoint_commit: primary.checkpoint_commit,
        source_bundle: primary.source_bundle,
        artifacts: checkpoint_artifacts,
        fence: plan.reconciliation.spec.fence.clone(),
        label: Some(format!("parallel-reconciliation: {}", plan.plan_id)),
        created_at: now,
    };
    reconciliation_manifest
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    let bytes = serde_json::to_vec(&reconciliation_manifest)
        .map_err(|error| invalid(format!("encode reconciliation checkpoint: {error}")))?;
    let descriptor = blobs
        .put(&bytes, Some(RECONCILIATION_CHECKPOINT_MEDIA_TYPE))
        .await?;
    Ok(PreparedReconciliationInput {
        checkpoint: WorkEnvironmentCheckpoint::from_manifest(descriptor),
        manifest,
    })
}

pub fn reconciliation_work_payload(
    plan: &ParallelWorkPlan,
    prepared: &PreparedReconciliationInput,
) -> WorkEnvironmentJobPayload {
    let mut payload = plan.reconciliation.clone();
    payload.spec.checkpoint_ref = Some(prepared.checkpoint.clone());
    payload.execution.environment.insert(
        "MEDOUSA_RECONCILIATION_MANIFEST".to_string(),
        format!("/workspace/{RECONCILIATION_ROOT}/manifest.json"),
    );
    payload
}

#[async_trait]
impl JobHandler for ParallelWorkCoordinatorJobHandler {
    fn job_type(&self) -> &'static str {
        PARALLEL_WORK_COORDINATOR_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> StasisResult<JobExecutionOutcome> {
        let _ = job;
        Err(invalid("parallel coordinators require Stasis JobContext"))
    }

    async fn execute_with_context(
        &self,
        job: &Job,
        ctx: JobContext,
    ) -> StasisResult<JobExecutionOutcome> {
        let plan: ParallelWorkPlan = serde_json::from_str(&job.payload_ref)
            .map_err(|error| invalid(format!("decode parallel work plan: {error}")))?;
        if let Err(error) = plan.validate(Utc::now()) {
            return Ok(fatal(job, error.to_string(), None));
        }

        for child in &plan.children {
            let child_job_id = parallel_child_job_id(&job.id, &child.child_id)?;
            if self.jobs.get(&child_job_id).await?.is_some() {
                continue;
            }
            let placement = child.work.spec.placement_constraints();
            let selected_target = if let Some(dispatcher) = self.dispatcher.as_ref() {
                dispatcher.select_target(&child_job_id, &placement).await?
            } else {
                placement.target_node.clone()
            };
            let mut candidate = if let Some(target_runtime_id) = selected_target.as_deref() {
                remote_proxy_job(child, target_runtime_id, &child_job_id, &job.queue, job)?
            } else {
                child
                    .work
                    .clone()
                    .into_job(&child_job_id, &job.queue, &job.id, Utc::now())?
            };
            candidate.correlation_id = job.correlation_id.clone();
            candidate.trace_id = job.trace_id.clone();
            ensure_job(&self.jobs, candidate).await?;
        }

        let mut observations = Vec::with_capacity(plan.children.len());
        let mut results = Vec::with_capacity(plan.children.len());
        let mut all_terminal = true;
        let mut failed = false;
        for child in &plan.children {
            let child_job_id = parallel_child_job_id(&job.id, &child.child_id)?;
            let child_job =
                self.jobs.get(&child_job_id).await?.ok_or_else(|| {
                    invalid(format!("parallel child disappeared: {child_job_id}"))
                })?;
            let progress = decode_work_progress(&child_job)?;
            let checkpoint = progress
                .as_ref()
                .and_then(|progress| progress.checkpoint.clone());
            let publication = progress
                .as_ref()
                .and_then(|progress| progress.publication.clone());
            let terminal = is_terminal(&child_job.state);
            all_terminal &= terminal;
            let mut error_message = child_job.last_error.clone();
            if child_job.state == JobState::Succeeded && checkpoint.is_none() {
                error_message = Some("successful parallel child has no checkpoint".to_string());
                failed = true;
            } else if terminal && child_job.state != JobState::Succeeded {
                failed = true;
            }
            if child_job.state == JobState::Succeeded
                && let Some(checkpoint) = checkpoint.clone()
            {
                results.push(ParallelWorkChildResult {
                    child_id: child.child_id.clone(),
                    job_id: child_job_id.clone(),
                    checkpoint,
                });
            }
            observations.push(ParallelChildObservation {
                child_id: child.child_id.clone(),
                job_id: child_job_id,
                state: state_name(&child_job.state),
                checkpoint,
                publication,
                error_message,
            });
        }

        let mut progress = ParallelCoordinatorProgress {
            schema_version: COORDINATOR_PROGRESS_SCHEMA_VERSION,
            children: observations,
            reconciliation_job_id: None,
            reconciliation_state: None,
            updated_at: Utc::now(),
        };
        if !all_terminal {
            ctx.progress(&progress).await?;
            return Ok(deferred(job, "waiting for parallel children"));
        }
        if failed {
            ctx.progress(&progress).await?;
            return Ok(fatal(
                job,
                "one or more parallel children failed; every result was preserved".to_string(),
                Some(json!({ "children": progress.children }).to_string()),
            ));
        }

        let reconciliation_job_id = parallel_reconciliation_job_id(&job.id)?;
        let payload = ParallelReconciliationPayload {
            schema_version: RECONCILIATION_PAYLOAD_SCHEMA_VERSION,
            parent_job_id: job.id.clone(),
            plan,
            children: results,
            created_at: job.started_at.unwrap_or(job.scheduled_at),
        };
        let payload_ref = serde_json::to_string(&payload)
            .map_err(|error| invalid(format!("encode parallel reconciliation: {error}")))?;
        ensure_job(
            &self.jobs,
            NewJob {
                id: reconciliation_job_id.clone(),
                queue: job.queue.clone(),
                job_type: PARALLEL_RECONCILIATION_JOB_TYPE.to_string(),
                payload_ref,
                priority: job.priority,
                max_attempts: 12,
                idempotency_key: format!("idem-{reconciliation_job_id}"),
                correlation_id: job.correlation_id.clone(),
                causation_id: job.id.clone(),
                trace_id: job.trace_id.clone(),
                input_provenance: None,
                placement: PlacementConstraints::unrestricted(),
                scheduled_at: Utc::now(),
                backoff_policy: BackoffPolicy::default(),
            },
        )
        .await?;
        let reconciliation = self
            .jobs
            .get(&reconciliation_job_id)
            .await?
            .ok_or_else(|| invalid("parallel reconciliation job disappeared"))?;
        progress.reconciliation_job_id = Some(reconciliation_job_id);
        progress.reconciliation_state = Some(state_name(&reconciliation.state));
        progress.updated_at = Utc::now();
        ctx.progress(&progress).await?;
        match reconciliation.state {
            JobState::Succeeded => Ok(JobExecutionOutcome::Success {
                output_provenance: reconciliation.output_provenance,
                execution_id: Some(job.id.clone()),
                diagnostics: Some(json!({ "reconciliation": progress }).to_string()),
            }),
            state if is_terminal(&state) => Ok(fatal(
                job,
                reconciliation.last_error.unwrap_or_else(|| {
                    format!("parallel reconciliation ended as {}", state_name(&state))
                }),
                Some(json!({ "reconciliation": progress }).to_string()),
            )),
            _ => Ok(deferred(job, "waiting for parallel reconciliation")),
        }
    }
}

#[async_trait]
impl JobHandler for RemoteWorkEnvironmentProxyJobHandler {
    fn job_type(&self) -> &'static str {
        REMOTE_WORK_ENVIRONMENT_PROXY_JOB_TYPE
    }

    async fn execute(&self, _job: &Job) -> StasisResult<JobExecutionOutcome> {
        Err(invalid(
            "remote work-environment proxies require Stasis JobContext",
        ))
    }

    async fn execute_with_context(
        &self,
        job: &Job,
        ctx: JobContext,
    ) -> StasisResult<JobExecutionOutcome> {
        let payload: RemoteWorkEnvironmentProxyPayload = serde_json::from_str(&job.payload_ref)
            .map_err(|error| invalid(format!("decode remote work-environment proxy: {error}")))?;
        if payload.schema_version != 1 {
            return Ok(fatal(
                job,
                "unsupported remote work-environment proxy payload".to_string(),
                None,
            ));
        }
        validate_id("target_runtime_id", &payload.target_runtime_id)?;
        if payload.envelope_id != remote_child_envelope_id(&job.id)? {
            return Ok(fatal(
                job,
                "remote work-environment proxy envelope identity changed".to_string(),
                None,
            ));
        }

        if let Some(result) =
            load_recorded_terminal_result(&self.runtime, self.blobs.as_ref(), &payload.envelope_id)
                .await?
        {
            return complete_remote_proxy(job, &ctx, &payload, result, self.blobs.as_ref()).await;
        }
        if payload.deadline <= Utc::now() {
            return Ok(fatal(
                job,
                "remote work-environment deadline elapsed".to_string(),
                None,
            ));
        }
        let Some(dispatcher) = self.dispatcher.as_ref() else {
            return Ok(fatal(
                job,
                "remote work-environment transport is unavailable on this daemon".to_string(),
                None,
            ));
        };
        let staged =
            stage_remote_work_environment_payload(self.blobs.as_ref(), &payload.work).await?;
        let envelope = build_remote_work_environment_envelope(
            payload.envelope_id.clone(),
            staged,
            job.idempotency_key.clone(),
            job.correlation_id.clone(),
            job.id.clone(),
            payload.deadline,
            dispatcher.origin_authority(),
            dispatcher.terminal_delivery(),
            payload.work.spec.placement_constraints(),
        )?;
        dispatcher
            .submit_remote_job(&payload.target_runtime_id, envelope)
            .await?;

        if let Some(result) =
            load_recorded_terminal_result(&self.runtime, self.blobs.as_ref(), &payload.envelope_id)
                .await?
        {
            return complete_remote_proxy(job, &ctx, &payload, result, self.blobs.as_ref()).await;
        }
        Ok(deferred(job, "waiting for remote work environment"))
    }
}

async fn complete_remote_proxy(
    job: &Job,
    ctx: &JobContext,
    payload: &RemoteWorkEnvironmentProxyPayload,
    result: stasis::domain::runtime::federation::FederatedTerminalResult,
    blobs: &dyn BlobTransferPort,
) -> StasisResult<JobExecutionOutcome> {
    if result.correlation_id != job.correlation_id || result.causation_id != job.id {
        return Ok(fatal(
            job,
            "remote terminal result does not belong to this proxy job".to_string(),
            None,
        ));
    }
    let remote = decode_remote_terminal_result(blobs, &result).await?;
    let progress = WorkEnvironmentJobProgress {
        schema_version: 1,
        phase: WorkEnvironmentWorkflowPhase::CleanupEnqueued,
        attempt: ctx.attempt,
        fence: payload.work.spec.fence.clone(),
        environment_state: None,
        execution_result: remote.execution_result.clone(),
        portable_coder_result: remote.portable_coder_result.clone(),
        checkpoint: remote.checkpoint.clone(),
        publication: remote.publication.clone(),
        cleanup_job_id: None,
        updated_at: remote.finished_at,
    };
    ctx.progress(&progress).await?;
    let diagnostics = Some(
        json!({
            "target_runtime_id": payload.target_runtime_id,
            "remote_job_id": remote.remote_job_id,
            "terminal_state": remote.terminal_state,
        })
        .to_string(),
    );
    if remote.succeeded {
        Ok(JobExecutionOutcome::Success {
            output_provenance: result.output_provenance,
            execution_id: Some(remote.remote_job_id),
            diagnostics,
        })
    } else {
        Ok(fatal(
            job,
            remote
                .error_message
                .unwrap_or_else(|| "remote work environment failed".to_string()),
            diagnostics,
        ))
    }
}

#[async_trait]
impl JobHandler for ParallelReconciliationJobHandler {
    fn job_type(&self) -> &'static str {
        PARALLEL_RECONCILIATION_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> StasisResult<JobExecutionOutcome> {
        let _ = job;
        Err(invalid(
            "parallel reconciliation requires Stasis JobContext",
        ))
    }

    async fn execute_with_context(
        &self,
        job: &Job,
        ctx: JobContext,
    ) -> StasisResult<JobExecutionOutcome> {
        let payload: ParallelReconciliationPayload = serde_json::from_str(&job.payload_ref)
            .map_err(|error| invalid(format!("decode parallel reconciliation: {error}")))?;
        if payload.schema_version != RECONCILIATION_PAYLOAD_SCHEMA_VERSION {
            return Ok(fatal(
                job,
                "unsupported parallel reconciliation payload".to_string(),
                None,
            ));
        }
        let prepared = match prepare_reconciliation_input(
            self.blobs.as_ref(),
            &payload.plan,
            &payload.children,
            payload.created_at,
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(error) => return Ok(fatal(job, error.to_string(), None)),
        };
        let work_payload = reconciliation_work_payload(&payload.plan, &prepared);
        let work_job_id = format!("{}:work", job.id);
        let mut candidate = work_payload.into_job(&work_job_id, &job.queue, &job.id, Utc::now())?;
        candidate.correlation_id = job.correlation_id.clone();
        candidate.trace_id = job.trace_id.clone();
        ensure_job(&self.jobs, candidate).await?;
        let work = self
            .jobs
            .get(&work_job_id)
            .await?
            .ok_or_else(|| invalid("reconciliation work job disappeared"))?;
        let work_progress = decode_work_progress(&work)?;
        let progress = ParallelReconciliationProgress {
            schema_version: COORDINATOR_PROGRESS_SCHEMA_VERSION,
            work_job_id,
            checkpoint: work_progress
                .as_ref()
                .and_then(|progress| progress.checkpoint.clone()),
            publication: work_progress
                .as_ref()
                .and_then(|progress| progress.publication.clone()),
            updated_at: Utc::now(),
        };
        ctx.progress(&progress).await?;
        match work.state {
            JobState::Succeeded => Ok(completed_reconciliation_outcome(job, &work, &progress)),
            state if is_terminal(&state) => Ok(fatal(
                job,
                work.last_error.unwrap_or_else(|| {
                    format!("reconciliation work ended as {}", state_name(&state))
                }),
                Some(json!({ "reconciliation": progress }).to_string()),
            )),
            _ => Ok(deferred(job, "waiting for reconciliation work environment")),
        }
    }
}

fn completed_reconciliation_outcome(
    coordinator: &Job,
    work: &Job,
    progress: &ParallelReconciliationProgress,
) -> JobExecutionOutcome {
    match progress.publication.as_ref() {
        Some(medousa_runtime::WorkEnvironmentPublicationResult::Published { .. })
        | Some(medousa_runtime::WorkEnvironmentPublicationResult::AlreadyPublished { .. }) => {
            JobExecutionOutcome::Success {
                output_provenance: work.output_provenance.clone(),
                execution_id: Some(coordinator.id.clone()),
                diagnostics: Some(json!({ "reconciliation": progress }).to_string()),
            }
        }
        Some(medousa_runtime::WorkEnvironmentPublicationResult::Conflict { .. }) => fatal(
            coordinator,
            "reconciliation publication conflicted; all child results remain preserved".to_string(),
            Some(json!({ "reconciliation": progress }).to_string()),
        ),
        None => fatal(
            coordinator,
            "successful reconciliation work has no publication result".to_string(),
            Some(json!({ "reconciliation": progress }).to_string()),
        ),
    }
}

async fn ensure_job(jobs: &Arc<dyn JobStore>, candidate: NewJob) -> StasisResult<()> {
    if let Some(existing) = jobs.get(&candidate.id).await? {
        if existing.job_type != candidate.job_type || existing.payload_ref != candidate.payload_ref
        {
            return Err(invalid(format!(
                "durable parallel job identity collided with different work: {}",
                candidate.id
            )));
        }
        return Ok(());
    }
    jobs.insert(candidate.into_job()).await
}

fn decode_work_progress(job: &Job) -> StasisResult<Option<WorkEnvironmentJobProgress>> {
    job.progress_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| invalid(format!("decode work-environment progress: {error}")))
}

fn is_terminal(state: &JobState) -> bool {
    matches!(
        state,
        &JobState::Succeeded | &JobState::Failed | &JobState::DeadLetter | &JobState::Canceled
    )
}

fn state_name(state: &JobState) -> String {
    format!("{state:?}").to_lowercase()
}

fn deferred(job: &Job, message: &str) -> JobExecutionOutcome {
    JobExecutionOutcome::Deferred {
        scheduled_at: Utc::now() + chrono::Duration::milliseconds(COORDINATOR_DELAY_MILLIS),
        message: message.to_string(),
        execution_id: Some(job.id.clone()),
        diagnostics: None,
    }
}

fn fatal(job: &Job, message: String, diagnostics: Option<String>) -> JobExecutionOutcome {
    JobExecutionOutcome::FatalFailure {
        message,
        execution_id: Some(job.id.clone()),
        diagnostics,
    }
}

fn exact_results<'a>(
    plan: &ParallelWorkPlan,
    results: &'a [ParallelWorkChildResult],
) -> StasisResult<BTreeMap<&'a str, &'a ParallelWorkChildResult>> {
    if results.len() != plan.children.len() {
        return Err(invalid("parallel result set is incomplete or has extras"));
    }
    let expected: BTreeSet<&str> = plan
        .children
        .iter()
        .map(|child| child.child_id.as_str())
        .collect();
    let mut found = BTreeMap::new();
    for result in results {
        validate_id("result child_id", &result.child_id)?;
        if !expected.contains(result.child_id.as_str())
            || found.insert(result.child_id.as_str(), result).is_some()
        {
            return Err(invalid("parallel result identity is unknown or duplicated"));
        }
    }
    Ok(found)
}

fn require_expected_base(
    publication: Option<&WorkEnvironmentPublication>,
    base_commit: &str,
) -> StasisResult<()> {
    let Some(publication) = publication else {
        return Err(invalid(
            "reconciliation must publish through expected-base CAS",
        ));
    };
    if publication.expected_value.as_deref() != Some(base_commit) {
        return Err(invalid(
            "reconciliation publication expected_value must equal the exact base",
        ));
    }
    Ok(())
}

fn validate_id(name: &str, value: &str) -> StasisResult<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(invalid(format!("{name} is invalid")));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> StasisError {
    StasisError::PortFailure(message.into())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration as StdDuration;

    use chrono::Duration;
    use medousa_runtime::{
        WorkEnvironmentCheckpointPolicy, WorkEnvironmentExecRequest, WorkEnvironmentFence,
        WorkEnvironmentId, WorkEnvironmentImage, WorkEnvironmentNetworkPolicy,
        WorkEnvironmentPublication, WorkEnvironmentRepository, WorkEnvironmentRequirements,
        WorkEnvironmentRetention, WorkEnvironmentSpec, WorkspaceId,
    };
    use stasis::application::runtime::in_memory_runtime::InMemoryRuntime;
    use stasis::domain::runtime::placement::WorkerCapabilities;
    use stasis::domain::runtime::provenance::ContentDigest;
    use stasis::domain::runtime::remote_job_envelope::{
        OriginAuthority, RemoteJobEnvelope, TerminalDeliveryEndpoint,
    };
    use stasis::domain::runtime::resource_lease::FencingToken;
    use stasis::infrastructure::runtime::in_memory_blob_transfer::InMemoryBlobTransfer;

    use super::*;

    const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[derive(Default)]
    struct RecordingRemoteDispatcher {
        submissions: Mutex<Vec<(String, RemoteJobEnvelope)>>,
        automatic_target: Mutex<Option<String>>,
    }

    #[async_trait]
    impl RemoteWorkEnvironmentDispatcher for RecordingRemoteDispatcher {
        fn origin_authority(&self) -> OriginAuthority {
            OriginAuthority {
                runtime_id: "origin-runtime".to_string(),
                authority_id: "origin-runtime".to_string(),
                realm: None,
            }
        }

        fn terminal_delivery(&self) -> TerminalDeliveryEndpoint {
            TerminalDeliveryEndpoint {
                endpoint_id: "origin-runtime:terminal".to_string(),
                protocol: "test".to_string(),
                address: "origin-runtime".to_string(),
            }
        }

        async fn select_target(
            &self,
            _selection_key: &str,
            placement: &PlacementConstraints,
        ) -> StasisResult<Option<String>> {
            Ok(placement
                .target_node
                .clone()
                .or_else(|| self.automatic_target.lock().unwrap().clone()))
        }

        async fn submit_remote_job(
            &self,
            target_runtime_id: &str,
            envelope: RemoteJobEnvelope,
        ) -> StasisResult<String> {
            self.submissions
                .lock()
                .unwrap()
                .push((target_runtime_id.to_string(), envelope));
            Ok("destination-job".to_string())
        }
    }

    fn payload(id: &str, publication: bool) -> WorkEnvironmentJobPayload {
        WorkEnvironmentJobPayload {
            spec: WorkEnvironmentSpec {
                environment_id: WorkEnvironmentId::parse(format!("environment-{id}")).unwrap(),
                workspace_id: WorkspaceId::parse(format!("workspace-{id}")).unwrap(),
                repository: WorkEnvironmentRepository {
                    repository_id: "repository".to_string(),
                    authorized_origin: "https://example.test/repository.git".to_string(),
                },
                base_commit: BASE.to_string(),
                image: WorkEnvironmentImage {
                    reference: "registry.example.test/medousa/dev".to_string(),
                    digest: ContentDigest::sha256_bytes(b"phase-7-image"),
                    platform: "linux/amd64".to_string(),
                },
                checkpoint_ref: None,
                requirements: WorkEnvironmentRequirements::default(),
                mounts: Vec::new(),
                network_policy: WorkEnvironmentNetworkPolicy::Deny,
                secret_refs: Vec::new(),
                fence: WorkEnvironmentFence {
                    stasis_attempt: FencingToken(1),
                    forge_environment_generation: None,
                    forge_execution_generation: None,
                },
                publication: publication.then(|| WorkEnvironmentPublication {
                    target_ref: "refs/heads/main".to_string(),
                    expected_value: Some(BASE.to_string()),
                }),
                retention: WorkEnvironmentRetention::Delete,
            },
            execution: WorkEnvironmentExecRequest {
                idempotency_key: format!("execute-{id}"),
                program: "sh".to_string(),
                args: vec!["-lc".to_string(), "true".to_string()],
                working_directory: Some("/workspace".to_string()),
                environment: BTreeMap::new(),
                stdin: None,
                timeout_seconds: 60,
                max_output_bytes: 1024,
            },
            checkpoint: WorkEnvironmentCheckpointPolicy::default(),
            require_successful_exit: true,
            deadline_at: Some(Utc::now() + Duration::minutes(5)),
            display_name: Some(id.to_string()),
            federation: None,
            portable_coder: None,
        }
    }

    fn plan() -> ParallelWorkPlan {
        ParallelWorkPlan {
            schema_version: PARALLEL_WORK_PLAN_SCHEMA_VERSION,
            plan_id: "parallel-plan".to_string(),
            base_commit: BASE.to_string(),
            children: ["alpha", "beta", "gamma"]
                .into_iter()
                .map(|id| ParallelWorkChild {
                    child_id: id.to_string(),
                    work: payload(id, false),
                })
                .collect(),
            reconciliation: payload("reconcile", true),
        }
    }

    fn capabilities() -> WorkerCapabilities {
        WorkerCapabilities::any().with_capability(medousa_runtime::OCI_WORK_ENVIRONMENT_CAPABILITY)
    }

    async fn child_result(
        blobs: &InMemoryBlobTransfer,
        child: &ParallelWorkChild,
        byte: u8,
    ) -> ParallelWorkChildResult {
        let bundle = blobs
            .put(&[byte; 32], Some("application/vnd.git.bundle"))
            .await
            .unwrap();
        let artifact = blobs.put(&[byte; 8], Some("text/plain")).await.unwrap();
        let manifest = WorkEnvironmentCheckpointManifest {
            schema_version: medousa_runtime::WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION,
            environment_id: child.work.spec.environment_id.clone(),
            workspace_id: child.work.spec.workspace_id.clone(),
            base_commit: BASE.to_string(),
            checkpoint_commit: format!("{byte:040x}"),
            source_bundle: bundle,
            artifacts: vec![WorkEnvironmentArtifact {
                path: "evidence/report.txt".to_string(),
                blob: artifact,
            }],
            fence: child.work.spec.fence.clone(),
            label: Some(child.child_id.clone()),
            created_at: Utc::now(),
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let descriptor = blobs
            .put(&bytes, Some(RECONCILIATION_CHECKPOINT_MEDIA_TYPE))
            .await
            .unwrap();
        ParallelWorkChildResult {
            child_id: child.child_id.clone(),
            job_id: format!("job-{}", child.child_id),
            checkpoint: WorkEnvironmentCheckpoint::from_manifest(descriptor),
        }
    }

    async fn mark_child_terminal(
        runtime: &InMemoryRuntime,
        parent_job_id: &str,
        child: &ParallelWorkChild,
        result: Option<&ParallelWorkChildResult>,
        state: JobState,
    ) {
        let job_id = parallel_child_job_id(parent_job_id, &child.child_id).unwrap();
        let mut job = runtime.job_store.get(&job_id).await.unwrap().unwrap();
        let checkpoint = result.map(|result| result.checkpoint.clone());
        job.state = state.clone();
        job.finished_at = Some(Utc::now());
        job.last_error = (state != JobState::Succeeded).then(|| "child failed".to_string());
        job.output_provenance = checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.provenance.clone());
        job.progress_json = Some(
            serde_json::to_string(&WorkEnvironmentJobProgress {
                schema_version: 1,
                phase: crate::work_environment_job::WorkEnvironmentWorkflowPhase::CleanupEnqueued,
                attempt: 1,
                fence: child.work.spec.fence.clone(),
                environment_state: None,
                execution_result: None,
                portable_coder_result: None,
                checkpoint,
                publication: None,
                cleanup_job_id: Some(format!("{job_id}:cleanup")),
                updated_at: Utc::now(),
            })
            .unwrap(),
        );
        runtime.job_store.save(job).await.unwrap();
    }

    async fn process_once(runtime: &InMemoryRuntime) -> Option<String> {
        runtime
            .process_once_with_capabilities(
                "default",
                "phase-7-worker",
                Utc::now(),
                &capabilities(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn three_isolated_results_become_one_portable_reconciliation_checkpoint() {
        let blobs = InMemoryBlobTransfer::new();
        let plan = plan();
        let mut results = Vec::new();
        for (index, child) in plan.children.iter().enumerate() {
            results.push(child_result(&blobs, child, (index + 1) as u8).await);
        }
        results.reverse();

        let prepared = prepare_reconciliation_input(&blobs, &plan, &results, Utc::now())
            .await
            .unwrap();
        assert_eq!(prepared.manifest.primary_child_id, "alpha");
        assert_eq!(prepared.manifest.children.len(), 3);
        assert!(
            prepared.manifest.children[0]
                .materialized_bundle_path
                .is_none()
        );
        assert!(
            prepared.manifest.children[1]
                .materialized_bundle_path
                .as_deref()
                .unwrap()
                .ends_with("beta.bundle")
        );

        let bytes = blobs.get(&prepared.checkpoint.manifest).await.unwrap();
        let checkpoint: WorkEnvironmentCheckpointManifest = serde_json::from_slice(&bytes).unwrap();
        checkpoint.validate().unwrap();
        assert_eq!(checkpoint.artifacts.len(), 6);
        assert!(
            checkpoint.artifacts.iter().any(|artifact| {
                artifact.path == ".medousa/reconciliation/children/gamma.bundle"
            })
        );
        assert!(
            checkpoint
                .artifacts
                .iter()
                .any(|artifact| { artifact.path == ".medousa/reconciliation/manifest.json" })
        );

        let work = reconciliation_work_payload(&plan, &prepared);
        assert_eq!(work.spec.checkpoint_ref, Some(prepared.checkpoint));
        assert_eq!(
            work.execution
                .environment
                .get("MEDOUSA_RECONCILIATION_MANIFEST")
                .map(String::as_str),
            Some("/workspace/.medousa/reconciliation/manifest.json")
        );
    }

    #[tokio::test]
    async fn missing_or_duplicated_child_results_never_choose_a_silent_winner() {
        let blobs = InMemoryBlobTransfer::new();
        let plan = plan();
        let alpha = child_result(&blobs, &plan.children[0], 1).await;
        let duplicate = alpha.clone();
        let error = prepare_reconciliation_input(&blobs, &plan, &[alpha, duplicate], Utc::now())
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("incomplete or has extras"));
    }

    #[test]
    fn parallel_children_cannot_race_the_reconciliation_publication() {
        let mut plan = plan();
        plan.children[0].work.spec.publication = Some(WorkEnvironmentPublication {
            target_ref: "refs/heads/main".to_string(),
            expected_value: Some(BASE.to_string()),
        });
        let error = plan.validate(Utc::now()).unwrap_err();
        assert!(error.to_string().contains("instead of publishing"));
    }

    #[test]
    fn child_and_reconciliation_job_identities_are_replay_stable() {
        assert_eq!(
            parallel_child_job_id("parent-job", "alpha").unwrap(),
            "parent-job:parallel:alpha"
        );
        assert_eq!(
            parallel_reconciliation_job_id("parent-job").unwrap(),
            "parent-job:reconcile"
        );
    }

    #[tokio::test]
    async fn automatic_selection_enters_the_same_replay_stable_proxy_path() {
        let runtime = InMemoryRuntime::new();
        let composition = RuntimeComposition::InMemory(runtime.clone());
        let blobs = Arc::new(InMemoryBlobTransfer::new());
        let dispatcher = Arc::new(RecordingRemoteDispatcher::default());
        *dispatcher.automatic_target.lock().unwrap() = Some("worker-runtime".to_string());
        register_parallel_work_environment_job_handlers(&composition, blobs, Some(dispatcher))
            .await
            .unwrap();
        let plan = plan();
        runtime
            .job_store
            .insert(
                plan.into_job("automatic-parent", "default", "root", Utc::now())
                    .unwrap()
                    .into_job(),
            )
            .await
            .unwrap();

        assert_eq!(
            process_once(&runtime).await.as_deref(),
            Some("automatic-parent")
        );
        for child_id in ["alpha", "beta", "gamma"] {
            let child_job_id = parallel_child_job_id("automatic-parent", child_id).unwrap();
            let child = runtime.job_store.get(&child_job_id).await.unwrap().unwrap();
            assert_eq!(child.job_type, REMOTE_WORK_ENVIRONMENT_PROXY_JOB_TYPE);
            assert!(child.placement.is_unrestricted());
            let proxy: RemoteWorkEnvironmentProxyPayload =
                serde_json::from_str(&child.payload_ref).unwrap();
            assert_eq!(proxy.target_runtime_id, "worker-runtime");
            assert_eq!(
                proxy
                    .work
                    .spec
                    .placement_constraints()
                    .target_node
                    .as_deref(),
                Some("worker-runtime")
            );
            assert_eq!(
                proxy.envelope_id,
                remote_child_envelope_id(&child_job_id).unwrap()
            );
        }
    }

    #[tokio::test]
    async fn targeted_child_dispatches_once_and_completes_from_local_terminal_truth() {
        let runtime = InMemoryRuntime::new();
        let composition = RuntimeComposition::InMemory(runtime.clone());
        let blobs = Arc::new(InMemoryBlobTransfer::new());
        let dispatcher = Arc::new(RecordingRemoteDispatcher::default());
        register_parallel_work_environment_job_handlers(
            &composition,
            blobs.clone(),
            Some(dispatcher.clone()),
        )
        .await
        .unwrap();
        let mut plan = plan();
        plan.children[0]
            .work
            .spec
            .requirements
            .placement
            .target_node = Some("worker-runtime".to_string());
        runtime
            .job_store
            .insert(
                plan.clone()
                    .into_job("targeted-parent", "default", "root", Utc::now())
                    .unwrap()
                    .into_job(),
            )
            .await
            .unwrap();

        assert_eq!(
            process_once(&runtime).await.as_deref(),
            Some("targeted-parent")
        );
        let child_job_id = parallel_child_job_id("targeted-parent", "alpha").unwrap();
        let child = runtime.job_store.get(&child_job_id).await.unwrap().unwrap();
        assert_eq!(child.job_type, REMOTE_WORK_ENVIRONMENT_PROXY_JOB_TYPE);
        assert_eq!(
            process_once(&runtime).await.as_deref(),
            Some(child_job_id.as_str())
        );
        let submitted = dispatcher.submissions.lock().unwrap().clone();
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].0, "worker-runtime");
        assert_eq!(
            submitted[0].1.envelope_id,
            remote_child_envelope_id(&child_job_id).unwrap()
        );

        let child_result = child_result(&blobs, &plan.children[0], 7).await;
        let remote = crate::work_environment_federation::RemoteWorkEnvironmentResult {
            schema_version:
                crate::work_environment_federation::WORK_ENVIRONMENT_RESULT_SCHEMA_VERSION,
            envelope_id: submitted[0].1.envelope_id.clone(),
            remote_job_id: "destination-job".to_string(),
            succeeded: true,
            terminal_state: "succeeded".to_string(),
            execution_result: None,
            portable_coder_result: None,
            checkpoint: Some(child_result.checkpoint.clone()),
            publication: None,
            error_message: None,
            finished_at: Utc::now(),
        };
        let output = blobs
            .put(
                &serde_json::to_vec(&remote).unwrap(),
                Some(crate::work_environment_federation::WORK_ENVIRONMENT_RESULT_MEDIA_TYPE),
            )
            .await
            .unwrap();
        let terminal = stasis::domain::runtime::federation::FederatedTerminalResult {
            schema_version:
                stasis::domain::runtime::federation::FEDERATED_TERMINAL_RESULT_SCHEMA_VERSION_V1,
            result_id: format!("{}:terminal", remote.envelope_id),
            envelope_id: remote.envelope_id.clone(),
            job_id: remote.remote_job_id.clone(),
            job_type: crate::work_environment_job::WORK_ENVIRONMENT_JOB_TYPE.to_string(),
            succeeded: true,
            output: Some(output),
            output_provenance: Some(child_result.checkpoint.provenance.clone()),
            error_message: None,
            origin_authority: dispatcher.origin_authority(),
            terminal_delivery: dispatcher.terminal_delivery(),
            correlation_id: "targeted-parent".to_string(),
            causation_id: child_job_id.clone(),
            occurred_at: remote.finished_at,
            signature: stasis::domain::runtime::remote_job_envelope::EnvelopeSignature {
                algorithm: "test".to_string(),
                key_id: "test".to_string(),
                signature_hex: "test".to_string(),
            },
        };
        let stored = blobs
            .put(
                &serde_json::to_vec(&terminal).unwrap(),
                Some("application/vnd.stasis.federated-terminal-result+json"),
            )
            .await
            .unwrap();
        crate::work_environment_federation::record_remote_terminal_result(
            &composition,
            &terminal,
            &stored,
        )
        .await
        .unwrap();

        for _ in 0..4 {
            tokio::time::sleep(StdDuration::from_millis(12)).await;
            let _ = process_once(&runtime).await;
            let child = runtime.job_store.get(&child_job_id).await.unwrap().unwrap();
            if child.state == JobState::Succeeded {
                let progress = decode_work_progress(&child).unwrap().unwrap();
                assert_eq!(progress.checkpoint, Some(child_result.checkpoint));
                assert_eq!(dispatcher.submissions.lock().unwrap().len(), 1);
                return;
            }
        }
        panic!("targeted child did not consume its local terminal receipt");
    }

    #[tokio::test]
    async fn durable_parent_fans_out_and_waits_for_one_reconciliation_job() {
        let runtime = InMemoryRuntime::new();
        let composition = RuntimeComposition::InMemory(runtime.clone());
        let blobs = Arc::new(InMemoryBlobTransfer::new());
        register_parallel_work_environment_job_handlers(&composition, blobs.clone(), None)
            .await
            .unwrap();
        let plan = plan();
        runtime
            .job_store
            .insert(
                plan.clone()
                    .into_job("parallel-parent", "default", "root", Utc::now())
                    .unwrap()
                    .into_job(),
            )
            .await
            .unwrap();

        assert_eq!(
            process_once(&runtime).await.as_deref(),
            Some("parallel-parent")
        );
        let mut results = Vec::new();
        for (index, child) in plan.children.iter().enumerate() {
            let result = child_result(&blobs, child, (index + 1) as u8).await;
            mark_child_terminal(
                &runtime,
                "parallel-parent",
                child,
                Some(&result),
                JobState::Succeeded,
            )
            .await;
            results.push(result);
        }

        tokio::time::sleep(StdDuration::from_millis(12)).await;
        assert_eq!(
            process_once(&runtime).await.as_deref(),
            Some("parallel-parent")
        );
        let reconciliation_id = parallel_reconciliation_job_id("parallel-parent").unwrap();
        assert!(
            runtime
                .job_store
                .get(&reconciliation_id)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            process_once(&runtime).await.as_deref(),
            Some(reconciliation_id.as_str())
        );

        let work_id = format!("{reconciliation_id}:work");
        let mut work = runtime.job_store.get(&work_id).await.unwrap().unwrap();
        let work_payload: WorkEnvironmentJobPayload =
            serde_json::from_str(&work.payload_ref).unwrap();
        let checkpoint = work_payload.spec.checkpoint_ref.unwrap();
        let value = checkpoint.provenance.compact();
        work.state = JobState::Succeeded;
        work.finished_at = Some(Utc::now());
        work.output_provenance = Some(checkpoint.provenance.clone());
        work.progress_json = Some(
            serde_json::to_string(&WorkEnvironmentJobProgress {
                schema_version: 1,
                phase: crate::work_environment_job::WorkEnvironmentWorkflowPhase::CleanupEnqueued,
                attempt: 1,
                fence: plan.reconciliation.spec.fence.clone(),
                environment_state: None,
                execution_result: None,
                portable_coder_result: None,
                checkpoint: Some(checkpoint),
                publication: Some(
                    medousa_runtime::WorkEnvironmentPublicationResult::Published {
                        target_ref: "refs/heads/main".to_string(),
                        value,
                        previous: Some(BASE.to_string()),
                    },
                ),
                cleanup_job_id: Some(format!("{work_id}:cleanup")),
                updated_at: Utc::now(),
            })
            .unwrap(),
        );
        runtime.job_store.save(work).await.unwrap();

        for _ in 0..8 {
            tokio::time::sleep(StdDuration::from_millis(12)).await;
            let _ = process_once(&runtime).await;
            let parent = runtime
                .job_store
                .get("parallel-parent")
                .await
                .unwrap()
                .unwrap();
            if parent.state == JobState::Succeeded {
                let progress: ParallelCoordinatorProgress =
                    serde_json::from_str(parent.progress_json.as_deref().unwrap()).unwrap();
                assert_eq!(progress.children.len(), 3);
                assert_eq!(
                    progress.reconciliation_job_id.as_deref(),
                    Some(reconciliation_id.as_str())
                );
                assert_eq!(
                    runtime
                        .job_store
                        .get(&reconciliation_id)
                        .await
                        .unwrap()
                        .unwrap()
                        .state,
                    JobState::Succeeded
                );
                return;
            }
        }
        panic!("parallel parent did not observe reconciliation completion");
    }

    #[tokio::test]
    async fn failed_child_is_preserved_and_never_enqueues_reconciliation() {
        let runtime = InMemoryRuntime::new();
        let composition = RuntimeComposition::InMemory(runtime.clone());
        let blobs = Arc::new(InMemoryBlobTransfer::new());
        register_parallel_work_environment_job_handlers(&composition, blobs.clone(), None)
            .await
            .unwrap();
        let plan = plan();
        runtime
            .job_store
            .insert(
                plan.clone()
                    .into_job("failed-parent", "default", "root", Utc::now())
                    .unwrap()
                    .into_job(),
            )
            .await
            .unwrap();
        let _ = process_once(&runtime).await;
        for (index, child) in plan.children.iter().enumerate() {
            let result = child_result(&blobs, child, (index + 1) as u8).await;
            let state = if index == 1 {
                JobState::Failed
            } else {
                JobState::Succeeded
            };
            mark_child_terminal(&runtime, "failed-parent", child, Some(&result), state).await;
        }
        tokio::time::sleep(StdDuration::from_millis(12)).await;
        let _ = process_once(&runtime).await;
        let parent = runtime
            .job_store
            .get("failed-parent")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(parent.state, JobState::DeadLetter);
        let progress: ParallelCoordinatorProgress =
            serde_json::from_str(parent.progress_json.as_deref().unwrap()).unwrap();
        assert_eq!(progress.children.len(), 3);
        assert_eq!(progress.children[1].state, "failed");
        assert!(progress.children[1].error_message.is_some());
        assert!(
            runtime
                .job_store
                .get(&parallel_reconciliation_job_id("failed-parent").unwrap())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn reconciliation_cas_conflict_is_terminal_and_keeps_the_preserved_checkpoint() {
        let plan = plan();
        let coordinator = plan
            .clone()
            .into_job("parent", "default", "root", Utc::now())
            .unwrap()
            .into_job();
        let mut work = plan
            .reconciliation
            .clone()
            .into_job("parent:reconcile:work", "default", "parent", Utc::now())
            .unwrap()
            .into_job();
        let checkpoint = WorkEnvironmentCheckpoint::from_manifest(
            BlobDescriptor::from_bytes(b"preserved-conflict")
                .with_media_type(RECONCILIATION_CHECKPOINT_MEDIA_TYPE),
        );
        work.output_provenance = Some(checkpoint.provenance.clone());
        let progress = ParallelReconciliationProgress {
            schema_version: COORDINATOR_PROGRESS_SCHEMA_VERSION,
            work_job_id: work.id.clone(),
            checkpoint: Some(checkpoint.clone()),
            publication: Some(
                medousa_runtime::WorkEnvironmentPublicationResult::Conflict {
                    target_ref: "refs/heads/main".to_string(),
                    expected: Some(BASE.to_string()),
                    found: Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
                    preserved_checkpoint: Box::new(checkpoint),
                },
            ),
            updated_at: Utc::now(),
        };
        match completed_reconciliation_outcome(&coordinator, &work, &progress) {
            JobExecutionOutcome::FatalFailure {
                message,
                diagnostics: Some(diagnostics),
                ..
            } => {
                assert!(message.contains("publication conflicted"));
                assert!(diagnostics.contains("preserved_checkpoint"));
                assert!(diagnostics.contains("conflict"));
            }
            other => panic!("CAS conflict must be terminal, got {other:?}"),
        }
    }
}
