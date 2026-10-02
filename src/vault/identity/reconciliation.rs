//! Evidence-based locator repair and conservative quarantine of native journals.

use super::*;
use crate::vault::{contracts::VaultMutationIntent, relocate::RelocateIntent};

#[cfg(test)]
mod tests;

const MAX_RECONCILIATIONS: usize = 256;
const MAX_PENDING_ENTRIES: usize = 128;
const MAX_JOURNAL_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct VaultReconciliationKey {
    pub id: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VaultReconciliationReceipt {
    pub command_key: String,
    pub request_digest: String,
    pub revision: u64,
    pub disposition: ReconciliationDisposition,
    pub resource_ids: Vec<String>,
    pub operation_id: Option<String>,
    pub intent_digest: Option<String>,
    #[serde(default)]
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReconciliationDisposition {
    ExternalMoveAdopted,
    JournalQuarantined,
}

#[derive(Serialize)]
pub(crate) struct PendingVaultJournal {
    operation_id: String,
    intent_digest: String,
    kind: &'static str,
    source: String,
    destination: Option<String>,
    resource_id: Option<String>,
    has_publication_witness: bool,
    has_native_receipt: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuarantinedJournal {
    schema_version: u16,
    operation_id: String,
    vault_id: String,
    intent_digest: String,
    original_intent: String,
    original_receipt: Option<String>,
    disposition: ReconciliationDisposition,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn validate_receipts(snapshot: &IdentitySnapshot) -> Result<(), VaultMutationError> {
    if snapshot.reconciliations.len() > MAX_RECONCILIATIONS {
        return Err(VaultMutationError::Overloaded);
    }
    for (key, receipt) in &snapshot.reconciliations {
        if key != &receipt.command_key
            || !valid_digest(key)
            || !valid_digest(&receipt.request_digest)
            || receipt.revision == 0
            || receipt.revision > snapshot.revision
            || receipt.replayed
            || receipt.resource_ids.len() > 2
            || (receipt.resource_ids.len() == 2
                && receipt.resource_ids[0] == receipt.resource_ids[1])
            || receipt.resource_ids.iter().any(|id| !valid_nonce(id))
            || receipt
                .resource_ids
                .iter()
                .any(|id| !snapshot.resources.contains_key(id))
            || match receipt.disposition {
                ReconciliationDisposition::ExternalMoveAdopted => {
                    receipt.resource_ids.is_empty()
                        || receipt.operation_id.is_some()
                        || receipt.intent_digest.is_some()
                }
                ReconciliationDisposition::JournalQuarantined => {
                    receipt
                        .operation_id
                        .as_deref()
                        .is_none_or(|id| !valid_nonce(id))
                        || receipt
                            .intent_digest
                            .as_deref()
                            .is_none_or(|digest| !valid_digest(digest))
                }
            }
        {
            return Err(VaultMutationError::Persistence(
                "invalid vault reconciliation receipt".into(),
            ));
        }
    }
    Ok(())
}

fn operation_path(directory: &str, operation_id: &str) -> Result<StorePath, VaultMutationError> {
    if !valid_nonce(operation_id) {
        return Err(VaultMutationError::Invalid(
            "invalid native operation ID".into(),
        ));
    }
    Ok(StorePath::parse(&format!(
        ".medousa/vault/{directory}/{operation_id}.json"
    ))?)
}

pub(super) fn pending_ids(owner: &VaultIndexOwner) -> Result<Vec<String>, VaultMutationError> {
    let root = StorePath::parse(".medousa/vault/intents")?;
    let dir = match owner.files.open_dir_capability(&root) {
        Ok(dir) => dir,
        Err(error) if error.is_not_found() => return Ok(vec![]),
        Err(error) => return Err(error.into()),
    };
    let mut ids = Vec::new();
    for (count, entry) in dir
        .entries()
        .map_err(|e| VaultMutationError::Persistence(e.to_string()))?
        .enumerate()
    {
        if count >= MAX_PENDING_ENTRIES {
            return Err(VaultMutationError::Overloaded);
        }
        let entry = entry.map_err(|e| VaultMutationError::Persistence(e.to_string()))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if let Some(id) = name.strip_suffix(".json") {
            operation_path("intents", id)?;
            ids.push(id.to_string());
        }
    }
    ids.sort();
    Ok(ids)
}

struct Journal {
    bytes: Vec<u8>,
    kind: &'static str,
    source: String,
    destination: Option<String>,
    binding: Option<VaultIdentityBinding>,
}
fn journal(owner: &VaultIndexOwner, id: &str) -> Result<Journal, VaultMutationError> {
    let bytes = owner
        .files
        .read_limited(&operation_path("intents", id)?, MAX_JOURNAL_BYTES)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| VaultMutationError::Persistence(e.to_string()))?;
    if value.get("kind").is_some() {
        let intent: RelocateIntent = serde_json::from_value(value)
            .map_err(|e| VaultMutationError::Persistence(e.to_string()))?;
        if intent.operation_id != id {
            return Err(VaultMutationError::Invalid(
                "journal operation mismatch".into(),
            ));
        }
        Ok(Journal {
            bytes,
            kind: "relocate",
            source: intent.source,
            destination: intent.destination,
            binding: intent.identity,
        })
    } else {
        let intent: VaultMutationIntent = serde_json::from_value(value)
            .map_err(|e| VaultMutationError::Persistence(e.to_string()))?;
        if intent.operation_id != id {
            return Err(VaultMutationError::Invalid(
                "journal operation mismatch".into(),
            ));
        }
        VaultPath::parse(&intent.path)?;
        Ok(Journal {
            bytes,
            kind: "write",
            source: intent.path,
            destination: None,
            binding: intent.identity,
        })
    }
}

impl VaultIdentityTransaction<'_> {
    pub(crate) fn inspect_pending(&self) -> Result<Vec<PendingVaultJournal>, VaultMutationError> {
        self.ensure_usable()?;
        pending_ids(self.owner)?
            .into_iter()
            .map(|id| {
                let journal = journal(self.owner, &id)?;
                self.validate_binding(journal.binding.as_ref())?;
                let receipt = self.owner.files.metadata(&operation_path("receipts", &id)?);
                let has_native_receipt = match receipt {
                    Ok(meta) if meta.kind == StoreEntryKind::File => true,
                    Ok(_) => {
                        return Err(VaultMutationError::Invalid(
                            "native receipt is not a file".into(),
                        ));
                    }
                    Err(e) if e.is_not_found() => false,
                    Err(e) => return Err(e.into()),
                };
                Ok(PendingVaultJournal {
                    operation_id: id,
                    intent_digest: format!("{:x}", Sha256::digest(&journal.bytes)),
                    kind: journal.kind,
                    source: journal.source,
                    destination: journal.destination,
                    resource_id: journal.binding.as_ref().map(|b| b.resource_id.clone()),
                    has_publication_witness: journal
                        .binding
                        .as_ref()
                        .is_some_and(|b| b.published_file.is_some()),
                    has_native_receipt,
                })
            })
            .collect()
    }

    fn validate_binding(
        &self,
        binding: Option<&VaultIdentityBinding>,
    ) -> Result<(), VaultMutationError> {
        if binding
            .is_some_and(|b| b.vault_id != self.snapshot.vault_id || !valid_nonce(&b.resource_id))
        {
            return Err(VaultMutationError::Conflict(
                "journal identity belongs to a different vault".into(),
            ));
        }
        Ok(())
    }

    fn replay(
        &self,
        key: &VaultReconciliationKey,
    ) -> Result<Option<VaultReconciliationReceipt>, VaultMutationError> {
        self.ensure_usable()?;
        if !valid_digest(&key.id) || !valid_digest(&key.digest) {
            return Err(VaultMutationError::Invalid(
                "invalid reconciliation key".into(),
            ));
        }
        if let Some(receipt) = self.snapshot.reconciliations.get(&key.id) {
            if receipt.request_digest != key.digest {
                return Err(VaultMutationError::Conflict(
                    "reconciliation key describes different intent".into(),
                ));
            }
            self.owner.files.sync_parent_of(&store_path("json")?)?;
            let mut receipt = receipt.clone();
            receipt.replayed = true;
            return Ok(Some(receipt));
        }
        if self.snapshot.reconciliations.len() >= MAX_RECONCILIATIONS {
            return Err(VaultMutationError::Overloaded);
        }
        self.admit_native_projection(false)?;
        Ok(None)
    }

    fn save_receipt(
        &mut self,
        key: VaultReconciliationKey,
        disposition: ReconciliationDisposition,
        resource_ids: Vec<String>,
        operation_id: Option<String>,
        intent_digest: Option<String>,
    ) -> Result<VaultReconciliationReceipt, VaultMutationError> {
        self.snapshot.revision = self
            .snapshot
            .revision
            .checked_add(1)
            .ok_or(VaultMutationError::Overloaded)?;
        let receipt = VaultReconciliationReceipt {
            command_key: key.id.clone(),
            request_digest: key.digest,
            revision: self.snapshot.revision,
            disposition,
            resource_ids,
            operation_id,
            intent_digest,
            replayed: false,
        };
        self.snapshot
            .reconciliations
            .insert(key.id, receipt.clone());
        self.persist()?;
        Ok(receipt)
    }

    pub(crate) fn reconcile_move(
        &mut self,
        key: VaultReconciliationKey,
        resource_id: &str,
        expected_revision: &str,
        destination: &VaultPath,
    ) -> Result<VaultReconciliationReceipt, VaultMutationError> {
        if let Some(receipt) = self.replay(&key)? {
            return Ok(receipt);
        }
        if !pending_ids(self.owner)?.is_empty() {
            return Err(VaultMutationError::ExternallyAmbiguous(
                "repair pending native journals before adopting a move".into(),
            ));
        }
        let mut record = self
            .snapshot
            .resources
            .get(resource_id)
            .cloned()
            .ok_or_else(|| VaultMutationError::Invalid("unknown vault resource identity".into()))?;
        if expected_revision
            != format!(
                "vault-observation-v1:{}:{}",
                self.snapshot.vault_id, record.revision
            )
            || record.resolution == ResourceResolution::Tombstoned
        {
            return Err(VaultMutationError::Conflict(
                "native identity changed or is permanently tombstoned".into(),
            ));
        }
        let metadata = fingerprint(self.owner, destination, record.kind)?;
        if VaultFileIdentity::from(metadata) != record.file {
            return Err(VaultMutationError::ExternallyAmbiguous(
                "destination is not the original physical resource; equal content is insufficient"
                    .into(),
            ));
        }
        if destination.as_str() == record.path {
            return Err(VaultMutationError::Invalid(
                "resource is already at this locator".into(),
            ));
        }
        let source = VaultPath::parse(&record.path)?;
        match self.owner.files.metadata(&source) {
            Ok(meta) if VaultFileIdentity::from(meta) == record.file => {
                return Err(VaultMutationError::ExternallyAmbiguous(
                    "original locator still owns the physical resource".into(),
                ));
            }
            Ok(_) => {}
            Err(e) if e.is_not_found() => {}
            Err(e) => return Err(e.into()),
        }
        if self.snapshot.resources.values().any(|r| {
            r.resource_id != resource_id
                && r.resolution != ResourceResolution::Tombstoned
                && r.file == record.file
        }) {
            return Err(VaultMutationError::Conflict(
                "destination has another retained identity; identities cannot be merged".into(),
            ));
        }
        let replaced: Vec<_> = self
            .snapshot
            .resources
            .values()
            .filter(|r| {
                r.resource_id != resource_id
                    && r.path == destination.as_str()
                    && r.resolution == ResourceResolution::Available
            })
            .cloned()
            .collect();
        if replaced.len() > 1 {
            return Err(VaultMutationError::Conflict(
                "destination has conflicting availability ownership".into(),
            ));
        }
        let mut resource_ids = vec![resource_id.to_string()];
        for mut old in replaced {
            old.resolution = ResourceResolution::Unavailable;
            old.observation = None;
            resource_ids.push(old.resource_id.clone());
            self.update_record(old)?;
        }
        record.path = destination.to_string();
        record.resolution = ResourceResolution::Unavailable; // Refresh bounded content only after adoption.
        record.observation = None;
        self.update_record(record)?;
        self.save_receipt(
            key,
            ReconciliationDisposition::ExternalMoveAdopted,
            resource_ids,
            None,
            None,
        )
    }

    pub(crate) fn quarantine_journal(
        &mut self,
        key: VaultReconciliationKey,
        operation_id: &str,
        expected_digest: &str,
    ) -> Result<VaultReconciliationReceipt, VaultMutationError> {
        if !valid_digest(expected_digest) {
            return Err(VaultMutationError::Invalid("invalid intent digest".into()));
        }
        if let Some(receipt) = self.replay(&key)? {
            self.finish_quarantine(&receipt)?;
            return Ok(receipt);
        }
        let journal = journal(self.owner, operation_id)?;
        self.validate_binding(journal.binding.as_ref())?;
        let digest = format!("{:x}", Sha256::digest(&journal.bytes));
        if digest != expected_digest {
            return Err(VaultMutationError::Conflict(
                "native journal changed; inspect it again".into(),
            ));
        }
        let original_receipt = match self.owner.files.read_limited(
            &operation_path("receipts", operation_id)?,
            MAX_JOURNAL_BYTES,
        ) {
            Ok(bytes) => Some(
                String::from_utf8(bytes)
                    .map_err(|e| VaultMutationError::Persistence(e.to_string()))?,
            ),
            Err(e) if e.is_not_found() => None,
            Err(e) => return Err(e.into()),
        };
        let archive = QuarantinedJournal {
            schema_version: 1,
            operation_id: operation_id.into(),
            vault_id: self.snapshot.vault_id.clone(),
            intent_digest: digest.clone(),
            original_intent: String::from_utf8(journal.bytes)
                .map_err(|e| VaultMutationError::Persistence(e.to_string()))?,
            original_receipt,
            disposition: ReconciliationDisposition::JournalQuarantined,
        };
        let archive_path = operation_path("reconciliations", operation_id)?;
        let bytes = serde_json::to_vec(&archive)
            .map_err(|e| VaultMutationError::Persistence(e.to_string()))?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES * 4 {
            return Err(VaultMutationError::Overloaded);
        }
        let transaction = FileTransaction::new(self.owner.files.clone());
        match self
            .owner
            .files
            .read_limited(&archive_path, MAX_JOURNAL_BYTES * 4)
        {
            Ok(existing) if existing == bytes => self.owner.files.sync_parent_of(&archive_path)?,
            Ok(_) => {
                return Err(VaultMutationError::Conflict(
                    "quarantine archive differs from the current journal".into(),
                ));
            }
            Err(e) if e.is_not_found() => {
                transaction.create_only(
                    &archive_path,
                    &bytes,
                    medousa_store::DurabilityLevel::Synced,
                )?;
            }
            Err(e) => return Err(e.into()),
        }
        let mut resource_ids = Vec::new();
        if let Some(binding) = journal.binding {
            if let Some(mut record) = self.snapshot.resources.get(&binding.resource_id).cloned() {
                if record.resolution != ResourceResolution::Tombstoned {
                    record.resolution = ResourceResolution::Unavailable;
                    record.observation = None;
                    if let Some(published_file) = binding.published_file {
                        record.file = published_file;
                    }
                    self.update_record(record)?;
                }
                resource_ids.push(binding.resource_id);
            }
        } else {
            let records: Vec<_> = self
                .snapshot
                .resources
                .values()
                .filter(|r| {
                    r.resolution == ResourceResolution::Available
                        && (r.path == journal.source
                            || journal.destination.as_deref() == Some(r.path.as_str()))
                })
                .cloned()
                .collect();
            if records.len() > 2 {
                return Err(VaultMutationError::Conflict(
                    "legacy journal has ambiguous identity ownership".into(),
                ));
            }
            for mut record in records {
                record.resolution = ResourceResolution::Unavailable;
                record.observation = None;
                resource_ids.push(record.resource_id.clone());
                self.update_record(record)?;
            }
        }
        let receipt = self.save_receipt(
            key,
            ReconciliationDisposition::JournalQuarantined,
            resource_ids,
            Some(operation_id.into()),
            Some(digest),
        )?;
        self.finish_quarantine(&receipt)?;
        Ok(receipt)
    }

    fn finish_quarantine(
        &self,
        receipt: &VaultReconciliationReceipt,
    ) -> Result<(), VaultMutationError> {
        let id = receipt.operation_id.as_deref().ok_or_else(|| {
            VaultMutationError::Invalid("quarantine receipt has no operation".into())
        })?;
        let archive_path = operation_path("reconciliations", id)?;
        let archive: QuarantinedJournal = serde_json::from_slice(
            &self
                .owner
                .files
                .read_limited(&archive_path, MAX_JOURNAL_BYTES * 4)?,
        )
        .map_err(|e| VaultMutationError::Persistence(e.to_string()))?;
        if archive.schema_version != 1
            || archive.vault_id != self.snapshot.vault_id
            || archive.operation_id != id
            || archive.disposition != ReconciliationDisposition::JournalQuarantined
            || Some(&archive.intent_digest) != receipt.intent_digest.as_ref()
            || archive.intent_digest
                != format!("{:x}", Sha256::digest(archive.original_intent.as_bytes()))
        {
            return Err(VaultMutationError::Conflict(
                "quarantine archive does not prove the retained journal".into(),
            ));
        }
        self.owner.files.sync_parent_of(&archive_path)?;
        let path = operation_path("intents", id)?;
        match self.owner.files.read_limited(&path, MAX_JOURNAL_BYTES) {
            Ok(bytes) if format!("{:x}", Sha256::digest(&bytes)) == archive.intent_digest => {
                self.owner.files.remove_file(&path)?
            }
            Ok(_) => {
                return Err(VaultMutationError::Conflict(
                    "active journal changed after quarantine; retained for inspection".into(),
                ));
            }
            Err(e) if e.is_not_found() => {}
            Err(e) => return Err(e.into()),
        }
        self.owner.files.sync_parent_of(&path)?;
        Ok(())
    }

    /// Replay the cleanup of a receipt published before process interruption.
    pub(crate) fn finish_recorded_quarantines(&self) -> Result<(), VaultMutationError> {
        self.ensure_usable()?;
        let pending = pending_ids(self.owner)?;
        for receipt in self.snapshot.reconciliations.values().filter(|r| {
            r.disposition == ReconciliationDisposition::JournalQuarantined
                && r.operation_id
                    .as_ref()
                    .is_some_and(|id| pending.contains(id))
        }) {
            // Snapshot may have been published before its parent sync failed.
            self.owner.files.sync_parent_of(&store_path("json")?)?;
            self.finish_quarantine(receipt)?;
        }
        Ok(())
    }
}
