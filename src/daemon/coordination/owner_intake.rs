//! Deliver verified peer outcomes to the session that admitted them.
use super::{LocalPeerDispatcher, MAX_CONTEXT_BYTES, actor};
use crate::request_principal::{Capability, RequestPrincipal};
use anyhow::{Context, Result, bail};
use medousa_acp_client::coordination::store::CoordinationStore;
use medousa_acp_client::coordination::store::intake::OwnerIntakeClaim;
use medousa_forge::execution::ExecutionClass;
use medousa_types::TranscriptEntryRef;
use medousa_types::coordination::*;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerIntakeResult {
    Delivered,
    WorkScoped,
    AlreadyConsumed,
    DeferredBusy,
    NeedsReconciliation,
}

impl LocalPeerDispatcher {
    pub(super) async fn stored<T: Send + 'static>(
        &self,
        work: impl FnOnce(&CoordinationStore) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.store.clone();
        self.state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(work(&store))
            })
            .await?
    }

    /// Return a completion only when the authenticated source's exact durable
    /// proposal route has a committed owner acknowledgment and decision.
    pub async fn remote_peer_completion_result(
        &self,
        source_device_id: &str,
        route: &crate::peer_coordination_mesh::RemotePeerCompletionDestination,
    ) -> Result<Option<crate::peer_coordination_mesh::RemotePeerCompletionResult>> {
        use crate::peer_coordination_mesh::RemotePeerCompletionResult;

        let source_device_id = source_device_id.to_string();
        let mut route = route.clone();
        let local_runtime_id = self.local_runtime_id.clone();
        let route_and_ack = self
            .stored(move |store| {
                if route.source_device_id != source_device_id {
                    bail!("completion route does not match authenticated source peer");
                }
                // Dispatch persists canonical custody before updating the mesh
                // route. Recover that exact binding after a crash between those
                // writes; absence remains pending and never triggers dispatch.
                if route.binding.is_none() {
                    let expected = route
                        .expected_binding
                        .as_ref()
                        .context("remote completion route has no expected assignment identity")?;
                    route.binding =
                        store.peer_if_recorded(&expected.channel, &route.assignment_id)?;
                }
                let Some(binding) = route.binding.as_ref() else {
                    return Ok(None);
                };
                if binding.target.execution_runtime_id != local_runtime_id
                    || route.assignment_id != binding.assignment_id
                    || route
                        .expected_binding
                        .as_ref()
                        .is_none_or(|expected| !expected.matches(binding))
                {
                    bail!("remote completion route binding does not match this destination");
                }
                let Some(receipt) =
                    store.receipt_if_recorded(&binding.channel, &binding.assignment_id)?
                else {
                    return Ok(None);
                };
                if receipt.binding != *binding {
                    bail!("persisted terminal receipt differs from the completion route");
                }
                let Some(ack) = store.owner_intake_acknowledgment(&receipt)? else {
                    return Ok(None);
                };
                if route.remote_owner_session != ack.decision.session
                    || ack.intake.receipt != receipt
                    || ack.decision.entry_seq == 0
                    || ack.decision_digest.trim().is_empty()
                {
                    bail!("durable owner acknowledgment does not match the completion route");
                }
                Ok(Some((route, receipt, ack)))
            })
            .await?;
        let Some((route, receipt, acknowledgment)) = route_and_ack else {
            return Ok(None);
        };

        let store = crate::session_store::get_session_store();
        let decision_session = acknowledgment.decision.session.session_id.clone();
        let decision_ref = acknowledgment.decision.clone();
        let decision_digest = acknowledgment.decision_digest.clone();
        let lookup_decision_digest = decision_digest.clone();
        let owner_profile_id = acknowledgment
            .intake
            .receipt
            .binding
            .owner_principal_id
            .clone();
        let decision = self
            .state
            .forge_execution
            .run(
                ExecutionClass::StoreIo,
                medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
                move || {
                    Ok((|| -> Result<_> {
                        if !crate::session_catalog::session_visible_to_profile(
                            decision_session.as_str(),
                            &owner_profile_id,
                        ) {
                            bail!("remote owner session is no longer visible to its owner");
                        }
                        let mut before = decision_ref.entry_seq.checked_add(1);
                        for _ in 0..80 {
                            let page = store.load_transcript_entries_page(
                                &decision_session,
                                128,
                                before,
                            );
                            if let Some(entry) = page.entries.into_iter().find(|entry| {
                                entry.entry_id == decision_ref.entry_id
                                    && entry.entry_seq == decision_ref.entry_seq
                            }) {
                                if entry.turn.role != "assistant"
                                    || entry.turn.answer_state.as_deref() == Some("failed")
                                    || (entry.turn.content.trim().is_empty()
                                        && entry.turn.tool_names.is_empty())
                                    || entry.content_digest != lookup_decision_digest
                                    || crate::session_store::transcript_content_digest(&entry.turn)?
                                        != lookup_decision_digest
                                {
                                    bail!("durable owner decision content does not match its acknowledgment");
                                }
                                return Ok(entry.turn);
                            }
                            before = page.next_cursor;
                            if before.is_none() {
                                break;
                            }
                        }
                        bail!("committed owner decision entry is missing from the remote session")
                    })())
                },
            )
            .await??;
        Ok(Some(RemotePeerCompletionResult {
            schema_version: crate::peer_coordination_mesh::PEER_COMPLETION_RESULT_SCHEMA_VERSION,
            source_device_id: route.source_device_id,
            request_digest: route.source_request_digest,
            proposal_id: route.proposal_id,
            proposal_request_digest: route.proposal_request_digest,
            binding: route.binding.expect("binding checked"),
            receipt,
            owner_acknowledgment: acknowledgment,
            committed_decision: decision,
            committed_decision_digest: decision_digest,
        }))
    }

    pub async fn approve_owner_continuation(
        &self,
        principal: &RequestPrincipal,
        grant: PeerOwnerContinuationGrant,
    ) -> Result<()> {
        if !principal.capabilities().contains(Capability::AdminExecute) {
            bail!("owner continuation approval requires an operator");
        }
        self.local_request(principal, &grant.request).await?;
        let ttl = grant.expires_at - chrono::Utc::now();
        if ttl.num_seconds() <= 0 || ttl.num_seconds() > 86_400 {
            bail!("owner continuation approval expiry must be within 24 hours");
        }
        self.hydrate(principal, &grant.request, false).await?;
        self.stored(move |store| store.approve_owner_continuation(&grant))
            .await?;
        self.wake.notify_one();
        Ok(())
    }

    /// Called on completion and by the future startup/retry host. Busy sessions
    /// retain the receipt. A started-but-unacknowledged turn is never rerun blindly.
    pub async fn resume_owner_intake(
        &self,
        principal: &RequestPrincipal,
        channel: CoordinationChannelRef,
        assignment_id: &str,
    ) -> Result<OwnerIntakeResult> {
        let profile = actor(principal)?;
        let saved_channel = channel.clone();
        let assignment_id = assignment_id.to_string();
        let receipt = self
            .stored(move |store| {
                store.require_owner(&saved_channel, &profile)?;
                store.receipt(&saved_channel, &assignment_id)
            })
            .await?;
        let event_receipt = receipt.clone();
        let handoff = self
            .stored(move |store| {
                store.handoff(
                    &event_receipt.binding.channel,
                    &event_receipt.binding.assignment_id,
                )
            })
            .await?;
        if handoff.is_some() {
            let saved = receipt.clone();
            let event = self
                .stored(move |store| store.peer_owner_event_for_receipt(&saved))
                .await?;
            return self.resume_handoff_event(event).await;
        }
        let saved = receipt.clone();
        if self
            .stored(move |store| store.retain_work_terminal_for_controller(&saved))
            .await?
        {
            return Ok(OwnerIntakeResult::WorkScoped);
        }
        let saved = receipt.clone();
        let intentionally_silent = self
            .stored(move |store| {
                Ok(store
                    .proposal_for_assignment(&saved.binding.channel, &saved.binding.assignment_id)?
                    .is_some_and(|p| !p.continue_owner))
            })
            .await?;
        if intentionally_silent {
            let saved = receipt.clone();
            let event = self
                .stored(move |store| store.peer_owner_event_for_receipt(&saved))
                .await?;
            return self
                .block_owner_event(
                    &event,
                    "legacy proposal did not request owner continuation",
                    false,
                )
                .await;
        }
        let saved = receipt.clone();
        let request = self
            .stored(move |store| store.require_owner_continuation(&saved, chrono::Utc::now()))
            .await?;
        self.hydrate(principal, &request, false).await?;
        let saved = request.clone();
        let Some(lease) = self
            .stored(move |store| store.try_owner_intake_lease(&saved))
            .await?
        else {
            return Ok(OwnerIntakeResult::DeferredBusy);
        };
        let lease = Arc::new(lease);
        let saved = receipt.clone();
        let claimed_lease = lease.clone();
        let claim = self
            .stored(move |store| store.begin_owner_intake(&saved, &claimed_lease))
            .await?;
        let intake = match claim {
            OwnerIntakeClaim::Consumed(_) => return Ok(OwnerIntakeResult::AlreadyConsumed),
            OwnerIntakeClaim::Unresolved(intake) | OwnerIntakeClaim::Started(intake) => intake,
        };
        let entry = self.commit_session_result(&request, &receipt).await?;
        self.stored(move |store| {
            store.acknowledge_owner_intake(
                &PeerOwnerIntakeAcknowledgment {
                    intake,
                    decision: TranscriptEntryRef {
                        session: request.owner_session,
                        entry_id: entry.entry_id,
                        entry_seq: entry.entry_seq,
                    },
                    decision_digest: entry.content_digest,
                },
                &lease,
            )?;
            Ok(OwnerIntakeResult::Delivered)
        })
        .await
    }

    pub(super) async fn block_owner_event(
        &self,
        event: &OwnerEvent,
        reason: &str,
        requires_user_decision: bool,
    ) -> Result<OwnerIntakeResult> {
        let blocked = OwnerEventBlocked {
            event_id: event.event_id.clone(),
            reason: reason.to_string(),
            blocked_at: chrono::Utc::now(),
            requires_user_decision,
        };
        let channel = event.channel.clone();
        self.stored(move |store| store.block_owner_event(&channel, &blocked))
            .await?;
        Ok(OwnerIntakeResult::NeedsReconciliation)
    }

    pub async fn drain_pending_owner_intake(
        &self,
        principal: &RequestPrincipal,
        channel: CoordinationChannelRef,
    ) -> Result<Vec<OwnerIntakeResult>> {
        let profile = actor(principal)?;
        let saved = channel.clone();
        let receipts = self
            .stored(move |store| store.pending_owner_receipts(&saved, &profile, 32))
            .await?;
        let mut outcomes = Vec::new();
        for receipt in receipts {
            outcomes.push(
                self.resume_owner_intake(
                    principal,
                    channel.clone(),
                    &receipt.binding.assignment_id,
                )
                .await?,
            );
        }
        Ok(outcomes)
    }
}
