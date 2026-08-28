//! Explicit daemon-to-daemon delegation over Stasis waitable turns.
//!
//! The binding is a disabled-or-enabled Stasis delivery endpoint. Stasis owns
//! job, retry, wait, and correlation lifecycle; Medousa binds exact workshop
//! identity, bounded transcript context, and signed transport provenance.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use medousa_types::session::{AuthorityId, ConversationTurn, TranscriptEntryId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::domain::agent::envelope::{
    AGENT_ENVELOPE_SCHEMA_VERSION_V1, AgentEnvelope, AgentEnvelopeKind,
};
use stasis::domain::agent::turn_wait::{TurnWaitRecord, TurnWaitStatus};
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::domain::runtime::delivery_endpoint::{
    DeliveryEndpoint, DeliveryProtocol, NewDeliveryEndpoint,
};
use stasis::domain::runtime::durable_wait::{DurableWaitRecord, DurableWaitStatus};
use stasis::domain::runtime::job::{BackoffPolicy, Job, JobState, NewJob};
use stasis::infrastructure::agent::{InMemoryAgentEventIngress, WaitCorrelatingAgentEventIngress};
use stasis::ports::outbound::agent::{AgentEventIngress, IngressDisposition, TurnWaitStore};
use stasis::ports::outbound::runtime::delivery_endpoint_store::DeliveryEndpointStore;
use stasis::ports::outbound::runtime::durable_wait_store::DurableWaitStore;
use stasis::prelude::{RuntimeComposition, RuntimeFactory};

use crate::daemon_runtime_handlers::DaemonRuntimeRegistrar;
use crate::delegated_task::{
    DelegatedTaskObservation, DelegatedTaskRequest, DelegatedTaskTransport,
    build_bounded_context_grant, source_execution_from_grant, validate_task_observation,
};
use crate::execution_context::active_turn_execution_context;
use crate::runtime_composition_ext::{RuntimeCompositionExt, process_once};
use crate::session_store::{SessionStore, TranscriptAppend};

pub const DELEGATION_ENDPOINT_ID: &str = "stasisd:endpoint:medousa-delegation";
const DELEGATION_TIMEOUT_SECONDS: u64 = 120;
const DELEGATION_JOB_PREFIX: &str = "delegation-job-";
const DELEGATION_TURN_PREFIX: &str = "delegation-turn-";
const DELEGATION_WAIT_SIGNAL_TYPE: &str = "medousa.delegated_turn";
const DELEGATION_JOB_TYPE: &str = "workflow.medousa.delegation";

/// Maps Stasis' agent-turn wait contract onto its existing runtime-owned
/// durable wait store. Medousa owns only the record shape and identity mapping.
struct RuntimeDelegationWaitStore {
    waits: Arc<dyn DurableWaitStore>,
}

impl RuntimeDelegationWaitStore {
    fn new(runtime: &RuntimeComposition) -> Self {
        let waits = match runtime {
            RuntimeComposition::InMemory(runtime) => Arc::new(runtime.wait_store.clone()) as _,
            RuntimeComposition::Surreal(runtime) => Arc::new(runtime.wait_store.clone()) as _,
        };
        Self { waits }
    }

    fn job_id_for_turn(turn_id: &str) -> StasisResult<String> {
        let identity = turn_id
            .strip_prefix(DELEGATION_TURN_PREFIX)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(format!(
                    "delegation wait has invalid turn identity: {turn_id}"
                ))
            })?;
        Ok(format!("{DELEGATION_JOB_PREFIX}{identity}"))
    }

    fn turn_id_for_job(job_id: &str) -> StasisResult<String> {
        let identity = job_id
            .strip_prefix(DELEGATION_JOB_PREFIX)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(format!(
                    "delegation wait has invalid job identity: {job_id}"
                ))
            })?;
        Ok(format!("{DELEGATION_TURN_PREFIX}{identity}"))
    }

    fn decode(durable: DurableWaitRecord) -> StasisResult<TurnWaitRecord> {
        let mut record: TurnWaitRecord =
            serde_json::from_str(durable.signal_payload.as_deref().ok_or_else(|| {
                StasisError::PortFailure("delegation wait payload is missing".into())
            })?)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        if record.turn_id != durable.wait_id || record.job_id != durable.job_id {
            return Err(StasisError::PortFailure(
                "delegation wait identity does not match its durable record".to_string(),
            ));
        }
        match durable.status {
            DurableWaitStatus::Pending if record.status != TurnWaitStatus::Pending => {
                return Err(StasisError::PortFailure(
                    "pending delegation wait contains a terminal result".to_string(),
                ));
            }
            DurableWaitStatus::Signaled if record.status == TurnWaitStatus::Pending => {
                return Err(StasisError::PortFailure(
                    "completed delegation wait contains a pending result".to_string(),
                ));
            }
            DurableWaitStatus::TimedOut => record.status = TurnWaitStatus::TimedOut,
            DurableWaitStatus::Cancelled => record.status = TurnWaitStatus::Cancelled,
            DurableWaitStatus::Pending | DurableWaitStatus::Signaled => {}
        }
        Ok(record)
    }
}

#[async_trait::async_trait]
impl TurnWaitStore for RuntimeDelegationWaitStore {
    async fn insert(&self, record: TurnWaitRecord) -> StasisResult<()> {
        if Self::job_id_for_turn(&record.turn_id)? != record.job_id {
            return Err(StasisError::PortFailure(
                "delegation wait turn and job identities do not match".to_string(),
            ));
        }
        let payload = serde_json::to_string(&record)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        self.waits
            .insert_wait(DurableWaitRecord {
                wait_id: record.turn_id.clone(),
                job_id: record.job_id,
                signal_type: DELEGATION_WAIT_SIGNAL_TYPE.to_string(),
                correlation_key: record.turn_id,
                status: DurableWaitStatus::Pending,
                deadline_at: Some(record.deadline_at),
                created_at: record.created_at,
                updated_at: record.updated_at,
                signal_payload: Some(payload),
                consumed_signal_ids: Vec::new(),
            })
            .await
    }

    async fn get(&self, turn_id: &str) -> StasisResult<Option<TurnWaitRecord>> {
        self.waits
            .get_wait(turn_id)
            .await?
            .map(Self::decode)
            .transpose()
    }

    async fn get_by_job_id(&self, job_id: &str) -> StasisResult<Option<TurnWaitRecord>> {
        self.get(&Self::turn_id_for_job(job_id)?).await
    }

    async fn complete(
        &self,
        turn_id: &str,
        status: TurnWaitStatus,
        result_payload: Option<Value>,
        error_message: Option<String>,
        updated_at: DateTime<Utc>,
    ) -> StasisResult<bool> {
        if status == TurnWaitStatus::Pending {
            return Err(StasisError::PortFailure(
                "cannot complete turn wait as pending".to_string(),
            ));
        }
        let Some(mut record) = self.get(turn_id).await? else {
            return Ok(false);
        };
        if record.status != TurnWaitStatus::Pending {
            return Ok(true);
        }
        let durable_status = match status {
            TurnWaitStatus::Completed | TurnWaitStatus::Failed => DurableWaitStatus::Signaled,
            TurnWaitStatus::Cancelled => DurableWaitStatus::Cancelled,
            TurnWaitStatus::TimedOut => DurableWaitStatus::TimedOut,
            TurnWaitStatus::Pending => unreachable!(),
        };
        record.status = status;
        record.result_payload = result_payload;
        record.error_message = error_message;
        record.updated_at = updated_at;
        let payload = serde_json::to_string(&record)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        self.waits
            .complete_wait(turn_id, durable_status, Some(payload), None, updated_at)
            .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DelegationTarget {
    /// Host-owned opaque route key, normally the exact Home workshop id.
    pub route_ref: String,
    /// Paired daemon identity pinned during pairing.
    pub peer_device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl DelegationTarget {
    pub fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("route_ref", self.route_ref.trim()),
            ("peer_device_id", self.peer_device_id.trim()),
        ] {
            if value.is_empty() {
                bail!("delegation target {name} is required");
            }
            if value.chars().count() > 256 {
                bail!("delegation target {name} exceeds 256 characters");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DelegationBinding {
    pub target: DelegationTarget,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

fn binding_from_endpoint(endpoint: DeliveryEndpoint) -> Result<Option<DelegationBinding>> {
    if !endpoint.enabled {
        return Ok(None);
    }
    let target = endpoint
        .metadata
        .as_deref()
        .ok_or_else(|| anyhow!("delegation endpoint is missing target metadata"))
        .and_then(|raw| {
            serde_json::from_str::<DelegationTarget>(raw).map_err(anyhow::Error::new)
        })?;
    target.validate()?;
    Ok(Some(DelegationBinding {
        target,
        created_at: endpoint.created_at,
        updated_at: endpoint.updated_at,
    }))
}

fn deterministic_identity(prefix: &str, chunks: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"medousa/delegation/stasis/v1\0");
    for chunk in chunks {
        digest.update((chunk.len() as u64).to_be_bytes());
        digest.update(chunk);
    }
    format!("{prefix}{}", &format!("{:x}", digest.finalize())[..32])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DelegationJobPayload {
    #[serde(default)]
    work_id: String,
    target: DelegationTarget,
    request: DelegatedTaskRequest,
    #[serde(default)]
    intent: String,
    #[serde(default)]
    user_ack: String,
    deadline_at: DateTime<Utc>,
    poll_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationTicket {
    pub work_id: String,
    pub job_id: String,
    pub turn_id: String,
    pub status: &'static str,
}

#[derive(Debug, Clone)]
pub struct DelegationCompletionEvent {
    pub source_turn_id: String,
    pub work_id: String,
    pub status: TurnWaitStatus,
    pub text: String,
    pub tool_names: Vec<String>,
}

#[async_trait::async_trait]
pub trait DelegationCompletionSink: Send + Sync {
    async fn deliver(&self, event: DelegationCompletionEvent) -> Result<()>;
}

struct DelegationResultDelivery {
    session_store: Arc<dyn SessionStore>,
    live_sink: RwLock<Option<Arc<dyn DelegationCompletionSink>>>,
}

#[async_trait::async_trait]
trait DelegationTerminalDelivery: Send + Sync {
    async fn deliver_terminal(
        &self,
        payload: &DelegationJobPayload,
        result: &crate::delegated_task::DelegatedTaskResult,
    ) -> StasisResult<bool>;

    async fn deliver_local_terminal(
        &self,
        payload: &DelegationJobPayload,
        status: TurnWaitStatus,
        message: String,
    ) -> StasisResult<bool>;
}

impl DelegationResultDelivery {
    fn new(session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            session_store,
            live_sink: RwLock::new(None),
        }
    }

    fn set_live_sink(&self, sink: Arc<dyn DelegationCompletionSink>) {
        *self.live_sink.write().expect("delegation live sink") = Some(sink);
    }

    async fn deliver_cancelled(&self, payload: &DelegationJobPayload) -> StasisResult<bool> {
        self.commit_presentation(
            payload,
            payload.request.source_execution.clone(),
            TurnWaitStatus::Cancelled,
            "Remote delegation was cancelled on this workshop.".to_string(),
            Vec::new(),
        )
        .await
    }

    async fn commit_presentation(
        &self,
        payload: &DelegationJobPayload,
        caused_by: medousa_types::session::ExecutionRef,
        status: TurnWaitStatus,
        text: String,
        tool_names: Vec<String>,
    ) -> StasisResult<bool> {
        let source = &payload.request.source_execution;
        let entries = self
            .session_store
            .load_transcript_entries(&source.session_id);
        let parent_receipt_committed = entries.iter().any(|entry| {
            entry.turn.role == "assistant" && entry.caused_by.as_ref() == Some(source)
        });
        if !parent_receipt_committed {
            return Ok(false);
        }

        let entry_id = TranscriptEntryId::parse(deterministic_identity(
            "ent_",
            &[
                b"delegated-result",
                payload
                    .request
                    .grant
                    .turn_id
                    .as_deref()
                    .unwrap_or_default()
                    .as_bytes(),
            ],
        ))
        .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        if entries.iter().any(|entry| entry.entry_id == entry_id) {
            return Ok(true);
        }

        let turn = ConversationTurn::plain(
            "assistant",
            text.clone(),
            Utc::now(),
            tool_names.clone(),
            Some("worker_synthesis".to_string()),
        );
        self.session_store
            .append_transcript_batch(
                &source.session_id,
                &[TranscriptAppend {
                    turn,
                    caused_by: Some(caused_by),
                    existing_entry_id: Some(entry_id),
                    source: None,
                    expected_digest: None,
                }],
            )
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;

        let sink = self.live_sink.read().expect("delegation live sink").clone();
        if let Some(sink) = sink {
            let event = DelegationCompletionEvent {
                source_turn_id: source.execution_id.to_string(),
                work_id: payload.work_id.clone(),
                status,
                text,
                tool_names,
            };
            if let Err(error) = sink.deliver(event).await {
                tracing::warn!(error = %error, "delegated result committed without live delivery");
            }
        }
        Ok(true)
    }
}

#[async_trait::async_trait]
impl DelegationTerminalDelivery for DelegationResultDelivery {
    async fn deliver_terminal(
        &self,
        payload: &DelegationJobPayload,
        result: &crate::delegated_task::DelegatedTaskResult,
    ) -> StasisResult<bool> {
        let (status, text, tool_names) = terminal_presentation(result);
        self.commit_presentation(payload, result.execution.clone(), status, text, tool_names)
            .await
    }

    async fn deliver_local_terminal(
        &self,
        payload: &DelegationJobPayload,
        status: TurnWaitStatus,
        message: String,
    ) -> StasisResult<bool> {
        self.commit_presentation(
            payload,
            payload.request.source_execution.clone(),
            status,
            message,
            Vec::new(),
        )
        .await
    }
}

fn terminal_presentation(
    result: &crate::delegated_task::DelegatedTaskResult,
) -> (TurnWaitStatus, String, Vec<String>) {
    let payload = &result.terminal.payload;
    match result.terminal.kind {
        AgentEnvelopeKind::TurnCompleted => (
            TurnWaitStatus::Completed,
            payload
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("Remote worker completed without a response.")
                .to_string(),
            payload
                .get("tool_names")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        ),
        AgentEnvelopeKind::Cancelled => (
            TurnWaitStatus::Cancelled,
            payload
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Remote worker was cancelled.")
                .to_string(),
            Vec::new(),
        ),
        AgentEnvelopeKind::Failed => (
            TurnWaitStatus::Failed,
            payload
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Remote worker failed.")
                .to_string(),
            Vec::new(),
        ),
        _ => unreachable!("validated delegated terminal kind"),
    }
}

struct DelegationJobHandler {
    host: Arc<dyn DelegatedTaskTransport>,
    ingress: Arc<dyn AgentEventIngress>,
    waits: Arc<dyn TurnWaitStore>,
    endpoints: Arc<dyn DeliveryEndpointStore>,
    delivery: Arc<dyn DelegationTerminalDelivery>,
}

impl DelegationJobHandler {
    fn parse(job: &Job) -> StasisResult<DelegationJobPayload> {
        let mut payload: DelegationJobPayload = serde_json::from_str(&job.payload_ref)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        if payload.work_id.trim().is_empty() {
            let identity = job
                .id
                .strip_prefix(DELEGATION_JOB_PREFIX)
                .unwrap_or(job.id.as_str());
            payload.work_id = format!("work-delegation-{identity}");
        }
        payload
            .target
            .validate()
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        crate::delegated_task::validate_task_request(&payload.request)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        if payload.poll_interval_seconds == 0 {
            return Err(StasisError::PortFailure(
                "delegation poll interval must be at least one second".to_string(),
            ));
        }
        let grant = &payload.request.grant;
        if grant.job_id.as_deref() != Some(job.id.as_str())
            || grant.correlation_id != job.correlation_id
            || grant.causation_id != job.causation_id
        {
            return Err(StasisError::PortFailure(
                "delegation job identity does not match its Stasis grant".to_string(),
            ));
        }
        Ok(payload)
    }

    fn deferred(
        payload: &DelegationJobPayload,
        message: impl Into<String>,
        observation: Option<&DelegatedTaskObservation>,
    ) -> JobExecutionOutcome {
        JobExecutionOutcome::Deferred {
            scheduled_at: Utc::now()
                + chrono::Duration::seconds(payload.poll_interval_seconds.max(1) as i64),
            message: message.into(),
            execution_id: payload.request.grant.turn_id.clone(),
            diagnostics: Some(
                json!({
                    "provider": "medousa-delegation",
                    "status": "deferred",
                    "wait_status": "pending",
                    "turn_id": payload.request.grant.turn_id,
                    "remote_work_id": observation.map(|value| value.work_id.as_str()),
                    "remote_status": observation.map(|value| value.status),
                })
                .to_string(),
            ),
        }
    }

    fn terminal_outcome(job: &Job, record: TurnWaitRecord) -> JobExecutionOutcome {
        match record.status {
            TurnWaitStatus::Completed => JobExecutionOutcome::Success {
                sttp_output_node_id: format!("sttp:medousa-delegation:{}", job.id),
                execution_id: Some(record.turn_id),
                diagnostics: Some(
                    json!({
                        "provider": "medousa-delegation",
                        "status": "success",
                        "wait_status": "completed",
                        "result": record.result_payload,
                    })
                    .to_string(),
                ),
            },
            TurnWaitStatus::Failed | TurnWaitStatus::Cancelled | TurnWaitStatus::TimedOut => {
                JobExecutionOutcome::FatalFailure {
                    message: record
                        .error_message
                        .unwrap_or_else(|| format!("delegated turn ended as {:?}", record.status)),
                    execution_id: Some(record.turn_id),
                    diagnostics: Some(
                        json!({
                            "provider": "medousa-delegation",
                            "status": "failure",
                            "wait_status": format!("{:?}", record.status).to_ascii_lowercase(),
                        })
                        .to_string(),
                    ),
                }
            }
            TurnWaitStatus::Pending => unreachable!("pending wait is not terminal"),
        }
    }

    async fn complete_failure(
        &self,
        job: &Job,
        payload: &DelegationJobPayload,
        turn_id: &str,
        status: TurnWaitStatus,
        message: impl Into<String>,
    ) -> StasisResult<JobExecutionOutcome> {
        let message = message.into().chars().take(2_048).collect::<String>();
        if !self
            .delivery
            .deliver_local_terminal(payload, status.clone(), message.clone())
            .await?
        {
            return Ok(Self::deferred(
                payload,
                "source turn handoff receipt is not committed yet",
                None,
            ));
        }
        self.waits
            .complete(turn_id, status, None, Some(message.clone()), Utc::now())
            .await?;
        let record = self.waits.get(turn_id).await?.ok_or_else(|| {
            StasisError::PortFailure("delegation wait disappeared during completion".to_string())
        })?;
        let mut outcome = Self::terminal_outcome(job, record);
        if let JobExecutionOutcome::FatalFailure {
            message: outcome_message,
            ..
        } = &mut outcome
        {
            *outcome_message = message;
        }
        Ok(outcome)
    }
}

#[async_trait::async_trait]
impl JobHandler for DelegationJobHandler {
    fn job_type(&self) -> &'static str {
        DELEGATION_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> StasisResult<JobExecutionOutcome> {
        let payload = Self::parse(job)?;
        let turn_id = payload
            .request
            .grant
            .turn_id
            .as_deref()
            .expect("validated delegated turn id");
        let now = Utc::now();
        let existing = self.waits.get(turn_id).await?;
        if let Some(record) = existing.as_ref()
            && record.status != TurnWaitStatus::Pending
        {
            return Ok(Self::terminal_outcome(job, record.clone()));
        }
        if existing.is_none() {
            self.waits
                .insert(TurnWaitRecord {
                    turn_id: turn_id.to_string(),
                    job_id: job.id.clone(),
                    session_id: payload.request.grant.session_id.clone(),
                    correlation_id: job.correlation_id.clone(),
                    participant_id: payload
                        .request
                        .grant
                        .participant_id
                        .clone()
                        .unwrap_or_else(|| "paired-medousa-daemon".to_string()),
                    status: TurnWaitStatus::Pending,
                    deadline_at: payload.deadline_at,
                    created_at: payload.request.grant.occurred_at,
                    updated_at: now,
                    result_payload: None,
                    error_message: None,
                })
                .await?;
        }
        if now >= payload.deadline_at {
            return self
                .complete_failure(
                    job,
                    &payload,
                    turn_id,
                    TurnWaitStatus::TimedOut,
                    "delegated turn did not reach a terminal state before its deadline",
                )
                .await;
        }

        let binding = self
            .endpoints
            .get(DELEGATION_ENDPOINT_ID)
            .await?
            .map(binding_from_endpoint)
            .transpose()
            .map_err(|error| StasisError::PortFailure(error.to_string()))?
            .flatten();
        let still_bound = binding.as_ref().is_some_and(|value| {
            value.target.route_ref == payload.target.route_ref
                && value.target.peer_device_id == payload.target.peer_device_id
        });
        if !still_bound {
            return self
                .complete_failure(
                    job,
                    &payload,
                    turn_id,
                    TurnWaitStatus::Cancelled,
                    "delegation binding was revoked or changed while work was pending",
                )
                .await;
        }

        let observation = match self
            .host
            .submit_or_observe(&payload.target, payload.request.clone())
            .await
        {
            Ok(observation) => observation,
            Err(error) => {
                return Ok(Self::deferred(
                    &payload,
                    format!("delegation transport unavailable: {error}"),
                    None,
                ));
            }
        };
        if let Err(error) = validate_task_observation(&payload.request, &observation) {
            return self
                .complete_failure(
                    job,
                    &payload,
                    turn_id,
                    TurnWaitStatus::Failed,
                    format!("delegated observation was rejected: {error}"),
                )
                .await;
        }
        let Some(result) = observation.result.as_ref() else {
            return Ok(Self::deferred(
                &payload,
                "remote delegated worker is still active",
                Some(&observation),
            ));
        };
        if !self.delivery.deliver_terminal(&payload, result).await? {
            return Ok(Self::deferred(
                &payload,
                "source turn handoff receipt is not committed yet",
                Some(&observation),
            ));
        }
        let ack = self.ingress.accept(result.terminal.clone()).await?;
        if ack.disposition == IngressDisposition::Rejected {
            return self
                .complete_failure(
                    job,
                    &payload,
                    turn_id,
                    TurnWaitStatus::Failed,
                    ack.message
                        .unwrap_or_else(|| "delegated terminal ingress rejected".to_string()),
                )
                .await;
        }
        let record = self.waits.get(turn_id).await?.ok_or_else(|| {
            StasisError::PortFailure(
                "delegation wait disappeared after terminal ingress".to_string(),
            )
        })?;
        Ok(Self::terminal_outcome(job, record))
    }
}

pub struct DelegationService {
    runtime: Arc<RuntimeComposition>,
    authority_id: AuthorityId,
    session_store: Arc<dyn SessionStore>,
    endpoints: Arc<dyn DeliveryEndpointStore>,
    waits: Arc<dyn TurnWaitStore>,
    delivery: Arc<DelegationResultDelivery>,
    active_drivers: Mutex<HashSet<String>>,
}

impl DelegationService {
    pub async fn binding(&self) -> Result<Option<DelegationBinding>> {
        self.endpoints
            .get(DELEGATION_ENDPOINT_ID)
            .await
            .map_err(anyhow::Error::new)?
            .map(binding_from_endpoint)
            .transpose()
            .map(Option::flatten)
    }

    pub async fn bind(&self, target: DelegationTarget) -> Result<DelegationBinding> {
        target.validate()?;
        let now = Utc::now();
        let created_at = self
            .endpoints
            .get(DELEGATION_ENDPOINT_ID)
            .await
            .map_err(anyhow::Error::new)?
            .map(|endpoint| endpoint.created_at)
            .unwrap_or(now);
        let endpoint = self
            .endpoints
            .upsert(NewDeliveryEndpoint {
                endpoint_id: DELEGATION_ENDPOINT_ID.to_string(),
                name: "Explicit paired Medousa delegation".to_string(),
                protocol: DeliveryProtocol::HttpWebhook,
                target: format!("medousa-peer://{}", target.peer_device_id.trim()),
                metadata: Some(serde_json::to_string(&target)?),
                created_at,
            })
            .await
            .map_err(anyhow::Error::new)?;
        binding_from_endpoint(endpoint)?
            .ok_or_else(|| anyhow!("delegation binding stayed disabled"))
    }

    pub async fn clear(&self) -> Result<bool> {
        self.endpoints
            .set_enabled(DELEGATION_ENDPOINT_ID, false)
            .await
            .map_err(anyhow::Error::new)
    }

    pub fn set_completion_sink(&self, sink: Arc<dyn DelegationCompletionSink>) {
        self.delivery.set_live_sink(sink);
    }

    pub async fn submit(
        self: &Arc<Self>,
        task: &str,
        user_ack: &str,
        intent: &str,
    ) -> StasisResult<DelegationTicket> {
        let task = task.trim();
        if task.is_empty() {
            return Err(StasisError::PortFailure(
                "delegated task is required".to_string(),
            ));
        }
        let user_ack = user_ack.trim();
        if user_ack.is_empty() {
            return Err(StasisError::PortFailure(
                "delegated user acknowledgement is required".to_string(),
            ));
        }
        let binding = self
            .binding()
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?
            .ok_or_else(|| {
                StasisError::PortFailure("no delegation workshop is explicitly bound".to_string())
            })?;
        let execution = active_turn_execution_context().ok_or_else(|| {
            StasisError::PortFailure("delegation requires an admitted daemon turn".to_string())
        })?;
        let identity = deterministic_identity(
            "",
            &[
                execution.session_id().as_str().as_bytes(),
                execution.turn_id().as_bytes(),
                task.as_bytes(),
            ],
        );
        let job_id = format!("delegation-job-{identity}");
        let turn_id = format!("delegation-turn-{identity}");
        let work_id = format!("work-delegation-{identity}");
        if self.runtime.get_job(&job_id).await?.is_none() {
            let now = Utc::now();
            let deadline_at = now + chrono::Duration::seconds(DELEGATION_TIMEOUT_SECONDS as i64);
            let grant = AgentEnvelope {
                schema_version: AGENT_ENVELOPE_SCHEMA_VERSION_V1,
                kind: AgentEnvelopeKind::TurnGranted,
                envelope_id: format!("grant-{turn_id}"),
                session_id: execution.session_id().to_string(),
                thread_id: Some(execution.correlation_id().to_string()),
                turn_id: Some(turn_id.clone()),
                job_id: Some(job_id.clone()),
                correlation_id: execution.correlation_id().to_string(),
                causation_id: execution.turn_id().to_string(),
                participant_id: Some("paired-medousa-daemon".to_string()),
                occurred_at: now,
                payload: json!({
                    "user_prompt": task,
                    "system_prompt": null,
                    "deadline_at": deadline_at,
                }),
            };
            let context = build_bounded_context_grant(
                self.session_store.as_ref(),
                &self.authority_id,
                execution.session_id(),
                &format!("daemon:{}", self.authority_id),
                &turn_id,
                now,
            )
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
            let source_execution = source_execution_from_grant(&self.authority_id, &grant)
                .map_err(|error| StasisError::PortFailure(error.to_string()))?;
            let payload = DelegationJobPayload {
                work_id: work_id.clone(),
                target: binding.target,
                request: DelegatedTaskRequest {
                    schema_version: crate::delegated_task::DELEGATED_TASK_SCHEMA_VERSION,
                    grant,
                    source_execution,
                    context,
                },
                intent: intent.trim().to_string(),
                user_ack: user_ack.to_string(),
                deadline_at,
                poll_interval_seconds: 1,
            };
            self.runtime
                .enqueue_job(NewJob {
                    id: job_id.clone(),
                    queue: "default".to_string(),
                    job_type: DELEGATION_JOB_TYPE.to_string(),
                    payload_ref: serde_json::to_string(&payload)
                        .map_err(|error| StasisError::PortFailure(error.to_string()))?,
                    priority: 100,
                    max_attempts: 3,
                    idempotency_key: format!("delegation:{identity}"),
                    correlation_id: execution.correlation_id().to_string(),
                    causation_id: execution.turn_id().to_string(),
                    trace_id: execution.correlation_id().to_string(),
                    sttp_input_node_id: format!("sttp:in:medousa:{job_id}"),
                    scheduled_at: Utc::now(),
                    backoff_policy: BackoffPolicy::default(),
                })
                .await?;
        }
        self.start_driver(job_id.clone(), turn_id.clone());
        Ok(DelegationTicket {
            work_id,
            job_id,
            turn_id,
            status: "pending",
        })
    }

    pub async fn resume_pending(self: &Arc<Self>) -> StasisResult<usize> {
        let jobs = self.all_delegation_jobs().await?;
        let mut resumed = 0;
        for job in jobs.into_iter().filter(|job| {
            matches!(
                job.state,
                JobState::Enqueued | JobState::Leased | JobState::Running
            )
        }) {
            let payload = DelegationJobHandler::parse(&job)?;
            let Some(turn_id) = payload.request.grant.turn_id.clone() else {
                continue;
            };
            if self.start_driver(job.id, turn_id) {
                resumed += 1;
            }
        }
        Ok(resumed)
    }

    pub async fn status(
        &self,
        work_id: Option<&str>,
        session_id: Option<&str>,
    ) -> StasisResult<Value> {
        let execution = active_turn_execution_context().ok_or_else(|| {
            StasisError::PortFailure("workshop status requires an admitted daemon turn".to_string())
        })?;
        let active_session = execution.session_id().as_str();
        if session_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some_and(|requested| requested != active_session)
        {
            return Err(StasisError::PortFailure(
                "workshop status is scoped to the active host session".to_string(),
            ));
        }

        let requested_work = work_id.map(str::trim).filter(|value| !value.is_empty());
        let mut workers = Vec::new();
        for job in self.all_delegation_jobs().await? {
            let payload = DelegationJobHandler::parse(&job)?;
            if payload.request.source_execution.session_id.as_str() != active_session
                || requested_work.is_some_and(|requested| requested != payload.work_id)
            {
                continue;
            }
            let turn_id = payload.request.grant.turn_id.as_deref().unwrap_or_default();
            let wait = self.waits.get(turn_id).await?;
            let status = wait
                .as_ref()
                .map(|record| wait_status_name(record.status.clone()))
                .unwrap_or_else(|| job_status_name(&job.state));
            workers.push(json!({
                "work_id": payload.work_id,
                "stasis_job_id": job.id,
                "status": status,
                "intent": payload.intent,
                "task": payload.request.grant.payload.get("user_prompt"),
                "result": wait.as_ref().and_then(|record| record.result_payload.clone()),
                "error": wait.as_ref().and_then(|record| record.error_message.clone()),
            }));
        }
        if requested_work.is_some() && workers.is_empty() {
            return Err(StasisError::PortFailure(format!(
                "work_id not found in active session: {}",
                requested_work.unwrap_or_default()
            )));
        }
        Ok(json!({
            "ok": true,
            "execution_target": "bound_remote",
            "workers": workers,
        }))
    }

    pub async fn cancel(&self, work_id: &str) -> StasisResult<Value> {
        let work_id = work_id.trim();
        if work_id.is_empty() {
            return Err(StasisError::PortFailure("work_id is required".to_string()));
        }
        let execution = active_turn_execution_context().ok_or_else(|| {
            StasisError::PortFailure(
                "workshop cancellation requires an admitted daemon turn".to_string(),
            )
        })?;
        for mut job in self.all_delegation_jobs().await? {
            let payload = DelegationJobHandler::parse(&job)?;
            if payload.work_id != work_id
                || payload.request.source_execution.session_id != *execution.session_id()
            {
                continue;
            }
            if !self.delivery.deliver_cancelled(&payload).await? {
                return Err(StasisError::PortFailure(
                    "delegation handoff receipt is not committed yet".to_string(),
                ));
            }
            if let Some(turn_id) = payload.request.grant.turn_id.as_deref() {
                let _ = self
                    .waits
                    .complete(
                        turn_id,
                        TurnWaitStatus::Cancelled,
                        None,
                        Some("delegated work cancelled by the host".to_string()),
                        Utc::now(),
                    )
                    .await?;
            }
            job.state = JobState::Canceled;
            job.finished_at = Some(Utc::now());
            job.last_error = Some("cancelled by host".to_string());
            self.runtime.save_job(job).await?;
            return Ok(json!({
                "ok": true,
                "work_id": work_id,
                "status": "cancelled",
                "execution_target": "bound_remote",
            }));
        }
        Err(StasisError::PortFailure(format!(
            "work_id not found in active session: {work_id}"
        )))
    }

    async fn all_delegation_jobs(&self) -> StasisResult<Vec<Job>> {
        let mut jobs = Vec::new();
        for state in [
            JobState::Enqueued,
            JobState::Leased,
            JobState::Running,
            JobState::Succeeded,
            JobState::Failed,
            JobState::DeadLetter,
            JobState::Canceled,
        ] {
            jobs.extend(
                self.runtime
                    .list_jobs_by_state(state)
                    .await?
                    .into_iter()
                    .filter(|job| job.job_type == DELEGATION_JOB_TYPE),
            );
        }
        Ok(jobs)
    }

    fn start_driver(self: &Arc<Self>, job_id: String, turn_id: String) -> bool {
        {
            let mut active = self.active_drivers.lock().expect("delegation drivers");
            if !active.insert(job_id.clone()) {
                return false;
            }
        }
        let service = self.clone();
        tokio::spawn(async move {
            let worker_id = format!("delegation-driver-{turn_id}");
            loop {
                let terminal_wait = service
                    .waits
                    .get(&turn_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|record| record.status != TurnWaitStatus::Pending);
                let terminal_job = service
                    .runtime
                    .get_job(&job_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|job| {
                        matches!(
                            job.state,
                            JobState::Succeeded
                                | JobState::Failed
                                | JobState::DeadLetter
                                | JobState::Canceled
                        )
                    });
                if terminal_wait || terminal_job {
                    break;
                }
                if let Err(error) = process_once(service.runtime.as_ref(), &worker_id).await {
                    tracing::warn!(%job_id, %error, "delegation driver pass failed");
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            service
                .active_drivers
                .lock()
                .expect("delegation drivers")
                .remove(&job_id);
        });
        true
    }
}

fn wait_status_name(status: TurnWaitStatus) -> &'static str {
    match status {
        TurnWaitStatus::Pending => "pending",
        TurnWaitStatus::Completed => "completed",
        TurnWaitStatus::Failed => "failed",
        TurnWaitStatus::Cancelled => "cancelled",
        TurnWaitStatus::TimedOut => "timed_out",
    }
}

fn job_status_name(status: &JobState) -> &'static str {
    match status {
        JobState::Enqueued => "pending",
        JobState::Leased | JobState::Running => "running",
        JobState::Succeeded => "completed",
        JobState::Failed | JobState::DeadLetter => "failed",
        JobState::Canceled => "cancelled",
    }
}

pub fn install_delegation_runtime(
    runtime: Arc<RuntimeComposition>,
    authority_id: AuthorityId,
    session_store: Arc<dyn SessionStore>,
    host: Arc<dyn DelegatedTaskTransport>,
) -> Result<Arc<DelegationService>> {
    let waits: Arc<dyn TurnWaitStore> = Arc::new(RuntimeDelegationWaitStore::new(runtime.as_ref()));
    let delivery = Arc::new(DelegationResultDelivery::new(session_store.clone()));
    let raw_ingress: Arc<dyn AgentEventIngress> = Arc::new(InMemoryAgentEventIngress::new());
    let ingress: Arc<dyn AgentEventIngress> = Arc::new(WaitCorrelatingAgentEventIngress::new(
        raw_ingress,
        waits.clone(),
    ));
    let endpoints = RuntimeFactory::resolve_delivery_endpoint_store(runtime.as_ref(), None);
    let handler = DelegationJobHandler {
        host,
        ingress,
        waits: waits.clone(),
        endpoints: endpoints.clone(),
        delivery: delivery.clone(),
    };
    match runtime.as_ref() {
        RuntimeComposition::InMemory(inner) => inner.register_daemon_handler(handler)?,
        RuntimeComposition::Surreal(inner) => inner.register_daemon_handler(handler)?,
    }
    Ok(Arc::new(DelegationService {
        runtime,
        authority_id,
        session_store,
        endpoints,
        waits,
        delivery,
        active_drivers: Mutex::new(HashSet::new()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use medousa_types::session::{
        ContextManifest, ContextManifestId, ConversationRangeSelection, ConversationTurn,
        DerivationId, ExecutionId, ExecutionRef, ResolvedConversationRange, SessionDerivation,
        SessionHistorySummary, SessionId, SessionRef, TranscriptEntry, TranscriptEntryId,
        TranscriptEntryRef,
    };
    use stasis::application::runtime::in_memory_runtime::InMemoryRuntime;

    use crate::delegated_task::{
        DELEGATED_TASK_SCHEMA_VERSION, DelegatedContextEntry, DelegatedContextGrant,
        DelegatedTaskAdmission, DelegatedTaskError, DelegatedTaskResult, DelegatedTaskStatus,
    };
    use crate::session_store::{
        CommitDurability, CommitReceipt, DerivationCommitOutcome, DerivationCommitRequest,
        DerivationLookup, StoreError, TranscriptSearchMatch, transcript_content_digest,
    };

    #[derive(Default)]
    struct MemorySessionStore {
        entries: Mutex<std::collections::HashMap<SessionId, Vec<TranscriptEntry>>>,
    }

    impl MemorySessionStore {
        fn seed_parent_receipt(&self, request: &DelegatedTaskRequest) {
            let turn = ConversationTurn::plain(
                "assistant",
                "Working on it.".to_string(),
                Utc::now(),
                Vec::new(),
                Some("worker_handoff".to_string()),
            );
            self.entries.lock().expect("memory transcripts").insert(
                request.source_execution.session_id.clone(),
                vec![TranscriptEntry {
                    entry_id: TranscriptEntryId::parse("ent_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                        .expect("entry id"),
                    entry_seq: 1,
                    caused_by: Some(request.source_execution.clone()),
                    source: None,
                    content_digest: transcript_content_digest(&turn).expect("digest"),
                    turn,
                }],
            );
        }
    }

    #[async_trait::async_trait]
    impl SessionStore for MemorySessionStore {
        fn load_transcript_entries(&self, session_id: &SessionId) -> Vec<TranscriptEntry> {
            self.entries
                .lock()
                .expect("memory transcripts")
                .get(session_id)
                .cloned()
                .unwrap_or_default()
        }

        async fn append_transcript_batch(
            &self,
            session_id: &SessionId,
            appends: &[TranscriptAppend],
        ) -> Result<CommitReceipt, StoreError> {
            let mut stores = self.entries.lock().expect("memory transcripts");
            let entries = stores.entry(session_id.clone()).or_default();
            for append in appends {
                let entry_id = append.existing_entry_id.clone().ok_or_else(|| {
                    StoreError::InvalidInput("test append requires an entry id".to_string())
                })?;
                if entries.iter().any(|entry| entry.entry_id == entry_id) {
                    continue;
                }
                entries.push(TranscriptEntry {
                    entry_id,
                    entry_seq: entries.len() as u64 + 1,
                    caused_by: append.caused_by.clone(),
                    source: append.source.clone(),
                    content_digest: transcript_content_digest(&append.turn)?,
                    turn: append.turn.clone(),
                });
            }
            Ok(CommitReceipt {
                turns: appends.len(),
                bytes: 0,
                durability: CommitDurability::FilesystemWrite,
            })
        }

        async fn materialize_derivation(
            &self,
            _request: &DerivationCommitRequest,
        ) -> Result<DerivationCommitOutcome, StoreError> {
            Err(StoreError::Backend("unused in test".to_string()))
        }

        fn load_derivation(
            &self,
            _target_session_id: &SessionId,
        ) -> Result<Option<DerivationLookup>, StoreError> {
            Ok(None)
        }

        fn delete_session(&self, session_id: &SessionId) -> Result<(), String> {
            self.entries
                .lock()
                .expect("memory transcripts")
                .remove(session_id);
            Ok(())
        }

        fn list_history_sessions(&self, _limit: usize) -> Vec<SessionHistorySummary> {
            Vec::new()
        }

        fn search_transcripts(
            &self,
            _session_ids: &[String],
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<TranscriptSearchMatch>, StoreError> {
            Ok(Vec::new())
        }

        fn build_backfill_summaries(&self, _limit: usize) -> Vec<SessionHistorySummary> {
            Vec::new()
        }

        fn has_persisted_sessions(&self) -> bool {
            !self.entries.lock().expect("memory transcripts").is_empty()
        }
    }

    fn request_for(job_id: &str, turn_id: &str) -> DelegatedTaskRequest {
        let source_authority =
            AuthorityId::parse(format!("auth_{}", "a".repeat(64))).expect("source authority");
        let source_session = SessionId::parse("ses_source").expect("source session");
        let source = SessionRef {
            authority_id: source_authority.clone(),
            session_id: source_session.clone(),
        };
        let turn = ConversationTurn {
            role: "user".to_string(),
            content: "bounded context".to_string(),
            timestamp: Utc::now(),
            tool_names: Vec::new(),
            answer_state: None,
            parts: None,
            slice_summary: None,
            speaker_profile_id: None,
        };
        let entry = DelegatedContextEntry {
            source: TranscriptEntryRef {
                session: source.clone(),
                entry_id: TranscriptEntryId::parse("ent_0123456789abcdef0123456789abcdef")
                    .expect("entry id"),
                entry_seq: 1,
            },
            caused_by: None,
            content_digest: transcript_content_digest(&turn).expect("content digest"),
            turn,
        };
        let mut digest = Sha256::new();
        digest.update(b"medousa/conversation-range/v1\0");
        digest.update(source.authority_id.as_str().as_bytes());
        digest.update(source.session_id.as_str().as_bytes());
        digest.update(entry.source.entry_seq.to_be_bytes());
        digest.update(entry.source.entry_id.as_str().as_bytes());
        digest.update(entry.content_digest.as_bytes());
        let context = DelegatedContextGrant {
            manifest: ContextManifest {
                manifest_id: ContextManifestId::parse("ctx_0123456789abcdef0123456789abcdef")
                    .expect("manifest id"),
                sources: vec![ResolvedConversationRange {
                    selection: ConversationRangeSelection {
                        session: source.clone(),
                        after_entry_seq: None,
                        through_entry_seq: 1,
                    },
                    selection_digest: format!("sha256:{:x}", digest.finalize()),
                }],
                created_by: "daemon:test".to_string(),
                created_at: Utc::now(),
            },
            entries: vec![entry],
        };
        DelegatedTaskRequest {
            schema_version: DELEGATED_TASK_SCHEMA_VERSION,
            grant: AgentEnvelope {
                schema_version: AGENT_ENVELOPE_SCHEMA_VERSION_V1,
                kind: AgentEnvelopeKind::TurnGranted,
                envelope_id: format!("grant-{turn_id}"),
                session_id: source_session.to_string(),
                thread_id: Some("thread-1".to_string()),
                turn_id: Some(turn_id.to_string()),
                job_id: Some(job_id.to_string()),
                correlation_id: "corr-1".to_string(),
                causation_id: "source-exec-1".to_string(),
                participant_id: Some("paired-medousa-daemon".to_string()),
                occurred_at: Utc::now(),
                payload: json!({ "user_prompt": "do the heavy work" }),
            },
            source_execution: ExecutionRef {
                authority_id: source_authority,
                session_id: source_session,
                execution_id: ExecutionId::parse("source-exec-1").expect("source execution"),
            },
            context,
        }
    }

    fn terminal_observation(request: &DelegatedTaskRequest) -> DelegatedTaskObservation {
        let authority =
            AuthorityId::parse(format!("auth_{}", "b".repeat(64))).expect("remote authority");
        let session_id = SessionId::parse("ses_remote").expect("remote session");
        let execution = ExecutionRef {
            authority_id: authority.clone(),
            session_id: session_id.clone(),
            execution_id: ExecutionId::parse("work-remote").expect("remote execution"),
        };
        let derivation = SessionDerivation {
            derivation_id: DerivationId::parse(format!("drv_{}", "b".repeat(32)))
                .expect("derivation id"),
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
        let result = DelegatedTaskResult {
            schema_version: DELEGATED_TASK_SCHEMA_VERSION,
            terminal: AgentEnvelope {
                schema_version: AGENT_ENVELOPE_SCHEMA_VERSION_V1,
                kind: AgentEnvelopeKind::TurnCompleted,
                envelope_id: "result-work-remote".to_string(),
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
            derivation: derivation.clone(),
        };
        DelegatedTaskObservation {
            schema_version: DELEGATED_TASK_SCHEMA_VERSION,
            work_id: execution.execution_id.to_string(),
            admission: DelegatedTaskAdmission::Existing,
            status: DelegatedTaskStatus::Completed,
            execution,
            derivation,
            result: Some(result),
        }
    }

    #[tokio::test]
    async fn delegated_result_waits_for_handoff_and_commits_once() {
        let request = request_for("delegation-job-delivery", "delegation-turn-delivery");
        let observation = terminal_observation(&request);
        let result = observation.result.as_ref().expect("terminal result");
        let store = Arc::new(MemorySessionStore::default());
        let delivery = DelegationResultDelivery::new(store.clone());
        let payload = DelegationJobPayload {
            work_id: "work-delegation-delivery".to_string(),
            target: DelegationTarget {
                route_ref: "workshop-1".to_string(),
                peer_device_id: "remote-daemon".to_string(),
                label: None,
            },
            request: request.clone(),
            intent: "research".to_string(),
            user_ack: "Working on it.".to_string(),
            deadline_at: Utc::now() + chrono::Duration::seconds(30),
            poll_interval_seconds: 1,
        };

        assert!(!delivery.deliver_terminal(&payload, result).await.unwrap());
        store.seed_parent_receipt(&request);
        assert!(delivery.deliver_terminal(&payload, result).await.unwrap());
        assert!(delivery.deliver_terminal(&payload, result).await.unwrap());

        let entries = store.load_transcript_entries(&request.source_execution.session_id);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].turn.content, "done");
        assert_eq!(entries[1].caused_by, Some(result.execution.clone()));
    }

    struct RecoveringTransport {
        calls: AtomicUsize,
    }

    struct AcceptingDelivery;

    #[async_trait::async_trait]
    impl DelegationTerminalDelivery for AcceptingDelivery {
        async fn deliver_terminal(
            &self,
            _payload: &DelegationJobPayload,
            _result: &DelegatedTaskResult,
        ) -> StasisResult<bool> {
            Ok(true)
        }

        async fn deliver_local_terminal(
            &self,
            _payload: &DelegationJobPayload,
            _status: TurnWaitStatus,
            _message: String,
        ) -> StasisResult<bool> {
            Ok(true)
        }
    }

    #[async_trait::async_trait]
    impl DelegatedTaskTransport for RecoveringTransport {
        async fn submit_or_observe(
            &self,
            _target: &DelegationTarget,
            request: DelegatedTaskRequest,
        ) -> Result<DelegatedTaskObservation, DelegatedTaskError> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(DelegatedTaskError::transport("phone suspended"));
            }
            Ok(terminal_observation(&request))
        }
    }

    fn delegation_handler(
        runtime: &RuntimeComposition,
        host: Arc<dyn DelegatedTaskTransport>,
        endpoints: Arc<dyn DeliveryEndpointStore>,
    ) -> (DelegationJobHandler, Arc<dyn TurnWaitStore>) {
        let waits: Arc<dyn TurnWaitStore> = Arc::new(RuntimeDelegationWaitStore::new(runtime));
        let ingress: Arc<dyn AgentEventIngress> = Arc::new(WaitCorrelatingAgentEventIngress::new(
            Arc::new(InMemoryAgentEventIngress::new()),
            waits.clone(),
        ));
        (
            DelegationJobHandler {
                host,
                ingress,
                waits: waits.clone(),
                endpoints,
                delivery: Arc::new(AcceptingDelivery),
            },
            waits,
        )
    }

    #[tokio::test]
    async fn delegation_job_resumes_the_same_wait_after_transport_interruption() {
        let runtime = RuntimeComposition::InMemory(InMemoryRuntime::new());
        let endpoints = RuntimeFactory::resolve_delivery_endpoint_store(&runtime, None);
        let target = DelegationTarget {
            route_ref: "workshop-1".to_string(),
            peer_device_id: "remote-daemon".to_string(),
            label: None,
        };
        endpoints
            .upsert(NewDeliveryEndpoint {
                endpoint_id: DELEGATION_ENDPOINT_ID.to_string(),
                name: "test delegation".to_string(),
                protocol: DeliveryProtocol::HttpWebhook,
                target: "medousa-peer://remote-daemon".to_string(),
                metadata: Some(serde_json::to_string(&target).expect("target json")),
                created_at: Utc::now(),
            })
            .await
            .expect("bind delegation target");
        let request = request_for("delegation-job-recovery", "delegation-turn-recovery");
        let payload = DelegationJobPayload {
            work_id: "work-delegation-recovery".to_string(),
            target,
            intent: "research".to_string(),
            user_ack: "Working on it.".to_string(),
            deadline_at: Utc::now() + chrono::Duration::seconds(30),
            poll_interval_seconds: 1,
            request,
        };
        let job = NewJob {
            id: "delegation-job-recovery".to_string(),
            queue: "default".to_string(),
            job_type: DELEGATION_JOB_TYPE.to_string(),
            payload_ref: serde_json::to_string(&payload).expect("payload json"),
            priority: 100,
            max_attempts: 3,
            idempotency_key: "delegation:recovery".to_string(),
            correlation_id: "corr-1".to_string(),
            causation_id: "source-exec-1".to_string(),
            trace_id: "corr-1".to_string(),
            sttp_input_node_id: "sttp:in:recovery".to_string(),
            scheduled_at: Utc::now(),
            backoff_policy: BackoffPolicy::default(),
        }
        .into_job();
        let transport = Arc::new(RecoveringTransport {
            calls: AtomicUsize::new(0),
        });

        let (first_handler, first_waits) =
            delegation_handler(&runtime, transport.clone(), endpoints.clone());
        assert!(matches!(
            first_handler.execute(&job).await.expect("first pass"),
            JobExecutionOutcome::Deferred { .. }
        ));
        assert_eq!(
            first_waits
                .get("delegation-turn-recovery")
                .await
                .expect("read wait")
                .expect("durable wait")
                .status,
            TurnWaitStatus::Pending
        );

        let (rebuilt_handler, rebuilt_waits) =
            delegation_handler(&runtime, transport.clone(), endpoints);
        assert!(matches!(
            rebuilt_handler.execute(&job).await.expect("resumed pass"),
            JobExecutionOutcome::Success { .. }
        ));
        let completed = rebuilt_waits
            .get("delegation-turn-recovery")
            .await
            .expect("read completed wait")
            .expect("completed wait");
        assert_eq!(completed.status, TurnWaitStatus::Completed);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn delegation_wait_reuses_the_runtime_durable_store() {
        let runtime = RuntimeComposition::InMemory(InMemoryRuntime::new());
        let now = Utc::now();
        let pending = TurnWaitRecord {
            turn_id: "delegation-turn-ticket-1".to_string(),
            job_id: "delegation-job-ticket-1".to_string(),
            session_id: "session-1".to_string(),
            correlation_id: "correlation-1".to_string(),
            participant_id: "paired-medousa-daemon".to_string(),
            status: TurnWaitStatus::Pending,
            deadline_at: now + chrono::Duration::seconds(30),
            created_at: now,
            updated_at: now,
            result_payload: None,
            error_message: None,
        };

        RuntimeDelegationWaitStore::new(&runtime)
            .insert(pending)
            .await
            .expect("insert durable delegation wait");

        let rebuilt = RuntimeDelegationWaitStore::new(&runtime);
        assert_eq!(
            rebuilt
                .get_by_job_id("delegation-job-ticket-1")
                .await
                .expect("read delegation wait")
                .expect("delegation wait")
                .status,
            TurnWaitStatus::Pending
        );

        rebuilt
            .complete(
                "delegation-turn-ticket-1",
                TurnWaitStatus::Completed,
                Some(json!({ "answer": "done" })),
                None,
                Utc::now(),
            )
            .await
            .expect("complete delegation wait");
        let completed = rebuilt
            .get("delegation-turn-ticket-1")
            .await
            .expect("read completed wait")
            .expect("completed wait");
        assert_eq!(completed.status, TurnWaitStatus::Completed);
        assert_eq!(completed.result_payload, Some(json!({ "answer": "done" })));

        let durable = match &runtime {
            RuntimeComposition::InMemory(runtime) => runtime
                .wait_store
                .get_wait("delegation-turn-ticket-1")
                .await
                .expect("read canonical durable wait")
                .expect("canonical durable wait"),
            RuntimeComposition::Surreal(_) => unreachable!(),
        };
        assert_eq!(durable.status, DurableWaitStatus::Signaled);
    }
}
