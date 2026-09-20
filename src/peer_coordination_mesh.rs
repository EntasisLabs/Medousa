//! Signed mesh contract for preparing an external-peer proposal on the
//! workshop that owns the governed work. This transfers context and intent;
//! it never carries an operator decision or execution grant.

use medousa_types::coordination::{ExternalPeerRuntime, PeerAssignmentProposal};
use medousa_types::session::{ExecutionRef, SessionId};
use serde::{Deserialize, Serialize};

use crate::delegated_task::{DelegatedContextGrant, DelegatedTaskError, validate_context_grant};

#[cfg(feature = "full-daemon")]
use crate::session_store::{
    DerivationCommitOutcome, DerivationCommitRequest, SessionStore, TranscriptAppend,
};
#[cfg(feature = "full-daemon")]
use medousa_types::session::{DerivationId, SessionDerivation};

pub const REMOTE_PEER_PROPOSAL_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePeerProposalRequest {
    pub schema_version: u32,
    pub source_execution: ExecutionRef,
    pub owner_session_id: SessionId,
    pub target_runtime_id: String,
    pub forge_work_id: String,
    pub request_key: String,
    pub runtime: ExternalPeerRuntime,
    pub instructions: String,
    pub continue_owner: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_agent_session_id: Option<String>,
    pub context: DelegatedContextGrant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePeerProposalResponse {
    pub schema_version: u32,
    pub proposal: PeerAssignmentProposal,
}

pub fn validate_remote_peer_proposal_request(
    request: &RemotePeerProposalRequest,
) -> Result<(), DelegatedTaskError> {
    if request.schema_version != REMOTE_PEER_PROPOSAL_SCHEMA_VERSION {
        return Err(DelegatedTaskError::invalid(
            "unsupported remote peer proposal schema",
        ));
    }
    for (label, value, max) in [
        (
            "target runtime",
            request.target_runtime_id.as_str(),
            256usize,
        ),
        ("Forge work", request.forge_work_id.as_str(), 512usize),
        ("request key", request.request_key.as_str(), 128usize),
        (
            "instructions",
            request.instructions.as_str(),
            16 * 1024usize,
        ),
    ] {
        if value.trim().is_empty() || value.trim() != value || value.len() > max {
            return Err(DelegatedTaskError::invalid(format!(
                "remote peer proposal requires a bounded exact {label}"
            )));
        }
    }
    if let Some(session_id) = request.existing_agent_session_id.as_deref()
        && (session_id.trim().is_empty()
            || session_id.trim() != session_id
            || session_id.len() > 512)
    {
        return Err(DelegatedTaskError::invalid(
            "existing agent session id must be exact and bounded",
        ));
    }
    validate_context_grant(&request.context)?;
    let source = request
        .context
        .manifest
        .sources
        .first()
        .expect("validated context has one source");
    if source.selection.session.session_id != request.owner_session_id
        || request.source_execution.session_id != request.owner_session_id
        || source.selection.session.authority_id != request.source_execution.authority_id
    {
        return Err(DelegatedTaskError::conflict(
            "proposal context and source execution must belong to the owner session authority",
        ));
    }
    Ok(())
}

#[cfg(feature = "full-daemon")]
fn digest(domain: &[u8], values: &[&[u8]]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(domain);
    for value in values {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
    }
    format!("{:x}", hash.finalize())
}

/// Materialize the phone's immutable context into a request-scoped shadow
/// session in the destination authority. This keeps repeated assignments from
/// one chat immutable without conflating remote and local authority.
#[cfg(feature = "full-daemon")]
pub async fn materialize_remote_peer_proposal_context(
    store: &dyn SessionStore,
    target_authority: &medousa_types::AuthorityId,
    sender_device_id: &str,
    owner_profile_id: &str,
    request: &RemotePeerProposalRequest,
) -> Result<DerivationCommitOutcome, DelegatedTaskError> {
    validate_remote_peer_proposal_request(request)?;
    let key_digest = format!(
        "sha256:{}",
        digest(
            b"medousa/remote-peer-proposal-key/v1\0",
            &[
                target_authority.as_str().as_bytes(),
                sender_device_id.as_bytes(),
                request.owner_session_id.as_str().as_bytes(),
                request.request_key.as_bytes(),
            ],
        )
    );
    let request_bytes = serde_json::to_vec(request)
        .map_err(|error| DelegatedTaskError::internal(error.to_string()))?;
    let request_digest = format!(
        "sha256:{}",
        digest(
            b"medousa/remote-peer-proposal-request/v1\0",
            &[&request_bytes],
        )
    );
    let derivation_id = DerivationId::parse(format!(
        "drv_{}",
        &digest(
            b"medousa/remote-peer-proposal-derivation/v1\0",
            &[key_digest.as_bytes()],
        )[..32]
    ))
    .map_err(|error| DelegatedTaskError::internal(error.to_string()))?;
    let shadow_session_id = SessionId::parse(format!(
        "session_peer_{}",
        &digest(
            b"medousa/remote-peer-proposal-session/v1\0",
            &[
                target_authority.as_str().as_bytes(),
                sender_device_id.as_bytes(),
                request.owner_session_id.as_str().as_bytes(),
                request.request_key.as_bytes(),
            ],
        )[..32]
    ))
    .map_err(|error| DelegatedTaskError::internal(error.to_string()))?;
    let target_session = medousa_types::SessionRef {
        authority_id: target_authority.clone(),
        session_id: shadow_session_id,
    };
    let outcome = store
        .materialize_derivation(&DerivationCommitRequest {
            derivation: SessionDerivation {
                derivation_id,
                target_session: target_session.clone(),
                manifest: request.context.manifest.clone(),
                intent: "mesh.peer.proposal".to_string(),
                caused_by: Some(request.source_execution.clone()),
                created_by: format!("peer:{}", sender_device_id.trim()),
                created_at: chrono::Utc::now(),
            },
            idempotency_key_digest: key_digest,
            request_digest,
            entries: request
                .context
                .entries
                .iter()
                .map(|entry| TranscriptAppend {
                    turn: entry.turn.clone(),
                    caused_by: entry.caused_by.clone(),
                    existing_entry_id: Some(entry.source.entry_id.clone()),
                    source: Some(entry.source.clone()),
                    expected_digest: Some(entry.content_digest.clone()),
                })
                .collect(),
        })
        .await
        .map_err(DelegatedTaskError::from)?;
    let turns = store
        .load_transcript_entries(&target_session.session_id)
        .into_iter()
        .map(|entry| entry.turn)
        .collect::<Vec<_>>();
    crate::session_catalog::replace_derived_session(
        &target_session.session_id,
        Some("Assistant handoff".to_string()),
        owner_profile_id,
        &turns,
    );
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medousa_types::session::{
        AuthorityId, ContextManifest, ContextManifestId, ConversationRangeSelection,
        ConversationTurn, ExecutionId, ResolvedConversationRange, SessionRef, TranscriptEntryId,
        TranscriptEntryRef,
    };

    fn request() -> RemotePeerProposalRequest {
        let authority = AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap();
        let session_id = SessionId::parse("session-phone").unwrap();
        let session = SessionRef {
            authority_id: authority.clone(),
            session_id: session_id.clone(),
        };
        let source = TranscriptEntryRef {
            session: session.clone(),
            entry_id: TranscriptEntryId::parse("ent_0123456789abcdef0123456789abcdef")
                .unwrap(),
            entry_seq: 1,
        };
        let turn = ConversationTurn::plain(
            "user",
            "Send this for review".to_string(),
            Utc::now(),
            vec![],
            None,
        );
        let content_digest = crate::session_store::transcript_content_digest(&turn).unwrap();
        let selection_digest = medousa_types::coordination::context::conversation_range_digest(
            &session,
            [(&source, content_digest.as_str())],
        );
        RemotePeerProposalRequest {
            schema_version: REMOTE_PEER_PROPOSAL_SCHEMA_VERSION,
            source_execution: ExecutionRef {
                authority_id: authority,
                session_id: session_id.clone(),
                execution_id: ExecutionId::parse("turn-one").unwrap(),
            },
            owner_session_id: session_id,
            target_runtime_id: "runtime-mac".into(),
            forge_work_id: "work-one".into(),
            request_key: "cursor-review".into(),
            runtime: ExternalPeerRuntime::Cursor,
            instructions: "Review the current implementation.".into(),
            continue_owner: true,
            existing_agent_session_id: None,
            context: DelegatedContextGrant {
                manifest: ContextManifest {
                    manifest_id: ContextManifestId::parse(
                        "ctx_0123456789abcdef0123456789abcdef",
                    )
                    .unwrap(),
                    sources: vec![ResolvedConversationRange {
                        selection: ConversationRangeSelection {
                            session,
                            after_entry_seq: None,
                            through_entry_seq: 1,
                        },
                        selection_digest,
                    }],
                    created_by: "daemon:phone".into(),
                    created_at: Utc::now(),
                },
                entries: vec![crate::delegated_task::DelegatedContextEntry {
                    source,
                    caused_by: None,
                    content_digest,
                    turn,
                }],
            },
        }
    }

    #[test]
    fn proposal_transfer_is_bounded_and_carries_no_decision() {
        let request = request();
        validate_remote_peer_proposal_request(&request).unwrap();
        let wire = serde_json::to_value(request).unwrap();
        assert!(wire.get("approved").is_none());
        assert!(wire.get("executionGrantId").is_none());
    }

    #[test]
    fn proposal_transfer_rejects_context_from_another_session() {
        let mut request = request();
        request.owner_session_id = SessionId::parse("session-other").unwrap();
        assert!(
            validate_remote_peer_proposal_request(&request)
                .unwrap_err()
                .to_string()
                .contains("owner session")
        );
    }
}
