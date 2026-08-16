//! Vault index owner registry and lifecycle (H07.1a scaffolding).
//!
//! Until H07.1d, legacy `VaultStore` writers remain authoritative. This owner
//! is constructed and tested but is not the active write path.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use medousa_store::{DurabilityLevel, FileTransaction, StoreRoot};

use crate::vault::admission::VaultAdmission;
use crate::vault::contracts::{VaultCommitOutcome, VaultLaneKey, VaultMutationError, VaultRootId};
use crate::vault::lanes::VaultLaneRegistry;
use crate::vault::note::VaultNoteSource;

#[derive()]
pub struct VaultIndexOwner {
    pub root_id: VaultRootId,
    pub files: Arc<StoreRoot>,
    pub lanes: Arc<VaultLaneRegistry>,
    pub admission: Arc<VaultAdmission>,
    pub vault_generation: AtomicU64,
    pub active: AtomicBool,
    transaction: RwLock<FileTransaction>,
}

impl VaultIndexOwner {
    pub fn new(root_id: VaultRootId, files: Arc<StoreRoot>) -> Arc<Self> {
        Arc::new(Self {
            root_id,
            transaction: RwLock::new(FileTransaction::new(Arc::clone(&files))),
            files,
            lanes: VaultLaneRegistry::new(),
            admission: VaultAdmission::new(),
            vault_generation: AtomicU64::new(1),
            active: AtomicBool::new(true),
        })
    }

    pub fn transaction(&self) -> FileTransaction {
        self.transaction
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_transaction(&self, transaction: FileTransaction) {
        *self
            .transaction
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = transaction;
    }

    pub fn current_generation(&self) -> u64 {
        self.vault_generation.load(Ordering::Acquire)
    }

    pub fn bump_generation(&self) -> u64 {
        self.vault_generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn shutdown(&self) {
        self.active.store(false, Ordering::Release);
        self.lanes.stop_admission();
        self.lanes.bump_root_epoch(&self.root_id);
    }

    pub fn lane_key(&self, source: VaultNoteSource, path: &str) -> VaultLaneKey {
        VaultLaneKey::new(self.root_id.clone(), source, path.to_string())
    }

    pub fn durability() -> DurabilityLevel {
        DurabilityLevel::Synced
    }
}

#[derive(Default)]
pub struct VaultRegistry {
    owners: Mutex<HashMap<String, Arc<VaultIndexOwner>>>,
}

impl VaultRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, owner: Arc<VaultIndexOwner>) {
        self.owners
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(owner.root_id.as_str().to_string(), owner);
    }

    pub fn get(&self, root_id: &str) -> Option<Arc<VaultIndexOwner>> {
        self.owners
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(root_id)
            .cloned()
    }

    pub fn remove(&self, root_id: &str) -> Option<Arc<VaultIndexOwner>> {
        let owner = self
            .owners
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(root_id)?;
        owner.shutdown();
        Some(owner)
    }
}

static REGISTRY: once_cell::sync::Lazy<VaultRegistry> =
    once_cell::sync::Lazy::new(VaultRegistry::new);

pub fn vault_registry() -> &'static VaultRegistry {
    &REGISTRY
}

static OWNER_MUTATIONS_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Feature gate: owner mutations stay off until H07.1d flips this.
pub fn owner_mutations_active() -> bool {
    OWNER_MUTATIONS_ACTIVE.load(Ordering::Acquire)
}

pub fn set_owner_mutations_active(active: bool) {
    OWNER_MUTATIONS_ACTIVE.store(active, Ordering::Release);
}

pub fn ensure_owner_for_active_root() -> Result<Arc<VaultIndexOwner>, VaultMutationError> {
    let root_path = crate::vault::path::user_vault_root();
    let root_key = root_path.display().to_string();
    if let Some(existing) = vault_registry().get(&root_key) {
        return Ok(existing);
    }
    let files = crate::vault::path::user_vault_capability()
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let owner = VaultIndexOwner::new(VaultRootId::new(root_key), files);
    vault_registry().insert(Arc::clone(&owner));
    Ok(owner)
}

/// Drop cached owners so the next ensure binds the current vault root.
pub fn reset_vault_owners() {
    let mut owners = REGISTRY
        .owners
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for owner in owners.values() {
        owner.shutdown();
    }
    owners.clear();
}

#[allow(dead_code)]
pub type OwnerCommit = Result<VaultCommitOutcome, VaultMutationError>;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn owner_lifecycle_shuts_down_lanes() {
        let dir = tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        let root = StoreRoot::open_or_create_nofollow(&path).unwrap();
        let owner = VaultIndexOwner::new(VaultRootId::new("test"), Arc::new(root));
        assert_eq!(owner.current_generation(), 1);
        assert_eq!(owner.bump_generation(), 2);
        owner.shutdown();
        assert!(!owner.active.load(Ordering::Acquire));
    }
}
