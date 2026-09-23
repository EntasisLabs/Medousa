//! Signed mesh contract for preparing an external-peer assignment on the
//! workshop that owns the governed work. The request transfers context and
//! intent, never an operator decision or execution grant. The destination may
//! return a binding when its own stored owner policy admits unattended launch.

use anyhow::{Context, Result, bail};
use medousa_types::coordination::{
    ExternalPeerAssignmentBinding, ExternalPeerAssignmentReceipt, ExternalPeerRuntime,
    PeerAssignmentProposal, PeerOwnerIntakeAcknowledgment,
};
use medousa_types::session::{ConversationTurn, ExecutionRef, SessionId, SessionRef};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_request_digest: Option<String>,
    pub proposal: PeerAssignmentProposal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ExternalPeerAssignmentBinding>,
}

pub const PEER_COMPLETION_RESULT_SCHEMA_VERSION: u32 = 1;
pub const PEER_COMPLETION_QUERY_SCHEMA_VERSION: u32 = 2;
const PEER_COMPLETION_ASSOCIATIONS_CAP: usize = 2_000;
const PEER_COMPLETION_ASSOCIATIONS_FILE_MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PeerCompletionAssociationFile {
    #[serde(default)]
    origins: Vec<RemotePeerOriginAssociation>,
    #[serde(default)]
    destinations: Vec<RemotePeerCompletionDestination>,
    #[serde(default)]
    origin_sync_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePeerCompletionQuery {
    pub schema_version: u32,
    pub source_device_id: String,
    pub source_request_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_request_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePeerCompletionResponse {
    pub schema_version: u32,
    pub source_request_digest: String,
    #[serde(default)]
    pub proposal: Option<RemotePeerProposalResponse>,
    #[serde(default)]
    pub completion: Option<RemotePeerCompletionResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePeerExpectedBinding {
    pub assignment_id: String,
    pub owner_principal_id: String,
    pub channel: medousa_types::coordination::CoordinationChannelRef,
    pub target: medousa_types::coordination::ExternalPeerTarget,
    pub execution_session: SessionRef,
}

impl RemotePeerExpectedBinding {
    fn from_proposal(proposal: &PeerAssignmentProposal) -> Self {
        Self {
            assignment_id: proposal.request.assignment_id.clone(),
            owner_principal_id: proposal.request.owner_principal_id.clone(),
            channel: proposal.request.channel.clone(),
            target: proposal.request.target.clone(),
            execution_session: proposal.request.execution_session.clone(),
        }
    }

    pub fn matches(&self, binding: &ExternalPeerAssignmentBinding) -> bool {
        self.assignment_id == binding.assignment_id
            && self.owner_principal_id == binding.owner_principal_id
            && self.channel == binding.channel
            && self.target == binding.target
            && self.execution_session == binding.execution_session
    }
}

static PEER_COMPLETION_ASSOCIATIONS_LOCK: Mutex<()> = Mutex::new(());

fn peer_completion_associations_path() -> std::path::PathBuf {
    crate::paths::medousa_data_dir()
        .join("mesh")
        .join("peer-completion-associations.json")
}

fn peer_completion_associations() -> Result<PeerCompletionAssociationFile> {
    let path = peer_completion_associations_path();
    if !path.is_file() {
        return Ok(PeerCompletionAssociationFile::default());
    }
    let metadata = fs::metadata(&path).with_context(|| format!("stat {}", path.display()))?;
    if metadata.len() > PEER_COMPLETION_ASSOCIATIONS_FILE_MAX_BYTES {
        bail!(
            "{} exceeds the bounded peer completion association file limit",
            path.display()
        );
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    if raw.trim().is_empty() {
        bail!(
            "{} is empty; refusing to discard durable peer completion routes",
            path.display()
        );
    }
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

fn save_peer_completion_associations(file: &PeerCompletionAssociationFile) -> Result<()> {
    let path = peer_completion_associations_path();
    let bytes = serde_json::to_vec_pretty(file).context("encode peer completion associations")?;
    if bytes.len() as u64 > PEER_COMPLETION_ASSOCIATIONS_FILE_MAX_BYTES {
        bail!("peer completion association capacity exhausted");
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    crate::session::atomic_write(&path, &bytes).with_context(|| format!("write {}", path.display()))
}

pub fn remote_peer_proposal_request_digest(request: &RemotePeerProposalRequest) -> Result<String> {
    let bytes = serde_json::to_vec(request).context("encode remote peer proposal request")?;
    Ok(peer_digest(
        b"medousa/remote-peer-proposal-request/v1\0",
        &bytes,
    ))
}

// Admission accounts for queued request/response captures, not the on-disk
// association file. The file has a separate bounded read/atomic-write limit.
const PEER_COMPLETION_STORE_BUDGET: usize = medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES;

pub async fn record_remote_peer_origin_pending_admitted(
    source_device_id: &str,
    target_device_id: &str,
    owner_profile_id: &str,
    request: &RemotePeerProposalRequest,
) -> Result<RemotePeerOriginAssociation> {
    let source_device_id = source_device_id.to_string();
    let target_device_id = target_device_id.to_string();
    let owner_profile_id = owner_profile_id.to_string();
    let request = request.clone();
    crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            PEER_COMPLETION_STORE_BUDGET,
            move || {
                Ok(record_remote_peer_origin_pending(
                    &source_device_id,
                    &target_device_id,
                    &owner_profile_id,
                    &request,
                ))
            },
        )
        .await?
}

pub async fn record_remote_peer_origin_pending_for_current_profile_admitted(
    source_device_id: &str,
    target_device_id: &str,
    request: &RemotePeerProposalRequest,
) -> Result<RemotePeerOriginAssociation> {
    let source_device_id = source_device_id.to_string();
    let target_device_id = target_device_id.to_string();
    let request = request.clone();
    crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            PEER_COMPLETION_STORE_BUDGET,
            move || {
                let owner_profile_id = crate::user_profiles::resolve_workshop_identity_user_id();
                Ok(record_remote_peer_origin_pending(
                    &source_device_id,
                    &target_device_id,
                    &owner_profile_id,
                    &request,
                ))
            },
        )
        .await?
}

pub async fn record_remote_peer_origin_response_admitted(
    target_device_id: &str,
    request: &RemotePeerProposalRequest,
    response: &RemotePeerProposalResponse,
) -> Result<RemotePeerOriginAssociation> {
    let target_device_id = target_device_id.to_string();
    let request = request.clone();
    let response = response.clone();
    crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            PEER_COMPLETION_STORE_BUDGET,
            move || {
                Ok(record_remote_peer_origin_response(
                    &target_device_id,
                    &request,
                    &response,
                ))
            },
        )
        .await?
}

pub async fn next_remote_peer_origin_sync_page_admitted(
    limit: usize,
) -> Result<Vec<RemotePeerOriginAssociation>> {
    crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            PEER_COMPLETION_STORE_BUDGET,
            move || Ok(next_remote_peer_origin_sync_page(limit)),
        )
        .await?
}

pub async fn mark_remote_peer_origin_applied_admitted(
    source_device_id: &str,
    target_device_id: &str,
    request_digest: &str,
    proposal_id: &str,
) -> Result<bool> {
    let source_device_id = source_device_id.to_string();
    let target_device_id = target_device_id.to_string();
    let request_digest = request_digest.to_string();
    let proposal_id = proposal_id.to_string();
    crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            PEER_COMPLETION_STORE_BUDGET,
            move || {
                Ok(mark_remote_peer_origin_applied(
                    &source_device_id,
                    &target_device_id,
                    &request_digest,
                    &proposal_id,
                ))
            },
        )
        .await?
}

pub async fn record_remote_peer_completion_destination_proposal_admitted(
    source_device_id: &str,
    request: &RemotePeerProposalRequest,
    proposal: &PeerAssignmentProposal,
) -> Result<RemotePeerCompletionDestination> {
    let source_device_id = source_device_id.to_string();
    let request = request.clone();
    let proposal = proposal.clone();
    crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            PEER_COMPLETION_STORE_BUDGET,
            move || {
                Ok(record_remote_peer_completion_destination_proposal(
                    &source_device_id,
                    &request,
                    &proposal,
                ))
            },
        )
        .await?
}

pub async fn record_remote_peer_completion_destination_binding_admitted(
    proposal_id: &str,
    binding: &ExternalPeerAssignmentBinding,
) -> Result<bool> {
    let proposal_id = proposal_id.to_string();
    let binding = binding.clone();
    crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            PEER_COMPLETION_STORE_BUDGET,
            move || {
                Ok(record_remote_peer_completion_destination_binding(
                    &proposal_id,
                    &binding,
                ))
            },
        )
        .await?
}

pub async fn remote_peer_completion_destination_for_query_admitted(
    source_device_id: &str,
    query: &RemotePeerCompletionQuery,
) -> Result<Option<RemotePeerCompletionDestination>> {
    let source_device_id = source_device_id.to_string();
    let query = query.clone();
    crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            PEER_COMPLETION_STORE_BUDGET,
            move || {
                Ok(remote_peer_completion_destination_for_query(
                    &source_device_id,
                    &query,
                ))
            },
        )
        .await?
}

fn peer_proposal_digest(proposal: &PeerAssignmentProposal) -> Result<String> {
    let bytes = serde_json::to_vec(proposal).context("encode remote peer proposal")?;
    Ok(peer_digest(b"medousa/remote-peer-proposal/v1\0", &bytes))
}

fn peer_digest(domain: &[u8], bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
    format!("sha256:{:x}", hash.finalize())
}

/// Persist the source binding before sending the signed proposal request.
pub fn record_remote_peer_origin_pending(
    source_device_id: &str,
    target_device_id: &str,
    owner_profile_id: &str,
    request: &RemotePeerProposalRequest,
) -> Result<RemotePeerOriginAssociation> {
    validate_remote_peer_proposal_request(request).map_err(anyhow::Error::msg)?;
    for (name, value) in [
        ("source device", source_device_id),
        ("target device", target_device_id),
        ("owner profile", owner_profile_id),
    ] {
        if value.trim().is_empty() || value.trim() != value || value.len() > 512 {
            bail!("remote peer origin {name} must be exact and bounded");
        }
    }
    if request.target_runtime_id != target_device_id {
        bail!("remote proposal target runtime does not match its paired peer");
    }
    let request_digest = remote_peer_proposal_request_digest(request)?;
    let _guard = PEER_COMPLETION_ASSOCIATIONS_LOCK
        .lock()
        .expect("peer completion associations lock");
    let mut file = peer_completion_associations()?;
    if let Some(existing) = file.origins.iter().find(|item| {
        item.source_device_id == source_device_id
            && item.target_device_id == target_device_id
            && item.request_digest == request_digest
    }) {
        if existing.owner_profile_id != owner_profile_id {
            bail!("remote peer request digest is already bound to different origin data");
        }
        return Ok(existing.clone());
    }
    let association = RemotePeerOriginAssociation {
        schema_version: 1,
        source_device_id: source_device_id.to_string(),
        target_device_id: target_device_id.to_string(),
        request_digest,
        request: request.clone(),
        owner_profile_id: owner_profile_id.to_string(),
        assignment_id: None,
        expected_binding: None,
        completion_applied: false,
        proposal_id: None,
        proposal_request_digest: None,
        binding: None,
        remote_owner_session: None,
    };
    file.origins.push(association.clone());
    if file.origins.len() + file.destinations.len() > PEER_COMPLETION_ASSOCIATIONS_CAP {
        bail!("remote peer completion association capacity exhausted");
    }
    save_peer_completion_associations(&file)?;
    Ok(association)
}

/// Finalize the source route after validating the exact signed proposal reply.
pub fn record_remote_peer_origin_response(
    target_device_id: &str,
    request: &RemotePeerProposalRequest,
    response: &RemotePeerProposalResponse,
) -> Result<RemotePeerOriginAssociation> {
    if response.schema_version != REMOTE_PEER_PROPOSAL_SCHEMA_VERSION
        || response.source_request_digest.as_deref()
            != Some(remote_peer_proposal_request_digest(request)?.as_str())
        || response.proposal.request.target.execution_runtime_id != request.target_runtime_id
        || response.proposal.request.target.runtime != request.runtime
        || response.proposal.request.forge_work_id != request.forge_work_id
        || response.proposal.request.instructions != request.instructions
        || response.proposal.continue_owner != request.continue_owner
        || response.proposal.request.existing_agent_session_id != request.existing_agent_session_id
    {
        bail!("remote proposal response does not match its exact source request");
    }
    let request_digest = remote_peer_proposal_request_digest(request)?;
    let proposal_request_digest = peer_proposal_digest(&response.proposal)?;
    if let Some(binding) = response.binding.as_ref()
        && (binding.assignment_id != response.proposal.request.assignment_id
            || binding.channel != response.proposal.request.channel
            || binding.target.execution_runtime_id != request.target_runtime_id
            || binding.target.runtime != request.runtime
            || !RemotePeerExpectedBinding::from_proposal(&response.proposal).matches(binding))
    {
        bail!("remote proposal binding does not match its exact proposal");
    }
    let _guard = PEER_COMPLETION_ASSOCIATIONS_LOCK
        .lock()
        .expect("peer completion associations lock");
    let mut file = peer_completion_associations()?;
    let association = file
        .origins
        .iter_mut()
        .find(|item| {
            item.target_device_id == target_device_id && item.request_digest == request_digest
        })
        .context("remote proposal has no durable pre-dispatch origin association")?;
    if association.request_digest != request_digest {
        bail!("durable remote proposal origin differs from signed request");
    }
    validate_remote_proposal_shadow(&association.source_device_id, request, &response.proposal)?;
    finalize_remote_peer_origin(
        association,
        &response.proposal,
        proposal_request_digest,
        response.binding.as_ref(),
    )?;
    let saved = association.clone();
    save_peer_completion_associations(&file)?;
    Ok(saved)
}

fn finalize_remote_peer_origin(
    association: &mut RemotePeerOriginAssociation,
    proposal: &PeerAssignmentProposal,
    proposal_request_digest: String,
    binding: Option<&ExternalPeerAssignmentBinding>,
) -> Result<()> {
    let proposal_id = proposal.proposal_id.clone();
    let remote_owner_session = proposal.request.owner_session.clone();
    if association.proposal_id.is_some() {
        let same_proposal = association.proposal_id.as_deref() == Some(proposal_id.as_str())
            && association.proposal_request_digest.as_deref()
                == Some(proposal_request_digest.as_str())
            && association.remote_owner_session.as_ref() == Some(&remote_owner_session);
        if !same_proposal
            || association
                .binding
                .as_ref()
                .zip(binding)
                .is_some_and(|(saved, incoming)| saved != incoming)
        {
            bail!("remote proposal retry returned a different immutable association");
        }
    }
    if let Some(binding) = binding
        && !RemotePeerExpectedBinding::from_proposal(proposal).matches(binding)
    {
        bail!("remote proposal retry binding differs from exact proposal identity");
    }
    association.proposal_id = Some(proposal_id);
    association.assignment_id = Some(proposal.request.assignment_id.clone());
    association.expected_binding = Some(RemotePeerExpectedBinding::from_proposal(proposal));
    association.proposal_request_digest = Some(proposal_request_digest);
    if binding.is_some() {
        association.binding = binding.cloned();
    }
    association.remote_owner_session = Some(remote_owner_session);
    Ok(())
}

/// Persist the authenticated sender and immutable proposal binding on the
/// destination before its signed proposal response is returned.
pub fn record_remote_peer_completion_destination_proposal(
    source_device_id: &str,
    request: &RemotePeerProposalRequest,
    proposal: &PeerAssignmentProposal,
) -> Result<RemotePeerCompletionDestination> {
    if source_device_id.trim().is_empty()
        || proposal.request.target.execution_runtime_id != request.target_runtime_id
        || proposal.request.forge_work_id != request.forge_work_id
        || proposal.request.instructions != request.instructions
        || proposal.continue_owner != request.continue_owner
    {
        bail!("remote completion route does not match authenticated proposal request");
    }
    validate_remote_proposal_shadow(source_device_id, request, proposal)?;
    let association = RemotePeerCompletionDestination {
        schema_version: 1,
        source_device_id: source_device_id.to_string(),
        source_request_digest: remote_peer_proposal_request_digest(request)?,
        proposal_id: proposal.proposal_id.clone(),
        assignment_id: proposal.request.assignment_id.clone(),
        proposal_request_digest: peer_proposal_digest(proposal)?,
        binding: None,
        proposal: Some(proposal.clone()),
        expected_binding: Some(RemotePeerExpectedBinding::from_proposal(proposal)),
        remote_owner_session: proposal.request.owner_session.clone(),
    };
    let _guard = PEER_COMPLETION_ASSOCIATIONS_LOCK
        .lock()
        .expect("peer completion associations lock");
    let mut file = peer_completion_associations()?;
    if let Some(existing_index) = file
        .destinations
        .iter()
        .position(|item| item.proposal_id == association.proposal_id)
    {
        let existing = &file.destinations[existing_index];
        if existing.source_device_id != association.source_device_id
            || existing.source_request_digest != association.source_request_digest
            || existing.proposal_request_digest != association.proposal_request_digest
            || existing.remote_owner_session != association.remote_owner_session
            || existing
                .proposal
                .as_ref()
                .is_some_and(|saved| saved != proposal)
        {
            bail!("proposal already has a different remote completion route");
        }
        if existing.proposal.is_none() {
            file.destinations[existing_index].proposal = Some(proposal.clone());
            save_peer_completion_associations(&file)?;
        }
        return Ok(file.destinations[existing_index].clone());
    }
    file.destinations.push(association.clone());
    if file.origins.len() + file.destinations.len() > PEER_COMPLETION_ASSOCIATIONS_CAP {
        bail!("remote peer completion association capacity exhausted");
    }
    save_peer_completion_associations(&file)?;
    Ok(association)
}

pub fn record_remote_peer_completion_destination_binding(
    proposal_id: &str,
    binding: &ExternalPeerAssignmentBinding,
) -> Result<bool> {
    let _guard = PEER_COMPLETION_ASSOCIATIONS_LOCK
        .lock()
        .expect("peer completion associations lock");
    let mut file = peer_completion_associations()?;
    let Some(association) = file
        .destinations
        .iter_mut()
        .find(|item| item.proposal_id == proposal_id.trim())
    else {
        return Ok(false);
    };
    if binding.assignment_id.trim().is_empty()
        || binding.target.execution_runtime_id.trim().is_empty()
        || association
            .expected_binding
            .as_ref()
            .is_none_or(|expected| !expected.matches(binding))
    {
        bail!("dispatch binding does not match persisted remote proposal association");
    }
    if association
        .binding
        .as_ref()
        .is_some_and(|saved| saved != binding)
    {
        bail!("remote proposal was already bound to a different assignment");
    }
    association.binding = Some(binding.clone());
    save_peer_completion_associations(&file)?;
    Ok(true)
}

/// Return one rotating page of source associations eligible for a pull. The
/// cursor advances durably before network work begins so an offline oldest
/// peer cannot starve other origin chats across retries/restarts.
pub fn next_remote_peer_origin_sync_page(limit: usize) -> Result<Vec<RemotePeerOriginAssociation>> {
    if !(1..=64).contains(&limit) {
        bail!("remote peer completion page size must be between 1 and 64");
    }
    let _guard = PEER_COMPLETION_ASSOCIATIONS_LOCK
        .lock()
        .expect("peer completion associations lock");
    let mut file = peer_completion_associations()?;
    let page = eligible_origin_sync_page(&mut file, limit);
    if !page.is_empty() {
        save_peer_completion_associations(&file)?;
    }
    Ok(page)
}

fn eligible_origin_sync_page(
    file: &mut PeerCompletionAssociationFile,
    limit: usize,
) -> Vec<RemotePeerOriginAssociation> {
    let mut eligible = file
        .origins
        .iter()
        .filter(|item| {
            item.schema_version == 1 && !item.completion_applied && item.request.continue_owner
        })
        .cloned()
        .collect::<Vec<_>>();
    eligible.sort_by_key(origin_sync_key);
    if eligible.is_empty() {
        return Vec::new();
    }
    let cursor = file.origin_sync_cursor.as_deref();
    let start = cursor
        .and_then(|cursor| {
            eligible
                .iter()
                .position(|item| origin_sync_key(item).as_str() > cursor)
        })
        .unwrap_or(0);
    let count = limit.min(eligible.len());
    let page = (0..count)
        .map(|offset| eligible[(start + offset) % eligible.len()].clone())
        .collect::<Vec<_>>();
    if let Some(last) = page.last() {
        file.origin_sync_cursor = Some(origin_sync_key(last));
    }
    page
}

fn origin_sync_key(origin: &RemotePeerOriginAssociation) -> String {
    format!("{}:{}", origin.target_device_id, origin.request_digest)
}

pub fn mark_remote_peer_origin_applied(
    source_device_id: &str,
    target_device_id: &str,
    request_digest: &str,
    proposal_id: &str,
) -> Result<bool> {
    let _guard = PEER_COMPLETION_ASSOCIATIONS_LOCK
        .lock()
        .expect("peer completion associations lock");
    let mut file = peer_completion_associations()?;
    let origin = file
        .origins
        .iter_mut()
        .find(|item| {
            item.source_device_id == source_device_id
                && item.target_device_id == target_device_id
                && item.request_digest == request_digest
                && item.proposal_id.as_deref() == Some(proposal_id)
        })
        .context("cannot mark unknown remote completion route applied")?;
    if origin.completion_applied {
        return Ok(false);
    }
    origin.completion_applied = true;
    save_peer_completion_associations(&file)?;
    Ok(true)
}

pub fn remote_peer_completion_destination_for_query(
    source_device_id: &str,
    query: &RemotePeerCompletionQuery,
) -> Result<Option<RemotePeerCompletionDestination>> {
    if query.schema_version != PEER_COMPLETION_QUERY_SCHEMA_VERSION
        || source_device_id != query.source_device_id
        || !is_sha256_digest(&query.source_request_digest)
        || !valid_optional_query_route(
            &query.proposal_id,
            &query.proposal_request_digest,
            &query.assignment_id,
        )
    {
        bail!("invalid remote peer completion query identity");
    }
    let matches = peer_completion_associations()?
        .destinations
        .into_iter()
        .filter(|item| {
            item.source_device_id == source_device_id
                && item.source_request_digest == query.source_request_digest
                && query
                    .proposal_id
                    .as_ref()
                    .is_none_or(|id| &item.proposal_id == id)
                && query
                    .proposal_request_digest
                    .as_ref()
                    .is_none_or(|digest| &item.proposal_request_digest == digest)
                && query
                    .assignment_id
                    .as_ref()
                    .is_none_or(|id| &item.assignment_id == id)
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        bail!("completion query matches multiple persisted proposal routes");
    }
    Ok(matches.into_iter().next())
}

fn valid_optional_query_route(
    proposal_id: &Option<String>,
    proposal_digest: &Option<String>,
    assignment_id: &Option<String>,
) -> bool {
    match (proposal_id, proposal_digest, assignment_id) {
        (None, None, None) => true,
        (Some(id), Some(digest), Some(assignment)) => {
            !id.trim().is_empty()
                && id.trim() == id
                && !assignment.trim().is_empty()
                && assignment.trim() == assignment
                && is_sha256_digest(digest)
        }
        _ => false,
    }
}

fn is_sha256_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// Durable source-side route to the originating chat. It is written before the
/// signed proposal request leaves this daemon and completed only after the
/// signed response binds an exact proposal and assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePeerOriginAssociation {
    pub schema_version: u32,
    pub source_device_id: String,
    pub target_device_id: String,
    pub request_digest: String,
    pub request: RemotePeerProposalRequest,
    pub owner_profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_binding: Option<RemotePeerExpectedBinding>,
    #[serde(default)]
    pub completion_applied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_request_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ExternalPeerAssignmentBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_owner_session: Option<SessionRef>,
}

/// Durable destination-side return address. The request digest is computed
/// from the exact signed request bytes accepted by the proposal route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePeerCompletionDestination {
    pub schema_version: u32,
    pub source_device_id: String,
    pub source_request_digest: String,
    pub proposal_id: String,
    pub assignment_id: String,
    pub proposal_request_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ExternalPeerAssignmentBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<PeerAssignmentProposal>,
    pub expected_binding: Option<RemotePeerExpectedBinding>,
    pub remote_owner_session: SessionRef,
}

/// Signed TaskResult payload returned after the remote owner's committed
/// continuation. The answer is already committed remotely; receivers append
/// it as evidence and never start a second owner turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePeerCompletionResult {
    pub schema_version: u32,
    pub source_device_id: String,
    pub request_digest: String,
    pub proposal_id: String,
    pub proposal_request_digest: String,
    pub binding: ExternalPeerAssignmentBinding,
    pub receipt: ExternalPeerAssignmentReceipt,
    pub owner_acknowledgment: PeerOwnerIntakeAcknowledgment,
    pub committed_decision: ConversationTurn,
    pub committed_decision_digest: String,
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

fn remote_proposal_shadow_session(
    target_authority: &medousa_types::AuthorityId,
    sender_device_id: &str,
    request: &RemotePeerProposalRequest,
) -> SessionId {
    SessionId::parse(format!(
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
    .expect("digest-derived shadow session id")
}

fn validate_remote_proposal_shadow(
    sender_device_id: &str,
    request: &RemotePeerProposalRequest,
    proposal: &PeerAssignmentProposal,
) -> Result<()> {
    let owner = &proposal.request.owner_session;
    // The destination creates its own immutable context manifest over a derived
    // session. Those coordinates must differ from the phone's source session.
    if owner.session_id
        != remote_proposal_shadow_session(&owner.authority_id, sender_device_id, request)
        || proposal.request.target.authority_id != owner.authority_id
        || proposal.request.channel.authority_id != owner.authority_id
        || proposal.request.execution_session.authority_id != owner.authority_id
        || proposal.request.context.sources.len() != 1
        || proposal.request.context.sources[0].selection.session != *owner
    {
        bail!("remote proposal does not reference the exact request-scoped destination context");
    }
    Ok(())
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
    let shadow_session_id =
        remote_proposal_shadow_session(target_authority, sender_device_id, request);
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

    #[tokio::test]
    async fn completion_storage_and_verification_fit_execution_admission() {
        for budget in [
            PEER_COMPLETION_STORE_BUDGET,
            crate::peer_completion_delivery::MAX_REMOTE_COMPLETION_PAYLOAD_BYTES,
        ] {
            let executed = crate::peer_completion_delivery::completion_execution_service()
                .run(
                    medousa_forge::execution::ExecutionClass::StoreIo,
                    budget,
                    || Ok(true),
                )
                .await
                .expect("completion work must fit the executor's store admission");
            assert!(executed);
        }
    }

    fn request() -> RemotePeerProposalRequest {
        let authority = AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap();
        let session_id = SessionId::parse("session-phone").unwrap();
        let session = SessionRef {
            authority_id: authority.clone(),
            session_id: session_id.clone(),
        };
        let source = TranscriptEntryRef {
            session: session.clone(),
            entry_id: TranscriptEntryId::parse("ent_0123456789abcdef0123456789abcdef").unwrap(),
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
                    manifest_id: ContextManifestId::parse("ctx_0123456789abcdef0123456789abcdef")
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

    fn origin(key: &str, continue_owner: bool) -> RemotePeerOriginAssociation {
        let mut request = request();
        request.continue_owner = continue_owner;
        RemotePeerOriginAssociation {
            schema_version: 1,
            source_device_id: "phone".into(),
            target_device_id: key.into(),
            request_digest: format!("sha256:{}", "a".repeat(64)),
            request,
            owner_profile_id: "owner-profile".into(),
            assignment_id: Some(format!("assignment-{key}")),
            expected_binding: None,
            completion_applied: false,
            proposal_id: Some(format!("proposal-{key}")),
            proposal_request_digest: Some(format!("sha256:{}", "b".repeat(64))),
            binding: None,
            remote_owner_session: None,
        }
    }

    fn proposal_for(request: &RemotePeerProposalRequest) -> PeerAssignmentProposal {
        let authority = AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
        let owner_session = SessionRef {
            authority_id: authority.clone(),
            session_id: remote_proposal_shadow_session(&authority, "phone", request),
        };
        let execution_session = SessionRef {
            authority_id: authority.clone(),
            session_id: SessionId::parse("session-worker").unwrap(),
        };
        let channel = medousa_types::coordination::CoordinationChannelRef {
            authority_id: authority.clone(),
            channel_id: "channel-1".into(),
        };
        let target = medousa_types::coordination::ExternalPeerTarget {
            authority_id: authority,
            execution_runtime_id: request.target_runtime_id.clone(),
            runtime: request.runtime,
        };
        let mut context = request.context.manifest.clone();
        context.sources[0].selection.session = owner_session.clone();
        let entries = request
            .context
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                (
                    TranscriptEntryRef {
                        session: owner_session.clone(),
                        entry_id: entry.source.entry_id.clone(),
                        entry_seq: index as u64 + 1,
                    },
                    entry.content_digest.as_str(),
                )
            })
            .collect::<Vec<_>>();
        context.sources[0].selection_digest =
            medousa_types::coordination::context::conversation_range_digest(
                &owner_session,
                entries.iter().map(|(entry, digest)| (entry, *digest)),
            );
        PeerAssignmentProposal {
            proposal_id: "proposal-1".into(),
            request: medousa_types::coordination::ExternalPeerAssignmentRequest {
                assignment_id: "assignment-1".into(),
                idempotency_key: request.request_key.clone(),
                owner_principal_id: "owner-principal".into(),
                owner_session,
                channel,
                target,
                context,
                execution_session,
                instructions: request.instructions.clone(),
                execution_grant_id: "grant-1".into(),
                forge_work_id: request.forge_work_id.clone(),
                existing_agent_session_id: request.existing_agent_session_id.clone(),
            },
            expires_at: Utc::now(),
            continue_owner: request.continue_owner,
        }
    }

    #[test]
    fn source_and_destination_routes_accept_derived_context_and_reject_other_requests() {
        let temp = tempfile::tempdir().unwrap();
        let _root = crate::paths::scoped_test_data_dir(temp.path());
        let request = request();
        let proposal = proposal_for(&request);
        assert_ne!(
            proposal.request.owner_session.session_id,
            request.owner_session_id
        );
        assert_ne!(proposal.request.context, request.context.manifest);
        record_remote_peer_origin_pending("phone", "runtime-mac", "profile", &request).unwrap();
        record_remote_peer_completion_destination_proposal("phone", &request, &proposal).unwrap();
        let response = RemotePeerProposalResponse {
            schema_version: REMOTE_PEER_PROPOSAL_SCHEMA_VERSION,
            source_request_digest: Some(remote_peer_proposal_request_digest(&request).unwrap()),
            proposal: proposal.clone(),
            binding: None,
        };
        let saved = record_remote_peer_origin_response("runtime-mac", &request, &response).unwrap();
        assert_eq!(
            saved.remote_owner_session.as_ref(),
            Some(&proposal.request.owner_session)
        );
        let replay =
            record_remote_peer_origin_response("runtime-mac", &request, &response).unwrap();
        assert_eq!(
            replay.proposal_request_digest,
            saved.proposal_request_digest
        );
        let mut wrong_request = request.clone();
        wrong_request.request_key.push_str("-other");
        assert!(validate_remote_proposal_shadow("phone", &wrong_request, &proposal).is_err());
        assert!(validate_remote_proposal_shadow("another-phone", &request, &proposal).is_err());
        let mut wrong_context = proposal;
        wrong_context.request.context = request.context.manifest.clone();
        assert!(validate_remote_proposal_shadow("phone", &request, &wrong_context).is_err());
        let mut uncorrelated = response;
        uncorrelated.source_request_digest = None;
        assert!(
            record_remote_peer_origin_response("runtime-mac", &request, &uncorrelated).is_err()
        );
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

    #[test]
    fn completion_pull_page_rotates_and_skips_non_owner_continuations() {
        let mut pending_response = origin("target-d", true);
        pending_response.proposal_id = None;
        pending_response.assignment_id = None;
        pending_response.proposal_request_digest = None;
        let mut file = PeerCompletionAssociationFile {
            origins: vec![
                origin("target-a", true),
                origin("target-b", true),
                origin("target-c", false),
                pending_response,
            ],
            destinations: Vec::new(),
            origin_sync_cursor: None,
        };
        let first = eligible_origin_sync_page(&mut file, 1);
        assert_eq!(first[0].target_device_id, "target-a");
        let expected_cursor = origin_sync_key(&first[0]);
        assert_eq!(
            file.origin_sync_cursor.as_deref(),
            Some(expected_cursor.as_str())
        );
        let second = eligible_origin_sync_page(&mut file, 1);
        assert_eq!(second[0].target_device_id, "target-b");
        let third = eligible_origin_sync_page(&mut file, 1);
        assert_eq!(third[0].target_device_id, "target-d");
        let wrapped = eligible_origin_sync_page(&mut file, 1);
        assert_eq!(wrapped[0].target_device_id, "target-a");
        assert!(
            first
                .iter()
                .chain(&second)
                .chain(&third)
                .chain(&wrapped)
                .all(|route| route.request.continue_owner)
        );
    }

    #[test]
    fn completion_query_recovery_selector_is_all_or_none_and_digest_pinned() {
        let digest = format!("sha256:{}", "a".repeat(64));
        assert!(valid_optional_query_route(&None, &None, &None));
        assert!(!valid_optional_query_route(
            &Some("proposal-1".into()),
            &None,
            &None
        ));
        assert!(valid_optional_query_route(
            &Some("proposal-1".into()),
            &Some(digest.clone()),
            &Some("assignment-1".into()),
        ));
        assert!(!valid_optional_query_route(
            &Some("proposal-1".into()),
            &Some("unverified".into()),
            &Some("assignment-1".into()),
        ));
    }

    #[test]
    fn completion_query_response_keeps_explicit_pending_null() {
        let wire = serde_json::to_value(RemotePeerCompletionResponse {
            schema_version: PEER_COMPLETION_QUERY_SCHEMA_VERSION,
            source_request_digest: format!("sha256:{}", "c".repeat(64)),
            proposal: None,
            completion: None,
        })
        .unwrap();
        assert_eq!(wire.get("completion"), Some(&serde_json::Value::Null));
    }

    #[test]
    fn source_route_finalization_is_idempotent_and_allows_later_dispatch_binding() {
        let request = request();
        let proposal = proposal_for(&request);
        let proposal_digest = peer_proposal_digest(&proposal).unwrap();
        let mut route = origin("runtime-mac", true);
        route.request = request.clone();
        route.request_digest = remote_peer_proposal_request_digest(&request).unwrap();
        route.assignment_id = None;
        route.proposal_id = None;
        route.proposal_request_digest = None;

        finalize_remote_peer_origin(&mut route, &proposal, proposal_digest.clone(), None).unwrap();
        let finalized = route.clone();
        finalize_remote_peer_origin(&mut route, &proposal, proposal_digest.clone(), None).unwrap();
        assert_eq!(route.proposal_id, finalized.proposal_id);
        assert_eq!(
            route.proposal_request_digest,
            finalized.proposal_request_digest
        );
        assert!(route.binding.is_none());

        let expected = RemotePeerExpectedBinding::from_proposal(&proposal);
        let binding = ExternalPeerAssignmentBinding {
            assignment_id: expected.assignment_id.clone(),
            owner_principal_id: expected.owner_principal_id.clone(),
            channel: expected.channel.clone(),
            target: expected.target.clone(),
            execution_session: expected.execution_session.clone(),
            agent_session_id: "agent-session-1".into(),
        };
        finalize_remote_peer_origin(
            &mut route,
            &proposal,
            proposal_digest.clone(),
            Some(&binding),
        )
        .unwrap();
        assert_eq!(route.binding.as_ref(), Some(&binding));

        // The original pending reply may arrive after a recovery poll observed
        // dispatch. Absence in that older reply must not erase known custody.
        finalize_remote_peer_origin(&mut route, &proposal, proposal_digest.clone(), None).unwrap();
        assert_eq!(route.binding.as_ref(), Some(&binding));

        let mut changed_proposal = proposal.clone();
        changed_proposal.proposal_id = "proposal-other".into();
        assert!(
            finalize_remote_peer_origin(
                &mut route,
                &changed_proposal,
                peer_proposal_digest(&changed_proposal).unwrap(),
                Some(&binding),
            )
            .is_err()
        );
        let mut changed_binding = binding.clone();
        changed_binding.assignment_id = "assignment-other".into();
        assert!(
            finalize_remote_peer_origin(
                &mut route,
                &proposal,
                proposal_digest,
                Some(&changed_binding),
            )
            .is_err()
        );
    }
}
