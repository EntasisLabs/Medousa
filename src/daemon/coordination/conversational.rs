//! Model intent compilation. No approval or execution occurs here.
use super::*;
use medousa_types::coordination::context::conversation_range_digest;
use medousa_types::{
    ContextManifest, ContextManifestId, ConversationRangeSelection, ResolvedConversationRange,
    TranscriptEntryRef,
};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PeerProposalIntent {
    /// Stable request key; reuse for retries, never for different work.
    pub request_key: String,
    pub runtime: ExternalPeerRuntime,
    pub instructions: String,
    /// Exclusive lower bound from coordination discovery.
    pub after_entry_seq: u64,
    /// Inclusive committed upper bound from coordination discovery.
    pub through_entry_seq: u64,
    /// Request a separately approved result-only reply in this chat.
    pub continue_owner: bool,
    /// Exact active ACP session returned by discovery. Omit to start new work.
    pub existing_agent_session_id: Option<String>,
}

fn identity(owner: &str, session: &SessionRef, key: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"medousa/conversational-peer/v1\0");
    digest.update(serde_json::to_vec(&(owner, session, key)).expect("identity tuple"));
    format!("{:x}", digest.finalize())
}

impl LocalPeerDispatcher {
    pub async fn active_work_for_turn(
        &self,
        principal: &RequestPrincipal,
        include_terminal: bool,
    ) -> Result<serde_json::Value> {
        let owner = actor(principal)?;
        self.active_work_for_owner(owner, include_terminal).await
    }

    pub(crate) async fn active_work_for_owner(
        &self,
        owner: String,
        include_terminal: bool,
    ) -> Result<serde_json::Value> {
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        let runtime_id = self.local_runtime_id.clone();
        let forge = self.state.forge.clone();
        let owner_for_items = owner.clone();
        let (mut projects, truncated) = self
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                let mut items = forge
                    .list()?
                    .into_iter()
                    .filter(|item| {
                        item.owner == owner_for_items
                            && (include_terminal || !item.state.is_terminal())
                    })
                    .collect::<Vec<_>>();
                items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
                let truncated = items.len() > 128;
                items.truncate(128);
                let projects = items
                    .into_iter()
                    .map(|item| {
                        serde_json::json!({
                            "forge_work_id": item.id.to_string(),
                            "title": item.title,
                            "brief": item.brief,
                            "state": item.state.to_string(),
                            "workspace_mode": item.workspace_mode,
                            "active_attempt_count": item.active_attempts.len(),
                            "created_at": item.created_at,
                            "updated_at": item.updated_at,
                            "agent_sessions": [],
                        })
                    })
                    .collect::<Vec<_>>();
                Ok((projects, truncated))
            })
            .await?;
        let sessions = super::super::agents::discover_visible_agent_sessions(&owner).await;
        let mut unbound_agent_sessions = Vec::new();
        for session in sessions {
            if !include_terminal && (session.terminal || session.cancelled) {
                continue;
            }
            let Some(work_id) = session.forge_work_id.as_deref() else {
                unbound_agent_sessions.push(serde_json::to_value(session)?);
                continue;
            };
            if let Some(project) = projects.iter_mut().find(|project| {
                project
                    .get("forge_work_id")
                    .and_then(serde_json::Value::as_str)
                    == Some(work_id)
            }) {
                project["agent_sessions"]
                    .as_array_mut()
                    .expect("project agent_sessions is an array")
                    .push(serde_json::to_value(session)?);
            }
        }
        Ok(serde_json::json!({
            "coverage": {
                "kind": "current_workshop",
                "complete_mesh": false,
                "authority_id": authority,
                "execution_runtime_id": runtime_id,
                "sources": ["forge", "acp_live_registry"],
                "note": "This inventory covers the current workshop. Connected-workshop federation is not yet included."
            },
            "generated_at": Utc::now(),
            "projects": projects,
            "unbound_agent_sessions": unbound_agent_sessions,
            "truncated": truncated,
            "policy": "Read-only inventory. Discovery does not adopt, delegate, cancel, steer, or grant authority."
        }))
    }

    pub async fn proposal_tool_result(
        &self,
        principal: &RequestPrincipal,
        proposal: PeerAssignmentProposal,
    ) -> Result<serde_json::Value> {
        let owner = self.local_request(principal, &proposal.request)?;
        let store = self.store.clone();
        self.state.forge_execution.run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
            Ok((|| -> Result<_> {
                store.require_owner(&proposal.request.channel, &owner)?;
                let decision = store.proposal_decision(&proposal)?;
                    let binding = store.peer_if_recorded(&proposal.request.channel, &proposal.request.assignment_id)?;
                let status = proposal_status(decision.as_ref().map(|decision| decision.approved), binding.is_some(), proposal.expires_at <= Utc::now());
                Ok(serde_json::json!({"status": status, "proposal": proposal, "decision": decision, "binding": binding, "started": binding.is_some()}))
            })())
        }).await?
    }
    pub async fn discover_for_turn(
        &self,
        principal: &RequestPrincipal,
        session: SessionId,
    ) -> Result<serde_json::Value> {
        let owner = actor(principal)?;
        let owner_for_scope = owner.clone();
        let runtime = self.local_runtime_id.clone();
        let scope = self.state.forge_execution.run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
            Ok((|| -> Result<_> {
                if !crate::session_catalog::session_visible_to_profile(session.as_str(), &owner_for_scope) {
                    bail!("owner session is not visible");
                }
                let binding = crate::agent_mode_state::get_session_code_binding(session.as_str()).ok();
                let work = binding.filter(|binding| binding.execution_runtime_id.as_deref() == Some(runtime.as_str()))
                    .and_then(|binding| binding.work_id);
                let entries = crate::session_store::get_session_store().load_transcript_entries(&session);
                let through = entries.last().map_or(0, |entry| entry.entry_seq);
                Ok(serde_json::json!({"forge_work_id": work, "after_entry_seq": through.saturating_sub(32), "through_entry_seq": through}))
            })())
        }).await??;
        let peers = LocalPeerCall {
            host: self,
            principal: principal.clone(),
        }
        .discover()
        .await?;
        let adoptable = match scope.get("forge_work_id").and_then(|value| value.as_str()) {
            Some(work_id) => {
                super::super::agents::discover_adoptable_agent_sessions(&owner, work_id).await
            }
            None => Vec::new(),
        };
        Ok(
            serde_json::json!({"peers": peers, "adoptable_sessions": adoptable, "scope": scope, "policy": "Local workshop only. Propose requires this chat's bound Forge project. Proposal is not approval or execution. Adopt only an exact agent_session_id returned here."}),
        )
    }

    pub async fn propose_for_turn(
        &self,
        principal: &RequestPrincipal,
        session_id: SessionId,
        intent: PeerProposalIntent,
    ) -> Result<PeerAssignmentProposal> {
        let owner = actor(principal)?;
        if intent.request_key.trim().is_empty()
            || intent.request_key.len() > 128
            || intent.instructions.trim().is_empty()
            || intent.instructions.len() > 16 * 1024
            || intent.through_entry_seq <= intent.after_entry_seq
            || intent.through_entry_seq - intent.after_entry_seq > 256
        {
            bail!("proposal requires a bounded key, instructions, and 1-256 committed entries");
        }
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        let session = SessionRef {
            authority_id: authority.clone(),
            session_id,
        };
        let channel = CoordinationChannelRef {
            authority_id: authority.clone(),
            channel_id: format!("peer_chat_{}", identity(&owner, &session, "channel")),
        };
        let id = identity(&owner, &session, &intent.request_key);
        let assignment_id = format!("peer_assignment_{id}");
        let store = self.store.clone();
        let channel_copy = channel.clone();
        let saved_id = assignment_id.clone();
        let owner_copy = owner.clone();
        let session_copy = session.clone();
        let runtime = self.local_runtime_id.clone();
        let after = intent.after_entry_seq;
        let through = intent.through_entry_seq;
        let (work_id, source, previous) = self
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok((|| -> Result<_> {
                    if !crate::session_catalog::session_visible_to_profile(
                        session_copy.session_id.as_str(),
                        &owner_copy,
                    ) {
                        bail!("owner session is not visible");
                    }
                    let binding = crate::agent_mode_state::get_session_code_binding(
                        session_copy.session_id.as_str(),
                    )
                    .map_err(anyhow::Error::msg)?;
                    if binding.execution_runtime_id.as_deref() != Some(runtime.as_str()) {
                        bail!(
                            "bind this chat to a project on this workshop before proposing a peer"
                        );
                    }
                    let work = binding
                        .work_id
                        .filter(|id| !id.trim().is_empty())
                        .ok_or_else(|| anyhow::anyhow!("chat has no governed Forge work item"))?;
                    let entries: Vec<_> = crate::session_store::get_session_store()
                        .load_transcript_entries(&session_copy.session_id)
                        .into_iter()
                        .filter(|entry| entry.entry_seq > after && entry.entry_seq <= through)
                        .collect();
                    if entries.len() as u64 != through - after
                        || entries
                            .iter()
                            .enumerate()
                            .any(|(offset, entry)| entry.entry_seq != after + offset as u64 + 1)
                    {
                        bail!("selected context is not contiguous committed history");
                    }
                    let refs: Vec<_> = entries
                        .iter()
                        .map(|entry| {
                            (
                                TranscriptEntryRef {
                                    session: session_copy.clone(),
                                    entry_id: entry.entry_id.clone(),
                                    entry_seq: entry.entry_seq,
                                },
                                entry.content_digest.clone(),
                            )
                        })
                        .collect();
                    let source = ResolvedConversationRange {
                        selection: ConversationRangeSelection {
                            session: session_copy.clone(),
                            after_entry_seq: Some(after),
                            through_entry_seq: through,
                        },
                        selection_digest: conversation_range_digest(
                            &session_copy,
                            refs.iter()
                                .map(|(reference, digest)| (reference, digest.as_str())),
                        ),
                    };
                    store.create_channel(&CoordinationChannelRecord {
                        channel: channel_copy.clone(),
                        owner_principal_id: owner_copy.clone(),
                        member_principal_ids: vec![owner_copy],
                        attached_sessions: vec![session_copy],
                    })?;
                    let previous = store.proposal_for_assignment(&channel_copy, &saved_id)?;
                    Ok((work, source, previous))
                })())
            })
            .await??;
        if let Some(agent_session_id) = intent.existing_agent_session_id.as_deref() {
            super::super::agents::require_adoptable_agent_session(
                &owner,
                &work_id,
                super::runtime_kind(intent.runtime).as_str(),
                agent_session_id,
            )
            .await?;
        }
        let created_at = previous
            .as_ref()
            .map_or_else(Utc::now, |proposal| proposal.request.context.created_at);
        let request = ExternalPeerAssignmentRequest {
            assignment_id,
            idempotency_key: format!("peer_command_{id}"),
            owner_principal_id: owner.clone(),
            owner_session: session,
            execution_session: self
                .execution_session(&channel, &format!("peer_assignment_{id}"))?,
            channel,
            target: ExternalPeerTarget {
                authority_id: authority,
                execution_runtime_id: self.local_runtime_id.clone(),
                runtime: intent.runtime,
            },
            context: ContextManifest {
                manifest_id: ContextManifestId::parse(format!("ctx_{}", &id[..32]))
                    .map_err(anyhow::Error::msg)?,
                sources: vec![source],
                created_by: owner,
                created_at,
            },
            instructions: intent.instructions,
            execution_grant_id: format!("peer_grant_{id}"),
            forge_work_id: work_id,
            existing_agent_session_id: intent.existing_agent_session_id,
        };
        if let Some(proposal) = previous {
            if proposal.request != request || proposal.continue_owner != intent.continue_owner {
                bail!("request key already belongs to a different immutable proposal");
            }
            self.hydrate(principal, &request, false).await?;
            // Repair an interrupted index write without altering the snapshot.
            let store = self.store.clone();
            let saved = proposal.clone();
            self.state
                .forge_execution
                .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                    Ok(store.record_proposal(&saved))
                })
                .await??;
            return Ok(proposal);
        }
        self.propose_assignment(
            principal,
            request,
            created_at + chrono::Duration::hours(1),
            intent.continue_owner,
        )
        .await
    }
}

fn proposal_status(approved: Option<bool>, has_custody: bool, expired: bool) -> &'static str {
    if has_custody {
        "peer_custody_recorded"
    } else if approved == Some(false) {
        "declined_requires_new_request_key"
    } else if expired {
        "expired_requires_new_request_key"
    } else if approved == Some(true) {
        "approved_requires_operator_start"
    } else {
        "requires_operator_review"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retries_report_custody_denial_expiry_and_pending_without_claiming_completion() {
        assert_eq!(
            proposal_status(None, false, false),
            "requires_operator_review"
        );
        assert_eq!(
            proposal_status(Some(true), false, false),
            "approved_requires_operator_start"
        );
        assert_eq!(
            proposal_status(Some(false), false, false),
            "declined_requires_new_request_key"
        );
        assert_eq!(
            proposal_status(Some(true), false, true),
            "expired_requires_new_request_key"
        );
        assert_eq!(
            proposal_status(Some(true), true, true),
            "peer_custody_recorded"
        );
    }
    #[test]
    fn model_intent_cannot_supply_execution_authority() {
        let base = serde_json::json!({"request_key":"review", "runtime":"cursor", "instructions":"Review changes", "after_entry_seq":0, "through_entry_seq":3, "continue_owner":true});
        assert!(serde_json::from_value::<PeerProposalIntent>(base.clone()).is_ok());
        let mut adoption = base.clone();
        adoption["existing_agent_session_id"] = serde_json::json!("agent-existing");
        assert!(serde_json::from_value::<PeerProposalIntent>(adoption).is_ok());
        for field in [
            "owner_principal_id",
            "session_id",
            "execution_runtime_id",
            "execution_grant_id",
            "approved",
            "context",
        ] {
            let mut spoofed = base.clone();
            spoofed[field] = serde_json::json!("spoofed");
            assert!(
                serde_json::from_value::<PeerProposalIntent>(spoofed).is_err(),
                "{field}"
            );
        }
    }
    #[test]
    fn request_identity_is_owner_session_and_authority_scoped() {
        let session = SessionRef {
            authority_id: AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap(),
            session_id: SessionId::parse("ses_owner").unwrap(),
        };
        assert_eq!(
            identity("owner", &session, "key"),
            identity("owner", &session, "key")
        );
        assert_ne!(
            identity("owner", &session, "key"),
            identity("other", &session, "key")
        );
        assert_ne!(
            identity("owner", &session, "key"),
            identity("owner", &session, "other")
        );
        let mut foreign = session.clone();
        foreign.authority_id = AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
        assert_ne!(
            identity("owner", &session, "key"),
            identity("owner", &foreign, "key")
        );
        foreign = session.clone();
        foreign.session_id = SessionId::parse("ses_other").unwrap();
        assert_ne!(
            identity("owner", &session, "key"),
            identity("owner", &foreign, "key")
        );
    }
}
