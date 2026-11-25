//! Project a verified terminal receipt into the originating session and settle its Stasis wait.
use super::*;
use crate::delegation::session_results::NativeSessionWait;
use anyhow::Context;
use medousa_acp_client::coordination::store::intake::OwnerEventIntakeClaim;
use medousa_types::{
    ConversationTurn, ExecutionId, ExecutionRef, TranscriptEntry, TranscriptEntryRef,
};
use stasis::domain::agent::envelope::AgentEnvelopeKind;

impl LocalPeerDispatcher {
    pub(super) async fn handoff_wait(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<NativeSessionWait> {
        Ok(NativeSessionWait::open(
            self.state.composition(),
            &serde_json::to_string(&request.channel)?,
            request.owner_session.session_id.as_str(),
            &request.assignment_id,
            request.execution_session.session_id.as_str(),
        )
        .await?)
    }

    pub(super) async fn commit_session_result(
        &self,
        request: &ExternalPeerAssignmentRequest,
        receipt: &ExternalPeerAssignmentReceipt,
    ) -> Result<TranscriptEntry> {
        let wait = self.handoff_wait(request).await?;
        let kind = match receipt.outcome {
            PeerAssignmentOutcome::Completed => AgentEnvelopeKind::TurnCompleted,
            PeerAssignmentOutcome::Cancelled => AgentEnvelopeKind::Cancelled,
            PeerAssignmentOutcome::Failed | PeerAssignmentOutcome::Interrupted => {
                AgentEnvelopeKind::Failed
            }
        };
        let cause = ExecutionRef {
            session_id: request.owner_session.session_id.clone(),
            authority_id: request.owner_session.authority_id.clone(),
            execution_id: ExecutionId::parse(&wait.id)?,
        };
        let store = crate::session_store::get_session_store();
        let lookup = || {
            let cause = cause.clone();
            let store = store.clone();
            let owner = request.owner_principal_id.clone();
            self.stored(move |_| {
                if !crate::session_catalog::session_visible_to_profile(
                    cause.session_id.as_str(),
                    &owner,
                ) {
                    bail!("originating chat visibility revoked")
                }
                crate::peer_completion_delivery::find_completion_entry(
                    &*store,
                    &cause.session_id,
                    &cause,
                )
            })
        };
        // The caller holds the durable intake lease across lookup, append and acknowledgment.
        let entry = match lookup().await? {
            Some(entry) => entry,
            None => {
                let result = serde_json::json!({"request_id": request.assignment_id,
                    "worker": request.execution_session, "outcome": receipt.outcome, "result": receipt.result});
                let turn = ConversationTurn::plain(
                    "assistant",
                    result.to_string(),
                    Utc::now(),
                    Vec::new(),
                    Some("worker_synthesis".into()),
                );
                store
                    .append_transcript_batch(
                        &cause.session_id,
                        &[crate::session_store::TranscriptAppend::native(
                            turn,
                            Some(cause.clone()),
                        )],
                    )
                    .await?;
                lookup()
                    .await?
                    .context("committed worker result missing from originating chat")?
            }
        };
        wait.settle(
            kind,
            &receipt.receipt_id,
            serde_json::json!({"text": receipt.result, "outcome": receipt.outcome}),
        )
        .await?;
        Ok(entry)
    }

    pub(super) async fn resume_handoff_event(
        &self,
        event: OwnerEvent,
    ) -> Result<OwnerIntakeResult> {
        if matches!(event.payload, OwnerEventPayload::AssignmentAccepted { .. }) {
            return Ok(OwnerIntakeResult::AlreadyConsumed);
        }
        let saved = event.clone();
        let claim = self
            .stored(move |store| {
                let record = store.require_handoff_event(&saved)?;
                let Some(lease) = store.try_handoff_event_lease(&saved)? else {
                    return Ok(None);
                };
                let claim = store.begin_handoff_event(&saved, &lease)?;
                Ok(Some((record, lease, claim)))
            })
            .await?;
        let Some((record, lease, claim)) = claim else {
            return Ok(OwnerIntakeResult::DeferredBusy);
        };
        let intake = match claim {
            OwnerEventIntakeClaim::Consumed(_) => return Ok(OwnerIntakeResult::AlreadyConsumed),
            OwnerEventIntakeClaim::Started(intake) | OwnerEventIntakeClaim::Unresolved(intake) => {
                intake
            }
        };
        let OwnerEventPayload::AssignmentTerminal { receipt, .. } = &event.payload else {
            bail!("expected terminal assignment receipt")
        };
        let entry = self.commit_session_result(&record.request, receipt).await?;
        // Queue before acknowledging durable intake. Recovery retries use the
        // same receipt identity, even if transcript projection already exists.
        if record.policy.wake_on_terminal {
            crate::agent_messaging::enqueue_terminal_event(
                &record.request.owner_principal_id,
                record.request.execution_session.session_id.as_str(),
                record.request.owner_session.session_id.as_str(),
                &receipt.receipt_id,
                &entry.turn.content,
                medousa_types::wake::WakeSource::PeerTerminal,
            )
            .await?;
        }
        self.stored(move |store| {
            store.acknowledge_handoff_event(
                &OwnerEventIntakeAcknowledgment {
                    intake,
                    decision: TranscriptEntryRef {
                        session: record.request.owner_session,
                        entry_id: entry.entry_id,
                        entry_seq: entry.entry_seq,
                    },
                    decision_digest: entry.content_digest,
                    command_refs: vec![],
                    terminal_delivery_ref: None,
                },
                &lease,
            )?;
            Ok(OwnerIntakeResult::Delivered)
        })
        .await
    }
}
