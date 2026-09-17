//! Durable terminal receipts and result-only owner intake. Locks are acquired
//! non-blockingly; async hosts must admit these synchronous operations.

use super::{CoordinationStore, object_path};
use anyhow::{Result, bail};
use fs2::FileExt;
use medousa_store::StorePath;
use medousa_types::coordination::*;
use sha2::{Digest, Sha256};
use std::fs::File;

pub const MAX_RECEIPT_BYTES: usize = 64 * 1024;
const MAX_INTAKE_ATTEMPTS: u32 = 8;

/// Held across the owner turn. Kernel release on process exit does not erase
/// the durable attempt: an uncertain turn still requires reconciliation.
pub struct OwnerIntakeLease {
    _file: File,
    store_identity: std::sync::Arc<()>,
    session: medousa_types::SessionRef,
    owner: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OwnerIntakeClaim {
    Started(PeerOwnerIntakeAttempt),
    Unresolved(PeerOwnerIntakeAttempt),
    Consumed(PeerOwnerIntakeAcknowledgment),
}

pub fn terminal_receipt_id(binding: &ExternalPeerAssignmentBinding) -> String {
    let mut hash = Sha256::new();
    for part in [
        "medousa/peer-terminal/v1",
        binding.channel.authority_id.as_str(),
        &binding.channel.channel_id,
        &binding.assignment_id,
        &binding.agent_session_id,
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    format!("peer_terminal_{:x}", hash.finalize())
}

impl CoordinationStore {
    /// Host recovery index. Source permissions and current approval must still
    /// be checked at intake; this list is never an anonymous public surface.
    pub fn pending_local_owner_receipts(
        &self,
        authority: &medousa_types::AuthorityId,
        runtime_id: &str,
        limit: usize,
        after_receipt_id: Option<&str>,
    ) -> Result<Vec<ExternalPeerAssignmentReceipt>> {
        if !(1..=256).contains(&limit) {
            bail!("invalid recovery limit");
        }
        let entries = self.root.list_root_utf8()?;
        if entries.len() > 10_000 {
            bail!("coordination recovery scan budget exhausted");
        }
        // Keep only the earliest page in memory, not every result in the inbox.
        let mut pending = std::collections::BTreeMap::new();
        for entry in entries {
            if !entry.name.starts_with("r1-") {
                continue;
            }
            let receipt: ExternalPeerAssignmentReceipt =
                self.read(&StorePath::parse(&entry.name)?)?;
            let binding = &receipt.binding;
            if binding.channel.authority_id != *authority
                || binding.target.authority_id != *authority
                || binding.execution_session.authority_id != *authority
                || binding.target.execution_runtime_id != runtime_id
            {
                continue;
            }
            if self.receipt(&binding.channel, &binding.assignment_id)? != receipt {
                bail!("recovery receipt identity mismatch");
            }
            self.validate_receipt_binding(&receipt)?;
            if self.owner_ack(&receipt)?.is_none()
                && after_receipt_id.is_none_or(|after| receipt.receipt_id.as_str() > after)
            {
                pending.insert(receipt.receipt_id.clone(), receipt);
                if pending.len() > limit {
                    pending.pop_last();
                }
            }
        }
        let page: Vec<_> = pending.into_values().collect();
        if serde_json::to_vec(&page)?.len() > 1024 * 1024 {
            bail!("coordination recovery page byte budget exhausted");
        }
        Ok(page)
    }
    pub fn assignment(
        &self,
        channel: &CoordinationChannelRef,
        assignment_id: &str,
    ) -> Result<ExternalPeerAssignmentRequest> {
        self.read(&object_path(channel, "assignment", assignment_id)?)
    }

    /// One immutable terminal per recorded execution. Conflicting terminal
    /// events cannot replace earlier evidence or create a second wakeup.
    pub fn record_receipt(&self, receipt: &ExternalPeerAssignmentReceipt) -> Result<bool> {
        self.validate_receipt_binding(receipt)?;
        let binding = &receipt.binding;
        // Expired/revoked dispatch approval must not discard completion evidence.
        self.create(
            &object_path(&binding.channel, "receipt", &binding.assignment_id)?,
            receipt,
        )
    }

    fn validate_receipt_binding(&self, receipt: &ExternalPeerAssignmentReceipt) -> Result<()> {
        let binding = &receipt.binding;
        if receipt.receipt_id != terminal_receipt_id(binding)
            || receipt.result.len() > MAX_RECEIPT_BYTES
            || self.peer(&binding.channel, &binding.assignment_id)? != *binding
        {
            bail!("terminal receipt does not match recorded peer custody or budget");
        }
        Ok(())
    }

    /// Lifecycle events after a terminal (e.g. closing completed custody) do
    /// not replace it. Only validated, already recorded custody may be ignored.
    pub fn observe_receipt_once(&self, receipt: &ExternalPeerAssignmentReceipt) -> Result<bool> {
        self.validate_receipt_binding(receipt)?;
        match self.record_receipt(receipt) {
            Ok(created) => Ok(created),
            Err(error) => {
                match self.receipt(&receipt.binding.channel, &receipt.binding.assignment_id) {
                    Ok(existing) if existing.binding == receipt.binding => Ok(false),
                    _ => Err(error),
                }
            }
        }
    }

    pub fn receipt(
        &self,
        channel: &CoordinationChannelRef,
        assignment_id: &str,
    ) -> Result<ExternalPeerAssignmentReceipt> {
        let receipt: ExternalPeerAssignmentReceipt =
            self.read(&object_path(channel, "receipt", assignment_id)?)?;
        if receipt.binding.channel != *channel
            || receipt.binding.assignment_id != assignment_id
            || receipt.receipt_id != terminal_receipt_id(&receipt.binding)
            || self.peer(channel, assignment_id)? != receipt.binding
        {
            bail!("stored terminal receipt identity mismatch");
        }
        Ok(receipt)
    }

    pub fn approve_owner_continuation(&self, grant: &PeerOwnerContinuationGrant) -> Result<bool> {
        self.require_owner(&grant.request.channel, &grant.request.owner_principal_id)?;
        self.create(
            &object_path(
                &grant.request.channel,
                "owner-grant",
                &grant.request.assignment_id,
            )?,
            grant,
        )
    }

    pub fn require_owner_continuation(
        &self,
        receipt: &ExternalPeerAssignmentReceipt,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<ExternalPeerAssignmentRequest> {
        let request = self.assignment(&receipt.binding.channel, &receipt.binding.assignment_id)?;
        self.require_owner(&request.channel, &request.owner_principal_id)?;
        if self.receipt(&request.channel, &request.assignment_id)? != *receipt {
            bail!("owner intake requires the committed terminal receipt");
        }
        let grant: PeerOwnerContinuationGrant = self.read(&object_path(
            &request.channel,
            "owner-grant",
            &request.assignment_id,
        )?)?;
        if grant.request != request || grant.expires_at <= now {
            bail!("owner continuation approval is stale or does not match assignment");
        }
        match self.root.metadata(&object_path(
            &request.channel,
            "revocation",
            &request.execution_grant_id,
        )?) {
            Ok(_) => bail!("owner continuation execution authority was revoked"),
            Err(error) if error.is_not_found() => {}
            Err(error) => return Err(error.into()),
        }
        Ok(request)
    }

    pub fn try_owner_intake_lease(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<Option<OwnerIntakeLease>> {
        self.require_owner(&request.channel, &request.owner_principal_id)?;
        // Shared by all channels addressing this owner session, not just one receipt.
        let scope = CoordinationChannelRef {
            authority_id: request.owner_session.authority_id.clone(),
            channel_id: request.owner_session.session_id.to_string(),
        };
        let file = self.root.open_lock_file(&object_path(
            &scope,
            "owner-lock",
            &request.owner_principal_id,
        )?)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(OwnerIntakeLease {
                _file: file,
                store_identity: self.intake_identity.clone(),
                session: request.owner_session.clone(),
                owner: request.owner_principal_id.clone(),
            })),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn validate_lease(
        &self,
        receipt: &ExternalPeerAssignmentReceipt,
        lease: &OwnerIntakeLease,
    ) -> Result<()> {
        let request = self.assignment(&receipt.binding.channel, &receipt.binding.assignment_id)?;
        if !std::sync::Arc::ptr_eq(&self.intake_identity, &lease.store_identity)
            || lease.session != request.owner_session
            || lease.owner != request.owner_principal_id
        {
            bail!("owner intake lease scope mismatch");
        }
        Ok(())
    }

    pub fn begin_owner_intake(
        &self,
        receipt: &ExternalPeerAssignmentReceipt,
        lease: &OwnerIntakeLease,
    ) -> Result<OwnerIntakeClaim> {
        self.validate_lease(receipt, lease)?;
        self.require_owner_continuation(receipt, chrono::Utc::now())?;
        let channel = &receipt.binding.channel;
        if let Some(ack) = self.owner_ack(receipt)? {
            return Ok(OwnerIntakeClaim::Consumed(ack));
        }
        for attempt in 0..MAX_INTAKE_ATTEMPTS {
            let key = format!("{}:{attempt}", receipt.receipt_id);
            let path = object_path(channel, "owner-attempt", &key)?;
            let intake = PeerOwnerIntakeAttempt {
                receipt: receipt.clone(),
                attempt,
                turn_id: format!("{}-owner-{attempt}", receipt.receipt_id),
            };
            match self.root.metadata(&path) {
                Ok(_) => {
                    let existing: PeerOwnerIntakeAttempt = self.read(&path)?;
                    if existing != intake {
                        bail!("owner attempt identity conflict");
                    }
                    match self
                        .root
                        .metadata(&object_path(channel, "owner-rejected", &key)?)
                    {
                        Ok(_) => {
                            if !self.read::<bool>(&object_path(channel, "owner-rejected", &key)?)? {
                                bail!("invalid rejected intake marker");
                            }
                            continue;
                        }
                        Err(error) if error.is_not_found() => {
                            return Ok(OwnerIntakeClaim::Unresolved(existing));
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(error) if error.is_not_found() => {
                    // A lease serializes normal intake, but a create-only claim is
                    // still the final fence against a stale/mis-scoped caller.
                    return Ok(if self.create(&path, &intake)? {
                        OwnerIntakeClaim::Started(intake)
                    } else {
                        OwnerIntakeClaim::Unresolved(intake)
                    });
                }
                Err(error) => return Err(error.into()),
            }
        }
        bail!("owner intake admission retry budget exhausted");
    }

    /// Only for a known pre-execution admission rejection, never a timeout or
    /// provider failure. This distinction is the replay side-effect boundary.
    pub fn reject_owner_admission(
        &self,
        intake: &PeerOwnerIntakeAttempt,
        lease: &OwnerIntakeLease,
    ) -> Result<()> {
        self.validate_lease(&intake.receipt, lease)?;
        self.validate_intake(intake)?;
        if self.owner_ack(&intake.receipt)?.is_some() {
            bail!("consumed owner intake cannot be rejected");
        }
        self.create(
            &object_path(
                &intake.receipt.binding.channel,
                "owner-rejected",
                &format!("{}:{}", intake.receipt.receipt_id, intake.attempt),
            )?,
            &true,
        )?;
        Ok(())
    }

    fn validate_intake(&self, intake: &PeerOwnerIntakeAttempt) -> Result<()> {
        let channel = &intake.receipt.binding.channel;
        let existing: PeerOwnerIntakeAttempt = self.read(&object_path(
            channel,
            "owner-attempt",
            &format!("{}:{}", intake.receipt.receipt_id, intake.attempt),
        )?)?;
        if existing != *intake
            || self.receipt(channel, &intake.receipt.binding.assignment_id)? != intake.receipt
        {
            bail!("owner intake identity mismatch");
        }
        Ok(())
    }

    fn require_not_rejected(&self, intake: &PeerOwnerIntakeAttempt) -> Result<()> {
        match self.root.metadata(&object_path(
            &intake.receipt.binding.channel,
            "owner-rejected",
            &format!("{}:{}", intake.receipt.receipt_id, intake.attempt),
        )?) {
            Ok(_) => bail!("rejected owner admission cannot be acknowledged"),
            Err(error) if error.is_not_found() => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn owner_ack(
        &self,
        receipt: &ExternalPeerAssignmentReceipt,
    ) -> Result<Option<PeerOwnerIntakeAcknowledgment>> {
        let path = object_path(&receipt.binding.channel, "owner-ack", &receipt.receipt_id)?;
        match self.root.metadata(&path) {
            Ok(_) => {}
            Err(error) if error.is_not_found() => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        let ack: PeerOwnerIntakeAcknowledgment = self.read(&path)?;
        let request = self.assignment(&receipt.binding.channel, &receipt.binding.assignment_id)?;
        if ack.intake.receipt != *receipt
            || ack.decision.session != request.owner_session
            || ack.decision.entry_seq == 0
            || ack.decision_digest.trim().is_empty()
        {
            bail!("stored owner acknowledgment identity mismatch");
        }
        self.validate_intake(&ack.intake)?;
        self.require_not_rejected(&ack.intake)?;
        Ok(Some(ack))
    }

    pub fn acknowledge_owner_intake(
        &self,
        ack: &PeerOwnerIntakeAcknowledgment,
        lease: &OwnerIntakeLease,
    ) -> Result<bool> {
        self.validate_lease(&ack.intake.receipt, lease)?;
        self.validate_intake(&ack.intake)?;
        self.require_not_rejected(&ack.intake)?;
        let request = self.require_owner_continuation(&ack.intake.receipt, chrono::Utc::now())?;
        if ack.decision.session != request.owner_session
            || ack.decision.entry_seq == 0
            || ack.decision_digest.trim().is_empty()
        {
            bail!("owner decision does not match its session");
        }
        // The host verifies committed decision content and execution correlation
        // before invoking this storage seam.
        self.create(
            &object_path(
                &request.channel,
                "owner-ack",
                &ack.intake.receipt.receipt_id,
            )?,
            ack,
        )
    }

    pub fn pending_owner_receipts(
        &self,
        channel: &CoordinationChannelRef,
        owner: &str,
        limit: usize,
    ) -> Result<Vec<ExternalPeerAssignmentReceipt>> {
        self.require_owner(channel, owner)?;
        if !(1..=256).contains(&limit) {
            bail!("invalid pending intake limit");
        }
        let entries = self.root.list_root_utf8()?;
        if entries.len() > 10_000 {
            bail!("coordination scan budget exhausted");
        }
        let mut pending = Vec::new();
        for entry in entries {
            if !entry.name.starts_with("r1-") {
                continue;
            }
            let receipt: ExternalPeerAssignmentReceipt =
                self.read(&StorePath::parse(&entry.name)?)?;
            if receipt.binding.channel != *channel {
                continue;
            }
            if self.receipt(channel, &receipt.binding.assignment_id)? != receipt {
                bail!("receipt index mismatch");
            }
            if self.owner_ack(&receipt)?.is_none() {
                pending.push(receipt);
            }
        }
        pending.sort_by(|left, right| left.receipt_id.cmp(&right.receipt_id));
        pending.truncate(limit);
        Ok(pending)
    }
}
