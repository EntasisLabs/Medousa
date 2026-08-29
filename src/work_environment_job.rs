//! Stasis-coordinated durable lifecycle for daemon-owned work environments.
//!
//! The parent job advances one idempotent boundary per lease. Its progress is
//! portable control state only: spec, logical environment identity, fences,
//! results, checkpoints, publication outcome, and provenance. Process-local
//! container ids and adapter handles never enter Stasis.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use medousa_runtime::{
    WorkEnvironmentCheckpoint, WorkEnvironmentCheckpointPolicy, WorkEnvironmentError,
    WorkEnvironmentExecRequest, WorkEnvironmentExecResult, WorkEnvironmentFence,
    WorkEnvironmentPort, WorkEnvironmentPublicationResult, WorkEnvironmentRetention,
    WorkEnvironmentSpec, WorkEnvironmentState,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::application::runtime::job_context::JobContext;
use stasis::application::runtime::job_lifecycle::JobLifecycleEvent;
use stasis::domain::runtime::job::{BackoffPolicy, Job, JobState, NewJob};
use stasis::domain::runtime::resource_lease::FencingToken;
use stasis::ports::outbound::runtime::blob_transfer::BlobTransferPort;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::work_environment_federation::{
    SignedFederatedTerminalDelivery, WorkEnvironmentFederationContext,
    WorkEnvironmentFederationServices, encode_remote_terminal_result,
};

pub const WORK_ENVIRONMENT_JOB_TYPE: &str = "workflow.medousa.work_environment";
pub const WORK_ENVIRONMENT_CLEANUP_JOB_TYPE: &str = "workflow.medousa.work_environment_cleanup";
pub const WORK_ENVIRONMENT_TERMINAL_DELIVERY_JOB_TYPE: &str =
    "workflow.medousa.work_environment_terminal_delivery";
const WORK_ENVIRONMENT_PROGRESS_SCHEMA_VERSION: u32 = 1;
const WORK_ENVIRONMENT_CLEANUP_SCHEMA_VERSION: u32 = 1;
const BOUNDARY_DELAY_MILLIS: i64 = 1;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

fn default_require_successful_exit() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkEnvironmentJobPayload {
    pub spec: WorkEnvironmentSpec,
    pub execution: WorkEnvironmentExecRequest,
    #[serde(default)]
    pub checkpoint: WorkEnvironmentCheckpointPolicy,
    #[serde(default = "default_require_successful_exit")]
    pub require_successful_exit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub federation: Option<WorkEnvironmentFederationContext>,
}

impl WorkEnvironmentJobPayload {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), WorkEnvironmentError> {
        self.spec.validate(now)?;
        self.checkpoint.validate()?;
        if let Some(deadline) = self.deadline_at
            && deadline <= now
        {
            return Err(WorkEnvironmentError::InvalidSpec(
                "work-environment job deadline must be in the future".to_string(),
            ));
        }
        if let Some(display_name) = self.display_name.as_deref()
            && (display_name.trim().is_empty()
                || display_name.len() > 256
                || display_name.chars().any(char::is_control))
        {
            return Err(WorkEnvironmentError::InvalidSpec(
                "work-environment display_name is invalid".to_string(),
            ));
        }
        Ok(())
    }

    pub fn to_payload_ref(&self) -> StasisResult<String> {
        serde_json::to_string(self).map_err(|error| {
            StasisError::PortFailure(format!("encode work-environment job payload: {error}"))
        })
    }

    pub fn into_job(
        self,
        job_id: impl Into<String>,
        queue: impl Into<String>,
        causation_id: impl Into<String>,
        scheduled_at: DateTime<Utc>,
    ) -> StasisResult<NewJob> {
        self.validate(scheduled_at)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let job_id = job_id.into();
        let input_provenance = self
            .spec
            .checkpoint_ref
            .as_ref()
            .map(|checkpoint| checkpoint.provenance.clone());
        let placement = self.spec.placement_constraints();
        let payload_ref = self.to_payload_ref()?;
        Ok(NewJob {
            idempotency_key: format!("idem-{job_id}"),
            correlation_id: job_id.clone(),
            trace_id: job_id.clone(),
            id: job_id,
            queue: queue.into(),
            job_type: WORK_ENVIRONMENT_JOB_TYPE.to_string(),
            payload_ref,
            priority: 100,
            // There are six durable lifecycle boundaries. Leave enough retry
            // budget for a lease takeover at every boundary plus transient
            // adapter failures without changing the logical job identity.
            max_attempts: 12,
            causation_id: causation_id.into(),
            input_provenance,
            placement,
            scheduled_at,
            backoff_policy: BackoffPolicy::default(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkEnvironmentWorkflowPhase {
    Pending,
    Materialized,
    Started,
    Executed,
    Checkpointed,
    Published,
    CleanupEnqueued,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkEnvironmentJobProgress {
    pub schema_version: u32,
    pub phase: WorkEnvironmentWorkflowPhase,
    pub attempt: u32,
    pub fence: WorkEnvironmentFence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_state: Option<WorkEnvironmentState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_result: Option<WorkEnvironmentExecResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<WorkEnvironmentCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<WorkEnvironmentPublicationResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_job_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl WorkEnvironmentJobProgress {
    fn new(spec: &WorkEnvironmentSpec, attempt: u32) -> Self {
        Self {
            schema_version: WORK_ENVIRONMENT_PROGRESS_SCHEMA_VERSION,
            phase: WorkEnvironmentWorkflowPhase::Pending,
            attempt,
            fence: fence_for_attempt(spec, attempt),
            environment_state: None,
            execution_result: None,
            checkpoint: None,
            publication: None,
            cleanup_job_id: None,
            updated_at: Utc::now(),
        }
    }

    fn validate(&self) -> StasisResult<()> {
        if self.schema_version != WORK_ENVIRONMENT_PROGRESS_SCHEMA_VERSION {
            return Err(StasisError::PortFailure(format!(
                "unsupported work-environment progress schema_version={}",
                self.schema_version
            )));
        }
        if self.attempt == 0 || self.fence.stasis_attempt.0 != u64::from(self.attempt) {
            return Err(StasisError::PortFailure(
                "work-environment progress attempt and fence disagree".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkEnvironmentCleanupPayload {
    schema_version: u32,
    environment_id: medousa_runtime::WorkEnvironmentId,
    fence: WorkEnvironmentFence,
    retention: WorkEnvironmentRetention,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output_checkpoint: Option<WorkEnvironmentCheckpoint>,
}

struct WorkEnvironmentJobHandler {
    environment: Arc<dyn WorkEnvironmentPort>,
    jobs: Arc<dyn JobStore>,
    federated_terminal_enabled: bool,
}

struct WorkEnvironmentCleanupJobHandler {
    environment: Arc<dyn WorkEnvironmentPort>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkEnvironmentTerminalDeliveryPayload {
    parent_job_id: String,
    federation: WorkEnvironmentFederationContext,
}

struct WorkEnvironmentTerminalDeliveryJobHandler {
    jobs: Arc<dyn JobStore>,
    blobs: Arc<dyn BlobTransferPort>,
    delivery: Arc<dyn SignedFederatedTerminalDelivery>,
}

pub async fn register_work_environment_job_handlers(
    composition: &RuntimeComposition,
    environment: Arc<dyn WorkEnvironmentPort>,
) -> anyhow::Result<()> {
    register_work_environment_job_handlers_inner(composition, environment, None).await
}

pub async fn register_federated_work_environment_job_handlers(
    composition: &RuntimeComposition,
    environment: Arc<dyn WorkEnvironmentPort>,
    federation: WorkEnvironmentFederationServices,
) -> anyhow::Result<()> {
    register_work_environment_job_handlers_inner(composition, environment, Some(federation)).await
}

async fn register_work_environment_job_handlers_inner(
    composition: &RuntimeComposition,
    environment: Arc<dyn WorkEnvironmentPort>,
    federation: Option<WorkEnvironmentFederationServices>,
) -> anyhow::Result<()> {
    let federated_terminal_enabled = federation.is_some();
    match composition {
        RuntimeComposition::InMemory(runtime) => {
            let jobs: Arc<dyn JobStore> = Arc::new(runtime.job_store.clone());
            runtime.register_handler(WorkEnvironmentJobHandler {
                environment: Arc::clone(&environment),
                jobs: Arc::clone(&jobs),
                federated_terminal_enabled,
            })?;
            runtime.register_handler(WorkEnvironmentCleanupJobHandler { environment })?;
            if let Some(federation) = federation {
                runtime.register_handler(WorkEnvironmentTerminalDeliveryJobHandler {
                    jobs,
                    blobs: federation.blobs,
                    delivery: federation.terminal_delivery,
                })?;
            }
        }
        RuntimeComposition::Surreal(runtime) => {
            let jobs: Arc<dyn JobStore> = Arc::new(runtime.job_store.clone());
            runtime.register_handler(WorkEnvironmentJobHandler {
                environment: Arc::clone(&environment),
                jobs: Arc::clone(&jobs),
                federated_terminal_enabled,
            })?;
            runtime.register_handler(WorkEnvironmentCleanupJobHandler { environment })?;
            if let Some(federation) = federation {
                runtime.register_handler(WorkEnvironmentTerminalDeliveryJobHandler {
                    jobs,
                    blobs: federation.blobs,
                    delivery: federation.terminal_delivery,
                })?;
            }
        }
    }
    Ok(())
}

impl WorkEnvironmentJobHandler {
    fn parse(job: &Job) -> StasisResult<WorkEnvironmentJobPayload> {
        let payload: WorkEnvironmentJobPayload =
            serde_json::from_str(&job.payload_ref).map_err(|error| {
                StasisError::PortFailure(format!(
                    "invalid work-environment payload for job {}: {error}",
                    job.id
                ))
            })?;
        if job.placement != payload.spec.placement_constraints() {
            return Err(StasisError::PortFailure(
                "work-environment job placement does not match its environment spec".to_string(),
            ));
        }
        Ok(payload)
    }

    fn progress(
        job: &Job,
        payload: &WorkEnvironmentJobPayload,
        attempt: u32,
    ) -> StasisResult<WorkEnvironmentJobProgress> {
        let mut progress = match job.progress_json.as_deref() {
            Some(raw) => serde_json::from_str(raw).map_err(|error| {
                StasisError::PortFailure(format!(
                    "invalid work-environment progress for job {}: {error}",
                    job.id
                ))
            })?,
            None => WorkEnvironmentJobProgress::new(&payload.spec, attempt),
        };
        progress.validate()?;
        if progress.attempt > attempt {
            return Err(StasisError::PortFailure(
                "work-environment progress belongs to a newer Stasis attempt".to_string(),
            ));
        }
        progress.attempt = attempt;
        progress.fence = fence_for_attempt(&payload.spec, attempt);
        Ok(progress)
    }

    async fn defer(
        ctx: &JobContext,
        progress: &mut WorkEnvironmentJobProgress,
        phase: WorkEnvironmentWorkflowPhase,
        message: &'static str,
    ) -> StasisResult<JobExecutionOutcome> {
        progress.phase = phase;
        progress.updated_at = Utc::now();
        ctx.progress(&*progress).await?;
        Ok(JobExecutionOutcome::Deferred {
            scheduled_at: Utc::now() + chrono::Duration::milliseconds(BOUNDARY_DELAY_MILLIS),
            message: message.to_string(),
            execution_id: Some(ctx.job_id.clone()),
            diagnostics: Some(
                json!({
                    "provider": "medousa-work-environment",
                    "phase": phase,
                    "attempt": progress.attempt,
                    "environment_id": progress.environment_state.as_ref().map(|state| state.environment_id.as_str()),
                })
                .to_string(),
            ),
        })
    }

    async fn enqueue_cleanup(
        &self,
        job: &Job,
        payload: &WorkEnvironmentJobPayload,
        progress: &WorkEnvironmentJobProgress,
    ) -> StasisResult<String> {
        let cleanup_id = format!("{}:cleanup", job.id);
        let cleanup_payload = WorkEnvironmentCleanupPayload {
            schema_version: WORK_ENVIRONMENT_CLEANUP_SCHEMA_VERSION,
            environment_id: payload.spec.environment_id.clone(),
            fence: progress.fence.clone(),
            retention: payload.spec.retention.clone(),
            output_checkpoint: progress.checkpoint.clone(),
        };
        let payload_ref = serde_json::to_string(&cleanup_payload).map_err(|error| {
            StasisError::PortFailure(format!("encode work-environment cleanup payload: {error}"))
        })?;
        if let Some(existing) = self.jobs.get(&cleanup_id).await? {
            if existing.job_type != WORK_ENVIRONMENT_CLEANUP_JOB_TYPE
                || existing.payload_ref != payload_ref
            {
                return Err(StasisError::PortFailure(
                    "work-environment cleanup identity collided with different work".to_string(),
                ));
            }
            return Ok(cleanup_id);
        }
        self.jobs
            .insert(
                NewJob {
                    id: cleanup_id.clone(),
                    queue: job.queue.clone(),
                    job_type: WORK_ENVIRONMENT_CLEANUP_JOB_TYPE.to_string(),
                    payload_ref,
                    priority: job.priority,
                    max_attempts: 10,
                    idempotency_key: format!("idem-{cleanup_id}"),
                    correlation_id: job.correlation_id.clone(),
                    causation_id: job.id.clone(),
                    trace_id: job.trace_id.clone(),
                    input_provenance: progress
                        .checkpoint
                        .as_ref()
                        .map(|checkpoint| checkpoint.provenance.clone()),
                    placement: payload.spec.placement_constraints(),
                    scheduled_at: Utc::now(),
                    backoff_policy: BackoffPolicy::default(),
                }
                .into_job(),
            )
            .await?;
        Ok(cleanup_id)
    }

    async fn enqueue_federated_terminal(
        &self,
        job: &Job,
        payload: &WorkEnvironmentJobPayload,
    ) -> StasisResult<Option<String>> {
        let Some(federation) = payload.federation.as_ref() else {
            return Ok(None);
        };
        if !self.federated_terminal_enabled {
            return Err(StasisError::PortFailure(
                "federated work-environment job was admitted without terminal delivery".to_string(),
            ));
        }
        let delivery_id = format!("{}:federated-terminal", job.id);
        let delivery_payload = WorkEnvironmentTerminalDeliveryPayload {
            parent_job_id: job.id.clone(),
            federation: federation.clone(),
        };
        let payload_ref = serde_json::to_string(&delivery_payload).map_err(|error| {
            StasisError::PortFailure(format!("encode federated terminal delivery: {error}"))
        })?;
        if let Some(existing) = self.jobs.get(&delivery_id).await? {
            if existing.job_type != WORK_ENVIRONMENT_TERMINAL_DELIVERY_JOB_TYPE
                || existing.payload_ref != payload_ref
            {
                return Err(StasisError::PortFailure(
                    "federated terminal delivery identity collided with different work".to_string(),
                ));
            }
            return Ok(Some(delivery_id));
        }
        self.jobs
            .insert(
                NewJob {
                    id: delivery_id.clone(),
                    queue: job.queue.clone(),
                    job_type: WORK_ENVIRONMENT_TERMINAL_DELIVERY_JOB_TYPE.to_string(),
                    payload_ref,
                    priority: job.priority,
                    max_attempts: 10,
                    idempotency_key: format!("idem-{delivery_id}"),
                    correlation_id: job.correlation_id.clone(),
                    causation_id: job.id.clone(),
                    trace_id: job.trace_id.clone(),
                    input_provenance: job.output_provenance.clone(),
                    placement:
                        stasis::domain::runtime::placement::PlacementConstraints::unrestricted(),
                    scheduled_at: Utc::now(),
                    backoff_policy: BackoffPolicy::default(),
                }
                .into_job(),
            )
            .await?;
        Ok(Some(delivery_id))
    }

    async fn terminal_success(
        &self,
        job: &Job,
        payload: &WorkEnvironmentJobPayload,
        ctx: &JobContext,
        progress: &mut WorkEnvironmentJobProgress,
    ) -> StasisResult<JobExecutionOutcome> {
        let cleanup_job_id = self.enqueue_cleanup(job, payload, progress).await?;
        progress.cleanup_job_id = Some(cleanup_job_id.clone());
        progress.phase = WorkEnvironmentWorkflowPhase::CleanupEnqueued;
        progress.updated_at = Utc::now();
        ctx.progress(&*progress).await?;
        let checkpoint = progress.checkpoint.as_ref().ok_or_else(|| {
            StasisError::PortFailure("work-environment completed without a checkpoint".to_string())
        })?;
        Ok(JobExecutionOutcome::Success {
            output_provenance: Some(checkpoint.provenance.clone()),
            execution_id: Some(job.id.clone()),
            diagnostics: Some(terminal_diagnostics(progress, &cleanup_job_id)),
        })
    }

    async fn terminal_execution_failure(
        &self,
        job: &Job,
        payload: &WorkEnvironmentJobPayload,
        ctx: &JobContext,
        progress: &mut WorkEnvironmentJobProgress,
    ) -> StasisResult<JobExecutionOutcome> {
        let cleanup_job_id = self.enqueue_cleanup(job, payload, progress).await?;
        progress.cleanup_job_id = Some(cleanup_job_id.clone());
        progress.updated_at = Utc::now();
        ctx.progress(&*progress).await?;
        let result = progress.execution_result.as_ref().ok_or_else(|| {
            StasisError::PortFailure("missing failed environment execution result".to_string())
        })?;
        Ok(JobExecutionOutcome::FatalFailure {
            message: format!(
                "work-environment execution exited with {}",
                result
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "no status".to_string())
            ),
            execution_id: Some(result.execution_id.clone()),
            diagnostics: Some(terminal_diagnostics(progress, &cleanup_job_id)),
        })
    }
}

#[async_trait]
impl JobHandler for WorkEnvironmentJobHandler {
    fn job_type(&self) -> &'static str {
        WORK_ENVIRONMENT_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> StasisResult<JobExecutionOutcome> {
        let _ = job;
        Err(StasisError::PortFailure(
            "work-environment jobs require Stasis JobContext".to_string(),
        ))
    }

    async fn execute_with_context(
        &self,
        job: &Job,
        ctx: JobContext,
    ) -> StasisResult<JobExecutionOutcome> {
        let payload = match Self::parse(job) {
            Ok(payload) => payload,
            Err(error) => return Ok(fatal(job, error.to_string(), None)),
        };
        if let Some(deadline) = payload.deadline_at
            && deadline <= Utc::now()
        {
            return Ok(fatal(
                job,
                "work-environment job deadline elapsed".to_string(),
                None,
            ));
        }
        let mut progress = match Self::progress(job, &payload, ctx.attempt) {
            Ok(progress) => progress,
            Err(error) => return Ok(fatal(job, error.to_string(), None)),
        };
        let mut spec = payload.spec.clone();
        spec.fence = progress.fence.clone();

        if progress.phase == WorkEnvironmentWorkflowPhase::CleanupEnqueued {
            return self
                .terminal_success(job, &payload, &ctx, &mut progress)
                .await;
        }

        let handle = match run_boundary(
            &ctx,
            payload.deadline_at,
            self.environment.materialize(spec.clone()),
        )
        .await
        {
            Ok(handle) => handle,
            Err(failure) => return Ok(boundary_failure(job, failure)),
        };

        match progress.phase {
            WorkEnvironmentWorkflowPhase::Pending => {
                let state = self.environment.inspect(&handle).await.map_err(map_port)?;
                progress.environment_state = Some(state);
                Self::defer(
                    &ctx,
                    &mut progress,
                    WorkEnvironmentWorkflowPhase::Materialized,
                    "work environment materialized",
                )
                .await
            }
            WorkEnvironmentWorkflowPhase::Materialized => {
                let state = match run_boundary(
                    &ctx,
                    payload.deadline_at,
                    self.environment.start(&handle, &progress.fence),
                )
                .await
                {
                    Ok(state) => state,
                    Err(failure) => return Ok(boundary_failure(job, failure)),
                };
                progress.environment_state = Some(state);
                Self::defer(
                    &ctx,
                    &mut progress,
                    WorkEnvironmentWorkflowPhase::Started,
                    "work environment started",
                )
                .await
            }
            WorkEnvironmentWorkflowPhase::Started => {
                // A newer Stasis fence may have reconstructed the same logical
                // environment as a fresh, ready container. Reconcile the last
                // completed durable boundary before replaying exec; `start` is
                // idempotent when the prior container is still running.
                let state = match run_boundary(
                    &ctx,
                    payload.deadline_at,
                    self.environment.start(&handle, &progress.fence),
                )
                .await
                {
                    Ok(state) => state,
                    Err(failure) => return Ok(boundary_failure(job, failure)),
                };
                progress.environment_state = Some(state);
                let mut execution = payload.execution.clone();
                execution.idempotency_key = format!("{}:execute", job.id);
                let result = match run_boundary(
                    &ctx,
                    payload.deadline_at,
                    self.environment.exec(&handle, execution, &progress.fence),
                )
                .await
                {
                    Ok(result) => result,
                    Err(failure) => return Ok(boundary_failure(job, failure)),
                };
                progress.execution_result = Some(result);
                Self::defer(
                    &ctx,
                    &mut progress,
                    WorkEnvironmentWorkflowPhase::Executed,
                    "work environment execution completed",
                )
                .await
            }
            WorkEnvironmentWorkflowPhase::Executed => {
                let mut policy = payload.checkpoint.clone();
                policy.idempotency_key = Some(format!("{}:checkpoint", job.id));
                let checkpoint = match run_boundary(
                    &ctx,
                    payload.deadline_at,
                    self.environment
                        .checkpoint(&handle, policy, &progress.fence),
                )
                .await
                {
                    Ok(checkpoint) => checkpoint,
                    Err(failure) => return Ok(boundary_failure(job, failure)),
                };
                progress.checkpoint = Some(checkpoint);
                Self::defer(
                    &ctx,
                    &mut progress,
                    WorkEnvironmentWorkflowPhase::Checkpointed,
                    "work environment checkpoint persisted",
                )
                .await
            }
            WorkEnvironmentWorkflowPhase::Checkpointed => {
                if payload.require_successful_exit
                    && progress
                        .execution_result
                        .as_ref()
                        .is_none_or(|result| result.exit_code != Some(0))
                {
                    return self
                        .terminal_execution_failure(job, &payload, &ctx, &mut progress)
                        .await;
                }
                if spec.publication.is_some() {
                    let checkpoint = progress.checkpoint.as_ref().ok_or_else(|| {
                        StasisError::PortFailure(
                            "checkpoint phase is missing its checkpoint".to_string(),
                        )
                    })?;
                    let publication = match run_boundary(
                        &ctx,
                        payload.deadline_at,
                        self.environment
                            .publish(&handle, checkpoint, &progress.fence),
                    )
                    .await
                    {
                        Ok(publication) => publication,
                        Err(failure) => return Ok(boundary_failure(job, failure)),
                    };
                    progress.publication = Some(publication);
                }
                Self::defer(
                    &ctx,
                    &mut progress,
                    WorkEnvironmentWorkflowPhase::Published,
                    "work environment result published",
                )
                .await
            }
            WorkEnvironmentWorkflowPhase::Published => {
                self.terminal_success(job, &payload, &ctx, &mut progress)
                    .await
            }
            WorkEnvironmentWorkflowPhase::CleanupEnqueued => unreachable!(),
        }
    }

    async fn on_lifecycle(&self, job: &Job, event: &JobLifecycleEvent) -> StasisResult<()> {
        if !matches!(
            event,
            JobLifecycleEvent::Succeeded
                | JobLifecycleEvent::Canceled { .. }
                | JobLifecycleEvent::DeadLettered { .. }
        ) {
            return Ok(());
        }
        let payload = Self::parse(job)?;
        let attempt = match event {
            JobLifecycleEvent::DeadLettered { .. } => job.attempts.max(1),
            JobLifecycleEvent::Succeeded | JobLifecycleEvent::Canceled { .. } => {
                job.attempts.saturating_add(1).max(1)
            }
            _ => unreachable!("filtered terminal lifecycle event"),
        };
        let progress = Self::progress(job, &payload, attempt)
            .unwrap_or_else(|_| WorkEnvironmentJobProgress::new(&payload.spec, attempt));
        self.enqueue_cleanup(job, &payload, &progress).await?;
        self.enqueue_federated_terminal(job, &payload).await?;
        Ok(())
    }
}

#[async_trait]
impl JobHandler for WorkEnvironmentCleanupJobHandler {
    fn job_type(&self) -> &'static str {
        WORK_ENVIRONMENT_CLEANUP_JOB_TYPE
    }

    async fn execute(&self, _job: &Job) -> StasisResult<JobExecutionOutcome> {
        Err(StasisError::PortFailure(
            "work-environment cleanup requires Stasis JobContext".to_string(),
        ))
    }

    async fn execute_with_context(
        &self,
        job: &Job,
        ctx: JobContext,
    ) -> StasisResult<JobExecutionOutcome> {
        let payload: WorkEnvironmentCleanupPayload = match serde_json::from_str(&job.payload_ref) {
            Ok(payload) => payload,
            Err(error) => {
                return Ok(fatal(
                    job,
                    format!("invalid cleanup payload: {error}"),
                    None,
                ));
            }
        };
        if payload.schema_version != WORK_ENVIRONMENT_CLEANUP_SCHEMA_VERSION {
            return Ok(fatal(
                job,
                "unsupported work-environment cleanup schema".to_string(),
                None,
            ));
        }
        let retention = effective_retention(payload.retention);
        match run_boundary(
            &ctx,
            None,
            self.environment
                .cleanup(&payload.environment_id, retention, &payload.fence),
        )
        .await
        {
            Ok(state) => Ok(JobExecutionOutcome::Success {
                output_provenance: payload
                    .output_checkpoint
                    .map(|checkpoint| checkpoint.provenance),
                execution_id: Some(job.id.clone()),
                diagnostics: Some(
                    json!({
                        "provider": "medousa-work-environment-cleanup",
                        "environment_id": payload.environment_id,
                        "phase": state.phase,
                    })
                    .to_string(),
                ),
            }),
            Err(failure) => Ok(JobExecutionOutcome::RetryableFailure {
                message: failure.message(),
                execution_id: Some(job.id.clone()),
                diagnostics: Some(
                    json!({
                        "provider": "medousa-work-environment-cleanup",
                        "environment_id": payload.environment_id,
                    })
                    .to_string(),
                ),
            }),
        }
    }
}

#[async_trait]
impl JobHandler for WorkEnvironmentTerminalDeliveryJobHandler {
    fn job_type(&self) -> &'static str {
        WORK_ENVIRONMENT_TERMINAL_DELIVERY_JOB_TYPE
    }

    async fn execute(&self, _job: &Job) -> StasisResult<JobExecutionOutcome> {
        Err(StasisError::PortFailure(
            "federated terminal delivery requires Stasis JobContext".to_string(),
        ))
    }

    async fn execute_with_context(
        &self,
        job: &Job,
        ctx: JobContext,
    ) -> StasisResult<JobExecutionOutcome> {
        let payload: WorkEnvironmentTerminalDeliveryPayload =
            match serde_json::from_str(&job.payload_ref) {
                Ok(payload) => payload,
                Err(error) => {
                    return Ok(fatal(
                        job,
                        format!("invalid federated terminal payload: {error}"),
                        None,
                    ));
                }
            };
        let Some(parent) = self.jobs.get(&payload.parent_job_id).await? else {
            return Ok(JobExecutionOutcome::RetryableFailure {
                message: "federated parent job is not visible yet".to_string(),
                execution_id: Some(job.id.clone()),
                diagnostics: None,
            });
        };
        if !matches!(
            parent.state,
            JobState::Succeeded | JobState::Failed | JobState::DeadLetter | JobState::Canceled
        ) {
            return Ok(JobExecutionOutcome::RetryableFailure {
                message: "federated parent job is not terminal yet".to_string(),
                execution_id: Some(job.id.clone()),
                diagnostics: None,
            });
        }
        if ctx.is_cancelled() {
            return Ok(fatal(
                job,
                "federated terminal delivery was canceled".to_string(),
                None,
            ));
        }
        ctx.heartbeat().await?;
        let result =
            match encode_remote_terminal_result(self.blobs.as_ref(), &parent, &payload.federation)
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    return Ok(JobExecutionOutcome::RetryableFailure {
                        message: error.to_string(),
                        execution_id: Some(job.id.clone()),
                        diagnostics: None,
                    });
                }
            };
        let output_provenance = result.output_provenance.clone();
        if let Err(error) = self.delivery.sign_and_deliver(result).await {
            return Ok(JobExecutionOutcome::RetryableFailure {
                message: error.to_string(),
                execution_id: Some(job.id.clone()),
                diagnostics: None,
            });
        }
        ctx.heartbeat().await?;
        Ok(JobExecutionOutcome::Success {
            output_provenance,
            execution_id: Some(job.id.clone()),
            diagnostics: Some(
                json!({
                    "provider": "medousa-work-environment-federation",
                    "envelope_id": payload.federation.envelope_id,
                    "parent_job_id": payload.parent_job_id,
                })
                .to_string(),
            ),
        })
    }
}

enum BoundaryFailure {
    Port(WorkEnvironmentError),
    Canceled,
    Deadline,
    Lease(String),
}

impl BoundaryFailure {
    fn message(&self) -> String {
        match self {
            Self::Port(error) => error.to_string(),
            Self::Canceled => "work-environment job was canceled".to_string(),
            Self::Deadline => "work-environment job deadline elapsed".to_string(),
            Self::Lease(message) => format!("work-environment lease heartbeat failed: {message}"),
        }
    }
}

async fn run_boundary<T, F>(
    ctx: &JobContext,
    deadline: Option<DateTime<Utc>>,
    operation: F,
) -> Result<T, BoundaryFailure>
where
    F: Future<Output = Result<T, WorkEnvironmentError>>,
{
    if ctx.is_cancelled() {
        return Err(BoundaryFailure::Canceled);
    }
    if deadline.is_some_and(|deadline| deadline <= Utc::now()) {
        return Err(BoundaryFailure::Deadline);
    }
    ctx.heartbeat()
        .await
        .map_err(|error| BoundaryFailure::Lease(error.to_string()))?;
    let mut cancellation = ctx.cancellation.clone();
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.tick().await;
    tokio::pin!(operation);
    loop {
        tokio::select! {
            result = &mut operation => return result.map_err(BoundaryFailure::Port),
            changed = cancellation.changed() => {
                if changed.is_err() || *cancellation.borrow() {
                    return Err(BoundaryFailure::Canceled);
                }
            }
            _ = heartbeat.tick() => {
                if deadline.is_some_and(|deadline| deadline <= Utc::now()) {
                    return Err(BoundaryFailure::Deadline);
                }
                ctx.heartbeat()
                    .await
                    .map_err(|error| BoundaryFailure::Lease(error.to_string()))?;
            }
        }
    }
}

fn fence_for_attempt(spec: &WorkEnvironmentSpec, attempt: u32) -> WorkEnvironmentFence {
    WorkEnvironmentFence {
        stasis_attempt: FencingToken(u64::from(attempt.max(1))),
        forge_environment_generation: spec.fence.forge_environment_generation,
        forge_execution_generation: spec.fence.forge_execution_generation,
    }
}

fn effective_retention(retention: WorkEnvironmentRetention) -> WorkEnvironmentRetention {
    match retention {
        WorkEnvironmentRetention::RetainWarmUntil(until)
        | WorkEnvironmentRetention::PreserveForDebugUntil(until)
            if until <= Utc::now() =>
        {
            WorkEnvironmentRetention::Delete
        }
        other => other,
    }
}

fn boundary_failure(job: &Job, failure: BoundaryFailure) -> JobExecutionOutcome {
    let message = failure.message();
    match failure {
        BoundaryFailure::Port(
            WorkEnvironmentError::InvalidSpec(_)
            | WorkEnvironmentError::AdmissionDenied(_)
            | WorkEnvironmentError::ImageUnavailable(_)
            | WorkEnvironmentError::Unsupported(_),
        )
        | BoundaryFailure::Canceled
        | BoundaryFailure::Deadline => fatal(job, message, None),
        BoundaryFailure::Port(_) | BoundaryFailure::Lease(_) => {
            JobExecutionOutcome::RetryableFailure {
                message,
                execution_id: Some(job.id.clone()),
                diagnostics: Some(
                    json!({
                        "provider": "medousa-work-environment",
                        "status": "retryable_failure",
                    })
                    .to_string(),
                ),
            }
        }
    }
}

fn fatal(job: &Job, message: String, diagnostics: Option<String>) -> JobExecutionOutcome {
    JobExecutionOutcome::FatalFailure {
        message,
        execution_id: Some(job.id.clone()),
        diagnostics,
    }
}

fn terminal_diagnostics(progress: &WorkEnvironmentJobProgress, cleanup_job_id: &str) -> String {
    json!({
        "provider": "medousa-work-environment",
        "phase": progress.phase,
        "attempt": progress.attempt,
        "execution_id": progress.execution_result.as_ref().map(|result| result.execution_id.as_str()),
        "exit_code": progress.execution_result.as_ref().and_then(|result| result.exit_code),
        "checkpoint": progress.checkpoint.as_ref().map(|checkpoint| checkpoint.provenance.compact()),
        "publication": progress.publication,
        "cleanup_job_id": cleanup_job_id,
    })
    .to_string()
}

fn map_port(error: WorkEnvironmentError) -> StasisError {
    StasisError::PortFailure(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Duration as ChronoDuration;
    use medousa_runtime::{
        InMemoryWorkEnvironmentPort, WorkEnvironmentImage, WorkEnvironmentNetworkPolicy,
        WorkEnvironmentPublication, WorkEnvironmentRepository, WorkEnvironmentRequirements,
        WorkEnvironmentRetention, WorkspaceId,
    };
    use stasis::application::runtime::in_memory_runtime::InMemoryRuntime;
    use stasis::domain::runtime::job::JobState;
    use stasis::domain::runtime::placement::WorkerCapabilities;
    use stasis::domain::runtime::provenance::ContentDigest;
    use stasis::ports::outbound::runtime::job_store::JobStore;

    use super::*;

    fn payload(environment_id: &str) -> WorkEnvironmentJobPayload {
        WorkEnvironmentJobPayload {
            spec: WorkEnvironmentSpec {
                environment_id: medousa_runtime::WorkEnvironmentId::parse(environment_id).unwrap(),
                workspace_id: WorkspaceId::parse("durable-workflow").unwrap(),
                repository: WorkEnvironmentRepository {
                    repository_id: "durable-workflow".to_string(),
                    authorized_origin: "https://example.invalid/repository.git".to_string(),
                },
                base_commit: "a".repeat(40),
                image: WorkEnvironmentImage {
                    reference: "example.invalid/medousa/dev".to_string(),
                    digest: ContentDigest::sha256_bytes(b"phase-5-image"),
                    platform: "linux/amd64".to_string(),
                },
                checkpoint_ref: None,
                requirements: WorkEnvironmentRequirements::default(),
                mounts: Vec::new(),
                network_policy: WorkEnvironmentNetworkPolicy::Deny,
                secret_refs: Vec::new(),
                fence: WorkEnvironmentFence {
                    stasis_attempt: FencingToken(1),
                    forge_environment_generation: Some(3),
                    forge_execution_generation: Some(7),
                },
                publication: Some(WorkEnvironmentPublication {
                    target_ref: format!("results/{environment_id}"),
                    expected_value: None,
                }),
                retention: WorkEnvironmentRetention::Delete,
            },
            execution: WorkEnvironmentExecRequest {
                idempotency_key: "replaced-by-handler".to_string(),
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "printf phase-5".to_string()],
                working_directory: Some("/workspace".to_string()),
                environment: BTreeMap::new(),
                stdin: None,
                timeout_seconds: 30,
                max_output_bytes: 64 * 1024,
            },
            checkpoint: WorkEnvironmentCheckpointPolicy::default(),
            require_successful_exit: true,
            deadline_at: Some(Utc::now() + ChronoDuration::minutes(5)),
            display_name: Some("Phase 5 proof".to_string()),
            federation: None,
        }
    }

    fn capabilities() -> WorkerCapabilities {
        WorkerCapabilities::any().with_capability(medousa_runtime::OCI_WORK_ENVIRONMENT_CAPABILITY)
    }

    async fn process_until_terminal(runtime: &InMemoryRuntime, job_id: &str) -> Job {
        for _ in 0..30 {
            if let Some(job) = runtime.job_store.get(job_id).await.unwrap()
                && matches!(
                    job.state,
                    JobState::Succeeded
                        | JobState::Failed
                        | JobState::DeadLetter
                        | JobState::Canceled
                )
            {
                return job;
            }
            runtime
                .process_once_with_capabilities(
                    "default",
                    "phase-5-worker",
                    Utc::now(),
                    &capabilities(),
                )
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("job {job_id} did not become terminal");
    }

    async fn process_until_phase(
        runtime: &InMemoryRuntime,
        job_id: &str,
        phase: WorkEnvironmentWorkflowPhase,
    ) -> Job {
        let mut last = None;
        for _ in 0..30 {
            if let Some(job) = runtime.job_store.get(job_id).await.unwrap()
                && job
                    .progress_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<WorkEnvironmentJobProgress>(raw).ok())
                    .is_some_and(|progress| progress.phase == phase)
            {
                return job;
            }
            last = runtime.job_store.get(job_id).await.unwrap();
            runtime
                .process_once_with_capabilities(
                    "default",
                    "phase-5-worker",
                    Utc::now(),
                    &capabilities(),
                )
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("job {job_id} did not reach phase {phase:?}; last={last:?}");
    }

    async fn expire_and_rewind(
        runtime: &InMemoryRuntime,
        job_id: &str,
        phase: WorkEnvironmentWorkflowPhase,
    ) {
        let mut job = runtime.job_store.get(job_id).await.unwrap().unwrap();
        let mut progress: WorkEnvironmentJobProgress =
            serde_json::from_str(job.progress_json.as_deref().unwrap()).unwrap();
        progress.phase = phase;
        job.progress_json = Some(serde_json::to_string(&progress).unwrap());
        job.state = JobState::Running;
        job.lease_owner = Some("dead-worker".to_string());
        job.lease_expires_at = Some(Utc::now() - ChronoDuration::seconds(1));
        runtime.job_store.save(job).await.unwrap();
    }

    #[tokio::test]
    async fn workflow_advances_durable_boundaries_and_cleans_up_independently() {
        let runtime = InMemoryRuntime::new();
        let environment = Arc::new(InMemoryWorkEnvironmentPort::new());
        register_work_environment_job_handlers(
            &RuntimeComposition::InMemory(runtime.clone()),
            environment,
        )
        .await
        .unwrap();
        let job = payload("phase-5-complete")
            .into_job("phase-5-job", "default", "test", Utc::now())
            .unwrap();
        runtime.enqueue(job).await.unwrap();

        assert!(
            runtime
                .process_once_with_capabilities(
                    "default",
                    "incapable-worker",
                    Utc::now(),
                    &WorkerCapabilities::any(),
                )
                .await
                .unwrap()
                .is_none(),
            "a worker without the OCI capability must not lease the job"
        );

        let completed = process_until_terminal(&runtime, "phase-5-job").await;
        assert_eq!(completed.state, JobState::Succeeded);
        assert!(completed.output_provenance.is_some());
        let progress: WorkEnvironmentJobProgress =
            serde_json::from_str(completed.progress_json.as_deref().unwrap()).unwrap();
        assert_eq!(
            progress.phase,
            WorkEnvironmentWorkflowPhase::CleanupEnqueued
        );
        assert_eq!(progress.attempt, 1);
        assert!(progress.execution_result.is_some());
        assert!(progress.checkpoint.is_some());
        assert!(matches!(
            progress.publication,
            Some(WorkEnvironmentPublicationResult::Published { .. })
        ));

        let cleanup_id = progress.cleanup_job_id.unwrap();
        assert_eq!(
            runtime
                .job_store
                .get(&cleanup_id)
                .await
                .unwrap()
                .expect("cleanup job")
                .state,
            JobState::Enqueued,
            "the parent must complete before independent cleanup runs"
        );
        let cleanup = process_until_terminal(&runtime, &cleanup_id).await;
        assert_eq!(cleanup.state, JobState::Succeeded);
        assert_eq!(cleanup.causation_id, completed.id);
    }

    #[tokio::test]
    async fn expired_lease_resumes_from_progress_with_a_new_fence() {
        let runtime = InMemoryRuntime::new();
        let environment = Arc::new(InMemoryWorkEnvironmentPort::new());
        register_work_environment_job_handlers(
            &RuntimeComposition::InMemory(runtime.clone()),
            environment,
        )
        .await
        .unwrap();
        let mut job = payload("phase-5-recovery")
            .into_job("phase-5-recovery-job", "default", "test", Utc::now())
            .unwrap();
        job.backoff_policy = BackoffPolicy {
            base_delay_seconds: 0,
            max_delay_seconds: 0,
        };
        runtime.enqueue(job).await.unwrap();
        runtime
            .process_once_with_capabilities("default", "first-worker", Utc::now(), &capabilities())
            .await
            .unwrap();

        let mut interrupted = runtime
            .job_store
            .get("phase-5-recovery-job")
            .await
            .unwrap()
            .unwrap();
        interrupted.state = JobState::Running;
        interrupted.lease_owner = Some("dead-worker".to_string());
        interrupted.lease_expires_at = Some(Utc::now() - ChronoDuration::seconds(1));
        runtime.job_store.save(interrupted).await.unwrap();

        let completed = process_until_terminal(&runtime, "phase-5-recovery-job").await;
        assert_eq!(completed.state, JobState::Succeeded);
        assert_eq!(completed.attempts, 1);
        let progress: WorkEnvironmentJobProgress =
            serde_json::from_str(completed.progress_json.as_deref().unwrap()).unwrap();
        assert_eq!(progress.attempt, 2);
        assert_eq!(progress.fence.stasis_attempt, FencingToken(2));
    }

    #[tokio::test]
    async fn publication_replay_after_lost_progress_keeps_one_result() {
        let runtime = InMemoryRuntime::new();
        let environment = Arc::new(InMemoryWorkEnvironmentPort::new());
        register_work_environment_job_handlers(
            &RuntimeComposition::InMemory(runtime.clone()),
            environment,
        )
        .await
        .unwrap();
        let mut job = payload("phase-5-publication-replay")
            .into_job(
                "phase-5-publication-replay-job",
                "default",
                "test",
                Utc::now(),
            )
            .unwrap();
        job.backoff_policy = BackoffPolicy {
            base_delay_seconds: 0,
            max_delay_seconds: 0,
        };
        runtime.enqueue(job).await.unwrap();

        let published = process_until_phase(
            &runtime,
            "phase-5-publication-replay-job",
            WorkEnvironmentWorkflowPhase::Published,
        )
        .await;
        let before: WorkEnvironmentJobProgress =
            serde_json::from_str(published.progress_json.as_deref().unwrap()).unwrap();
        expire_and_rewind(
            &runtime,
            "phase-5-publication-replay-job",
            WorkEnvironmentWorkflowPhase::Checkpointed,
        )
        .await;

        let completed = process_until_terminal(&runtime, "phase-5-publication-replay-job").await;
        assert_eq!(completed.state, JobState::Succeeded);
        let after: WorkEnvironmentJobProgress =
            serde_json::from_str(completed.progress_json.as_deref().unwrap()).unwrap();
        assert_eq!(after.attempt, 2);
        assert_eq!(after.checkpoint, before.checkpoint);
        let (before_target, before_value) = match before.publication.as_ref().unwrap() {
            WorkEnvironmentPublicationResult::Published {
                target_ref, value, ..
            } => (target_ref, value),
            other => panic!("initial publication should publish once, got {other:?}"),
        };
        match after.publication.as_ref().unwrap() {
            WorkEnvironmentPublicationResult::AlreadyPublished { target_ref, value } => {
                assert_eq!(target_ref, before_target);
                assert_eq!(value, before_value);
            }
            other => panic!("publication replay should preserve the winner, got {other:?}"),
        }
        assert_eq!(
            completed.output_provenance,
            before
                .checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.provenance.clone())
        );
    }

    #[tokio::test]
    async fn cancellation_enqueues_cleanup_without_reviving_the_parent() {
        let runtime = InMemoryRuntime::new();
        let environment = Arc::new(InMemoryWorkEnvironmentPort::new());
        register_work_environment_job_handlers(
            &RuntimeComposition::InMemory(runtime.clone()),
            environment,
        )
        .await
        .unwrap();
        let job = payload("phase-5-cancel")
            .into_job("phase-5-cancel-job", "default", "test", Utc::now())
            .unwrap();
        runtime.enqueue(job).await.unwrap();
        runtime
            .process_once_with_capabilities(
                "default",
                "phase-5-worker",
                Utc::now(),
                &capabilities(),
            )
            .await
            .unwrap();

        assert!(runtime.cancel("phase-5-cancel-job").await.unwrap());
        let parent = runtime
            .job_store
            .get("phase-5-cancel-job")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(parent.state, JobState::Canceled);
        let cleanup = process_until_terminal(&runtime, "phase-5-cancel-job:cleanup").await;
        assert_eq!(cleanup.state, JobState::Succeeded);
        assert_eq!(
            runtime
                .job_store
                .get("phase-5-cancel-job")
                .await
                .unwrap()
                .unwrap()
                .state,
            JobState::Canceled
        );
    }

    #[tokio::test]
    async fn elapsed_deadline_dead_letters_once_and_still_enqueues_cleanup() {
        let runtime = InMemoryRuntime::new();
        let environment = Arc::new(InMemoryWorkEnvironmentPort::new());
        register_work_environment_job_handlers(
            &RuntimeComposition::InMemory(runtime.clone()),
            environment,
        )
        .await
        .unwrap();
        let mut payload = payload("phase-5-timeout");
        payload.deadline_at = Some(Utc::now() + ChronoDuration::milliseconds(20));
        let job = payload
            .into_job("phase-5-timeout-job", "default", "test", Utc::now())
            .unwrap();
        runtime.enqueue(job).await.unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;

        let parent = process_until_terminal(&runtime, "phase-5-timeout-job").await;
        assert_eq!(parent.state, JobState::DeadLetter);
        let cleanup = process_until_terminal(&runtime, "phase-5-timeout-job:cleanup").await;
        assert_eq!(cleanup.state, JobState::Succeeded);
        assert_eq!(
            runtime
                .job_store
                .get("phase-5-timeout-job")
                .await
                .unwrap()
                .unwrap()
                .state,
            JobState::DeadLetter
        );
    }

    #[cfg(feature = "full-daemon")]
    #[tokio::test]
    #[ignore = "requires a running Docker engine and an explicitly selected local image"]
    async fn docker_workflow_survives_every_deferred_boundary_and_cleans_up() {
        use medousa_forge::execution::ForgeExecutionService;

        let image_reference = std::env::var("MEDOUSA_TEST_OCI_IMAGE")
            .expect("set MEDOUSA_TEST_OCI_IMAGE to a locally cached image repository");
        let image_digest = std::env::var("MEDOUSA_TEST_OCI_DIGEST")
            .expect("set MEDOUSA_TEST_OCI_DIGEST to its sha256 hex digest");
        let image_platform = std::env::var("MEDOUSA_TEST_OCI_PLATFORM")
            .expect("set MEDOUSA_TEST_OCI_PLATFORM to the image OS/architecture");
        let temp = tempfile::tempdir().unwrap();
        let test_root = std::fs::canonicalize(temp.path()).unwrap();
        let repository = test_root.join("origin");
        std::fs::create_dir_all(&repository).unwrap();
        run_git(&repository, &["init", "--quiet"]);
        std::fs::write(repository.join("README.md"), "phase 5 input\n").unwrap();
        run_git(&repository, &["add", "README.md"]);
        run_git(
            &repository,
            &[
                "-c",
                "user.name=Medousa Test",
                "-c",
                "user.email=test@medousa.local",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
        );
        let base_commit = git_output(&repository, &["rev-parse", "HEAD"]);
        let environment_id = format!("phase-5-live-{}", uuid::Uuid::new_v4().simple());
        let mut payload = payload(&environment_id);
        payload.spec.repository.authorized_origin = repository.display().to_string();
        payload.spec.base_commit = base_commit;
        payload.spec.image.reference = image_reference;
        payload.spec.image.digest = ContentDigest {
            algorithm: ContentDigest::SHA256.to_string(),
            hex: image_digest,
        };
        payload.spec.image.platform = image_platform;
        payload.execution.args = vec![
            "-c".to_string(),
            "test ! -e run-once || exit 73; touch run-once; printf 'phase 5 durable output' > generated.txt".to_string(),
        ];
        payload.checkpoint.include_untracked = true;

        let adapter_root = test_root.join("work-environments");
        let adapter = crate::daemon::work_environment_host::DockerCliWorkEnvironmentPort::detect(
            adapter_root.clone(),
            Arc::new(ForgeExecutionService::new()),
        )
        .await
        .unwrap()
        .expect("Docker adapter should be available");
        let runtime = InMemoryRuntime::new();
        register_work_environment_job_handlers(
            &RuntimeComposition::InMemory(runtime.clone()),
            adapter,
        )
        .await
        .unwrap();
        let mut job = payload
            .into_job("phase-5-live-job", "default", "test", Utc::now())
            .unwrap();
        job.backoff_policy = BackoffPolicy {
            base_delay_seconds: 0,
            max_delay_seconds: 0,
        };
        runtime.enqueue(job).await.unwrap();

        process_until_phase(
            &runtime,
            "phase-5-live-job",
            WorkEnvironmentWorkflowPhase::Executed,
        )
        .await;
        expire_and_rewind(
            &runtime,
            "phase-5-live-job",
            WorkEnvironmentWorkflowPhase::Started,
        )
        .await;
        process_until_phase(
            &runtime,
            "phase-5-live-job",
            WorkEnvironmentWorkflowPhase::Published,
        )
        .await;
        expire_and_rewind(
            &runtime,
            "phase-5-live-job",
            WorkEnvironmentWorkflowPhase::Checkpointed,
        )
        .await;

        let completed = process_until_terminal(&runtime, "phase-5-live-job").await;
        assert_eq!(completed.state, JobState::Succeeded);
        let progress: WorkEnvironmentJobProgress =
            serde_json::from_str(completed.progress_json.as_deref().unwrap()).unwrap();
        assert_eq!(progress.attempt, 3);
        let cleanup = process_until_terminal(
            &runtime,
            progress.cleanup_job_id.as_deref().expect("cleanup job id"),
        )
        .await;
        assert_eq!(cleanup.state, JobState::Succeeded);
        assert!(
            !adapter_root
                .join("environments")
                .join(environment_id)
                .exists(),
            "the independent cleanup job must remove the disposable environment"
        );
    }

    #[cfg(feature = "full-daemon")]
    fn run_git(cwd: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    #[cfg(feature = "full-daemon")]
    fn git_output(cwd: &std::path::Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }
}
