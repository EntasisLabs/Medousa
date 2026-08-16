//! Journaled move / delete / restore (H07.1c).

use std::sync::Arc;

use medousa_store::{DurabilityLevel, StorePath};
use serde::{Deserialize, Serialize};

use crate::vault::contracts::{
    vault_receipt, VaultCommitOutcome, VaultMutationError, VaultRootId,
};
use crate::vault::note::VaultNoteSource;
use crate::vault::owner::VaultIndexOwner;
use crate::vault::path::VaultPath;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelocateKind {
    Move,
    Delete,
    Restore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelocateIntent {
    pub operation_id: String,
    pub kind: RelocateKind,
    pub root_id: String,
    pub source: String,
    pub destination: Option<String>,
    pub vault_generation: u64,
}

pub fn relocate_move(
    owner: &Arc<VaultIndexOwner>,
    source: &str,
    destination: &str,
) -> Result<VaultCommitOutcome, VaultMutationError> {
    let source_path = VaultPath::parse(source)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let dest_path = VaultPath::parse(destination)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    commit_relocate(
        owner,
        RelocateKind::Move,
        source_path,
        Some(dest_path),
    )
}

pub fn relocate_delete(
    owner: &Arc<VaultIndexOwner>,
    source: &str,
) -> Result<VaultCommitOutcome, VaultMutationError> {
    let source_path = VaultPath::parse(source)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let trash = unique_trash_path(owner, &source_path)?;
    commit_relocate(owner, RelocateKind::Delete, source_path, Some(trash))
}

pub fn relocate_restore(
    owner: &Arc<VaultIndexOwner>,
    source: &str,
) -> Result<VaultCommitOutcome, VaultMutationError> {
    let source_path = VaultPath::parse(source)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let trash = source_path.trash_path();
    commit_relocate(owner, RelocateKind::Restore, trash, Some(source_path))
}

fn commit_relocate(
    owner: &Arc<VaultIndexOwner>,
    kind: RelocateKind,
    source: VaultPath,
    destination: Option<VaultPath>,
) -> Result<VaultCommitOutcome, VaultMutationError> {
    let _admit = owner.admission.admit_mutation()?;
    let mut keys = vec![owner.lane_key(VaultNoteSource::User, source.as_str())];
    if let Some(dest) = &destination {
        keys.push(owner.lane_key(VaultNoteSource::User, dest.as_str()));
    }
    let _guard = owner.lanes.acquire(keys)?;

    let operation_id = uuid::Uuid::new_v4().simple().to_string();
    let intent = RelocateIntent {
        operation_id: operation_id.clone(),
        kind,
        root_id: owner.root_id.as_str().to_string(),
        source: source.to_string(),
        destination: destination.as_ref().map(|path| path.to_string()),
        vault_generation: owner.current_generation(),
    };
    let intent_path = StorePath::parse(&format!(".medousa/vault/intents/{operation_id}.json"))
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let intent_bytes = serde_json::to_vec(&intent)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let tx = owner.transaction();
    tx.write_intent(&intent_path, &intent_bytes, DurabilityLevel::Synced)?;

    let dest = destination.ok_or_else(|| {
        VaultMutationError::Invalid("relocate destination required".into())
    })?;
    if matches!(kind, RelocateKind::Move | RelocateKind::Restore)
        && owner.files.is_file(&dest).unwrap_or(false)
    {
        return Err(VaultMutationError::Conflict(
            "destination already exists".into(),
        ));
    }
    if !owner.files.is_file(&source).unwrap_or(false) {
        return Err(VaultMutationError::Invalid(format!(
            "source not found: {source}"
        )));
    }

    match kind {
        RelocateKind::Restore => tx.restore_path(&source, &dest, DurabilityLevel::Synced)?,
        RelocateKind::Move | RelocateKind::Delete => {
            tx.move_path(&source, &dest, DurabilityLevel::Synced)?
        }
    }

    let vault_generation = owner.bump_generation();
    let receipt_path = StorePath::parse(&format!(".medousa/vault/receipts/{operation_id}.json"))
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let receipt_bytes = serde_json::to_vec(&intent)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    tx.write_receipt(&receipt_path, &receipt_bytes, DurabilityLevel::Synced)?;
    let _ = tx.root().remove_file(&intent_path);

    Ok(VaultCommitOutcome {
        receipt: vault_receipt(
            format!("relocate:{operation_id}"),
            vault_generation,
            0,
            DurabilityLevel::Synced,
        ),
        note_version: crate::vault::contracts::NoteVersion::from_digest(operation_id),
        vault_generation,
        index_repair_required: false,
    })
}

fn unique_trash_path(
    owner: &VaultIndexOwner,
    source: &VaultPath,
) -> Result<VaultPath, VaultMutationError> {
    let base = source.trash_path();
    if !owner.files.is_file(&base).unwrap_or(false) {
        return Ok(base);
    }
    // Never clobber an existing trash entry.
    for attempt in 0..32 {
        let candidate = VaultPath::internal(&format!(
            "{}.{}",
            base.as_str(),
            uuid::Uuid::new_v4().simple()
        ))
        .or_else(|_| {
            // Fall back to a sibling unique name under .trash
            VaultPath::internal(&format!(
                ".trash/{}-{attempt}.md",
                source.as_str().replace('/', "_")
            ))
        })
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
        if !owner.files.is_file(&candidate).unwrap_or(false) {
            return Ok(candidate);
        }
    }
    Err(VaultMutationError::Overloaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::mutation::{commit_write, WriteMutation};
    use crate::vault::contracts::MutationPrecondition;
    use medousa_store::StoreRoot;
    use tempfile::tempdir;

    fn owner() -> (tempfile::TempDir, Arc<VaultIndexOwner>) {
        let dir = tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&path).unwrap());
        (
            dir,
            VaultIndexOwner::new(VaultRootId::new("test"), root),
        )
    }

    #[test]
    fn delete_never_clobbers_existing_trash_entry() {
        let (_dir, owner) = owner();
        commit_write(
            &owner,
            WriteMutation {
                path: "keep.md".into(),
                content: "first\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap();
        relocate_delete(&owner, "keep.md").unwrap();
        commit_write(
            &owner,
            WriteMutation {
                path: "keep.md".into(),
                content: "second\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap();
        relocate_delete(&owner, "keep.md").unwrap();
        let trash_root = owner.files.list_directory_utf8(
            &VaultPath::internal(".trash").unwrap(),
        );
        let entries = trash_root.unwrap();
        assert!(entries.len() >= 2, "both trash versions retained");
    }

    #[test]
    fn move_is_one_operation() {
        let (_dir, owner) = owner();
        commit_write(
            &owner,
            WriteMutation {
                path: "from.md".into(),
                content: "body\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap();
        relocate_move(&owner, "from.md", "to.md").unwrap();
        assert!(!owner
            .files
            .is_file(&VaultPath::parse("from.md").unwrap())
            .unwrap());
        assert!(owner
            .files
            .is_file(&VaultPath::parse("to.md").unwrap())
            .unwrap());
    }
}
