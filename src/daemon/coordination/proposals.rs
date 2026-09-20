//! Exact-snapshot operator approval service. Public UI/HTTP adapters must never
//! expose grant issuance as a model action.
use super::{LocalPeerDispatcher, MAX_CONTEXT_BYTES, actor};
use crate::request_principal::{Capability, RequestPrincipal};
use anyhow::{Result, bail};
use medousa_acp_client::coordination::store::proposals::proposal_identity;
use medousa_forge::execution::ExecutionClass;
use medousa_types::coordination::*;

impl LocalPeerDispatcher {
    pub async fn proposal_inbox(
        &self,
        principal: &RequestPrincipal,
        session_id: medousa_types::SessionId,
        after: Option<String>,
    ) -> Result<PeerProposalInboxResponse> {
        let owner = actor(principal)?;
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        let store = self.store.clone();
        let rows = self
            .state
            .forge_execution
            .run(
                ExecutionClass::StoreIo,
                medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
                move || {
                    Ok((|| -> Result<_> {
                        if crate::session_catalog::session_visible_to_profile(
                            session_id.as_str(),
                            &owner,
                        ) {
                            store.proposal_inbox(
                                &owner,
                                &medousa_types::SessionRef {
                                    authority_id: authority,
                                    session_id,
                                },
                                after.as_deref(),
                            )
                        } else {
                            store.proposal_inbox_for_source_session(
                                &owner,
                                &session_id,
                                after.as_deref(),
                            )
                        }
                    })())
                },
            )
            .await??;
        // Review does not expose source bodies. Full visibility is rechecked at approval.
        for row in &rows {
            self.local_request(principal, &row.proposal.request)?;
        }
        let next_cursor =
            (rows.len() == 8).then(|| rows.last().unwrap().proposal.proposal_id.clone());
        Ok(PeerProposalInboxResponse {
            proposals: rows,
            next_cursor,
        })
    }

    pub async fn propose_assignment(
        &self,
        principal: &RequestPrincipal,
        request: ExternalPeerAssignmentRequest,
        expires_at: chrono::DateTime<chrono::Utc>,
        continue_owner: bool,
        projected_source_session_ids: Vec<medousa_types::SessionId>,
    ) -> Result<PeerAssignmentProposal> {
        self.local_request(principal, &request)?;
        let ttl = expires_at - chrono::Utc::now();
        if ttl.num_seconds() <= 0 || ttl.num_seconds() > 86_400 {
            bail!("proposal expiry must be within 24 hours");
        }
        self.hydrate(principal, &request, false).await?;
        let mut proposal = PeerAssignmentProposal {
            proposal_id: String::new(),
            request,
            expires_at,
            continue_owner,
        };
        proposal.proposal_id = proposal_identity(&proposal)?;
        let saved = proposal.clone();
        let store = self.store.clone();
        self.state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(store.record_proposal_with_source_sessions(
                    &saved,
                    &projected_source_session_ids,
                ))
            })
            .await??;
        Ok(proposal)
    }

    pub async fn review_proposal(
        &self,
        principal: &RequestPrincipal,
        channel: CoordinationChannelRef,
        proposal_id: String,
    ) -> Result<PeerAssignmentProposal> {
        let owner = actor(principal)?;
        let store = self.store.clone();
        let proposal = self
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok((|| -> Result<_> {
                    store.require_owner(&channel, &owner)?;
                    store.proposal(&channel, &proposal_id)
                })())
            })
            .await??;
        self.local_request(principal, &proposal.request)?;
        Ok(proposal)
    }

    /// Persist the exact decision first; grant compilation is idempotent so a
    /// partial approval write can be finished without changing the snapshot.
    pub async fn decide_proposal(
        &self,
        principal: &RequestPrincipal,
        channel: CoordinationChannelRef,
        proposal_id: String,
        approved: bool,
    ) -> Result<()> {
        if !principal.capabilities().contains(Capability::AdminExecute) {
            bail!("proposal decision requires an operator");
        }
        let proposal = self
            .review_proposal(principal, channel.clone(), proposal_id.clone())
            .await?;
        if approved {
            self.hydrate(principal, &proposal.request, false).await?;
        }
        let decision = PeerProposalDecision {
            proposal_id,
            owner_principal_id: actor(principal)?,
            approved,
        };
        let store = self.store.clone();
        self.state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(store.decide_proposal(&channel, &decision))
            })
            .await??;
        if approved {
            self.approve(
                principal,
                ExternalPeerAssignmentGrant {
                    request: proposal.request.clone(),
                    expires_at: proposal.expires_at,
                },
            )
            .await?;
            if proposal.continue_owner {
                self.approve_owner_continuation(
                    principal,
                    PeerOwnerContinuationGrant {
                        request: proposal.request,
                        expires_at: proposal.expires_at,
                    },
                )
                .await?;
            }
        }
        self.wake.notify_one();
        Ok(())
    }

    pub async fn dispatch_approved_proposal(
        &self,
        principal: &RequestPrincipal,
        channel: CoordinationChannelRef,
        proposal_id: String,
    ) -> Result<ExternalPeerAssignmentBinding> {
        let owner = actor(principal)?;
        let store = self.store.clone();
        let proposal = self
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(store.require_approved_proposal(&channel, &proposal_id, &owner))
            })
            .await??;
        // Repair partial grant compilation using only the already approved
        // snapshot. This cannot turn a pending/denied proposal into approval.
        self.decide_proposal(
            principal,
            proposal.request.channel.clone(),
            proposal.proposal_id.clone(),
            true,
        )
        .await?;
        self.dispatch(principal, &proposal.request).await
    }
}
