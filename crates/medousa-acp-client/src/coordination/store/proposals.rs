//! Approval binds a persisted snapshot, not a mutable client request. Async
//! hosts must admit storage calls and authenticate the deciding operator.
use super::{CoordinationStore, object_path};
use anyhow::{Result, bail};
use medousa_types::coordination::*;
use sha2::{Digest, Sha256};

pub fn proposal_identity(proposal: &PeerAssignmentProposal) -> Result<String> {
    let bytes = serde_json::to_vec(&(
        &proposal.request,
        proposal.expires_at,
        proposal.continue_owner,
    ))?;
    let mut digest = Sha256::new();
    digest.update(b"medousa/peer-proposal/v1\0");
    digest.update(bytes);
    Ok(format!("peer_proposal_{:x}", digest.finalize()))
}

impl CoordinationStore {
    pub fn record_proposal(&self, proposal: &PeerAssignmentProposal) -> Result<bool> {
        self.require_owner(
            &proposal.request.channel,
            &proposal.request.owner_principal_id,
        )?;
        crate::coordination::validate_assignment_request(&proposal.request)?;
        if proposal.proposal_id != proposal_identity(proposal)? {
            bail!("proposal identity does not match its snapshot");
        }
        // An assignment cannot have competing approval snapshots or silently
        // change whether owner continuation was part of the operator's choice.
        self.create(
            &object_path(
                &proposal.request.channel,
                "proposal-slot",
                &proposal.request.assignment_id,
            )?,
            &proposal.proposal_id,
        )?;
        self.create(
            &object_path(&proposal.request.channel, "proposal", &proposal.proposal_id)?,
            proposal,
        )
    }

    pub fn proposal(
        &self,
        channel: &CoordinationChannelRef,
        proposal_id: &str,
    ) -> Result<PeerAssignmentProposal> {
        let proposal: PeerAssignmentProposal =
            self.read(&object_path(channel, "proposal", proposal_id)?)?;
        if proposal.request.channel != *channel
            || proposal.proposal_id != proposal_id
            || proposal_identity(&proposal)? != proposal_id
        {
            bail!("stored proposal identity mismatch");
        }
        let slot: String = self.read(&object_path(
            channel,
            "proposal-slot",
            &proposal.request.assignment_id,
        )?)?;
        if slot != proposal_id {
            bail!("proposal slot identity mismatch");
        }
        Ok(proposal)
    }

    pub fn decide_proposal(
        &self,
        channel: &CoordinationChannelRef,
        decision: &PeerProposalDecision,
    ) -> Result<bool> {
        self.require_owner(channel, &decision.owner_principal_id)?;
        let proposal = self.proposal(channel, &decision.proposal_id)?;
        if proposal.request.owner_principal_id != decision.owner_principal_id {
            bail!("proposal decision owner mismatch");
        }
        if decision.approved && proposal.expires_at <= chrono::Utc::now() {
            bail!("expired proposal cannot be approved");
        }
        self.create(
            &object_path(channel, "proposal-decision", &decision.proposal_id)?,
            decision,
        )
    }

    pub fn require_approved_proposal(
        &self,
        channel: &CoordinationChannelRef,
        proposal_id: &str,
        owner: &str,
    ) -> Result<PeerAssignmentProposal> {
        self.require_owner(channel, owner)?;
        let proposal = self.proposal(channel, proposal_id)?;
        let decision: PeerProposalDecision =
            self.read(&object_path(channel, "proposal-decision", proposal_id)?)?;
        if !decision.approved
            || decision.proposal_id != proposal_id
            || decision.owner_principal_id != owner
            || proposal.request.owner_principal_id != owner
            || proposal.expires_at <= chrono::Utc::now()
        {
            bail!("proposal has no current approval for this owner");
        }
        Ok(proposal)
    }
}
