//! Runtime-neutral federation boundary for durable work-environment jobs.
//!
//! Stasis owns the signed remote job and terminal-result vocabulary. Medousa
//! only maps the existing work-environment payload onto an independent runtime
//! and moves immutable bytes through `BlobTransferPort`. Pairing, HTTP/Iroh,
//! and signing-key custody remain host transport adapters.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use medousa_runtime::{
    WorkEnvironmentCheckpoint, WorkEnvironmentCheckpointManifest, WorkEnvironmentExecResult,
    WorkEnvironmentPublicationResult,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use stasis::domain::runtime::blob_descriptor::BlobDescriptor;
use stasis::domain::runtime::federation::FederatedTerminalResult;
use stasis::domain::runtime::job::{BackoffPolicy, Job, JobState, NewJob};
use stasis::domain::runtime::placement::PlacementConstraints;
use stasis::domain::runtime::provenance::ProvenanceRef;
use stasis::domain::runtime::remote_job_envelope::{
    EnvelopeSignature, OriginAuthority, REMOTE_JOB_ENVELOPE_SCHEMA_VERSION_V1, RemoteJobEnvelope,
    TerminalDeliveryEndpoint,
};
use stasis::ports::outbound::runtime::blob_transfer::BlobTransferPort;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::runtime_composition_ext::RuntimeCompositionExt;
use crate::work_environment_job::{
    WORK_ENVIRONMENT_JOB_TYPE, WorkEnvironmentJobPayload, WorkEnvironmentJobProgress,
};

pub const WORK_ENVIRONMENT_JOB_PAYLOAD_MEDIA_TYPE: &str =
    "application/vnd.medousa.work-environment-job+json";
pub const WORK_ENVIRONMENT_RESULT_MEDIA_TYPE: &str =
    "application/vnd.medousa.work-environment-result+json";
pub const WORK_ENVIRONMENT_TERMINAL_RECORD_JOB_TYPE: &str =
    "workflow.medousa.remote_work_environment_result";
pub const WORK_ENVIRONMENT_RESULT_SCHEMA_VERSION: u32 = 1;
const MAX_REMOTE_WORK_ENVIRONMENT_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Portable origin/delivery coordinates attached by the accepting runtime.
/// They contain no route credentials, local container ids, or adapter handles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentFederationContext {
    pub envelope_id: String,
    pub origin_authority: OriginAuthority,
    pub terminal_delivery: TerminalDeliveryEndpoint,
}

/// Typed terminal body stored behind `FederatedTerminalResult.output`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteWorkEnvironmentResult {
    pub schema_version: u32,
    pub envelope_id: String,
    pub remote_job_id: String,
    pub succeeded: bool,
    pub terminal_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_result: Option<WorkEnvironmentExecResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<WorkEnvironmentCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<WorkEnvironmentPublicationResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub finished_at: DateTime<Utc>,
}

impl RemoteWorkEnvironmentResult {
    pub fn validate(&self) -> StasisResult<()> {
        if self.schema_version != WORK_ENVIRONMENT_RESULT_SCHEMA_VERSION {
            return Err(StasisError::PortFailure(format!(
                "unsupported remote work-environment result schema_version={}",
                self.schema_version
            )));
        }
        if self.envelope_id.trim().is_empty()
            || self.remote_job_id.trim().is_empty()
            || self.terminal_state.trim().is_empty()
        {
            return Err(StasisError::PortFailure(
                "remote work-environment result identity is missing".to_string(),
            ));
        }
        if let Some(checkpoint) = self.checkpoint.as_ref() {
            checkpoint
                .validate()
                .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        } else if self.succeeded {
            return Err(StasisError::PortFailure(
                "successful remote work-environment result has no checkpoint".to_string(),
            ));
        }
        Ok(())
    }
}

/// Host adapter contract that signs and delivers one Stasis terminal result.
/// Implementations own pairing keys and the actual HTTP/Iroh route.
#[async_trait]
pub trait SignedFederatedTerminalDelivery: Send + Sync {
    async fn sign_and_deliver(&self, result: FederatedTerminalResult) -> StasisResult<()>;
}

/// Origin-side port used by durable proxy jobs. Pairing, signing, transport,
/// and peer lookup remain adapter concerns; the proxy only owns stable work and
/// envelope identities.
#[async_trait]
pub trait RemoteWorkEnvironmentDispatcher: Send + Sync {
    fn origin_authority(&self) -> OriginAuthority;

    fn terminal_delivery(&self) -> TerminalDeliveryEndpoint;

    /// Return `None` when the local runtime is the selected worker, otherwise
    /// the remote runtime id that should receive the replay-stable envelope.
    async fn select_target(
        &self,
        selection_key: &str,
        placement: &PlacementConstraints,
    ) -> StasisResult<Option<String>>;

    async fn submit_remote_job(
        &self,
        target_runtime_id: &str,
        envelope: RemoteJobEnvelope,
    ) -> StasisResult<String>;
}

#[derive(Clone)]
pub struct WorkEnvironmentFederationServices {
    pub blobs: Arc<dyn BlobTransferPort>,
    pub terminal_delivery: Arc<dyn SignedFederatedTerminalDelivery>,
}

/// Persist a bounded job payload and return the descriptor carried by Stasis.
pub async fn stage_remote_work_environment_payload(
    blobs: &dyn BlobTransferPort,
    payload: &WorkEnvironmentJobPayload,
) -> StasisResult<BlobDescriptor> {
    payload
        .validate(Utc::now())
        .map_err(|error| StasisError::PortFailure(error.to_string()))?;
    let bytes = serde_json::to_vec(payload)
        .map_err(|error| StasisError::PortFailure(format!("encode remote payload: {error}")))?;
    if bytes.len() > MAX_REMOTE_WORK_ENVIRONMENT_PAYLOAD_BYTES {
        return Err(StasisError::PortFailure(format!(
            "remote work-environment payload exceeds {MAX_REMOTE_WORK_ENVIRONMENT_PAYLOAD_BYTES} bytes"
        )));
    }
    blobs
        .put(&bytes, Some(WORK_ENVIRONMENT_JOB_PAYLOAD_MEDIA_TYPE))
        .await
}

/// Build the unsigned Stasis envelope. The selected transport must sign it
/// before delivery using the paired runtime identity.
#[allow(clippy::too_many_arguments)]
pub fn build_remote_work_environment_envelope(
    envelope_id: impl Into<String>,
    payload: BlobDescriptor,
    idempotency_key: impl Into<String>,
    correlation_id: impl Into<String>,
    causation_id: impl Into<String>,
    deadline: DateTime<Utc>,
    origin_authority: OriginAuthority,
    terminal_delivery: TerminalDeliveryEndpoint,
    placement: stasis::domain::runtime::placement::PlacementConstraints,
) -> StasisResult<RemoteJobEnvelope> {
    let envelope_id = envelope_id.into();
    let idempotency_key = idempotency_key.into();
    if envelope_id.trim().is_empty() || idempotency_key.trim().is_empty() {
        return Err(StasisError::PortFailure(
            "remote work-environment envelope identity is required".to_string(),
        ));
    }
    let envelope = RemoteJobEnvelope {
        schema_version: REMOTE_JOB_ENVELOPE_SCHEMA_VERSION_V1,
        envelope_id,
        job_type: WORK_ENVIRONMENT_JOB_TYPE.to_string(),
        payload,
        idempotency_key,
        correlation_id: correlation_id.into(),
        causation_id: causation_id.into(),
        deadline,
        origin_authority,
        terminal_delivery,
        placement,
        signature: EnvelopeSignature {
            algorithm: String::new(),
            key_id: String::new(),
            signature_hex: String::new(),
        },
    };
    envelope
        .validate_for_acceptance(Utc::now())
        .map_err(StasisError::PortFailure)?;
    Ok(envelope)
}

/// Copy one immutable blob between independent stores and require the
/// destination to preserve its exact content identity.
pub async fn transfer_blob(
    source: &dyn BlobTransferPort,
    destination: &dyn BlobTransferPort,
    descriptor: &BlobDescriptor,
) -> StasisResult<BlobDescriptor> {
    let bytes = source.get(descriptor).await?;
    if !descriptor.verify(&bytes) {
        return Err(StasisError::PortFailure(
            "source blob failed descriptor verification".to_string(),
        ));
    }
    let stored = destination
        .put(&bytes, descriptor.media_type.as_deref())
        .await?;
    if stored.digest != descriptor.digest || stored.size_bytes != descriptor.size_bytes {
        return Err(StasisError::PortFailure(
            "destination blob identity changed during transfer".to_string(),
        ));
    }
    Ok(stored)
}

/// Transfer the complete immutable graph required to reconstruct a checkpoint.
pub async fn transfer_checkpoint_graph(
    source: &dyn BlobTransferPort,
    destination: &dyn BlobTransferPort,
    checkpoint: &WorkEnvironmentCheckpoint,
) -> StasisResult<()> {
    checkpoint
        .validate()
        .map_err(|error| StasisError::PortFailure(error.to_string()))?;
    transfer_blob(source, destination, &checkpoint.manifest).await?;
    let manifest_bytes = source.get(&checkpoint.manifest).await?;
    let manifest: WorkEnvironmentCheckpointManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| {
            StasisError::PortFailure(format!("decode checkpoint manifest: {error}"))
        })?;
    manifest
        .validate()
        .map_err(|error| StasisError::PortFailure(error.to_string()))?;
    if WorkEnvironmentCheckpoint::from_manifest(checkpoint.manifest.clone()).provenance
        != checkpoint.provenance
    {
        return Err(StasisError::PortFailure(
            "checkpoint manifest provenance does not match its descriptor".to_string(),
        ));
    }
    transfer_blob(source, destination, &manifest.source_bundle).await?;
    for artifact in &manifest.artifacts {
        transfer_blob(source, destination, &artifact.blob).await?;
    }
    Ok(())
}

/// Accept an already authenticated/signed Stasis envelope into this independent
/// runtime. Signature verification belongs to the production federation ingress
/// adapter; this function validates the immutable application mapping.
pub async fn accept_remote_work_environment_job(
    runtime: &RuntimeComposition,
    blobs: &dyn BlobTransferPort,
    envelope: &RemoteJobEnvelope,
) -> StasisResult<String> {
    envelope
        .validate_for_acceptance(Utc::now())
        .map_err(StasisError::PortFailure)?;
    if envelope.job_type != WORK_ENVIRONMENT_JOB_TYPE {
        return Err(StasisError::PortFailure(format!(
            "unsupported remote job type: {}",
            envelope.job_type
        )));
    }
    let bytes = blobs.get(&envelope.payload).await?;
    if bytes.len() > MAX_REMOTE_WORK_ENVIRONMENT_PAYLOAD_BYTES || !envelope.payload.verify(&bytes) {
        return Err(StasisError::PortFailure(
            "remote work-environment payload failed bounds or digest verification".to_string(),
        ));
    }
    let mut payload: WorkEnvironmentJobPayload = serde_json::from_slice(&bytes)
        .map_err(|error| StasisError::PortFailure(format!("decode remote payload: {error}")))?;
    if envelope.placement != payload.spec.placement_constraints() {
        return Err(StasisError::PortFailure(
            "remote envelope placement does not match the environment spec".to_string(),
        ));
    }
    payload.deadline_at = Some(
        payload
            .deadline_at
            .map(|deadline| deadline.min(envelope.deadline))
            .unwrap_or(envelope.deadline),
    );
    payload.federation = Some(WorkEnvironmentFederationContext {
        envelope_id: envelope.envelope_id.clone(),
        origin_authority: envelope.origin_authority.clone(),
        terminal_delivery: envelope.terminal_delivery.clone(),
    });
    payload
        .validate(Utc::now())
        .map_err(|error| StasisError::PortFailure(error.to_string()))?;

    let local_job_id = remote_job_id(envelope);
    let mut job = payload.clone().into_job(
        local_job_id.clone(),
        "default",
        envelope.causation_id.clone(),
        Utc::now(),
    )?;
    job.idempotency_key = envelope.idempotency_key.clone();
    job.correlation_id = envelope.correlation_id.clone();
    job.causation_id = envelope.causation_id.clone();
    job.trace_id = envelope.correlation_id.clone();
    job.placement = envelope.placement.clone();
    job.payload_ref = payload.to_payload_ref()?;

    if let Some(existing) = runtime.get_job(&local_job_id).await? {
        if existing.job_type != job.job_type
            || existing.payload_ref != job.payload_ref
            || existing.idempotency_key != job.idempotency_key
            || existing.placement != job.placement
        {
            return Err(StasisError::PortFailure(
                "remote work-environment identity collided with different work".to_string(),
            ));
        }
        return Ok(local_job_id);
    }
    runtime.enqueue_job(job).await?;
    Ok(local_job_id)
}

pub async fn encode_remote_terminal_result(
    blobs: &dyn BlobTransferPort,
    job: &Job,
    context: &WorkEnvironmentFederationContext,
) -> StasisResult<FederatedTerminalResult> {
    if !matches!(
        job.state,
        JobState::Succeeded | JobState::Failed | JobState::DeadLetter | JobState::Canceled
    ) {
        return Err(StasisError::PortFailure(
            "cannot encode a non-terminal federated job".to_string(),
        ));
    }
    let succeeded = job.state == JobState::Succeeded;
    let progress = job
        .progress_json
        .as_deref()
        .map(serde_json::from_str::<WorkEnvironmentJobProgress>)
        .transpose()
        .map_err(|error| {
            StasisError::PortFailure(format!(
                "decode terminal work-environment progress: {error}"
            ))
        })?;
    let result = RemoteWorkEnvironmentResult {
        schema_version: WORK_ENVIRONMENT_RESULT_SCHEMA_VERSION,
        envelope_id: context.envelope_id.clone(),
        remote_job_id: job.id.clone(),
        succeeded,
        terminal_state: format!("{:?}", job.state).to_lowercase(),
        execution_result: progress
            .as_ref()
            .and_then(|progress| progress.execution_result.clone()),
        checkpoint: progress
            .as_ref()
            .and_then(|progress| progress.checkpoint.clone()),
        publication: progress
            .as_ref()
            .and_then(|progress| progress.publication.clone()),
        error_message: (!succeeded)
            .then(|| job.last_error.clone())
            .flatten()
            .or_else(|| (!succeeded).then(|| format!("remote job ended as {:?}", job.state))),
        finished_at: job.finished_at.unwrap_or_else(Utc::now),
    };
    let bytes = serde_json::to_vec(&result)
        .map_err(|error| StasisError::PortFailure(format!("encode remote result: {error}")))?;
    let output = blobs
        .put(&bytes, Some(WORK_ENVIRONMENT_RESULT_MEDIA_TYPE))
        .await?;
    let output_provenance = job.output_provenance.clone().or_else(|| {
        result
            .checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.provenance.clone())
    });
    Ok(FederatedTerminalResult {
        schema_version:
            stasis::domain::runtime::federation::FEDERATED_TERMINAL_RESULT_SCHEMA_VERSION_V1,
        result_id: format!("{}:terminal", context.envelope_id),
        envelope_id: context.envelope_id.clone(),
        job_id: job.id.clone(),
        job_type: job.job_type.clone(),
        succeeded,
        output: Some(output),
        output_provenance,
        error_message: result.error_message.clone(),
        origin_authority: context.origin_authority.clone(),
        terminal_delivery: context.terminal_delivery.clone(),
        correlation_id: job.correlation_id.clone(),
        causation_id: job.causation_id.clone(),
        occurred_at: result.finished_at,
        signature: EnvelopeSignature {
            algorithm: String::new(),
            key_id: String::new(),
            signature_hex: String::new(),
        },
    })
}

pub async fn decode_remote_terminal_result(
    blobs: &dyn BlobTransferPort,
    result: &FederatedTerminalResult,
) -> StasisResult<RemoteWorkEnvironmentResult> {
    result
        .validate_schema_version()
        .map_err(StasisError::PortFailure)?;
    let output = result.output.as_ref().ok_or_else(|| {
        StasisError::PortFailure("federated terminal result has no output descriptor".to_string())
    })?;
    let bytes = blobs.get(output).await?;
    let decoded: RemoteWorkEnvironmentResult = serde_json::from_slice(&bytes)
        .map_err(|error| StasisError::PortFailure(format!("decode remote result: {error}")))?;
    decoded.validate()?;
    let checkpoint_provenance = decoded
        .checkpoint
        .as_ref()
        .map(|checkpoint| checkpoint.provenance.clone());
    if decoded.envelope_id != result.envelope_id
        || decoded.remote_job_id != result.job_id
        || decoded.succeeded != result.succeeded
        || checkpoint_provenance != result.output_provenance
    {
        return Err(StasisError::PortFailure(
            "federated terminal body does not match its signed result".to_string(),
        ));
    }
    Ok(decoded)
}

/// Persist terminal ingress as a terminal Stasis record. Remote proxy jobs can
/// observe this local record without polling the destination daemon.
pub async fn record_remote_terminal_result(
    runtime: &RuntimeComposition,
    result: &FederatedTerminalResult,
    stored: &BlobDescriptor,
) -> StasisResult<String> {
    let record_id = terminal_result_record_id(&result.envelope_id);
    let payload_ref = serde_json::to_string(stored).map_err(|error| {
        StasisError::PortFailure(format!("encode terminal result descriptor: {error}"))
    })?;
    if let Some(existing) = runtime.get_job(&record_id).await? {
        if existing.job_type != WORK_ENVIRONMENT_TERMINAL_RECORD_JOB_TYPE
            || existing.payload_ref != payload_ref
        {
            return Err(StasisError::PortFailure(
                "remote terminal result identity collided with different content".to_string(),
            ));
        }
        return Ok(record_id);
    }
    let mut provenance = ProvenanceRef::cas(stored.digest.clone());
    provenance.media_type = stored.media_type.clone();
    let mut record = NewJob {
        id: record_id.clone(),
        queue: "federation-results".to_string(),
        job_type: WORK_ENVIRONMENT_TERMINAL_RECORD_JOB_TYPE.to_string(),
        payload_ref,
        priority: 0,
        max_attempts: 1,
        idempotency_key: format!("idem-{record_id}"),
        correlation_id: result.correlation_id.clone(),
        causation_id: result.causation_id.clone(),
        trace_id: result.correlation_id.clone(),
        input_provenance: result.output_provenance.clone(),
        placement: PlacementConstraints::unrestricted(),
        scheduled_at: result.occurred_at,
        backoff_policy: BackoffPolicy::default(),
    }
    .into_job();
    record.state = JobState::Succeeded;
    record.output_provenance = Some(provenance);
    record.started_at = Some(result.occurred_at);
    record.finished_at = Some(result.occurred_at);
    runtime.save_job(record).await?;
    Ok(record_id)
}

pub async fn load_recorded_terminal_result(
    runtime: &RuntimeComposition,
    blobs: &dyn BlobTransferPort,
    envelope_id: &str,
) -> StasisResult<Option<FederatedTerminalResult>> {
    let Some(record) = runtime
        .get_job(&terminal_result_record_id(envelope_id))
        .await?
    else {
        return Ok(None);
    };
    if record.job_type != WORK_ENVIRONMENT_TERMINAL_RECORD_JOB_TYPE
        || record.state != JobState::Succeeded
    {
        return Err(StasisError::PortFailure(
            "remote terminal result record has invalid type or state".to_string(),
        ));
    }
    let descriptor: BlobDescriptor =
        serde_json::from_str(&record.payload_ref).map_err(|error| {
            StasisError::PortFailure(format!("decode terminal result descriptor: {error}"))
        })?;
    let bytes = blobs.get(&descriptor).await?;
    let result: FederatedTerminalResult = serde_json::from_slice(&bytes).map_err(|error| {
        StasisError::PortFailure(format!("decode recorded terminal result: {error}"))
    })?;
    if result.envelope_id != envelope_id {
        return Err(StasisError::PortFailure(
            "recorded terminal result envelope identity changed".to_string(),
        ));
    }
    Ok(Some(result))
}

pub fn terminal_result_record_id(envelope_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"medousa/remote-work-environment-result/v1\0");
    digest.update(envelope_id.trim().as_bytes());
    format!("remote-work-result-{:x}", digest.finalize())
}

fn remote_job_id(envelope: &RemoteJobEnvelope) -> String {
    let mut digest = Sha256::new();
    digest.update(b"medousa/remote-work-environment/v1\0");
    digest.update(envelope.origin_authority.runtime_id.as_bytes());
    digest.update([0]);
    digest.update(envelope.envelope_id.as_bytes());
    format!("remote-work-env-{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration as StdDuration;

    use chrono::Duration;
    use medousa_runtime::{
        InMemoryWorkEnvironmentPort, WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION,
        WorkEnvironmentArtifact, WorkEnvironmentCheckpointPolicy, WorkEnvironmentExecRequest,
        WorkEnvironmentFence, WorkEnvironmentId, WorkEnvironmentImage,
        WorkEnvironmentNetworkPolicy, WorkEnvironmentRepository, WorkEnvironmentRequirements,
        WorkEnvironmentRetention, WorkEnvironmentSpec, WorkspaceId,
    };
    use stasis::application::runtime::in_memory_runtime::InMemoryRuntime;
    use stasis::domain::runtime::federation::sign_federated_terminal_result;
    use stasis::domain::runtime::placement::WorkerCapabilities;
    use stasis::domain::runtime::provenance::ContentDigest;
    use stasis::domain::runtime::remote_job_envelope::sign_remote_job_envelope;
    use stasis::infrastructure::runtime::in_memory_blob_transfer::InMemoryBlobTransfer;
    use stasis::infrastructure::runtime::in_memory_federated_bus::InMemoryFederatedBus;
    use stasis::ports::outbound::runtime::federated_delivery::FederatedDeliveryPort;
    use stasis::ports::outbound::runtime::job_store::JobStore;

    use crate::work_environment_job::{
        WORK_ENVIRONMENT_TERMINAL_DELIVERY_JOB_TYPE,
        register_federated_work_environment_job_handlers,
    };

    use super::*;

    const TEST_KEY_ID: &str = "phase-6-key";
    const TEST_KEY: &[u8] = b"phase-6-federation-secret";

    #[derive(Clone)]
    struct HmacTerminalDelivery {
        delivery: Arc<dyn FederatedDeliveryPort>,
    }

    #[async_trait]
    impl SignedFederatedTerminalDelivery for HmacTerminalDelivery {
        async fn sign_and_deliver(&self, mut result: FederatedTerminalResult) -> StasisResult<()> {
            sign_federated_terminal_result(&mut result, TEST_KEY_ID, TEST_KEY)
                .map_err(StasisError::PortFailure)?;
            self.delivery.deliver_terminal_result(result).await
        }
    }

    fn payload(environment_id: &str) -> WorkEnvironmentJobPayload {
        WorkEnvironmentJobPayload {
            spec: WorkEnvironmentSpec {
                environment_id: WorkEnvironmentId::parse(environment_id).unwrap(),
                workspace_id: WorkspaceId::parse("federated-workload").unwrap(),
                repository: WorkEnvironmentRepository {
                    repository_id: "federated-workload".to_string(),
                    authorized_origin: "https://example.invalid/repository.git".to_string(),
                },
                base_commit: "a".repeat(40),
                image: WorkEnvironmentImage {
                    reference: "example.invalid/medousa/dev".to_string(),
                    digest: ContentDigest::sha256_bytes(b"phase-6-image"),
                    platform: "linux/amd64".to_string(),
                },
                checkpoint_ref: None,
                requirements: WorkEnvironmentRequirements::default(),
                mounts: Vec::new(),
                network_policy: WorkEnvironmentNetworkPolicy::Deny,
                secret_refs: Vec::new(),
                fence: WorkEnvironmentFence {
                    stasis_attempt: stasis::domain::runtime::resource_lease::FencingToken(1),
                    forge_environment_generation: None,
                    forge_execution_generation: None,
                },
                publication: None,
                retention: WorkEnvironmentRetention::Delete,
            },
            execution: WorkEnvironmentExecRequest {
                idempotency_key: "replaced-by-destination".to_string(),
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "printf phase-6".to_string()],
                working_directory: Some("/workspace".to_string()),
                environment: BTreeMap::new(),
                stdin: None,
                timeout_seconds: 30,
                max_output_bytes: 64 * 1024,
            },
            checkpoint: WorkEnvironmentCheckpointPolicy::default(),
            require_successful_exit: true,
            deadline_at: Some(Utc::now() + Duration::minutes(5)),
            display_name: Some("Phase 6 remote proof".to_string()),
            federation: None,
        }
    }

    fn origin() -> OriginAuthority {
        OriginAuthority {
            runtime_id: "origin-runtime".to_string(),
            authority_id: "origin-authority".to_string(),
            realm: Some("test".to_string()),
        }
    }

    fn terminal_endpoint() -> TerminalDeliveryEndpoint {
        TerminalDeliveryEndpoint {
            endpoint_id: "origin-terminal".to_string(),
            protocol: "memory-bus".to_string(),
            address: "origin-runtime://terminal".to_string(),
        }
    }

    fn oci_capabilities() -> WorkerCapabilities {
        WorkerCapabilities::any().with_capability(medousa_runtime::OCI_WORK_ENVIRONMENT_CAPABILITY)
    }

    async fn process_until_terminal(runtime: &InMemoryRuntime, job_id: &str) -> Job {
        for _ in 0..40 {
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
                    "phase-6-worker",
                    Utc::now(),
                    &oci_capabilities(),
                )
                .await
                .unwrap();
            tokio::time::sleep(StdDuration::from_millis(2)).await;
        }
        panic!("job {job_id} did not become terminal");
    }

    #[tokio::test]
    async fn terminal_ingress_becomes_a_replay_stable_local_stasis_record() {
        let runtime = RuntimeComposition::InMemory(InMemoryRuntime::new());
        let blobs = InMemoryBlobTransfer::new();
        let body = RemoteWorkEnvironmentResult {
            schema_version: WORK_ENVIRONMENT_RESULT_SCHEMA_VERSION,
            envelope_id: "remote-envelope".to_string(),
            remote_job_id: "remote-job".to_string(),
            succeeded: false,
            terminal_state: "failed".to_string(),
            execution_result: None,
            checkpoint: None,
            publication: None,
            error_message: Some("expected failure".to_string()),
            finished_at: Utc::now(),
        };
        let output = blobs
            .put(
                &serde_json::to_vec(&body).unwrap(),
                Some(WORK_ENVIRONMENT_RESULT_MEDIA_TYPE),
            )
            .await
            .unwrap();
        let result = FederatedTerminalResult {
            schema_version:
                stasis::domain::runtime::federation::FEDERATED_TERMINAL_RESULT_SCHEMA_VERSION_V1,
            result_id: "remote-result".to_string(),
            envelope_id: body.envelope_id.clone(),
            job_id: body.remote_job_id.clone(),
            job_type: WORK_ENVIRONMENT_JOB_TYPE.to_string(),
            succeeded: false,
            output: Some(output),
            output_provenance: None,
            error_message: body.error_message.clone(),
            origin_authority: origin(),
            terminal_delivery: terminal_endpoint(),
            correlation_id: "correlation".to_string(),
            causation_id: "causation".to_string(),
            occurred_at: body.finished_at,
            signature: EnvelopeSignature {
                algorithm: "test".to_string(),
                key_id: "test".to_string(),
                signature_hex: "test".to_string(),
            },
        };
        let stored = blobs
            .put(
                &serde_json::to_vec(&result).unwrap(),
                Some("application/vnd.stasis.federated-terminal-result+json"),
            )
            .await
            .unwrap();

        let first = record_remote_terminal_result(&runtime, &result, &stored)
            .await
            .unwrap();
        let replay = record_remote_terminal_result(&runtime, &result, &stored)
            .await
            .unwrap();
        assert_eq!(first, replay);
        let loaded = load_recorded_terminal_result(&runtime, &blobs, &body.envelope_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.result_id, result.result_id);
        assert_eq!(
            decode_remote_terminal_result(&blobs, &loaded)
                .await
                .unwrap()
                .error_message,
            body.error_message
        );
    }

    #[tokio::test]
    async fn signed_handoff_reconstructs_on_replacement_runtime_and_returns_terminal_result() {
        let bus = InMemoryFederatedBus::new();
        bus.ensure_runtime("origin-runtime").unwrap();
        bus.ensure_runtime("destination-runtime").unwrap();
        bus.register_verification_key(TEST_KEY_ID, TEST_KEY.to_vec())
            .unwrap();

        let source_runtime = RuntimeComposition::InMemory(InMemoryRuntime::new());
        let source_blobs = InMemoryBlobTransfer::new();
        let source_payload = payload("phase-6-remote");
        let payload_descriptor =
            stage_remote_work_environment_payload(&source_blobs, &source_payload)
                .await
                .unwrap();
        let mut envelope = build_remote_work_environment_envelope(
            "phase-6-envelope",
            payload_descriptor.clone(),
            "phase-6-idempotency",
            "phase-6-correlation",
            "phase-6-causation",
            Utc::now() + Duration::minutes(5),
            origin(),
            terminal_endpoint(),
            source_payload.spec.placement_constraints(),
        )
        .unwrap();
        sign_remote_job_envelope(&mut envelope, TEST_KEY_ID, TEST_KEY).unwrap();
        bus.delivery_port("destination-runtime")
            .submit_remote_job(envelope.clone())
            .await
            .unwrap();
        let accepted_envelope = bus
            .inbox("destination-runtime")
            .unwrap()
            .remote_jobs
            .into_iter()
            .next()
            .unwrap();

        // The first destination admits the signed job, then disappears before
        // execution. No shared database or blob store is used by its replacement.
        let abandoned_runtime = RuntimeComposition::InMemory(InMemoryRuntime::new());
        let abandoned_blobs = InMemoryBlobTransfer::new();
        transfer_blob(&source_blobs, &abandoned_blobs, &payload_descriptor)
            .await
            .unwrap();
        let abandoned_job_id = accept_remote_work_environment_job(
            &abandoned_runtime,
            &abandoned_blobs,
            &accepted_envelope,
        )
        .await
        .unwrap();

        let destination_runtime = InMemoryRuntime::new();
        let destination_composition = RuntimeComposition::InMemory(destination_runtime.clone());
        let destination_blobs = InMemoryBlobTransfer::new();
        transfer_blob(&source_blobs, &destination_blobs, &payload_descriptor)
            .await
            .unwrap();
        let destination_job_id = accept_remote_work_environment_job(
            &destination_composition,
            &destination_blobs,
            &accepted_envelope,
        )
        .await
        .unwrap();
        assert_eq!(destination_job_id, abandoned_job_id);
        register_federated_work_environment_job_handlers(
            &destination_composition,
            Arc::new(InMemoryWorkEnvironmentPort::new()),
            WorkEnvironmentFederationServices {
                blobs: Arc::new(destination_blobs.clone()),
                terminal_delivery: Arc::new(HmacTerminalDelivery {
                    delivery: Arc::new(bus.delivery_port("origin-runtime")),
                }),
            },
        )
        .await
        .unwrap();

        let parent = process_until_terminal(&destination_runtime, &destination_job_id).await;
        assert_eq!(parent.state, JobState::Succeeded);
        assert!(
            source_runtime
                .get_job(&destination_job_id)
                .await
                .unwrap()
                .is_none(),
            "origin and destination must not share a Stasis job store"
        );

        let terminal_job_id = format!("{destination_job_id}:federated-terminal");
        let terminal_job = process_until_terminal(&destination_runtime, &terminal_job_id).await;
        assert_eq!(
            terminal_job.job_type,
            WORK_ENVIRONMENT_TERMINAL_DELIVERY_JOB_TYPE
        );
        assert_eq!(terminal_job.state, JobState::Succeeded);

        let inbox = bus.inbox("origin-runtime").unwrap();
        assert_eq!(inbox.terminal_results.len(), 1);
        let terminal_result = inbox.terminal_results[0].clone();
        assert_eq!(terminal_result.output_provenance, parent.output_provenance);
        transfer_blob(
            &destination_blobs,
            &source_blobs,
            terminal_result.output.as_ref().unwrap(),
        )
        .await
        .unwrap();
        let decoded = decode_remote_terminal_result(&source_blobs, &terminal_result)
            .await
            .unwrap();
        assert!(decoded.succeeded);
        assert_eq!(decoded.remote_job_id, destination_job_id);
        assert!(decoded.checkpoint.is_some());
        assert_eq!(decoded.execution_result.unwrap().exit_code, Some(0));

        // Admission replay and terminal-result replay both retain one identity.
        assert_eq!(
            accept_remote_work_environment_job(
                &destination_composition,
                &destination_blobs,
                &accepted_envelope,
            )
            .await
            .unwrap(),
            destination_job_id
        );
        bus.delivery_port("origin-runtime")
            .deliver_terminal_result(terminal_result)
            .await
            .unwrap();
        assert_eq!(
            bus.inbox("origin-runtime").unwrap().terminal_results.len(),
            1
        );
    }

    #[tokio::test]
    async fn checkpoint_transfer_moves_the_complete_reconstruction_graph() {
        let source = InMemoryBlobTransfer::new();
        let destination = InMemoryBlobTransfer::new();
        let source_bundle = source
            .put(b"git bundle bytes", Some("application/x-git-bundle"))
            .await
            .unwrap();
        let artifact_blob = source
            .put(b"test report", Some("text/plain"))
            .await
            .unwrap();
        let manifest = WorkEnvironmentCheckpointManifest {
            schema_version: WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION,
            environment_id: WorkEnvironmentId::parse("phase-6-checkpoint").unwrap(),
            workspace_id: WorkspaceId::parse("federated-workload").unwrap(),
            base_commit: "a".repeat(40),
            checkpoint_commit: "b".repeat(40),
            source_bundle: source_bundle.clone(),
            artifacts: vec![WorkEnvironmentArtifact {
                path: "reports/test.txt".to_string(),
                blob: artifact_blob.clone(),
            }],
            fence: WorkEnvironmentFence {
                stasis_attempt: stasis::domain::runtime::resource_lease::FencingToken(1),
                forge_environment_generation: None,
                forge_execution_generation: None,
            },
            label: Some("phase-6".to_string()),
            created_at: Utc::now(),
        };
        let manifest_blob = source
            .put(
                &serde_json::to_vec(&manifest).unwrap(),
                Some("application/vnd.medousa.work-environment-checkpoint+json"),
            )
            .await
            .unwrap();
        let checkpoint = WorkEnvironmentCheckpoint::from_manifest(manifest_blob.clone());

        transfer_checkpoint_graph(&source, &destination, &checkpoint)
            .await
            .unwrap();

        assert!(destination.exists(&manifest_blob).await.unwrap());
        assert!(destination.exists(&source_bundle).await.unwrap());
        assert!(destination.exists(&artifact_blob).await.unwrap());
    }
}
