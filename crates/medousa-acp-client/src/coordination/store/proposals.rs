//! Approval binds a persisted snapshot, not a mutable client request. Async
//! hosts must admit storage calls and authenticate the deciding operator.
use super::{CoordinationStore, object_path};
use anyhow::{Result, bail};
use medousa_types::coordination::*;
use sha2::{Digest, Sha256};

#[derive(serde::Serialize, serde::Deserialize, PartialEq)]
struct ProposalIndex {
    channel: CoordinationChannelRef,
    proposal_id: String,
    owner: String,
    owner_session: medousa_types::SessionRef,
}

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
    pub fn proposal_for_assignment(
        &self,
        channel: &CoordinationChannelRef,
        assignment_id: &str,
    ) -> Result<Option<PeerAssignmentProposal>> {
        let id = match self.read::<String>(&object_path(channel, "proposal-slot", assignment_id)?) {
            Ok(id) => id,
            Err(error)
                if error
                    .downcast_ref::<medousa_store::StoreRootError>()
                    .is_some_and(|error| error.is_not_found()) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let proposal = self.proposal(channel, &id)?;
        if proposal.request.assignment_id != assignment_id {
            bail!("proposal slot assignment mismatch");
        }
        Ok(Some(proposal))
    }

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
        let created = self.create(
            &object_path(&proposal.request.channel, "proposal", &proposal.proposal_id)?,
            proposal,
        )?;
        self.create(
            &object_path(
                &proposal.request.channel,
                "proposal-index",
                &proposal.proposal_id,
            )?,
            &ProposalIndex {
                channel: proposal.request.channel.clone(),
                proposal_id: proposal.proposal_id.clone(),
                owner: proposal.request.owner_principal_id.clone(),
                owner_session: proposal.request.owner_session.clone(),
            },
        )?;
        Ok(created)
    }

    pub fn proposal_decision(
        &self,
        proposal: &PeerAssignmentProposal,
    ) -> Result<Option<PeerProposalDecision>> {
        match self.read::<PeerProposalDecision>(&object_path(
            &proposal.request.channel,
            "proposal-decision",
            &proposal.proposal_id,
        )?) {
            Ok(decision)
                if decision.proposal_id == proposal.proposal_id
                    && decision.owner_principal_id == proposal.request.owner_principal_id =>
            {
                Ok(Some(decision))
            }
            Ok(_) => bail!("stored proposal decision identity mismatch"),
            Err(error)
                if error
                    .downcast_ref::<medousa_store::StoreRootError>()
                    .is_some_and(|error| error.is_not_found()) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Operator inbox only. Indexed snapshots do not confer execution authority.
    pub fn proposal_inbox(
        &self,
        owner: &str,
        session: &medousa_types::SessionRef,
        after: Option<&str>,
    ) -> Result<Vec<PeerProposalReviewRecord>> {
        let entries = self.root.list_root_utf8()?;
        if entries.len() > 10_000 {
            bail!("proposal inbox scan budget exhausted");
        }
        let mut page = std::collections::BTreeMap::new();
        for entry in entries {
            if !entry.name.starts_with("p1-") {
                continue;
            }
            let index: ProposalIndex = self.read(&medousa_store::StorePath::parse(&entry.name)?)?;
            if index.owner != owner
                || index.owner_session != *session
                || after.is_some_and(|id| index.proposal_id.as_str() <= id)
            {
                continue;
            }
            if object_path(&index.channel, "proposal-index", &index.proposal_id)?.file_name()
                != entry.name
            {
                bail!("proposal index identity mismatch");
            }
            self.require_owner(&index.channel, owner)?;
            let proposal = self.proposal(&index.channel, &index.proposal_id)?;
            if proposal.request.owner_principal_id != owner
                || proposal.request.owner_session != *session
            {
                bail!("proposal index scope mismatch");
            }
            let decision = self.proposal_decision(&proposal)?;
            if decision.as_ref().is_some_and(|decision| !decision.approved) {
                continue;
            }
            let binding = self.peer_if_recorded(&index.channel, &proposal.request.assignment_id)?;
            if binding.is_some()
                && self
                    .receipt_if_recorded(&index.channel, &proposal.request.assignment_id)?
                    .is_some()
            {
                continue;
            }
            page.insert(
                proposal.proposal_id.clone(),
                PeerProposalReviewRecord {
                    proposal,
                    decision,
                    binding,
                },
            );
            if page.len() > 8 {
                page.pop_last();
            }
        }
        let rows: Vec<_> = page.into_values().collect();
        if serde_json::to_vec(&rows)?.len() > 1024 * 1024 {
            bail!("proposal inbox page budget exhausted");
        }
        Ok(rows)
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
