//! Native admission from a human turn or its verified scoped owner callback.
//! The model supplies intent, never approval.
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
fn require_callback_scope(
    owner: &str,
    work_id: &str,
    request_owner: &str,
    request_work_id: &str,
) -> Result<()> {
    if request_owner != owner || request_work_id != work_id {
        bail!("callback delegation must remain in its authorized owner and project scope");
    }
    Ok(())
}

fn require_handoff_admission(
    principal: &RequestPrincipal,
    admission: PeerHandoffAdmission,
) -> Result<()> {
    if admission.requires_human_admission()
        && !principal.capabilities().contains(Capability::AdminExecute)
        && !(principal.kind() == crate::request_principal::PrincipalKind::Continuation
            && principal
                .capabilities()
                .contains(Capability::WorkspaceWrite))
    {
        bail!("delegation requires the owning Assistant; executors must use a proposal");
    }
    Ok(())
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
        mut input: PeerHandoffIntent,
    ) -> Result<serde_json::Value> {
        if turn.cancellation().is_cancelled() {
            bail!("cancelled owner turn cannot delegate");
        }
        let session_title = input
            .session_title
            .take()
            .map(|title| title.trim().to_string());
        if session_title.as_ref().is_some_and(|title| {
            title.is_empty() || title.len() > 160 || title.chars().any(char::is_control)
        }) {
            bail!("session_title must be 1–160 bytes without control characters");
        }
        let direct = input.admission.requires_human_admission();
        let dispatch_now = input.admission.dispatches_immediately();
        require_handoff_admission(turn.principal(), input.admission)?;
        let owner = turn
            .principal()
            .profile_id()
            .or(turn.legacy_scope().identity_user_id.as_deref())
            .filter(|owner| !owner.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("handoff requires a frozen owner"))?
            .to_string();
        let principal = RequestPrincipal::worker(owner.clone());
        let mut session_id = SessionId::parse(turn.session_id().as_str())?;
        let execution =
            crate::workshop_authority::execution_ref(session_id.as_str(), turn.turn_id())
                .map_err(anyhow::Error::msg)?;
        // A callback is the same owner continuing saved work, not an operator.
        // Resolve native provenance before accepting any model-supplied intent.
        let callback = direct
            && turn.principal().kind() == crate::request_principal::PrincipalKind::Continuation;
        let mut inherited_source = None;
        let mut inherited_execution = None;
        if callback {
            let session = medousa_types::SessionRef {
                authority_id: execution.authority_id.clone(),
                session_id: session_id.clone(),
            };
            let callback_turn = turn.turn_id().to_string();
            let bot = turn.bot_identity().map(|bot| bot.bot_id().clone());
            let expected_owner = owner.clone();
            let expected_work = input.forge_work_id.clone();
            let (parent, source, parent_execution) = self.stored(move |store| {
                if let Ok(parent) = store.handoff_for_wake(&session, &callback_turn) {
                    require_callback_scope(&expected_owner, &expected_work,
                        &parent.request.owner_principal_id, &parent.request.forge_work_id)?;
                    if parent.sender_bot_id != bot {
                        bail!("callback delegation requires the originating sender Bot");
                    }
                    return Ok((Some(parent.request), Some((parent.source, parent.source_digest)), None));
                }
                if let Ok(parent) = store.owner_request_for_wake(&session, &callback_turn) {
                    require_callback_scope(&expected_owner, &expected_work,
                        &parent.owner_principal_id, &parent.forge_work_id)?;
                    return Ok((Some(parent), None, None));
                }
                // Parallel workshop workers resume the owner with a separate,
                // durable correlation ID. Do not trust a prompt claiming to be
                // a worker result or an arbitrary continuation principal.
                let records = crate::agent_runtime::turn_worker::turn_worker_store()
                    .list_for_session(session.session_id.as_str());
                let record = records.iter().find(|record|
                    format!("{}-host-resume", record.work_id) == callback_turn
                    && record.identity_user_id.as_deref() == Some(expected_owner.as_str())
                    && record.parent_code_work_id.as_deref() == Some(expected_work.as_str())
                    && matches!(record.status,
                        crate::agent_runtime::turn_worker::TurnWorkStatus::Completed
                        | crate::agent_runtime::turn_worker::TurnWorkStatus::Failed
                        | crate::agent_runtime::turn_worker::TurnWorkStatus::Cancelled)
                    && record.disposition == crate::agent_runtime::turn_worker::TurnWorkDisposition::Parallel
                    && !record.synthesis_delivered
                    && !record.archived
                ).ok_or_else(|| anyhow::anyhow!("no native callback admission for this owned project; use a proposal for new scope"))?;
                let parent_turn = record.parent_turn_correlation_id.as_deref()
                    .filter(|id| !id.trim().is_empty())
                    .ok_or_else(|| anyhow::anyhow!("callback has no saved originating turn; use a proposal"))?;
                let parent_execution = crate::workshop_authority::execution_ref(
                    session.session_id.as_str(), parent_turn,
                ).map_err(anyhow::Error::msg)?;
                Ok((None, None, Some(parent_execution)))
            }).await?;
            if let Some(parent) = parent {
                self.hydrate(&principal, &parent, false).await?;
                let selection = parent
                    .context
                    .sources
                    .iter()
                    .find(|source| source.selection.session == parent.owner_session)
                    .ok_or_else(|| anyhow::anyhow!("callback has no saved owner context"))?;
                session_id = parent.owner_session.session_id;
                input.after_entry_seq = selection.selection.after_entry_seq.unwrap_or(0);
                input.through_entry_seq = selection.selection.through_entry_seq;
            }
            inherited_source = source;
            inherited_execution = parent_execution;
        }
        let source_session = session_id.clone();
        let source_owner = owner.clone();
        let mut after = input.after_entry_seq;
        let mut through = input.through_entry_seq;
        let (source, source_digest, inherited_range) = self
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
                    let entry = if let Some(parent_execution) = &inherited_execution {
                        // A workshop callback inherits the human turn that
                        // spawned it, never model-selected newer conversation.
                        source_for_admission(&entries, parent_execution, true, 0, u64::MAX)?
                    } else if let Some((reference, digest)) = &inherited_source {
                        entries
                            .iter()
                            .find(|entry| {
                                entry.entry_id == reference.entry_id
                                    && entry.entry_seq == reference.entry_seq
                                    && entry.content_digest == *digest
                                    && entry.turn.role == "user"
                            })
                            .ok_or_else(|| {
                                anyhow::anyhow!("saved human delegation source changed")
                            })?
                    } else {
                        source_for_admission(
                            &entries,
                            &execution,
                            direct && !callback,
                            after,
                            through,
                        )?
                    };
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
                        inherited_execution
                            .as_ref()
                            .map(|_| (entry.entry_seq - 1, entry.entry_seq)),
                    ))
                })())
            })
            .await??;
        if let Some((start, end)) = inherited_range {
            after = start;
            through = end;
        }
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
                    existing_agent_session_id: input.existing_agent_session_id,
                },
                chrono::Duration::hours(24),
                true,
            )
            .await?;
        let mut record = PeerHandoffRecord {
            request: proposal.request.clone(),
            session_title,
            policy: input.handoff,
            admission: input.admission,
            source,
            source_digest,
            sender_bot_id: turn.bot_identity().map(|bot| bot.bot_id().clone()),
            // Native handoffs are admitted immediately; this immutable window
            // bounds their callbacks, not pending proposal review.
            expires_at: proposal.expires_at,
        };
        if turn.cancellation().is_cancelled() {
            bail!("cancelled owner turn cannot admit a handoff");
        }
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
                        saved.expires_at = previous.expires_at;
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
            if turn.cancellation().is_cancelled() {
                bail!("cancelled owner turn cannot start delegation");
            }
            // Native human admission or a verified owner callback compiles the
            // exact scoped grant. Never promote a callback/worker to operator.
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
            if dispatch_now {
                self.dispatch(&principal, &record.request).await?;
                self.wake.notify_one();
            }
        }
        let store = self.store.clone();
        self.state.forge_execution.run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
            Ok((|| -> Result<_> {
                let view = store.handoff_view(&record.request.channel, &record.request.assignment_id)?;
                Ok(serde_json::json!({ "admission": record.admission, "proposal": proposal, "handoff": view,
                    "status": if dispatch_now { "delegated" } else if direct { "staged" } else { "proposed" },
                    "next_action": if direct && !dispatch_now { Some("Register this exact proposal with work.coordinate before dispatch; admit the other requested stages within this authorized project.") } else { None } }))
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
            .ok_or_else(|| anyhow::anyhow!("handoff unavailable. Allowed path: use cognition_peer_propose, or wait until the handoff is admitted."))?;
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
                        .ok_or_else(|| anyhow::anyhow!("handoff unavailable. Allowed path: use cognition_peer_propose, or wait until the handoff is admitted."))?;
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
    fn callback_delegation_keeps_original_owner_and_project_scope() {
        assert!(require_callback_scope("owner", "work-a", "owner", "work-a").is_ok());
        assert!(require_callback_scope("owner", "work-b", "owner", "work-a").is_err());
        assert!(require_callback_scope("other", "work-a", "owner", "work-a").is_err());
        assert!(require_callback_scope("other", "work-b", "owner", "work-a").is_err());
    }

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
        let mut later = human.clone();
        later.entry_seq = 3;
        later.caused_by.as_mut().unwrap().execution_id =
            medousa_types::ExecutionId::parse("later-human-turn").unwrap();
        // Callback admission uses the saved originating execution, even if a
        // newer user message arrived while the worker was running.
        let entries = [human.clone(), later.clone()];
        assert_eq!(
            source_for_admission(&entries, &execution, true, 0, u64::MAX)
                .unwrap()
                .entry_seq,
            human.entry_seq,
        );
        assert!(source_for_admission(&[later], &execution, true, 0, u64::MAX).is_err());
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
    fn deferred_stages_allow_owner_callbacks_but_never_dispatch_immediately() {
        assert!(PeerHandoffAdmission::Stage.requires_human_admission());
        assert!(!PeerHandoffAdmission::Stage.dispatches_immediately());
        assert!(PeerHandoffAdmission::Delegate.requires_human_admission());
        assert!(PeerHandoffAdmission::Delegate.dispatches_immediately());
        assert!(!PeerHandoffAdmission::Propose.requires_human_admission());
        assert!(!PeerHandoffAdmission::Propose.dispatches_immediately());
        let intent: PeerHandoffIntent = serde_json::from_value(serde_json::json!({
            "request_key":"review-stage", "instructions":"Review the exact executor output",
            "forge_work_id":"work-1", "after_entry_seq":0, "through_entry_seq":2,
            "admission":"stage"
        }))
        .unwrap();
        assert_eq!(intent.admission, PeerHandoffAdmission::Stage);
        // Test the very same admission guard used before any native effect.
        let callback = RequestPrincipal::continuation("owner");
        assert!(require_handoff_admission(&callback, PeerHandoffAdmission::Stage).is_ok());
        assert!(require_handoff_admission(&callback, PeerHandoffAdmission::Delegate).is_ok());
        assert!(!callback.capabilities().contains(Capability::AdminExecute));
        // The guard alone is not admission: callback provenance and project
        // scope are resolved before any effect in handoff_for_turn.
        let worker = RequestPrincipal::worker("owner");
        assert!(require_handoff_admission(&worker, PeerHandoffAdmission::Stage).is_err());
        assert!(require_handoff_admission(&worker, PeerHandoffAdmission::Delegate).is_err());
        assert!(require_handoff_admission(&worker, PeerHandoffAdmission::Propose).is_ok());
        let operator = RequestPrincipal::local_app(
            std::sync::Arc::from("test-operator"),
            crate::request_principal::TransportClass::Loopback,
        );
        assert!(require_handoff_admission(&operator, PeerHandoffAdmission::Stage).is_ok());
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
