//! Explicit daemon-to-daemon delegation over Stasis waitable turns.
//!
//! The binding is a disabled-or-enabled Stasis delivery endpoint. Stasis owns
//! job, retry, wait, and correlation lifecycle; Medousa binds exact workshop
//! identity, bounded transcript context, and signed transport provenance.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use chrono::Utc;
use medousa_types::session::AuthorityId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::application::orchestration::runtime_job_payloads::AgentTurnWaitableJobPayload;
use stasis::application::runtime::agent_turn_waitable_job_handler::AgentTurnWaitableJobHandler;
use stasis::domain::agent::envelope::{AgentEnvelope, AgentEnvelopeKind, EncodedAgentMessage};
use stasis::domain::agent::turn_wait::TurnWaitStatus;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::domain::runtime::delivery_endpoint::{
    DeliveryEndpoint, DeliveryProtocol, NewDeliveryEndpoint,
};
use stasis::domain::runtime::job::{BackoffPolicy, NewJob};
use stasis::infrastructure::agent::{
    InMemoryAgentEventIngress, InMemoryTurnWaitStore, JsonAgentMessageCodec,
    WaitCorrelatingAgentEventIngress,
};
use stasis::ports::outbound::agent::{
    AgentEventIngress, AgentMessageCodec, AgentTransport, IngressDisposition, TurnWaitStore,
};
use stasis::ports::outbound::runtime::delivery_endpoint_store::DeliveryEndpointStore;
use stasis::prelude::{RuntimeComposition, RuntimeFactory};

use crate::daemon_runtime_handlers::DaemonRuntimeRegistrar;
use crate::delegated_task::{
    DelegatedTaskRequest, DelegatedTaskTransport, build_bounded_context_grant,
    source_execution_from_grant, validate_task_result,
};
use crate::execution_context::active_turn_execution_context;
use crate::runtime_composition_ext::{RuntimeCompositionExt, process_once};
use crate::session_store::SessionStore;

pub const DELEGATION_ENDPOINT_ID: &str = "stasisd:endpoint:medousa-delegation";
pub const COGNITION_DELEGATE: &str = "cognition_delegate";
const DELEGATION_TIMEOUT_SECONDS: u64 = 120;

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

struct DelegationAgentTransport {
    authority_id: AuthorityId,
    host: Arc<dyn DelegatedTaskTransport>,
    session_store: Arc<dyn SessionStore>,
    codec: Arc<dyn AgentMessageCodec>,
    ingress: Arc<dyn AgentEventIngress>,
}

impl DelegationAgentTransport {
    async fn accept_failure(
        &self,
        grant: &AgentEnvelope,
        message: impl Into<String>,
    ) -> StasisResult<()> {
        let terminal = AgentEnvelope {
            schema_version: grant.schema_version,
            kind: AgentEnvelopeKind::Failed,
            envelope_id: format!(
                "transport-failure-{}",
                grant.turn_id.as_deref().unwrap_or("unknown")
            ),
            session_id: grant.session_id.clone(),
            thread_id: grant.thread_id.clone(),
            turn_id: grant.turn_id.clone(),
            job_id: grant.job_id.clone(),
            correlation_id: grant.correlation_id.clone(),
            causation_id: grant.envelope_id.clone(),
            participant_id: Some("medousa-delegation-transport".to_string()),
            occurred_at: Utc::now(),
            payload: json!({
                "error": message.into().chars().take(2_048).collect::<String>(),
                "stage": "daemon_to_daemon_transport",
            }),
        };
        let ack = self.ingress.accept(terminal).await?;
        if ack.disposition == IngressDisposition::Rejected {
            return Err(StasisError::PortFailure(ack.message.unwrap_or_else(|| {
                "delegated failure ingress rejected".to_string()
            })));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl AgentTransport for DelegationAgentTransport {
    fn supports(&self, protocol: &DeliveryProtocol) -> bool {
        protocol == &DeliveryProtocol::HttpWebhook
    }

    async fn publish(
        &self,
        endpoint: &DeliveryEndpoint,
        message: &EncodedAgentMessage,
    ) -> StasisResult<()> {
        if endpoint.endpoint_id != DELEGATION_ENDPOINT_ID || !endpoint.enabled {
            return Err(StasisError::PortFailure(
                "delegation endpoint is not bound".to_string(),
            ));
        }
        let binding = binding_from_endpoint(endpoint.clone())
            .map_err(|error| StasisError::PortFailure(error.to_string()))?
            .ok_or_else(|| {
                StasisError::PortFailure("delegation endpoint is disabled".to_string())
            })?;
        let grant: AgentEnvelope = self.codec.decode(message)?;
        let session_id = crate::session_storage::SessionId::parse(&grant.session_id)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let turn_id = grant.turn_id.as_deref().ok_or_else(|| {
            StasisError::PortFailure("delegated grant turn_id is missing".to_string())
        })?;
        let context = build_bounded_context_grant(
            self.session_store.as_ref(),
            &self.authority_id,
            &session_id,
            &format!("daemon:{}", self.authority_id),
            turn_id,
            grant.occurred_at,
        )
        .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let source_execution = source_execution_from_grant(&self.authority_id, &grant)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let request = DelegatedTaskRequest {
            schema_version: crate::delegated_task::DELEGATED_TASK_SCHEMA_VERSION,
            grant,
            source_execution,
            context,
        };
        let result = match self.host.dispatch(&binding.target, request.clone()).await {
            Ok(result) => result,
            Err(error) => return self.accept_failure(&request.grant, error.to_string()).await,
        };
        if let Err(error) = validate_task_result(&request, &result) {
            return self.accept_failure(&request.grant, error.to_string()).await;
        }
        let ack = self.ingress.accept(result.terminal).await?;
        if ack.disposition == IngressDisposition::Rejected {
            return Err(StasisError::PortFailure(ack.message.unwrap_or_else(|| {
                "delegated terminal ingress rejected".to_string()
            })));
        }
        Ok(())
    }
}

pub struct DelegationService {
    runtime: Arc<RuntimeComposition>,
    endpoints: Arc<dyn DeliveryEndpointStore>,
    waits: Arc<dyn TurnWaitStore>,
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

    pub async fn delegate(&self, task: &str) -> StasisResult<Value> {
        let task = task.trim();
        if task.is_empty() {
            return Err(StasisError::PortFailure(
                "delegated task is required".to_string(),
            ));
        }
        if self
            .binding()
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?
            .is_none()
        {
            return Err(StasisError::PortFailure(
                "no delegation workshop is explicitly bound".to_string(),
            ));
        }
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
        if self.runtime.get_job(&job_id).await?.is_none() {
            let payload = AgentTurnWaitableJobPayload {
                agent_id: "paired-medousa-daemon".to_string(),
                session_id: execution.session_id().to_string(),
                turn_id: turn_id.clone(),
                thread_id: Some(execution.correlation_id().to_string()),
                user_prompt: task.to_string(),
                system_prompt: None,
                timeout_seconds: DELEGATION_TIMEOUT_SECONDS,
                poll_interval_seconds: 1,
                endpoint_ref: Some(DELEGATION_ENDPOINT_ID.to_string()),
                mcp_gateway_ref: None,
            };
            self.runtime
                .enqueue_job(NewJob {
                    id: job_id.clone(),
                    queue: "default".to_string(),
                    job_type: "workflow.stasis.agent_turn.waitable".to_string(),
                    payload_ref: payload.to_payload_ref()?,
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

        let worker_id = format!("delegation-{identity}");
        let started = Instant::now();
        loop {
            process_once(self.runtime.as_ref(), &worker_id)
                .await
                .map_err(|error| StasisError::PortFailure(error.to_string()))?;
            if let Some(record) = self.waits.get(&turn_id).await?
                && record.status != TurnWaitStatus::Pending
            {
                return match record.status {
                    TurnWaitStatus::Completed => {
                        Ok(record.result_payload.unwrap_or_else(|| json!({})))
                    }
                    TurnWaitStatus::Failed
                    | TurnWaitStatus::Cancelled
                    | TurnWaitStatus::TimedOut => Err(StasisError::PortFailure(
                        record.error_message.unwrap_or_else(|| {
                            format!("delegated turn ended as {:?}", record.status)
                        }),
                    )),
                    TurnWaitStatus::Pending => unreachable!(),
                };
            }
            if started.elapsed() >= Duration::from_secs(DELEGATION_TIMEOUT_SECONDS) {
                return Err(StasisError::PortFailure(
                    "delegated turn did not reach a terminal state".to_string(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

pub fn install_delegation_runtime(
    runtime: Arc<RuntimeComposition>,
    authority_id: AuthorityId,
    session_store: Arc<dyn SessionStore>,
    host: Arc<dyn DelegatedTaskTransport>,
) -> Result<Arc<DelegationService>> {
    let codec: Arc<dyn AgentMessageCodec> = Arc::new(JsonAgentMessageCodec::v1());
    let waits: Arc<dyn TurnWaitStore> = Arc::new(InMemoryTurnWaitStore::new());
    let raw_ingress: Arc<dyn AgentEventIngress> = Arc::new(InMemoryAgentEventIngress::new());
    let ingress: Arc<dyn AgentEventIngress> = Arc::new(WaitCorrelatingAgentEventIngress::new(
        raw_ingress,
        waits.clone(),
    ));
    let endpoints = RuntimeFactory::resolve_delivery_endpoint_store(runtime.as_ref(), None);
    let transport: Arc<dyn AgentTransport> = Arc::new(DelegationAgentTransport {
        authority_id,
        host,
        session_store,
        codec: codec.clone(),
        ingress: ingress.clone(),
    });
    let handler = AgentTurnWaitableJobHandler::new(waits.clone(), codec, Some(ingress))
        .with_endpoint_publish(endpoints.clone(), transport);
    match runtime.as_ref() {
        RuntimeComposition::InMemory(inner) => inner.register_daemon_handler(handler)?,
        RuntimeComposition::Surreal(inner) => inner.register_daemon_handler(handler)?,
    }
    Ok(Arc::new(DelegationService {
        runtime,
        endpoints,
        waits,
    }))
}
