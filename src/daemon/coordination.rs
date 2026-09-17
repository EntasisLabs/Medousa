//! Local, explicitly approved coordination dispatch. Native operator HTTP
//! adapters review persisted proposals; grant issuance is never a model tool.

use std::sync::Arc;

use anyhow::{Result, bail};
use async_trait::async_trait;
use chrono::Utc;
use medousa_acp_client::coordination::context::{
    MAX_PEER_CONTEXT_BYTES as MAX_CONTEXT_BYTES, hydrate_assignment_context,
};
use medousa_acp_client::coordination::store::{AssignmentClaim, CoordinationStore};
use medousa_acp_client::coordination::{
    ExternalPeerExecutionPort, PeerAssignmentAuthority, PeerDispatchJournal,
    dispatch_external_peer_assignment, validate_assignment_request,
};
use medousa_acp_client::{
    AgentRuntimeKind, RuntimeAuthStatus, runtime_auth_probe, runtime_availability,
};
use medousa_forge::execution::ExecutionClass;
use medousa_types::coordination::*;
use medousa_types::{AuthorityId, CreateAgentSessionRequest, SessionId, SessionRef};
use sha2::{Digest, Sha256};

use crate::daemon::state::AppState;
use crate::request_principal::{Capability, PrincipalKind, RequestPrincipal};
mod host;
pub mod http;
mod owner_intake;
mod proposals;
pub use host::{local_coordination_host, start_local_coordination_host};
pub use owner_intake::OwnerIntakeResult;

#[derive(Clone)]
pub struct LocalPeerDispatcher {
    state: AppState,
    local_runtime_id: String,
    store: Arc<CoordinationStore>,
    wake: Arc<tokio::sync::Notify>,
}

#[derive(Clone)]
pub(crate) struct PeerReceiptSink {
    host: LocalPeerDispatcher,
    binding: ExternalPeerAssignmentBinding,
}

impl PeerReceiptSink {
    pub(crate) async fn terminal(
        &self,
        outcome: PeerAssignmentOutcome,
        result: String,
    ) -> Result<()> {
        use medousa_acp_client::coordination::store::intake::{
            MAX_RECEIPT_BYTES, terminal_receipt_id,
        };
        let receipt = ExternalPeerAssignmentReceipt {
            receipt_id: terminal_receipt_id(&self.binding),
            binding: self.binding.clone(),
            outcome,
            result: crate::text_budget::truncate_text_for_budget(&result, MAX_RECEIPT_BYTES / 4),
        };
        let store = self.host.store.clone();
        let saved = receipt.clone();
        let created = self
            .host
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(store.observe_receipt_once(&saved))
            })
            .await??;
        if created {
            self.host.wake.notify_one();
        }
        Ok(())
    }
}

fn actor(principal: &RequestPrincipal) -> Result<String> {
    if !principal.capabilities().contains(Capability::ContentRead)
        || !principal
            .capabilities()
            .contains(Capability::WorkshopInteract)
    {
        bail!("principal cannot coordinate workshop work");
    }
    match principal.profile_id() {
        Some(profile) => Ok(profile.to_string()),
        None if principal.kind() == PrincipalKind::LocalApp => {
            Ok(crate::user_profiles::resolve_workshop_identity_user_id())
        }
        _ => bail!("coordination requires a bound owner identity"),
    }
}

fn executor_session(
    channel: &CoordinationChannelRef,
    assignment_id: &str,
    authority: &AuthorityId,
) -> SessionRef {
    let mut hash = Sha256::new();
    for value in [
        "medousa/peer-session/v1",
        channel.authority_id.as_str(),
        channel.channel_id.as_str(),
        assignment_id,
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    SessionRef {
        authority_id: authority.clone(),
        session_id: SessionId::parse(format!("ses_peer_{:x}", hash.finalize()))
            .expect("generated peer session id"),
    }
}

impl LocalPeerDispatcher {
    /// local_runtime_id comes from the admitted host scheduler, never a model.
    pub fn new(
        state: AppState,
        local_runtime_id: String,
        store: Arc<CoordinationStore>,
    ) -> Result<Self> {
        if local_runtime_id.trim().is_empty() {
            bail!("local execution runtime identity is required");
        }
        Ok(Self {
            state,
            local_runtime_id,
            store,
            wake: Arc::new(tokio::sync::Notify::new()),
        })
    }

    pub fn execution_session(
        &self,
        channel: &CoordinationChannelRef,
        assignment_id: &str,
    ) -> Result<SessionRef> {
        let authority = crate::workshop_authority::current().map_err(anyhow::Error::msg)?;
        Ok(executor_session(channel, assignment_id, authority))
    }

    fn local_request(
        &self,
        principal: &RequestPrincipal,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<String> {
        validate_assignment_request(request)?;
        let profile = actor(principal)?;
        let authority = crate::workshop_authority::current().map_err(anyhow::Error::msg)?;
        if request.owner_principal_id != profile
            || request.target.authority_id != *authority
            || request.channel.authority_id != *authority
            || request.owner_session.authority_id != *authority
            || request.target.execution_runtime_id != self.local_runtime_id
            || request.execution_session
                != executor_session(&request.channel, &request.assignment_id, authority)
        {
            bail!("peer assignment does not match the authenticated local owner/target/session");
        }
        Ok(profile)
    }

    pub async fn create_channel(
        &self,
        principal: &RequestPrincipal,
        record: CoordinationChannelRecord,
    ) -> Result<()> {
        let profile = actor(principal)?;
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        if record.owner_principal_id != profile || record.channel.authority_id != authority {
            bail!("channel does not match authenticated local ownership");
        }
        let store = self.store.clone();
        self.state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok((|| -> Result<()> {
                    for session in &record.attached_sessions {
                        if session.authority_id != authority
                            || !crate::session_catalog::session_visible_to_profile(
                                session.session_id.as_str(),
                                &profile,
                            )
                        {
                            bail!("channel attachment is not visible to its owner");
                        }
                    }
                    store.create_channel(&record)?;
                    Ok(())
                })())
            })
            .await??;
        Ok(())
    }

    /// Explicit operator approval only. No model-facing action may issue grants.
    pub async fn approve(
        &self,
        principal: &RequestPrincipal,
        grant: ExternalPeerAssignmentGrant,
    ) -> Result<()> {
        if !principal.capabilities().contains(Capability::AdminExecute) {
            bail!("peer approval requires an operator");
        }
        self.local_request(principal, &grant.request)?;
        let ttl = grant.expires_at - Utc::now();
        if ttl.num_seconds() <= 0 || ttl.num_seconds() > 86_400 {
            bail!("peer approval expiry must be within 24 hours");
        }
        self.hydrate(principal, &grant.request, false).await?;
        let store = self.store.clone();
        self.state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(store.approve_assignment(&grant))
            })
            .await??;
        Ok(())
    }

    pub async fn revoke(
        &self,
        principal: &RequestPrincipal,
        channel: CoordinationChannelRef,
        grant_id: String,
    ) -> Result<()> {
        if !principal.capabilities().contains(Capability::AdminExecute) {
            bail!("peer revocation requires an operator");
        }
        let profile = actor(principal)?;
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        if channel.authority_id != authority {
            bail!("cannot revoke a foreign workshop grant");
        }
        let store = self.store.clone();
        let request = self
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok((|| -> Result<_> {
                    store.require_owner(&channel, &profile)?;
                    let grant = store.assignment_grant(&channel, &grant_id)?;
                    store.revoke_assignment_grant(&channel, &grant_id)?;
                    Ok(grant.request)
                })())
            })
            .await??;
        super::agents::cancel_agent_session_for_chat(
            &self.state,
            request.execution_session.session_id.as_str(),
        )
        .await
        .map_err(|(_, message)| anyhow::anyhow!(message))?;
        Ok(())
    }

    async fn hydrate(
        &self,
        principal: &RequestPrincipal,
        request: &ExternalPeerAssignmentRequest,
        require_grant: bool,
    ) -> Result<String> {
        let profile = self.local_request(principal, request)?;
        let authority = request.target.authority_id.clone();
        let request = request.clone();
        let store = self.store.clone();
        let forge = self.state.forge.clone();
        self.state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok((|| -> Result<String> {
                    store.require_owner(&request.channel, &profile)?;
                    if require_grant {
                        store.require_assignment_grant(&request, Utc::now())?;
                    }
                    let channel = store.channel(&request.channel)?;
                    if !channel.attached_sessions.contains(&request.owner_session)
                        || request.context.sources.iter().any(|source| {
                            !channel
                                .attached_sessions
                                .contains(&source.selection.session)
                        })
                    {
                        bail!("peer source sessions are not attached to the coordination channel");
                    }
                    forge.load(&medousa_forge::model::WorkId::from(
                        request.forge_work_id.clone(),
                    ))?;
                    let sessions = crate::session_store::get_session_store();
                    hydrate_assignment_context(
                        &request,
                        |session| {
                            session.authority_id == authority
                                && crate::session_catalog::session_visible_to_profile(
                                    session.session_id.as_str(),
                                    &profile,
                                )
                        },
                        |session| sessions.load_transcript_entries(&session.session_id),
                    )
                })())
            })
            .await?
    }

    pub async fn dispatch(
        &self,
        principal: &RequestPrincipal,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<ExternalPeerAssignmentBinding> {
        let call = LocalPeerCall {
            host: self,
            principal: principal.clone(),
        };
        dispatch_external_peer_assignment(&call, &call, &call, request).await
    }
}

struct LocalPeerCall<'a> {
    host: &'a LocalPeerDispatcher,
    principal: RequestPrincipal,
}

#[async_trait]
impl PeerAssignmentAuthority for LocalPeerCall<'_> {
    async fn authorize(&self, request: &ExternalPeerAssignmentRequest) -> Result<()> {
        self.host.hydrate(&self.principal, request, true).await?;
        Ok(())
    }
}

#[async_trait]
impl PeerDispatchJournal for LocalPeerCall<'_> {
    async fn claim(&self, request: &ExternalPeerAssignmentRequest) -> Result<AssignmentClaim> {
        let store = self.host.store.clone();
        let request = request.clone();
        self.host
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(store.claim_assignment(&request))
            })
            .await?
    }
    async fn binding(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<Option<ExternalPeerAssignmentBinding>> {
        let store = self.host.store.clone();
        let request = request.clone();
        self.host
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(store.peer_if_recorded(&request.channel, &request.assignment_id))
            })
            .await?
    }
    async fn record(&self, binding: &ExternalPeerAssignmentBinding) -> Result<()> {
        let store = self.host.store.clone();
        let binding = binding.clone();
        let agent_session_id = binding.agent_session_id.clone();
        let persisted = self
            .host
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(store.record_peer(&binding))
            })
            .await
            .map_err(anyhow::Error::from)
            .and_then(|result| result);
        if let Err(error) = persisted {
            let _ =
                super::agents::cancel_live_agent_session(&self.host.state, &agent_session_id).await;
            return Err(error);
        }
        Ok(())
    }
}

fn runtime_kind(runtime: ExternalPeerRuntime) -> AgentRuntimeKind {
    match runtime {
        ExternalPeerRuntime::Codex => AgentRuntimeKind::Codex,
        ExternalPeerRuntime::Cursor => AgentRuntimeKind::Cursor,
        ExternalPeerRuntime::Hermes => AgentRuntimeKind::Hermes,
    }
}

#[async_trait]
impl ExternalPeerExecutionPort for LocalPeerCall<'_> {
    async fn discover(&self) -> Result<Vec<ExternalPeerCandidate>> {
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        let local_runtime = self.host.local_runtime_id.clone();
        self.host
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                let stub = std::env::var("MEDOUSA_ACP_FORCE_STUB")
                    .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
                Ok([
                    ExternalPeerRuntime::Codex,
                    ExternalPeerRuntime::Cursor,
                    ExternalPeerRuntime::Hermes,
                ]
                .into_iter()
                .map(|runtime| {
                    let kind = runtime_kind(runtime);
                    let (installed, _, _) = runtime_availability(kind);
                    let probe = runtime_auth_probe(kind);
                    let availability = if stub {
                        PeerAvailability::Unavailable {
                            reason: "development stub cannot execute delegated work".into(),
                        }
                    } else if !installed {
                        PeerAvailability::Unavailable {
                            reason: "install this agent from Settings → Packages".into(),
                        }
                    } else if probe.status != RuntimeAuthStatus::SignedIn {
                        PeerAvailability::Unavailable {
                            reason: probe.detail.unwrap_or_else(|| {
                                "agent authentication could not be verified".into()
                            }),
                        }
                    } else {
                        PeerAvailability::Ready
                    };
                    ExternalPeerCandidate {
                        target: ExternalPeerTarget {
                            authority_id: authority.clone(),
                            execution_runtime_id: local_runtime.clone(),
                            runtime,
                        },
                        availability,
                    }
                })
                .collect())
            })
            .await
            .map_err(Into::into)
    }
    async fn assign(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<ExternalPeerAssignmentBinding> {
        let prompt = self.host.hydrate(&self.principal, request, true).await?;
        if !self.discover().await?.iter().any(|candidate| {
            candidate.target == request.target && candidate.availability == PeerAvailability::Ready
        }) {
            bail!("requested peer runtime became unavailable before provider startup");
        }
        let response = super::agents::create_agent_session_service(
            self.host.state.clone(),
            CreateAgentSessionRequest {
                session_id: request.execution_session.session_id.to_string(),
                runtime: runtime_kind(request.target.runtime).as_str().into(),
                prompt: None,
                cwd: None,
                command: None,
                args: None,
                surface: None,
                work_id: Some(request.forge_work_id.clone()),
                resume_provider_token: None,
                code_context: None,
            },
        )
        .await
        .map_err(|(_, message)| anyhow::anyhow!(message))?;
        // Startup can await a provider handshake. Recheck approval before any
        // prompt is sent; on failure retain the uncertain claim and cancel custody.
        let binding = ExternalPeerAssignmentBinding {
            assignment_id: request.assignment_id.clone(),
            owner_principal_id: request.owner_principal_id.clone(),
            channel: request.channel.clone(),
            target: request.target.clone(),
            execution_session: request.execution_session.clone(),
            agent_session_id: response.agent_session_id.clone(),
        };
        let start = async {
            self.host.hydrate(&self.principal, request, true).await?;
            // Bind custody durably before a fast peer can publish completion.
            self.record(&binding).await?;
            super::agents::attach_peer_receipt_sink(
                &response.agent_session_id,
                PeerReceiptSink {
                    host: self.host.clone(),
                    binding: binding.clone(),
                },
            )
            .await?;
            self.host.hydrate(&self.principal, request, true).await?;
            super::agents::prompt_agent_session_service(
                self.host.state.clone(),
                response.agent_session_id.clone(),
                medousa_types::AgentSessionPromptRequest {
                    prompt,
                    code_context: None,
                },
            )
            .await
            .map_err(|(_, message)| anyhow::anyhow!(message))?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = start {
            // Binding persistence may precede a rejected prompt. Record that
            // outcome rather than leaving accepted custody with no result.
            let sink = PeerReceiptSink {
                host: self.host.clone(),
                binding: binding.clone(),
            };
            if let Err(receipt_error) = sink
                .terminal(PeerAssignmentOutcome::Interrupted, error.to_string())
                .await
            {
                tracing::warn!(error = %receipt_error, "peer startup claim requires reconciliation");
            }
            let _ = super::agents::cancel_live_agent_session(
                &self.host.state,
                &response.agent_session_id,
            )
            .await;
            return Err(error);
        }
        Ok(binding)
    }
}
