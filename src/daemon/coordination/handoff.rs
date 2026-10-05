//! Native admission from an authenticated human turn; the model supplies intent, never approval.
use super::*;
use crate::agent_runtime::execution_context::TurnExecutionContext;
use medousa_types::{TranscriptEntry, TranscriptEntryRef, work_handoff::*};

fn source_for_admission<'a>(
    entries: &'a [TranscriptEntry],
    execution: &medousa_types::ExecutionRef,
    direct: bool,
    after: u64,
    through: u64,
) -> Result<&'a TranscriptEntry> {
    entries.iter().rev().find(|entry| entry.turn.role == "user" && entry.entry_seq > after
        && entry.entry_seq <= through && (!direct || entry.caused_by.as_ref() == Some(execution)))
        .ok_or_else(|| anyhow::anyhow!("direct delegation requires this admitted human request in its committed context; use a proposal for recommendations"))
}
impl LocalPeerDispatcher {
    pub(super) async fn accept_handoff(
        &self,
        binding: &ExternalPeerAssignmentBinding,
    ) -> Result<()> {
        let saved = binding.clone();
        self.stored(move |store| store.record_handoff_acceptance(&saved))
            .await?;
        self.wake.notify_one();
        Ok(())
    }
    pub async fn handoff_for_turn(
        &self,
        turn: &TurnExecutionContext,
        input: PeerHandoffIntent,
    ) -> Result<serde_json::Value> {
        let direct = input.admission == PeerHandoffAdmission::Delegate;
        if direct
            && !turn
                .principal()
                .capabilities()
                .contains(Capability::AdminExecute)
        {
            bail!("this turn has no human delegation authority; use a proposal");
        }
        let owner = turn
            .principal()
            .profile_id()
            .or(turn.legacy_scope().identity_user_id.as_deref())
            .filter(|owner| !owner.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("handoff requires a frozen owner"))?
            .to_string();
        let principal = RequestPrincipal::worker(owner.clone());
        let session_id = SessionId::parse(turn.session_id().as_str())?;
        let execution =
            crate::workshop_authority::execution_ref(session_id.as_str(), turn.turn_id())
                .map_err(anyhow::Error::msg)?;
        let source_session = session_id.clone();
        let source_owner = owner.clone();
        let after = input.after_entry_seq;
        let through = input.through_entry_seq;
        let (source, source_digest) = self
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok((|| -> Result<_> {
                    if !crate::session_catalog::session_visible_to_profile(
                        source_session.as_str(),
                        &source_owner,
                    ) {
                        bail!("handoff source is not visible")
                    }
                    let entries = crate::session_store::get_session_store()
                        .load_transcript_entries(&source_session);
                    let entry = source_for_admission(&entries, &execution, direct, after, through)?;
                    if crate::session_store::transcript_content_digest(&entry.turn)?
                        != entry.content_digest
                    {
                        bail!("source digest mismatch")
                    }
                    Ok((
                        TranscriptEntryRef {
                            session: SessionRef {
                                authority_id: execution.authority_id,
                                session_id: source_session,
                            },
                            entry_id: entry.entry_id.clone(),
                            entry_seq: entry.entry_seq,
                        },
                        entry.content_digest.clone(),
                    ))
                })())
            })
            .await??;
        let proposal = self
            .compile_proposal_for_turn(
                &principal,
                session_id,
                PeerProposalIntent {
                    request_key: input.request_key,
                    runtime: input.runtime,
                    forge_work_id: Some(input.forge_work_id),
                    instructions: input.instructions,
                    after_entry_seq: after,
                    through_entry_seq: through,
                    continue_owner: false,
                    existing_agent_session_id: None,
                },
                chrono::Duration::hours(24),
                true,
            )
            .await?;
        let mut record = PeerHandoffRecord {
            request: proposal.request.clone(),
            policy: input.handoff,
            admission: input.admission,
            source,
            source_digest,
            sender_bot_id: turn.bot_identity().map(|bot| bot.bot_id().clone()),
            expires_at: proposal.expires_at,
        };
        let saved = record.clone();
        let store = self.store.clone();
        record = self
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok((|| -> Result<_> {
                    let mut saved = saved;
                    if let Some(previous) =
                        store.handoff(&saved.request.channel, &saved.request.assignment_id)?
                    {
                        // A retry in a later human turn keeps the original source authorization.
                        saved.source = previous.source.clone();
                        saved.source_digest = previous.source_digest.clone();
                        if saved != previous {
                            bail!("handoff retry changed its immutable policy or sender")
                        }
                    }
                    store.record_handoff(&saved)?;
                    Ok(saved)
                })())
            })
            .await??;
        if direct {
            // Only this admitted human turn can compile the exact native grant.
            // No continuation/worker principal is promoted to operator authority.
            self.hydrate(&principal, &record.request, false).await?;
            let store = self.store.clone();
            let saved = record.clone();
            let proposal_id = proposal.proposal_id.clone();
            self.state
                .forge_execution
                .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                    Ok((|| -> Result<()> {
                        if saved.expires_at <= Utc::now() {
                            bail!("delegation admission expired")
                        }
                        store.decide_proposal(
                            &saved.request.channel,
                            &PeerProposalDecision {
                                proposal_id,
                                owner_principal_id: saved.request.owner_principal_id.clone(),
                                approved: true,
                            },
                        )?;
                        store.approve_assignment(&ExternalPeerAssignmentGrant {
                            request: saved.request,
                            expires_at: saved.expires_at,
                        })?;
                        Ok(())
                    })())
                })
                .await??;
            self.dispatch(&principal, &record.request).await?;
            self.wake.notify_one();
        }
        let store = self.store.clone();
        self.state.forge_execution.run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
            Ok((|| -> Result<_> {
                let view = store.handoff_view(&record.request.channel, &record.request.assignment_id)?;
                Ok(serde_json::json!({ "admission": record.admission, "proposal": proposal, "handoff": view,
                    "status": if direct { "delegated" } else { "proposed" } }))
            })())
        }).await?
    }
    pub async fn review_handoff_for_turn(
        &self,
        turn: &TurnExecutionContext,
        input: PeerHandoffReviewInput,
    ) -> Result<serde_json::Value> {
        let owner = turn
            .principal()
            .profile_id()
            .or(turn.legacy_scope().identity_user_id.as_deref())
            .ok_or_else(|| anyhow::anyhow!("handoff review requires a frozen owner"))?
            .to_string();
        if !turn
            .principal()
            .capabilities()
            .contains(Capability::ContentWrite)
        {
            bail!("handoff review requires write authority")
        }
        let session = SessionRef {
            authority_id: crate::workshop_authority::current()
                .map_err(anyhow::Error::msg)?
                .clone(),
            session_id: SessionId::parse(turn.session_id().as_str())?,
        };
        let saved_channel = input.channel.clone();
        let saved_id = input.assignment_id.clone();
        let saved = self
            .stored(move |store| store.handoff(&saved_channel, &saved_id))
            .await?
            .ok_or_else(|| anyhow::anyhow!("handoff unavailable"))?;
        self.hydrate(
            &RequestPrincipal::worker(owner.clone()),
            &saved.request,
            false,
        )
        .await?;
        let forge = self.state.forge.clone();
        let bot_id = turn.bot_identity().map(|bot| bot.bot_id().clone());
        let decision = PeerHandoffReview {
            receipt_id: input.receipt_id,
            verdict: input.verdict,
            reason: input.reason,
            sender_session: session.clone(),
            turn_id: turn.turn_id().to_string(),
        };
        let store = self.store.clone();
        self.state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok((|| -> Result<_> {
                    let record = store
                        .handoff(&input.channel, &input.assignment_id)?
                        .ok_or_else(|| anyhow::anyhow!("handoff unavailable"))?;
                    if forge
                        .load(&medousa_forge::model::WorkId::from(
                            record.request.forge_work_id.clone(),
                        ))?
                        .owner
                        != owner
                    {
                        bail!("handoff work ownership changed");
                    }
                    let mut decision = decision;
                    if record.sender_bot_id != bot_id {
                        bail!("review requires the originating sender Bot")
                    }
                    let sender = if session != record.request.owner_session {
                        let admitted = store.handoff_for_wake(&session, &decision.turn_id)?;
                        if admitted != record {
                            bail!("callback belongs to another handoff")
                        }
                        decision.sender_session = record.request.owner_session.clone();
                        record.request.owner_session
                    } else {
                        session
                    };
                    if !crate::session_catalog::session_visible_to_profile(
                        sender.session_id.as_str(),
                        &owner,
                    ) {
                        bail!("sender session no longer visible")
                    }
                    store.review_handoff(
                        &input.channel,
                        &input.assignment_id,
                        &sender,
                        &owner,
                        &decision,
                    )?;
                    Ok(serde_json::to_value(
                        store.handoff_view(&input.channel, &input.assignment_id)?,
                    )?)
                })())
            })
            .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_admission_requires_this_turns_committed_human_request() {
        let execution = medousa_types::ExecutionRef {
            authority_id: format!("auth_{}", "a".repeat(64)).parse().unwrap(),
            session_id: "ses_sender".parse().unwrap(),
            execution_id: medousa_types::ExecutionId::parse("human-turn").unwrap(),
        };
        let human = TranscriptEntry {
            entry_id: format!("ent_{}", "b".repeat(32)).parse().unwrap(),
            entry_seq: 2,
            caused_by: Some(execution.clone()),
            source: None,
            content_digest: "digest".into(),
            turn: medousa_types::ConversationTurn::plain(
                "user",
                "Send this work".into(),
                Utc::now(),
                vec![],
                None,
            ),
        };
        assert!(source_for_admission(std::slice::from_ref(&human), &execution, true, 1, 2).is_ok());
        assert!(
            source_for_admission(std::slice::from_ref(&human), &execution, true, 2, 3).is_err()
        );
        let mut changed = human.clone();
        changed.caused_by.as_mut().unwrap().execution_id =
            medousa_types::ExecutionId::parse("earlier-turn").unwrap();
        assert!(source_for_admission(&[changed.clone()], &execution, true, 0, 2).is_err());
        assert!(source_for_admission(&[changed], &execution, false, 0, 2).is_ok());
        changed = human.clone();
        changed.turn.role = "assistant".into();
        assert!(source_for_admission(&[changed], &execution, true, 0, 2).is_err());
        changed = human;
        changed.caused_by = None;
        assert!(source_for_admission(&[changed], &execution, true, 0, 2).is_err());
    }
    #[test]
    fn handoff_intent_cannot_mint_authority_or_choose_a_sender() {
        let base = serde_json::json!({"request_key":"send", "instructions":"Implement", "forge_work_id":"work-1", "after_entry_seq":0, "through_entry_seq":2});
        let intent: PeerHandoffIntent = serde_json::from_value(base.clone()).unwrap();
        assert_eq!(intent.handoff, PeerHandoffPolicy::default());
        for field in [
            "authorized",
            "approved",
            "continue_owner",
            "owner_principal_id",
            "sender_bot_id",
            "execution_grant_id",
            "existing_agent_session_id",
        ] {
            let mut forged = base.clone();
            forged[field] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<PeerHandoffIntent>(forged).is_err(),
                "{field}"
            );
        }
    }
}
