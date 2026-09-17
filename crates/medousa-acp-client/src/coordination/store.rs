//! Create-only durable coordination records under a capability-confined root.
//!
//! Synchronous store: async hosts must admit calls through their execution service.
//! Claims fence duplicate dispatch even across processes. An unresolved claim is
//! intentionally not retried: persistence is not proof the external process survived.

use anyhow::{Context, Result, bail};
use medousa_store::{StorePath, StoreRoot};
use medousa_types::coordination::{
    CoordinationChannelRecord, CoordinationChannelRef, ExternalPeerAssignmentBinding,
    ExternalPeerAssignmentGrant, ExternalPeerAssignmentRequest,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::path::Path;

const MAX_RECORD_BYTES: u64 = 2 * 1024 * 1024;
const SCHEMA_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct Record<T> {
    version: u16,
    value: T,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AssignmentClaim {
    Claimed,
    Existing,
}

pub struct CoordinationStore {
    root: StoreRoot,
}

fn object_path(channel: &CoordinationChannelRef, kind: &str, id: &str) -> Result<StorePath> {
    let mut digest = Sha256::new();
    for part in [
        "medousa/coordination/v1",
        channel.authority_id.as_str(),
        channel.channel_id.as_str(),
        kind,
        id,
    ] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    Ok(StorePath::parse(&format!(
        "c1-{:x}.json",
        digest.finalize()
    ))?)
}

impl CoordinationStore {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            root: StoreRoot::open_or_create(path)?,
        })
    }

    fn read<T: DeserializeOwned>(&self, path: &StorePath) -> Result<T> {
        let bytes = self.root.read_limited(path, MAX_RECORD_BYTES)?;
        let record: Record<T> =
            serde_json::from_slice(&bytes).context("invalid coordination record")?;
        if record.version != SCHEMA_VERSION {
            bail!("unsupported coordination record version");
        }
        Ok(record.value)
    }

    /// Returns false only for an exact replay; conflicting or corrupt records fail closed.
    fn create<T: Serialize + DeserializeOwned + PartialEq>(
        &self,
        path: &StorePath,
        value: &T,
    ) -> Result<bool> {
        let bytes = serde_json::to_vec(&Record {
            version: SCHEMA_VERSION,
            value,
        })?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            bail!("coordination record exceeds size limit");
        }
        match self.root.atomic_create(path, &bytes) {
            Ok(()) => Ok(true),
            Err(error) => {
                if let Ok(existing) = self.read::<T>(path) {
                    if existing == *value {
                        return Ok(false);
                    }
                    bail!("conflicting coordination record; existing value is immutable");
                }
                Err(error.into())
            }
        }
    }

    pub fn create_channel(&self, record: &CoordinationChannelRecord) -> Result<bool> {
        if record.channel.channel_id.trim().is_empty()
            || record.owner_principal_id.trim().is_empty()
            || record
                .member_principal_ids
                .iter()
                .any(|member| member.trim().is_empty())
            || !record
                .member_principal_ids
                .contains(&record.owner_principal_id)
        {
            bail!("channel requires a valid owner membership");
        }
        self.create(
            &object_path(&record.channel, "channel", "snapshot")?,
            record,
        )
    }

    pub fn channel(&self, channel: &CoordinationChannelRef) -> Result<CoordinationChannelRecord> {
        let record: CoordinationChannelRecord =
            self.read(&object_path(channel, "channel", "snapshot")?)?;
        if record.channel != *channel {
            bail!("channel record identity mismatch");
        }
        Ok(record)
    }

    pub fn require_owner(&self, channel: &CoordinationChannelRef, principal: &str) -> Result<()> {
        let record = self.channel(channel)?;
        if record.owner_principal_id != principal
            || !record
                .member_principal_ids
                .iter()
                .any(|member| member == principal)
        {
            bail!("principal does not own this coordination channel");
        }
        Ok(())
    }

    /// Host-only approval seam: never expose this method as a model tool.
    pub fn approve_assignment(&self, grant: &ExternalPeerAssignmentGrant) -> Result<bool> {
        self.require_owner(&grant.request.channel, &grant.request.owner_principal_id)?;
        if grant.request.execution_grant_id.trim().is_empty() {
            bail!("approval requires an exact grant identity");
        }
        self.create(
            &object_path(
                &grant.request.channel,
                "grant",
                &grant.request.execution_grant_id,
            )?,
            grant,
        )
    }

    pub fn revoke_assignment_grant(
        &self,
        channel: &CoordinationChannelRef,
        grant_id: &str,
    ) -> Result<bool> {
        // A grant must exist; revocation is create-only and cannot be undone by replay.
        let _: ExternalPeerAssignmentGrant =
            self.read(&object_path(channel, "grant", grant_id)?)?;
        self.create(&object_path(channel, "revocation", grant_id)?, &true)
    }

    pub fn assignment_grant(
        &self,
        channel: &CoordinationChannelRef,
        grant_id: &str,
    ) -> Result<ExternalPeerAssignmentGrant> {
        self.read(&object_path(channel, "grant", grant_id)?)
    }

    pub fn require_assignment_grant(
        &self,
        request: &ExternalPeerAssignmentRequest,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        self.require_owner(&request.channel, &request.owner_principal_id)?;
        let path = object_path(&request.channel, "revocation", &request.execution_grant_id)?;
        match self.root.read_limited(&path, MAX_RECORD_BYTES) {
            Ok(_) => bail!("external peer assignment grant is revoked"),
            Err(error) if error.is_not_found() => {}
            Err(error) => return Err(error.into()),
        }
        let grant: ExternalPeerAssignmentGrant = self.read(&object_path(
            &request.channel,
            "grant",
            &request.execution_grant_id,
        )?)?;
        if grant.request != *request || grant.expires_at <= now {
            bail!("external peer assignment grant is expired or does not match the exact request");
        }
        Ok(())
    }

    pub fn claim_assignment(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<AssignmentClaim> {
        self.require_owner(&request.channel, &request.owner_principal_id)?;
        if request.assignment_id.trim().is_empty() || request.idempotency_key.trim().is_empty() {
            bail!("assignment requires stable assignment and command identities");
        }
        // Two create-only indexes prevent either command or assignment identity
        // being reused for different work. A crash between indexes is recoverable
        // only by replaying this exact request, never by guessing new instructions.
        self.create(
            &object_path(&request.channel, "command", &request.idempotency_key)?,
            request,
        )?;
        let created = self.create(
            &object_path(&request.channel, "assignment", &request.assignment_id)?,
            request,
        )?;
        Ok(if created {
            AssignmentClaim::Claimed
        } else {
            AssignmentClaim::Existing
        })
    }

    pub fn record_peer(&self, binding: &ExternalPeerAssignmentBinding) -> Result<bool> {
        self.require_owner(&binding.channel, &binding.owner_principal_id)?;
        let request: ExternalPeerAssignmentRequest = self.read(&object_path(
            &binding.channel,
            "assignment",
            &binding.assignment_id,
        )?)?;
        self.require_assignment_grant(&request, chrono::Utc::now())?;
        if binding.target != request.target
            || binding.execution_session != request.execution_session
            || binding.agent_session_id.trim().is_empty()
        {
            bail!("peer binding does not match the claimed execution");
        }
        self.create(
            &object_path(&binding.channel, "peer", &binding.assignment_id)?,
            binding,
        )
    }

    pub fn peer(
        &self,
        channel: &CoordinationChannelRef,
        assignment_id: &str,
    ) -> Result<ExternalPeerAssignmentBinding> {
        self.read(&object_path(channel, "peer", assignment_id)?)
    }

    pub fn peer_if_recorded(
        &self,
        channel: &CoordinationChannelRef,
        assignment_id: &str,
    ) -> Result<Option<ExternalPeerAssignmentBinding>> {
        match self.peer(channel, assignment_id) {
            Ok(binding) => Ok(Some(binding)),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel() -> CoordinationChannelRecord {
        CoordinationChannelRecord {
            channel: CoordinationChannelRef {
                authority_id: format!("auth_{}", "a".repeat(64)).parse().unwrap(),
                channel_id: "project/../../review".into(),
            },
            owner_principal_id: "user:alice".into(),
            member_principal_ids: vec!["user:alice".into(), "user:bob".into()],
            attached_sessions: vec![],
        }
    }

    #[test]
    fn channel_survives_reopen_and_exact_replay() {
        let temp = tempfile::tempdir().unwrap();
        let record = channel();
        let store = CoordinationStore::open(temp.path()).unwrap();
        assert!(store.create_channel(&record).unwrap());
        drop(store);
        let reopened = CoordinationStore::open(temp.path()).unwrap();
        assert_eq!(reopened.channel(&record.channel).unwrap(), record);
        assert!(!reopened.create_channel(&record).unwrap());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn membership_does_not_transfer_channel_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        let record = channel();
        store.create_channel(&record).unwrap();
        assert!(store.require_owner(&record.channel, "user:bob").is_err());
        assert!(store.require_owner(&record.channel, "user:alice").is_ok());
    }

    #[test]
    fn authority_and_immutable_membership_cannot_alias() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        let mut record = channel();
        store.create_channel(&record).unwrap();
        record.member_principal_ids.push("user:eve".into());
        assert!(store.create_channel(&record).is_err());
        record.channel.authority_id = format!("auth_{}", "b".repeat(64)).parse().unwrap();
        assert!(store.channel(&record.channel).is_err());
    }

    #[test]
    fn corrupt_record_cannot_be_replaced_by_replay() {
        let temp = tempfile::tempdir().unwrap();
        let store = CoordinationStore::open(temp.path()).unwrap();
        let record = channel();
        store.create_channel(&record).unwrap();
        let path = object_path(&record.channel, "channel", "snapshot").unwrap();
        store.root.atomic_write(&path, b"not json").unwrap();
        assert!(store.channel(&record.channel).is_err());
        assert!(store.create_channel(&record).is_err());
        assert_eq!(store.root.read(&path).unwrap(), b"not json");
    }
}
