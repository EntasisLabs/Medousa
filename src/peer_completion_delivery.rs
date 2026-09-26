//! Validate and apply a signed remote peer completion to its originating chat.
//!
//! A completion is already a committed assistant answer on the workshop. This
//! module records that answer as provenance in the source session; it never
//! starts another owner turn.

use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, bail};
pub use medousa_forge::execution::ExecutionClass;
use medousa_forge::execution::ForgeExecutionService;
use medousa_types::coordination::ExternalPeerAssignmentBinding;
use medousa_types::session::{ExecutionId, ExecutionRef, TranscriptEntry};

use crate::peer_coordination_mesh::{
    RemotePeerCompletionResult, RemotePeerOriginAssociation, remote_peer_proposal_request_digest,
};
use crate::session_store::{SessionStore, TranscriptAppend};

static COMPLETION_EXECUTION: OnceLock<Arc<ForgeExecutionService>> = OnceLock::new();
static APPLY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A pulled response must fit the store admission used for signature validation.
pub const MAX_REMOTE_COMPLETION_PAYLOAD_BYTES: usize =
    medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES;

/// Shared bounded admission for synchronous transcript and visibility reads
/// used by both the daemon and the embedded Home daemon.
pub fn completion_execution_service() -> Arc<ForgeExecutionService> {
    Arc::clone(COMPLETION_EXECUTION.get_or_init(|| Arc::new(ForgeExecutionService::new())))
}

/// Apply one verified remote answer to the exact source session. Returns true
/// when newly appended and false for an exact replay after a prior commit.
pub async fn apply_remote_peer_completion(
    store: Arc<dyn SessionStore>,
    origin: &RemotePeerOriginAssociation,
    result: &RemotePeerCompletionResult,
    current_authority: &medousa_types::session::AuthorityId,
    owner_profile_id: &str,
) -> Result<bool> {
    validate_remote_peer_completion(origin, result, current_authority, owner_profile_id)?;

    // One in-process fence closes the read/append race for concurrent polls.
    // The deterministic `caused_by` reference below makes crash replay
    // idempotent after the append commits.
    let _apply = APPLY_LOCK.lock().await;
    append_remote_peer_completion_once(
        store,
        origin.request.owner_session_id.clone(),
        origin.request.source_execution.authority_id.clone(),
        origin.owner_profile_id.clone(),
        result.clone(),
        crate::session_catalog::session_visible_to_profile,
    )
    .await
}

async fn append_remote_peer_completion_once<F>(
    store: Arc<dyn SessionStore>,
    source_session: medousa_types::SessionId,
    source_authority: medousa_types::AuthorityId,
    expected_profile: String,
    result: RemotePeerCompletionResult,
    visible: F,
) -> Result<bool>
where
    F: Fn(&str, &str) -> bool + Send + Sync + 'static,
{
    let completion_id = completion_execution_id(&result)?;
    let lookup_store = Arc::clone(&store);
    let visible_for_read = Arc::new(visible);
    let read_session = source_session.clone();
    let read_profile = expected_profile.clone();
    let lookup_cause = ExecutionRef {
        authority_id: source_authority.clone(),
        session_id: source_session.clone(),
        execution_id: completion_id.clone(),
    };
    let existing = completion_execution_service()
        .run(
            ExecutionClass::StoreIo,
            medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
            move || {
                Ok((|| -> Result<_> {
                    if !visible_for_read(read_session.as_str(), &read_profile) {
                        bail!("originating chat is no longer visible to its owner profile");
                    }
                    find_completion_entry(&*lookup_store, &read_session, &lookup_cause)
                })())
            },
        )
        .await
        .context("read originating transcript under bounded store admission")??;

    let caused_by = ExecutionRef {
        authority_id: source_authority,
        session_id: source_session.clone(),
        execution_id: completion_id,
    };
    if let Some(entry) = existing.as_ref() {
        validate_existing_completion(entry, &result)?;
        return Ok(false);
    }

    store
        .append_transcript_batch(
            &source_session,
            &[TranscriptAppend {
                turn: result.committed_decision,
                caused_by: Some(caused_by),
                existing_entry_id: None,
                source: Some(result.owner_acknowledgment.decision),
                expected_digest: Some(result.committed_decision_digest),
            }],
        )
        .await
        .context("append committed remote answer to originating session")?;
    Ok(true)
}

fn find_completion_entry(
    store: &dyn SessionStore,
    session: &medousa_types::SessionId,
    caused_by: &ExecutionRef,
) -> Result<Option<TranscriptEntry>> {
    const PAGE_SIZE: usize = 128;
    const MAX_PAGES: usize = 80;
    let mut before = None;
    for _ in 0..MAX_PAGES {
        let page = store.load_transcript_entries_page(session, PAGE_SIZE, before);
        if let Some(entry) = page
            .entries
            .into_iter()
            .find(|entry| entry.caused_by.as_ref() == Some(caused_by))
        {
            return Ok(Some(entry));
        }
        match page.next_cursor {
            Some(cursor) => before = Some(cursor),
            None => return Ok(None),
        }
    }
    bail!("transcript replay scan exceeded its bounded page budget")
}

/// Validate the result against durable source-side proposal evidence and the
/// caller's current authority/profile. Visibility itself is rechecked under
/// bounded store admission immediately before transcript access.
pub fn validate_remote_peer_completion(
    origin: &RemotePeerOriginAssociation,
    result: &RemotePeerCompletionResult,
    current_authority: &medousa_types::session::AuthorityId,
    owner_profile_id: &str,
) -> Result<()> {
    let request_digest = remote_peer_proposal_request_digest(&origin.request)?;
    let target_device_id = &origin.target_device_id;
    if origin.schema_version != 1
        || origin.request_digest != request_digest
        || origin.source_device_id.trim().is_empty()
        || origin.owner_profile_id.trim().is_empty()
        || origin.owner_profile_id != owner_profile_id
        || origin.request.owner_session_id != origin.request.source_execution.session_id
        || origin.request.source_execution.authority_id != *current_authority
        || result.schema_version
            != crate::peer_coordination_mesh::PEER_COMPLETION_RESULT_SCHEMA_VERSION
        || result.source_device_id != origin.source_device_id
        || result.request_digest != request_digest
        || origin.proposal_id.as_deref() != Some(result.proposal_id.as_str())
        || origin.proposal_request_digest.as_deref()
            != Some(result.proposal_request_digest.as_str())
        || origin.remote_owner_session.as_ref()
            != Some(&result.owner_acknowledgment.decision.session)
        || origin.assignment_id.as_deref() != Some(result.binding.assignment_id.as_str())
        || origin
            .expected_binding
            .as_ref()
            .is_none_or(|expected| !expected.matches(&result.binding))
        || result.binding.target.execution_runtime_id != origin.request.target_runtime_id
        || result.binding.target.execution_runtime_id != *target_device_id
        || result.binding.target.runtime != origin.request.runtime
        || origin
            .binding
            .as_ref()
            .is_some_and(|binding| binding != &result.binding)
        || result.receipt.binding != result.binding
        || result.receipt.receipt_id != terminal_receipt_id(&result.binding)
        || result.owner_acknowledgment.intake.receipt != result.receipt
        || result.owner_acknowledgment.decision.entry_seq == 0
        || result.owner_acknowledgment.intake.turn_id
            != format!(
                "{}-owner-{}",
                result.receipt.receipt_id, result.owner_acknowledgment.intake.attempt
            )
        || result
            .owner_acknowledgment
            .decision_digest
            .trim()
            .is_empty()
        || result.owner_acknowledgment.intake.attempt >= 8
        || result.owner_acknowledgment.decision_digest != result.committed_decision_digest
        || result.committed_decision.role != "assistant"
        || result.committed_decision.speaker_profile_id.is_some()
        || result.committed_decision.content.trim().is_empty()
        || crate::session_store::transcript_content_digest(&result.committed_decision)?
            != result.committed_decision_digest
    {
        bail!("remote completion does not match the durable source route and committed decision");
    }
    Ok(())
}

fn completion_execution_id(result: &RemotePeerCompletionResult) -> Result<ExecutionId> {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"medousa/remote-peer-completion-execution/v1\0");
    hash.update(result.request_digest.as_bytes());
    hash.update(result.proposal_id.as_bytes());
    hash.update(result.receipt.receipt_id.as_bytes());
    ExecutionId::parse(format!("peer-result-{:x}", hash.finalize())).map_err(anyhow::Error::from)
}

fn validate_existing_completion(
    entry: &TranscriptEntry,
    result: &RemotePeerCompletionResult,
) -> Result<()> {
    if entry.turn.role != "assistant"
        || entry.turn.content != result.committed_decision.content
        || entry.content_digest != result.committed_decision_digest
        || crate::session_store::transcript_content_digest(&entry.turn)?
            != result.committed_decision_digest
        || entry.source.as_ref() != Some(&result.owner_acknowledgment.decision)
    {
        bail!("completion correlation already exists with different content or provenance");
    }
    Ok(())
}

/// Stable receipt identity shared by full-daemon and embedded clients.
pub fn terminal_receipt_id(binding: &ExternalPeerAssignmentBinding) -> String {
    medousa_types::coordination::peer_terminal_receipt_id(binding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medousa_types::coordination::{
        CoordinationChannelRef, ExternalPeerAssignmentReceipt, ExternalPeerRuntime,
        ExternalPeerTarget, PeerAssignmentOutcome, PeerOwnerIntakeAcknowledgment,
        PeerOwnerIntakeAttempt,
    };
    use medousa_types::session::{
        ContextManifest, ContextManifestId, ConversationTurn, ExecutionId, SessionRef,
        TranscriptEntryId, TranscriptEntryRef,
    };

    fn auth(letter: char) -> medousa_types::AuthorityId {
        medousa_types::AuthorityId::parse(format!("auth_{}", letter.to_string().repeat(64)))
            .unwrap()
    }

    fn session(authority_id: medousa_types::AuthorityId, id: &str) -> SessionRef {
        SessionRef {
            authority_id,
            session_id: medousa_types::SessionId::parse(id).unwrap(),
        }
    }

    fn fixture() -> (
        RemotePeerOriginAssociation,
        RemotePeerCompletionResult,
        medousa_types::AuthorityId,
    ) {
        let source_authority = auth('a');
        let remote_authority = auth('b');
        let source_session =
            medousa_types::SessionId::parse(format!("session-source-{}", uuid::Uuid::new_v4()))
                .unwrap();
        let request = crate::peer_coordination_mesh::RemotePeerProposalRequest {
            schema_version: crate::peer_coordination_mesh::REMOTE_PEER_PROPOSAL_SCHEMA_VERSION,
            source_execution: ExecutionRef {
                authority_id: source_authority.clone(),
                session_id: source_session.clone(),
                execution_id: ExecutionId::parse("turn-source").unwrap(),
            },
            owner_session_id: source_session.clone(),
            target_runtime_id: "runtime-workshop".into(),
            forge_work_id: "work-one".into(),
            request_key: "request-one".into(),
            runtime: ExternalPeerRuntime::Cursor,
            instructions: "Review the result".into(),
            continue_owner: true,
            existing_agent_session_id: None,
            context: crate::delegated_task::DelegatedContextGrant {
                manifest: ContextManifest {
                    manifest_id: ContextManifestId::parse("ctx_0123456789abcdef0123456789abcdef")
                        .unwrap(),
                    sources: vec![],
                    created_by: "test".into(),
                    created_at: Utc::now(),
                },
                entries: vec![],
            },
        };
        let request_digest =
            crate::peer_coordination_mesh::remote_peer_proposal_request_digest(&request).unwrap();
        let channel = CoordinationChannelRef {
            authority_id: remote_authority.clone(),
            channel_id: "channel-remote".into(),
        };
        let owner_session = session(remote_authority.clone(), "session-remote-owner");
        let binding = ExternalPeerAssignmentBinding {
            assignment_id: "assignment-remote".into(),
            owner_principal_id: "profile-owner".into(),
            channel: channel.clone(),
            target: ExternalPeerTarget {
                authority_id: remote_authority.clone(),
                execution_runtime_id: "runtime-workshop".into(),
                runtime: ExternalPeerRuntime::Cursor,
            },
            execution_session: session(remote_authority.clone(), "session-agent"),
            agent_session_id: "agent-one".into(),
        };
        let receipt = ExternalPeerAssignmentReceipt {
            receipt_id: terminal_receipt_id(&binding),
            binding: binding.clone(),
            outcome: PeerAssignmentOutcome::Completed,
            result: "review finished".into(),
        };
        let decision = ConversationTurn::plain(
            "assistant",
            "The review is complete.".into(),
            Utc::now(),
            vec![],
            None,
        );
        let decision_digest = crate::session_store::transcript_content_digest(&decision).unwrap();
        let acknowledgment = PeerOwnerIntakeAcknowledgment {
            intake: PeerOwnerIntakeAttempt {
                receipt: receipt.clone(),
                attempt: 0,
                turn_id: format!("{}-owner-0", receipt.receipt_id),
            },
            decision: TranscriptEntryRef {
                session: owner_session.clone(),
                entry_id: TranscriptEntryId::parse("ent_0123456789abcdef0123456789abcdef").unwrap(),
                entry_seq: 4,
            },
            decision_digest: decision_digest.clone(),
        };
        let proposal_id = "proposal-one".to_string();
        let proposal_request_digest = format!("sha256:{}", "c".repeat(64));
        let expected_binding = crate::peer_coordination_mesh::RemotePeerExpectedBinding {
            assignment_id: binding.assignment_id.clone(),
            owner_principal_id: binding.owner_principal_id.clone(),
            channel,
            target: binding.target.clone(),
            execution_session: binding.execution_session.clone(),
        };
        let origin = RemotePeerOriginAssociation {
            schema_version: 1,
            source_device_id: "phone-source".into(),
            target_device_id: "runtime-workshop".into(),
            request_digest: request_digest.clone(),
            request,
            owner_profile_id: "profile-source".into(),
            assignment_id: Some(binding.assignment_id.clone()),
            expected_binding: Some(expected_binding),
            completion_applied: false,
            proposal_id: Some(proposal_id.clone()),
            proposal_request_digest: Some(proposal_request_digest.clone()),
            binding: None,
            remote_owner_session: Some(owner_session),
        };
        let result = RemotePeerCompletionResult {
            schema_version: crate::peer_coordination_mesh::PEER_COMPLETION_RESULT_SCHEMA_VERSION,
            source_device_id: origin.source_device_id.clone(),
            request_digest,
            proposal_id,
            proposal_request_digest,
            binding,
            receipt,
            owner_acknowledgment: acknowledgment,
            committed_decision: decision,
            committed_decision_digest: decision_digest,
        };
        (origin, result, source_authority)
    }

    #[test]
    fn source_validation_rejects_changed_authority_profile_request_receipt_digest_or_binding() {
        let (origin, result, source_authority) = fixture();
        validate_remote_peer_completion(&origin, &result, &source_authority, "profile-source")
            .unwrap();

        assert!(
            validate_remote_peer_completion(&origin, &result, &auth('d'), "profile-source")
                .is_err()
        );
        assert!(
            validate_remote_peer_completion(&origin, &result, &source_authority, "other-profile")
                .is_err()
        );

        let mut changed_origin = origin.clone();
        changed_origin.request.instructions.push('!');
        assert!(
            validate_remote_peer_completion(
                &changed_origin,
                &result,
                &source_authority,
                "profile-source"
            )
            .is_err()
        );

        let mut changed = result.clone();
        changed.receipt.result.push('!');
        assert!(
            validate_remote_peer_completion(&origin, &changed, &source_authority, "profile-source")
                .is_err()
        );
        let mut changed = result.clone();
        changed.committed_decision_digest.push('!');
        assert!(
            validate_remote_peer_completion(&origin, &changed, &source_authority, "profile-source")
                .is_err()
        );
        let mut changed = result.clone();
        changed.binding.assignment_id.push('!');
        assert!(
            validate_remote_peer_completion(&origin, &changed, &source_authority, "profile-source")
                .is_err()
        );
    }

    #[tokio::test]
    async fn canonical_append_is_idempotent_after_reopen_and_rejects_conflicting_replay() {
        let temp = tempfile::tempdir().unwrap();
        let (origin, result, _) = fixture();
        let first_store =
            crate::session_store::test_file_session_store_at(temp.path().to_path_buf());
        assert!(
            append_remote_peer_completion_once(
                Arc::clone(&first_store),
                origin.request.owner_session_id.clone(),
                origin.request.source_execution.authority_id.clone(),
                origin.owner_profile_id.clone(),
                result.clone(),
                |_, _| true,
            )
            .await
            .unwrap()
        );
        assert_eq!(
            first_store
                .load_transcript_entries(&origin.request.owner_session_id)
                .len(),
            1
        );

        let reopened = crate::session_store::test_file_session_store_at(temp.path().to_path_buf());
        assert!(
            !append_remote_peer_completion_once(
                Arc::clone(&reopened),
                origin.request.owner_session_id.clone(),
                origin.request.source_execution.authority_id.clone(),
                origin.owner_profile_id.clone(),
                result.clone(),
                |_, _| true,
            )
            .await
            .unwrap()
        );
        assert_eq!(
            reopened
                .load_transcript_entries(&origin.request.owner_session_id)
                .len(),
            1
        );

        let mut conflict = result;
        conflict.committed_decision.content.push('!');
        conflict.committed_decision_digest =
            crate::session_store::transcript_content_digest(&conflict.committed_decision).unwrap();
        conflict.owner_acknowledgment.decision_digest = conflict.committed_decision_digest.clone();
        assert!(
            append_remote_peer_completion_once(
                reopened,
                origin.request.owner_session_id.clone(),
                origin.request.source_execution.authority_id,
                origin.owner_profile_id,
                conflict,
                |_, _| true,
            )
            .await
            .is_err()
        );
    }
}
