//! Origin-owned durable handoff and reconciliation for portable Coder work.
//!
//! Forge captures one immutable, policy-checked input. The existing remote
//! work-environment proxy transports and executes it. This coordinator then
//! consumes only the proxy's authenticated local terminal record and imports
//! the returned object graph behind create-only Forge refs. User branches,
//! indexes, and working trees are never updated by reconciliation.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use medousa_forge::error::ForgeError;
use medousa_forge::execution::{ExecutionClass, ForgeExecutionService, MAX_CAPTURE_BYTES};
use medousa_forge::forge::Forge;
use medousa_forge::model::{GitOid, PortableForgeCheckpoint, RepoId, WorkId, WorkTarget};
use medousa_runtime::{
    WorkEnvironmentCheckpoint, WorkEnvironmentCheckpointManifest, WorkEnvironmentCheckpointPolicy,
    WorkEnvironmentExecRequest, WorkEnvironmentFence, WorkEnvironmentId, WorkEnvironmentImage,
    WorkEnvironmentNetworkPolicy, WorkEnvironmentRepository, WorkEnvironmentRequirements,
    WorkEnvironmentRetention, WorkEnvironmentSpec, WorkspaceId,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::application::runtime::job_context::JobContext;
use stasis::domain::runtime::blob_descriptor::BlobDescriptor;
use stasis::domain::runtime::job::{BackoffPolicy, Job, JobState, NewJob};
use stasis::domain::runtime::placement::PlacementConstraints;
use stasis::domain::runtime::provenance::ProvenanceRef;
use stasis::domain::runtime::resource_lease::FencingToken;
use stasis::ports::outbound::runtime::blob_transfer::BlobTransferPort;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::portable_coder::{
    PORTABLE_CODER_TASK_SCHEMA_VERSION, PortableCoderResult, PortableCoderTask,
};
use crate::work_environment_federation::DurableBlobRetentionPort;
use crate::work_environment_job::{WorkEnvironmentJobPayload, WorkEnvironmentJobProgress};
use crate::work_environment_parallel::remote_work_environment_proxy_job;

pub const PORTABLE_CODER_HANDOFF_JOB_TYPE: &str = "workflow.medousa.portable_coder_handoff";
pub const PORTABLE_CODER_HANDOFF_SCHEMA_VERSION: u32 = 1;
pub const PORTABLE_FORGE_BUNDLE_MEDIA_TYPE: &str = "application/vnd.medousa.forge.git-bundle";
pub const PORTABLE_FORGE_CHECKPOINT_MEDIA_TYPE: &str =
    "application/vnd.medousa.forge.checkpoint+json";
const HANDOFF_PROGRESS_SCHEMA_VERSION: u32 = 1;
const HANDOFF_DELAY_MILLIS: i64 = 10;

#[derive(Clone, Debug)]
pub struct PortableCoderHandoffRequest {
    pub operation_id: String,
    pub work_id: String,
    pub parent_session_id: String,
    pub correlation_id: String,
    pub target_runtime_id: String,
    pub prompt: String,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub max_tool_rounds: usize,
    pub requested_tool_names: Vec<String>,
    pub image: WorkEnvironmentImage,
    pub requirements: WorkEnvironmentRequirements,
    pub network_policy: WorkEnvironmentNetworkPolicy,
    pub secret_refs: Vec<String>,
    pub forge_execution_generation: Option<u64>,
    pub retention: WorkEnvironmentRetention,
    pub requested_at: DateTime<Utc>,
    pub deadline_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortableCoderHandoffPlan {
    pub schema_version: u32,
    pub target_runtime_id: String,
    pub origin_checkpoint: PortableForgeCheckpoint,
    pub work: WorkEnvironmentJobPayload,
}

impl PortableCoderHandoffPlan {
    pub fn validate(&self, now: DateTime<Utc>) -> StasisResult<()> {
        if self.schema_version != PORTABLE_CODER_HANDOFF_SCHEMA_VERSION {
            return Err(invalid(format!(
                "unsupported portable Coder handoff schema_version={}",
                self.schema_version
            )));
        }
        validate_id("target_runtime_id", &self.target_runtime_id)?;
        validate_portable_checkpoint(&self.origin_checkpoint)?;
        self.work
            .validate(now)
            .map_err(|error| invalid(error.to_string()))?;
        if self.work.federation.is_some() {
            return Err(invalid(
                "portable Coder source plans cannot manufacture federation authority",
            ));
        }
        if !self.work.spec.mounts.is_empty() || self.work.spec.publication.is_some() {
            return Err(invalid(
                "portable Coder handoff cannot carry host mounts or destination publication",
            ));
        }
        if self.work.spec.requirements.placement.target_node.as_deref()
            != Some(self.target_runtime_id.as_str())
        {
            return Err(invalid(
                "portable Coder target does not match the work-environment placement",
            ));
        }
        let task = self
            .work
            .portable_coder
            .as_ref()
            .ok_or_else(|| invalid("portable Coder handoff has no typed Coder task"))?;
        if task.task_execution_grant.is_some() {
            return Err(invalid(
                "portable Coder source plans cannot carry a destination grant",
            ));
        }
        if task.work_id != self.origin_checkpoint.work_id.as_str()
            || task.project_id != self.origin_checkpoint.repository_id.as_str()
            || task.expected_base_oid != self.origin_checkpoint.expected_base_oid.as_str()
            || self.work.spec.repository.repository_id != task.project_id
            || self.work.spec.base_commit != task.expected_base_oid
            || self.work.spec.fence.forge_environment_generation
                != Some(u64::from(self.origin_checkpoint.environment_generation))
        {
            return Err(invalid(
                "portable Coder plan does not match its exact Forge identity and fences",
            ));
        }
        if !self
            .work
            .spec
            .repository
            .authorized_origin
            .starts_with("medousa://portable/")
            || task.root_ref.starts_with(['/', '\\'])
        {
            return Err(invalid(
                "portable Coder repository authority must be path-free",
            ));
        }
        let input = self
            .work
            .spec
            .checkpoint_ref
            .as_ref()
            .ok_or_else(|| invalid("portable Coder plan has no immutable input checkpoint"))?;
        input
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
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
            .map_err(|error| invalid(format!("encode portable Coder handoff: {error}")))?;
        Ok(NewJob {
            id: job_id.clone(),
            queue: queue.into(),
            job_type: PORTABLE_CODER_HANDOFF_JOB_TYPE.to_string(),
            payload_ref,
            priority: 100,
            max_attempts: 12,
            idempotency_key: format!("idem-{job_id}"),
            correlation_id: job_id.clone(),
            causation_id: causation_id.into(),
            trace_id: job_id,
            input_provenance: self.work.spec.checkpoint_ref.map(|value| value.provenance),
            placement: PlacementConstraints::unrestricted(),
            scheduled_at,
            backoff_policy: BackoffPolicy::default(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct PortableCoderRetryRequest {
    pub operation_id: String,
    pub correlation_id: String,
    pub target_runtime_id: String,
    pub prompt: String,
    pub requested_tool_names: Vec<String>,
    pub forge_execution_generation: Option<u64>,
    pub requested_at: DateTime<Utc>,
    pub deadline_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PortableCoderReconciliation {
    ReviewReady {
        work_id: String,
        operation_id: String,
        destination_runtime_id: String,
        input_checkpoint_oid: String,
        output_checkpoint_oid: String,
        remote_ref: String,
        evidence_digest: String,
        reconciled_at: DateTime<Utc>,
    },
    Conflict {
        work_id: String,
        operation_id: String,
        destination_runtime_id: String,
        expected_base_oid: String,
        found_base_oid: String,
        origin_checkpoint_oid: String,
        local_checkpoint_oid: String,
        remote_checkpoint_oid: String,
        local_ref: String,
        remote_ref: String,
        reconciled_at: DateTime<Utc>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortableCoderHandoffProgress {
    pub schema_version: u32,
    pub remote_job_id: String,
    pub remote_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_result_provenance: Option<ProvenanceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_result: Option<PortableCoderResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<WorkEnvironmentCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciliation: Option<PortableCoderReconciliation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub struct PortableCoderHandoffService {
    forge: Arc<Forge>,
    execution: Arc<ForgeExecutionService>,
    blobs: Arc<dyn BlobTransferPort>,
    retention: Option<Arc<dyn DurableBlobRetentionPort>>,
}

impl PortableCoderHandoffService {
    pub fn new(
        forge: Arc<Forge>,
        execution: Arc<ForgeExecutionService>,
        blobs: Arc<dyn BlobTransferPort>,
        retention: Option<Arc<dyn DurableBlobRetentionPort>>,
    ) -> Self {
        Self {
            forge,
            execution,
            blobs,
            retention,
        }
    }

    /// Capture and persist a complete source checkpoint before any remote
    /// envelope can be built. Secret/path policy failures therefore happen
    /// before transport.
    pub async fn prepare(
        &self,
        request: PortableCoderHandoffRequest,
    ) -> StasisResult<PortableCoderHandoffPlan> {
        let work_id = WorkId::parse_storage(&request.work_id)
            .map_err(|reason| invalid(format!("invalid Forge work id: {reason}")))?;
        validate_id("operation_id", &request.operation_id)?;
        validate_id("target_runtime_id", &request.target_runtime_id)?;
        let capture = self.capture(&work_id, &request.operation_id).await?;
        let bundle = self
            .blobs
            .put(&capture.bundle, Some(PORTABLE_FORGE_BUNDLE_MEDIA_TYPE))
            .await?;
        if bundle.digest.algorithm != "sha256"
            || bundle.digest.hex != capture.checkpoint.bundle_digest.as_str()
            || bundle.size_bytes != capture.checkpoint.bundle_bytes
        {
            return Err(invalid(
                "portable Forge bundle changed identity while entering durable storage",
            ));
        }

        let suffix = stable_suffix(&request.operation_id);
        let environment_id = WorkEnvironmentId::parse(format!("portable-env-{suffix}"))
            .map_err(|error| invalid(error.to_string()))?;
        let workspace_id = WorkspaceId::parse(format!("portable-workspace-{suffix}"))
            .map_err(|error| invalid(error.to_string()))?;
        let fence = WorkEnvironmentFence {
            stasis_attempt: FencingToken(1),
            forge_environment_generation: Some(u64::from(
                capture.checkpoint.environment_generation,
            )),
            forge_execution_generation: request.forge_execution_generation,
        };
        let input_manifest = WorkEnvironmentCheckpointManifest {
            schema_version: medousa_runtime::WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION,
            environment_id: environment_id.clone(),
            workspace_id: workspace_id.clone(),
            base_commit: capture.checkpoint.expected_base_oid.as_str().to_string(),
            checkpoint_commit: capture.checkpoint.checkpoint_oid.as_str().to_string(),
            source_bundle: bundle.clone(),
            artifacts: Vec::new(),
            fence: fence.clone(),
            label: Some(format!("portable-coder-input:{}", request.operation_id)),
            created_at: capture.checkpoint.created_at,
        };
        input_manifest
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        let manifest_bytes = serde_json::to_vec(&input_manifest)
            .map_err(|error| invalid(format!("encode portable Forge checkpoint: {error}")))?;
        let manifest = self
            .blobs
            .put(&manifest_bytes, Some(PORTABLE_FORGE_CHECKPOINT_MEDIA_TYPE))
            .await?;
        let input_checkpoint = WorkEnvironmentCheckpoint::from_manifest(manifest.clone());

        let mut requirements = request.requirements;
        requirements.placement.target_node = Some(request.target_runtime_id.clone());
        let repository_id = capture.checkpoint.repository_id.as_str().to_string();
        let task = PortableCoderTask {
            schema_version: PORTABLE_CODER_TASK_SCHEMA_VERSION,
            operation_id: request.operation_id.clone(),
            work_id: request.work_id,
            parent_session_id: request.parent_session_id,
            correlation_id: request.correlation_id,
            project_id: repository_id.clone(),
            root_ref: format!("forge:{repository_id}:{}", capture.checkpoint.work_id),
            expected_base_oid: capture.checkpoint.expected_base_oid.as_str().to_string(),
            expected_checkpoint_oid: capture.checkpoint.checkpoint_oid.as_str().to_string(),
            prompt: request.prompt,
            provider: request.provider,
            model: request.model,
            response_depth_mode: request.response_depth_mode,
            max_tool_rounds: request.max_tool_rounds,
            work_policy: capture.policy,
            requested_tool_names: request.requested_tool_names,
            requested_at: request.requested_at,
            deadline_at: request.deadline_at,
            task_execution_grant: None,
        };
        let work = WorkEnvironmentJobPayload {
            spec: WorkEnvironmentSpec {
                environment_id,
                workspace_id,
                repository: WorkEnvironmentRepository {
                    repository_id: repository_id.clone(),
                    authorized_origin: format!("medousa://portable/{repository_id}"),
                },
                base_commit: capture.checkpoint.expected_base_oid.as_str().to_string(),
                image: request.image,
                checkpoint_ref: Some(input_checkpoint),
                requirements,
                mounts: Vec::new(),
                network_policy: request.network_policy,
                secret_refs: request.secret_refs,
                fence,
                publication: None,
                retention: request.retention.clone(),
            },
            execution: WorkEnvironmentExecRequest {
                idempotency_key: format!("portable-coder-{suffix}"),
                program: "true".to_string(),
                args: Vec::new(),
                working_directory: Some("/workspace".to_string()),
                environment: BTreeMap::new(),
                stdin: None,
                timeout_seconds: 60,
                max_output_bytes: 64 * 1024,
            },
            portable_coder: Some(task),
            checkpoint: WorkEnvironmentCheckpointPolicy {
                idempotency_key: Some(format!("portable-checkpoint-{suffix}")),
                include_untracked: true,
                label: Some(format!("portable-coder-output:{}", request.operation_id)),
                retain_until: retention_deadline(&request.retention),
                ..WorkEnvironmentCheckpointPolicy::default()
            },
            require_successful_exit: true,
            deadline_at: Some(request.deadline_at),
            display_name: Some("Portable Coder".to_string()),
            federation: None,
        };
        let plan = PortableCoderHandoffPlan {
            schema_version: PORTABLE_CODER_HANDOFF_SCHEMA_VERSION,
            target_runtime_id: request.target_runtime_id,
            origin_checkpoint: capture.checkpoint,
            work,
        };
        plan.validate(request.requested_at)?;
        self.pin_input(&plan, vec![manifest, bundle]).await?;
        Ok(plan)
    }

    /// Build a new replay-stable handoff from an acknowledged checkpoint. No
    /// destination-local environment or container state is consulted.
    pub async fn retry_from_checkpoint(
        &self,
        prior: &PortableCoderHandoffPlan,
        checkpoint: WorkEnvironmentCheckpoint,
        request: PortableCoderRetryRequest,
    ) -> StasisResult<PortableCoderHandoffPlan> {
        prior.validate(prior_task(prior)?.requested_at)?;
        validate_id("operation_id", &request.operation_id)?;
        validate_id("target_runtime_id", &request.target_runtime_id)?;
        let graph = checkpoint_graph(self.blobs.as_ref(), &checkpoint).await?;
        let manifest_bytes = self.blobs.get(&checkpoint.manifest).await?;
        let manifest: WorkEnvironmentCheckpointManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| invalid(format!("decode retry checkpoint: {error}")))?;
        if manifest.base_commit != prior.origin_checkpoint.expected_base_oid.as_str() {
            return Err(invalid(
                "portable Coder retry checkpoint changed the exact source base",
            ));
        }

        let mut plan = prior.clone();
        let suffix = stable_suffix(&request.operation_id);
        plan.target_runtime_id = request.target_runtime_id.clone();
        plan.work.spec.environment_id =
            WorkEnvironmentId::parse(format!("portable-env-{suffix}"))
                .map_err(|error| invalid(error.to_string()))?;
        // The checkpoint manifest owns the logical workspace identity. A new
        // destination gets a fresh environment while reconstructing that same
        // immutable workspace.
        plan.work.spec.checkpoint_ref = Some(checkpoint);
        plan.work.spec.requirements.placement.target_node = Some(request.target_runtime_id);
        plan.work.spec.fence.stasis_attempt = FencingToken(1);
        plan.work.spec.fence.forge_execution_generation = request.forge_execution_generation;
        plan.work.execution.idempotency_key = format!("portable-coder-{suffix}");
        plan.work.checkpoint.idempotency_key = Some(format!("portable-checkpoint-{suffix}"));
        plan.work.deadline_at = Some(request.deadline_at);
        let task = plan
            .work
            .portable_coder
            .as_mut()
            .expect("validated portable Coder plan");
        task.operation_id = request.operation_id;
        task.correlation_id = request.correlation_id;
        task.expected_checkpoint_oid = manifest.checkpoint_commit;
        task.prompt = request.prompt;
        task.requested_tool_names = request.requested_tool_names;
        task.requested_at = request.requested_at;
        task.deadline_at = request.deadline_at;
        task.task_execution_grant = None;
        plan.validate(request.requested_at)?;
        self.pin_input(&plan, graph).await?;
        Ok(plan)
    }

    pub async fn reconcile(
        &self,
        plan: &PortableCoderHandoffPlan,
        result: &PortableCoderResult,
        checkpoint: &WorkEnvironmentCheckpoint,
        signed_result_provenance: &ProvenanceRef,
    ) -> StasisResult<PortableCoderReconciliation> {
        plan.validate(prior_task(plan)?.requested_at)?;
        validate_returned_result(plan, result, checkpoint, signed_result_provenance)?;
        let manifest_bytes = self.blobs.get(&checkpoint.manifest).await?;
        let manifest: WorkEnvironmentCheckpointManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| invalid(format!("decode returned Coder checkpoint: {error}")))?;
        manifest
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        validate_returned_manifest(plan, &manifest)?;
        let bundle_bytes = self.blobs.get(&manifest.source_bundle).await?;
        if !manifest.source_bundle.verify(&bundle_bytes) {
            return Err(invalid("returned Coder bundle failed descriptor verification"));
        }

        let forge = Arc::clone(&self.forge);
        let plan = plan.clone();
        let result = result.clone();
        let output_oid = GitOid::new(manifest.checkpoint_commit);
        let operation_id = result.operation_id.clone();
        let staging = self
            .forge
            .store()
            .root()
            .join("portable-reconciliation")
            .join(format!(
                "{}-{}.bundle",
                stable_suffix(plan.origin_checkpoint.work_id.as_str()),
                stable_suffix(&operation_id)
            ));
        write_reconciliation_bundle(&staging, &bundle_bytes)?;
        let staging_for_work = staging.clone();
        let outcome = self
            .execution
            .run_on_repo(
                ExecutionClass::WorkEnvironment,
                MAX_CAPTURE_BYTES,
                Some(plan.origin_checkpoint.repository_id.storage_key()),
                move || {
                    reconcile_in_forge(
                        forge.as_ref(),
                        &plan,
                        &result,
                        &output_oid,
                        &staging_for_work,
                    )
                },
            )
            .await;
        let _ = fs::remove_file(&staging);
        outcome.map_err(|error| invalid(error.to_string()))
    }

    async fn capture(&self, work_id: &WorkId, operation_id: &str) -> StasisResult<Capture> {
        let forge = Arc::clone(&self.forge);
        let work_id = work_id.clone();
        let path = self
            .forge
            .store()
            .root()
            .join("portable-capture")
            .join(format!(
                "{}-{}.bundle",
                stable_suffix(work_id.as_str()),
                stable_suffix(operation_id)
            ));
        self.execution
            .run_on_repo(
                ExecutionClass::WorkEnvironment,
                MAX_CAPTURE_BYTES,
                Some(work_id.storage_key()),
                move || {
                    let result = (|| {
                        if let Some(parent) = path.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        let item = forge.load(&work_id)?;
                        let policy = item.policy.clone();
                        let checkpoint = forge.export_portable_checkpoint(&work_id, &path)?;
                        let size = fs::metadata(&path)?.len();
                        if size > MAX_CAPTURE_BYTES as u64 {
                            return Err(ForgeError::CaptureBlocked(format!(
                                "portable checkpoint bundle exceeds {MAX_CAPTURE_BYTES} bytes"
                            )));
                        }
                        let bundle = fs::read(&path)?;
                        Ok(Capture {
                            checkpoint,
                            policy,
                            bundle,
                        })
                    })();
                    let _ = fs::remove_file(&path);
                    result
                },
            )
            .await
            .map_err(|error| invalid(error.to_string()))
    }

    async fn pin_input(
        &self,
        plan: &PortableCoderHandoffPlan,
        graph: Vec<BlobDescriptor>,
    ) -> StasisResult<()> {
        if let Some(retention) = self.retention.as_ref() {
            retention
                .pin_root(
                    &format!(
                        "portable-coder-input:{}",
                        prior_task(plan)?.operation_id
                    ),
                    graph,
                    retention_deadline(&plan.work.spec.retention),
                )
                .await?;
        }
        Ok(())
    }
}

struct Capture {
    checkpoint: PortableForgeCheckpoint,
    policy: medousa_forge::model::WorkPolicy,
    bundle: Vec<u8>,
}

struct PortableCoderHandoffJobHandler {
    jobs: Arc<dyn JobStore>,
    service: Arc<PortableCoderHandoffService>,
}

pub async fn register_portable_coder_handoff_job_handler(
    composition: &RuntimeComposition,
    forge: Arc<Forge>,
    execution: Arc<ForgeExecutionService>,
    blobs: Arc<dyn BlobTransferPort>,
) -> anyhow::Result<()> {
    let service = Arc::new(PortableCoderHandoffService::new(
        forge, execution, blobs, None,
    ));
    match composition {
        RuntimeComposition::InMemory(runtime) => {
            runtime.register_handler(PortableCoderHandoffJobHandler {
                jobs: Arc::new(runtime.job_store.clone()),
                service,
            })?;
        }
        RuntimeComposition::Surreal(runtime) => {
            runtime.register_handler(PortableCoderHandoffJobHandler {
                jobs: Arc::new(runtime.job_store.clone()),
                service,
            })?;
        }
    }
    Ok(())
}

#[async_trait]
impl JobHandler for PortableCoderHandoffJobHandler {
    fn job_type(&self) -> &'static str {
        PORTABLE_CODER_HANDOFF_JOB_TYPE
    }

    async fn execute(&self, _job: &Job) -> StasisResult<JobExecutionOutcome> {
        Err(invalid("portable Coder handoffs require Stasis JobContext"))
    }

    async fn execute_with_context(
        &self,
        job: &Job,
        ctx: JobContext,
    ) -> StasisResult<JobExecutionOutcome> {
        let plan: PortableCoderHandoffPlan = serde_json::from_str(&job.payload_ref)
            .map_err(|error| invalid(format!("decode portable Coder handoff: {error}")))?;
        if let Err(error) = plan.validate(prior_task(&plan)?.requested_at) {
            return Ok(fatal(job, error.to_string(), None));
        }
        let proxy_id = format!("{}:remote", job.id);
        let candidate = remote_work_environment_proxy_job(
            plan.work.clone(),
            &plan.target_runtime_id,
            &proxy_id,
            &job.queue,
            &job.id,
            &job.correlation_id,
            &job.trace_id,
            job.started_at.unwrap_or(job.scheduled_at),
        )?;
        ensure_job(&self.jobs, candidate).await?;
        let remote = self
            .jobs
            .get(&proxy_id)
            .await?
            .ok_or_else(|| invalid("portable Coder remote proxy disappeared"))?;
        let remote_progress = decode_work_progress(&remote)?;
        let mut progress = PortableCoderHandoffProgress {
            schema_version: HANDOFF_PROGRESS_SCHEMA_VERSION,
            remote_job_id: proxy_id,
            remote_state: state_name(&remote.state),
            signed_result_provenance: remote.output_provenance.clone(),
            remote_result: remote_progress
                .as_ref()
                .and_then(|value| value.portable_coder_result.clone()),
            checkpoint: remote_progress
                .as_ref()
                .and_then(|value| value.checkpoint.clone()),
            reconciliation: None,
            error_message: remote.last_error.clone(),
            updated_at: Utc::now(),
        };
        if remote.state == JobState::Succeeded {
            let result = progress
                .remote_result
                .as_ref()
                .ok_or_else(|| invalid("successful portable Coder proxy has no typed result"))?;
            let checkpoint = progress
                .checkpoint
                .as_ref()
                .ok_or_else(|| invalid("successful portable Coder proxy has no durable checkpoint"))?;
            let signed = progress.signed_result_provenance.as_ref().ok_or_else(|| {
                invalid("successful portable Coder proxy has no signed result provenance")
            })?;
            match self.service.reconcile(&plan, result, checkpoint, signed).await {
                Ok(reconciliation) => progress.reconciliation = Some(reconciliation),
                Err(error) => {
                    progress.error_message = Some(error.to_string());
                    progress.updated_at = Utc::now();
                    ctx.progress(&progress).await?;
                    return Ok(fatal(
                        job,
                        error.to_string(),
                        Some(json!({ "portable_coder": progress }).to_string()),
                    ));
                }
            }
            progress.updated_at = Utc::now();
            ctx.progress(&progress).await?;
            return Ok(JobExecutionOutcome::Success {
                output_provenance: remote.output_provenance,
                execution_id: Some(job.id.clone()),
                diagnostics: Some(json!({ "portable_coder": progress }).to_string()),
            });
        }
        ctx.progress(&progress).await?;
        if is_terminal(&remote.state) {
            return Ok(fatal(
                job,
                remote.last_error.unwrap_or_else(|| {
                    format!("portable Coder proxy ended as {}", state_name(&remote.state))
                }),
                Some(json!({ "portable_coder": progress }).to_string()),
            ));
        }
        Ok(deferred(job, "waiting for portable Coder destination"))
    }
}

fn reconcile_in_forge(
    forge: &Forge,
    plan: &PortableCoderHandoffPlan,
    result: &PortableCoderResult,
    output_oid: &GitOid,
    remote_bundle: &Path,
) -> Result<PortableCoderReconciliation, ForgeError> {
    let work_id = WorkId::parse_storage(&result.work_id)
        .map_err(|reason| ForgeError::Store(format!("invalid work id: {reason}")))?;
    let item = forge.load(&work_id)?;
    let WorkTarget::Git(target) = &item.target;
    let identity = forge.git().repo_identity(&target.repo_path)?;
    let portable_repository_id = RepoId::from(identity.repo_id.storage_key());
    if portable_repository_id != plan.origin_checkpoint.repository_id
        || target.base_ref != plan.origin_checkpoint.base_ref
        || target.base_oid != plan.origin_checkpoint.expected_base_oid
    {
        return Err(ForgeError::EnvironmentDrift(
            "origin Forge target no longer matches the portable checkpoint".to_string(),
        ));
    }

    forge
        .git()
        .import_checkpoint_objects(&target.repo_path, remote_bundle, output_oid)?;
    let input_oid = GitOid::new(result.input_checkpoint_oid.clone());
    if !forge
        .git()
        .is_ancestor(&target.repo_path, &input_oid, output_oid)?
    {
        return Err(ForgeError::EnvironmentDrift(
            "returned portable Coder checkpoint is not descended from its exact input".to_string(),
        ));
    }

    let refs = reconciliation_refs(&work_id, &result.operation_id);
    ensure_create_only_ref(forge, &target.repo_path, &refs.remote, output_oid)?;

    let local_bundle = forge
        .store()
        .root()
        .join("portable-reconciliation")
        .join(format!(
            "{}-{}-origin.bundle",
            stable_suffix(work_id.as_str()),
            stable_suffix(&result.operation_id)
        ));
    let local_checkpoint = forge.export_portable_checkpoint(&work_id, &local_bundle);
    let _ = fs::remove_file(&local_bundle);
    let local_checkpoint = local_checkpoint?;
    let origin_tree = forge.git().tree_oid(
        &target.repo_path,
        &plan.origin_checkpoint.checkpoint_oid,
    )?;
    let local_tree = forge
        .git()
        .tree_oid(&target.repo_path, &local_checkpoint.checkpoint_oid)?;
    let found_base = forge.git().ref_oid(&target.repo_path, &target.base_ref)?;
    let unchanged =
        origin_tree == local_tree && found_base == plan.origin_checkpoint.expected_base_oid;
    if unchanged {
        return Ok(PortableCoderReconciliation::ReviewReady {
            work_id: result.work_id.clone(),
            operation_id: result.operation_id.clone(),
            destination_runtime_id: result.destination_runtime_id.clone(),
            input_checkpoint_oid: result.input_checkpoint_oid.clone(),
            output_checkpoint_oid: output_oid.as_str().to_string(),
            remote_ref: refs.remote,
            evidence_digest: result.evidence_digest.clone(),
            reconciled_at: Utc::now(),
        });
    }

    ensure_create_only_ref(
        forge,
        &target.repo_path,
        &refs.local,
        &local_checkpoint.checkpoint_oid,
    )?;
    Ok(PortableCoderReconciliation::Conflict {
        work_id: result.work_id.clone(),
        operation_id: result.operation_id.clone(),
        destination_runtime_id: result.destination_runtime_id.clone(),
        expected_base_oid: plan.origin_checkpoint.expected_base_oid.as_str().to_string(),
        found_base_oid: found_base.as_str().to_string(),
        origin_checkpoint_oid: plan.origin_checkpoint.checkpoint_oid.as_str().to_string(),
        local_checkpoint_oid: local_checkpoint.checkpoint_oid.as_str().to_string(),
        remote_checkpoint_oid: output_oid.as_str().to_string(),
        local_ref: refs.local,
        remote_ref: refs.remote,
        reconciled_at: Utc::now(),
    })
}

fn ensure_create_only_ref(
    forge: &Forge,
    repo: &Path,
    reference: &str,
    oid: &GitOid,
) -> Result<(), ForgeError> {
    match forge.git().forge_ref_oid(repo, reference)? {
        Some(existing) if existing == *oid => return Ok(()),
        Some(existing) => {
            return Err(ForgeError::Conflict(format!(
                "portable result ref {reference} already names {existing}; refused stale {oid}"
            )));
        }
        None => {}
    }
    if let Err(create_error) = forge.git().create_forge_ref_cas(repo, reference, oid) {
        return match forge.git().forge_ref_oid(repo, reference)? {
            Some(existing) if existing == *oid => Ok(()),
            Some(existing) => Err(ForgeError::Conflict(format!(
                "portable result ref {reference} concurrently advanced to {existing}; refused {oid}"
            ))),
            None => Err(create_error),
        };
    }
    Ok(())
}

struct ReconciliationRefs {
    remote: String,
    local: String,
}

fn reconciliation_refs(work_id: &WorkId, operation_id: &str) -> ReconciliationRefs {
    let work = stable_suffix(work_id.as_str());
    let operation = stable_suffix(operation_id);
    let root = format!("refs/medousa/forge/remote/{work}/{operation}");
    ReconciliationRefs {
        remote: format!("{root}/result"),
        local: format!("{root}/origin"),
    }
}

fn validate_returned_result(
    plan: &PortableCoderHandoffPlan,
    result: &PortableCoderResult,
    checkpoint: &WorkEnvironmentCheckpoint,
    signed_result_provenance: &ProvenanceRef,
) -> StasisResult<()> {
    result
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    checkpoint
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    let task = prior_task(plan)?;
    if result.operation_id != task.operation_id
        || result.work_id != task.work_id
        || result.project_id != task.project_id
        || result.input_checkpoint_oid != task.expected_checkpoint_oid
        || result.destination_runtime_id != plan.target_runtime_id
        || result.grant_id.is_none()
    {
        return Err(invalid(
            "returned portable Coder result does not match its signed handoff",
        ));
    }
    let requested = task
        .requested_tool_names
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if result
        .tool_names
        .iter()
        .any(|name| !requested.contains(name.as_str()))
    {
        return Err(invalid(
            "returned portable Coder result reports authority outside its request",
        ));
    }
    if signed_result_provenance.digest.is_none() {
        return Err(invalid(
            "returned portable Coder result has no content-bound signed provenance",
        ));
    }
    Ok(())
}

fn validate_returned_manifest(
    plan: &PortableCoderHandoffPlan,
    manifest: &WorkEnvironmentCheckpointManifest,
) -> StasisResult<()> {
    let expected = &plan.work.spec;
    if manifest.environment_id != expected.environment_id
        || manifest.workspace_id != expected.workspace_id
        || manifest.base_commit != expected.base_commit
        || manifest.fence.forge_environment_generation
            != expected.fence.forge_environment_generation
        || manifest.fence.forge_execution_generation != expected.fence.forge_execution_generation
        || manifest.fence.stasis_attempt.0 < expected.fence.stasis_attempt.0
    {
        return Err(invalid(
            "returned portable Coder checkpoint changed its environment identity or fences",
        ));
    }
    Ok(())
}

async fn checkpoint_graph(
    blobs: &dyn BlobTransferPort,
    checkpoint: &WorkEnvironmentCheckpoint,
) -> StasisResult<Vec<BlobDescriptor>> {
    checkpoint
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    let bytes = blobs.get(&checkpoint.manifest).await?;
    let manifest: WorkEnvironmentCheckpointManifest = serde_json::from_slice(&bytes)
        .map_err(|error| invalid(format!("decode checkpoint graph: {error}")))?;
    manifest
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    let mut graph = vec![checkpoint.manifest.clone(), manifest.source_bundle];
    graph.extend(manifest.artifacts.into_iter().map(|value| value.blob));
    for descriptor in &graph {
        if !blobs.exists(descriptor).await? {
            return Err(invalid(format!(
                "durable checkpoint graph is missing {}:{}",
                descriptor.digest.algorithm, descriptor.digest.hex
            )));
        }
    }
    Ok(graph)
}

fn validate_portable_checkpoint(checkpoint: &PortableForgeCheckpoint) -> StasisResult<()> {
    if checkpoint.schema_version != 1
        || checkpoint.work_id.as_str().is_empty()
        || checkpoint.repository_id.as_str().is_empty()
        || checkpoint.repository_id.as_str().contains(['/', '\\'])
        || !valid_git_oid(checkpoint.expected_base_oid.as_str())
        || !valid_git_oid(checkpoint.parent_oid.as_str())
        || !valid_git_oid(checkpoint.checkpoint_oid.as_str())
        || checkpoint.bundle_bytes == 0
        || checkpoint.bundle_digest.as_str().len() != 64
        || !checkpoint
            .bundle_digest
            .as_str()
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid("portable Forge checkpoint is invalid or path-bearing"));
    }
    Ok(())
}

fn prior_task(plan: &PortableCoderHandoffPlan) -> StasisResult<&PortableCoderTask> {
    plan.work
        .portable_coder
        .as_ref()
        .ok_or_else(|| invalid("portable Coder handoff has no task"))
}

fn write_reconciliation_bundle(path: &Path, bytes: &[u8]) -> StasisResult<()> {
    if bytes.len() > MAX_CAPTURE_BYTES {
        return Err(invalid(format!(
            "returned portable Coder bundle exceeds {MAX_CAPTURE_BYTES} bytes"
        )));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| invalid(format!("create reconciliation staging: {error}")))?;
    }
    fs::write(path, bytes)
        .map_err(|error| invalid(format!("write reconciliation staging: {error}")))
}

fn retention_deadline(retention: &WorkEnvironmentRetention) -> Option<DateTime<Utc>> {
    match retention {
        WorkEnvironmentRetention::Delete => None,
        WorkEnvironmentRetention::RetainWarmUntil(until)
        | WorkEnvironmentRetention::PreserveForDebugUntil(until) => Some(*until),
    }
}

fn stable_suffix(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")[..24].to_string()
}

fn valid_git_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_id(name: &str, value: &str) -> StasisResult<()> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.len() > 256
        || value.chars().any(char::is_control)
    {
        return Err(invalid(format!("portable Coder {name} is invalid")));
    }
    Ok(())
}

async fn ensure_job(jobs: &Arc<dyn JobStore>, candidate: NewJob) -> StasisResult<()> {
    if let Some(existing) = jobs.get(&candidate.id).await? {
        if existing.job_type != candidate.job_type || existing.payload_ref != candidate.payload_ref
        {
            return Err(invalid(format!(
                "portable Coder durable job identity collided: {}",
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
        .map_err(|error| invalid(format!("decode portable Coder proxy progress: {error}")))
}

fn is_terminal(state: &JobState) -> bool {
    matches!(
        state,
        JobState::Succeeded | JobState::Failed | JobState::DeadLetter | JobState::Canceled
    )
}

fn state_name(state: &JobState) -> String {
    format!("{state:?}").to_lowercase()
}

fn deferred(job: &Job, message: &str) -> JobExecutionOutcome {
    JobExecutionOutcome::Deferred {
        scheduled_at: Utc::now() + chrono::Duration::milliseconds(HANDOFF_DELAY_MILLIS),
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

fn invalid(message: impl Into<String>) -> StasisError {
    StasisError::PortFailure(message.into())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use chrono::Duration;
    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use medousa_forge::model::{ActorRef, ChangeStatus, ChangedFile};
    use stasis::domain::runtime::provenance::ContentDigest;
    use stasis::infrastructure::runtime::in_memory_blob_transfer::InMemoryBlobTransfer;
    use tempfile::TempDir;

    use super::*;

    struct Fixture {
        _repo_root: TempDir,
        _forge_root: TempDir,
        repo: std::path::PathBuf,
        workspace: std::path::PathBuf,
        forge: Arc<Forge>,
        execution: Arc<ForgeExecutionService>,
        blobs: Arc<InMemoryBlobTransfer>,
        work_id: WorkId,
    }

    fn fixture() -> Fixture {
        let repo_root = TempDir::new().unwrap();
        let forge_root = TempDir::new().unwrap();
        let git = GitEngine::detect().unwrap();
        run_git(repo_root.path(), &["init", "-b", "main", "--template="]);
        fs::write(repo_root.path().join("app.txt"), "v1\n").unwrap();
        run_git(repo_root.path(), &["add", "-A"]);
        git.commit_checkpoint(
            repo_root.path(),
            "initial",
            &CheckpointAuthor::default(),
        )
        .unwrap();
        let forge = Arc::new(Forge::open(forge_root.path()).unwrap());
        let actor: ActorRef = Forge::system_actor();
        let item = forge
            .register(
                "Portable Coder",
                "move this exact work",
                repo_root.path(),
                "main",
                "user-1",
                &actor,
            )
            .unwrap();
        let item = forge.provision(&item.id, &actor).unwrap();
        let workspace = item.workspace_environment().unwrap().worktree.clone();
        Fixture {
            repo: repo_root.path().to_path_buf(),
            workspace,
            forge,
            execution: Arc::new(ForgeExecutionService::new()),
            blobs: Arc::new(InMemoryBlobTransfer::new()),
            work_id: item.id,
            _repo_root: repo_root,
            _forge_root: forge_root,
        }
    }

    impl Fixture {
        fn service(&self) -> PortableCoderHandoffService {
            PortableCoderHandoffService::new(
                Arc::clone(&self.forge),
                Arc::clone(&self.execution),
                self.blobs.clone() as Arc<dyn BlobTransferPort>,
                None,
            )
        }

        fn request(&self, operation_id: &str) -> PortableCoderHandoffRequest {
            let now = Utc::now();
            PortableCoderHandoffRequest {
                operation_id: operation_id.to_string(),
                work_id: self.work_id.as_str().to_string(),
                parent_session_id: "session-portable-coder".to_string(),
                correlation_id: format!("correlation-{operation_id}"),
                target_runtime_id: "remote-runtime".to_string(),
                prompt: "Update app.txt and explain the verification.".to_string(),
                provider: "openai".to_string(),
                model: "gpt-test".to_string(),
                response_depth_mode: "standard".to_string(),
                max_tool_rounds: 12,
                requested_tool_names: vec![crate::public_api::COGNITION_TURN.to_string()],
                image: WorkEnvironmentImage {
                    reference: "registry.example.test/medousa/coder".to_string(),
                    digest: ContentDigest::sha256_bytes(b"portable-coder-image"),
                    platform: "linux/amd64".to_string(),
                },
                requirements: WorkEnvironmentRequirements::default(),
                network_policy: WorkEnvironmentNetworkPolicy::Deny,
                secret_refs: Vec::new(),
                forge_execution_generation: Some(7),
                retention: WorkEnvironmentRetention::Delete,
                requested_at: now,
                deadline_at: now + Duration::hours(1),
            }
        }
    }

    async fn remote_output(
        fixture: &Fixture,
        plan: &PortableCoderHandoffPlan,
        content: &str,
    ) -> (TempDir, PortableCoderResult, WorkEnvironmentCheckpoint) {
        let input = plan.work.spec.checkpoint_ref.as_ref().unwrap();
        let input_bytes = fixture.blobs.get(&input.manifest).await.unwrap();
        let input_manifest: WorkEnvironmentCheckpointManifest =
            serde_json::from_slice(&input_bytes).unwrap();
        let bundle = fixture
            .blobs
            .get(&input_manifest.source_bundle)
            .await
            .unwrap();

        let remote = TempDir::new().unwrap();
        let git = GitEngine::detect().unwrap();
        run_git(remote.path(), &["init", "--quiet", "--template="]);
        let input_bundle = remote.path().join("input.bundle");
        fs::write(&input_bundle, bundle).unwrap();
        git.import_checkpoint_bundle(
            remote.path(),
            &input_bundle,
            &GitOid::new(&input_manifest.checkpoint_commit),
        )
        .unwrap();
        fs::write(remote.path().join("app.txt"), content).unwrap();
        let output_oid = git
            .commit_checkpoint(
                remote.path(),
                "portable result",
                &CheckpointAuthor::default(),
            )
            .unwrap();
        let output_bundle = remote.path().join("output.bundle");
        git.export_checkpoint_bundle(remote.path(), &output_oid, &output_bundle)
            .unwrap();
        let source_bundle = fixture
            .blobs
            .put(
                &fs::read(output_bundle).unwrap(),
                Some(PORTABLE_FORGE_BUNDLE_MEDIA_TYPE),
            )
            .await
            .unwrap();
        let output_manifest = WorkEnvironmentCheckpointManifest {
            schema_version: medousa_runtime::WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION,
            environment_id: plan.work.spec.environment_id.clone(),
            workspace_id: plan.work.spec.workspace_id.clone(),
            base_commit: plan.work.spec.base_commit.clone(),
            checkpoint_commit: output_oid.as_str().to_string(),
            source_bundle,
            artifacts: Vec::new(),
            fence: plan.work.spec.fence.clone(),
            label: Some("portable result".to_string()),
            created_at: Utc::now(),
        };
        let descriptor = fixture
            .blobs
            .put(
                &serde_json::to_vec(&output_manifest).unwrap(),
                Some(PORTABLE_FORGE_CHECKPOINT_MEDIA_TYPE),
            )
            .await
            .unwrap();
        let checkpoint = WorkEnvironmentCheckpoint::from_manifest(descriptor);
        let task = prior_task(plan).unwrap();
        let result = PortableCoderResult {
            schema_version: crate::portable_coder::PORTABLE_CODER_RESULT_SCHEMA_VERSION,
            operation_id: task.operation_id.clone(),
            work_id: task.work_id.clone(),
            destination_runtime_id: plan.target_runtime_id.clone(),
            project_id: task.project_id.clone(),
            input_checkpoint_oid: task.expected_checkpoint_oid.clone(),
            response_text: "Updated and verified app.txt.".to_string(),
            tool_names: vec![crate::public_api::COGNITION_TURN.to_string()],
            changed_files: vec![ChangedFile {
                path: "app.txt".to_string(),
                status: ChangeStatus::Modified,
                old_path: None,
                is_binary: false,
                byte_size: Some(content.len() as u64),
            }],
            termination_reason: "finished".to_string(),
            workspace_state_digest: digest_label(content.as_bytes()),
            evidence_digest: digest_label(b"portable evidence"),
            grant_id: Some("grant-remote-runtime".to_string()),
            completed_at: Utc::now(),
        };
        (remote, result, checkpoint)
    }

    fn digest_label(bytes: &[u8]) -> String {
        let digest = ContentDigest::sha256_bytes(bytes);
        format!("{}:{}", digest.algorithm, digest.hex)
    }

    fn run_git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    async fn safe_dirty_capture_is_durable_and_path_free() {
        let fixture = fixture();
        fs::write(fixture.workspace.join("app.txt"), "safe dirty state\n").unwrap();
        let plan = fixture
            .service()
            .prepare(fixture.request("operation-safe"))
            .await
            .unwrap();

        let encoded = serde_json::to_string(&plan).unwrap();
        assert!(!encoded.contains(&fixture.repo.display().to_string()));
        assert!(!encoded.contains(&fixture.workspace.display().to_string()));
        assert!(!encoded.contains(fixture.forge.store().root().to_string_lossy().as_ref()));
        assert!(plan.origin_checkpoint.repository_id.as_str().starts_with("repo1-"));
        assert_eq!(
            plan.work.spec.requirements.placement.target_node.as_deref(),
            Some("remote-runtime")
        );
        let graph = checkpoint_graph(
            fixture.blobs.as_ref(),
            plan.work.spec.checkpoint_ref.as_ref().unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(graph.len(), 2);
    }

    #[tokio::test]
    async fn secret_bearing_capture_stops_before_a_plan_exists() {
        let fixture = fixture();
        let secret = ["ghp_", "123456789012345678901234567890123456"].concat();
        fs::write(fixture.workspace.join("token.txt"), secret).unwrap();
        let error = fixture
            .service()
            .prepare(fixture.request("operation-secret"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("unsafe portable capture"));
    }

    #[tokio::test]
    async fn reconciliation_preserves_checkout_and_rejects_stale_result_ref() {
        let fixture = fixture();
        fs::write(fixture.workspace.join("app.txt"), "origin dirty state\n").unwrap();
        let service = fixture.service();
        let plan = service
            .prepare(fixture.request("operation-reconcile"))
            .await
            .unwrap();
        let git = fixture.forge.git();
        let source_head = git.head_oid(&fixture.repo).unwrap();
        let source_index = git.index_tree_oid(&fixture.repo).unwrap();
        let source_file = fs::read_to_string(fixture.repo.join("app.txt")).unwrap();
        let (_remote, result, checkpoint) =
            remote_output(&fixture, &plan, "remote result one\n").await;

        let reconciliation = service
            .reconcile(&plan, &result, &checkpoint, &checkpoint.provenance)
            .await
            .unwrap();
        let remote_ref = match reconciliation {
            PortableCoderReconciliation::ReviewReady { remote_ref, .. } => remote_ref,
            other => panic!("expected review-ready result, found {other:?}"),
        };
        assert_eq!(git.head_oid(&fixture.repo).unwrap(), source_head);
        assert_eq!(git.index_tree_oid(&fixture.repo).unwrap(), source_index);
        assert_eq!(
            fs::read_to_string(fixture.repo.join("app.txt")).unwrap(),
            source_file
        );
        assert!(git.forge_ref_oid(&fixture.repo, &remote_ref).unwrap().is_some());

        let (_remote, stale_result, stale_checkpoint) =
            remote_output(&fixture, &plan, "different stale result\n").await;
        let error = service
            .reconcile(
                &plan,
                &stale_result,
                &stale_checkpoint,
                &stale_checkpoint.provenance,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("ref"));
        assert!(error.to_string().contains("refused"));
    }

    #[tokio::test]
    async fn conflict_preserves_local_and_remote_refs_and_retry_uses_checkpoint() {
        let fixture = fixture();
        fs::write(fixture.workspace.join("app.txt"), "captured origin\n").unwrap();
        let service = fixture.service();
        let plan = service
            .prepare(fixture.request("operation-conflict"))
            .await
            .unwrap();
        let (_remote, result, checkpoint) =
            remote_output(&fixture, &plan, "remote branch\n").await;
        fs::write(fixture.workspace.join("app.txt"), "newer local work\n").unwrap();

        let reconciliation = service
            .reconcile(&plan, &result, &checkpoint, &checkpoint.provenance)
            .await
            .unwrap();
        let (local_ref, remote_ref) = match reconciliation {
            PortableCoderReconciliation::Conflict {
                local_ref,
                remote_ref,
                ..
            } => (local_ref, remote_ref),
            other => panic!("expected conflict, found {other:?}"),
        };
        assert!(
            fixture
                .forge
                .git()
                .forge_ref_oid(&fixture.repo, &local_ref)
                .unwrap()
                .is_some()
        );
        assert!(
            fixture
                .forge
                .git()
                .forge_ref_oid(&fixture.repo, &remote_ref)
                .unwrap()
                .is_some()
        );

        let now = Utc::now();
        let retry = service
            .retry_from_checkpoint(
                &plan,
                checkpoint.clone(),
                PortableCoderRetryRequest {
                    operation_id: "operation-retry".to_string(),
                    correlation_id: "correlation-retry".to_string(),
                    target_runtime_id: "fourth-runtime".to_string(),
                    prompt: "Continue from the preserved checkpoint.".to_string(),
                    requested_tool_names: vec![crate::public_api::COGNITION_TURN.to_string()],
                    forge_execution_generation: Some(8),
                    requested_at: now,
                    deadline_at: now + Duration::hours(1),
                },
            )
            .await
            .unwrap();
        let retry_task = prior_task(&retry).unwrap();
        let output_manifest: WorkEnvironmentCheckpointManifest = serde_json::from_slice(
            &fixture.blobs.get(&checkpoint.manifest).await.unwrap(),
        )
        .unwrap();
        assert_eq!(
            retry_task.expected_checkpoint_oid,
            output_manifest.checkpoint_commit
        );
        assert_eq!(retry.target_runtime_id, "fourth-runtime");
        assert_ne!(
            retry.work.spec.environment_id,
            plan.work.spec.environment_id
        );
        assert_eq!(retry.work.spec.workspace_id, plan.work.spec.workspace_id);
    }
}
