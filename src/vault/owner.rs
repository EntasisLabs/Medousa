//! Vault index owner registry and lifecycle (H07.1a scaffolding).
//!
//! Until H07.1d, legacy `VaultStore` writers remain authoritative. This owner
//! is constructed and tested but is not the active write path.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use medousa_store::{DurabilityLevel, FileTransaction, StoreRoot};

use crate::vault::admission::VaultAdmission;
use crate::vault::contracts::{VaultCommitOutcome, VaultLaneKey, VaultMutationError, VaultRootId};
use crate::vault::lanes::VaultLaneRegistry;
use crate::vault::note::VaultNoteSource;

const CHANGE_LOG_CAP: usize = 4096;

#[derive(Debug, Clone)]
pub struct VaultChangeRecord {
    pub generation: u64,
    pub path: String,
    pub kind: String,
    pub note_version: Option<String>,
}

#[derive()]
pub struct VaultIndexOwner {
    pub root_id: VaultRootId,
    pub files: Arc<StoreRoot>,
    pub lanes: Arc<VaultLaneRegistry>,
    pub admission: Arc<VaultAdmission>,
    pub vault_generation: AtomicU64,
    pub active: AtomicBool,
    transaction: RwLock<FileTransaction>,
    generation_lock: Mutex<()>,
    persist_generation_fault: AtomicBool,
    change_log: Mutex<VecDeque<VaultChangeRecord>>,
}

impl VaultIndexOwner {
    pub fn new(root_id: VaultRootId, files: Arc<StoreRoot>) -> Arc<Self> {
        let loaded = load_persisted_generation(&files).unwrap_or(1);
        Arc::new(Self {
            root_id,
            transaction: RwLock::new(FileTransaction::new(Arc::clone(&files))),
            files,
            lanes: VaultLaneRegistry::new(),
            admission: VaultAdmission::new(),
            vault_generation: AtomicU64::new(loaded.max(1)),
            active: AtomicBool::new(true),
            generation_lock: Mutex::new(()),
            persist_generation_fault: AtomicBool::new(false),
            change_log: Mutex::new(VecDeque::new()),
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

    pub fn bump_generation(&self) -> Result<u64, VaultMutationError> {
        let _guard = self
            .generation_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let next = self.vault_generation.load(Ordering::Acquire).saturating_add(1);
        persist_generation(self, next)?;
        self.vault_generation.store(next, Ordering::Release);
        Ok(next)
    }

    pub fn set_persist_generation_fault(&self, fault: bool) {
        self.persist_generation_fault
            .store(fault, Ordering::Release);
    }

    pub fn record_change(&self, record: VaultChangeRecord) {
        let mut log = self
            .change_log
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        log.push_back(record);
        while log.len() > CHANGE_LOG_CAP {
            log.pop_front();
        }
    }

    pub fn changes_since(
        &self,
        since: u64,
        after_generation: Option<u64>,
        after_path: Option<&str>,
        limit: usize,
    ) -> (Vec<VaultChangeRecord>, bool, bool) {
        let current = self.current_generation();
        if since >= current {
            return (Vec::new(), false, false);
        }
        let log = self
            .change_log
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let first_gen = log.front().map(|record| record.generation);
        if first_gen.is_none_or(|first| first > since + 1) {
            return (Vec::new(), false, true);
        }
        let mut changes = Vec::new();
        let mut truncated = false;
        for record in log.iter() {
            if record.generation <= since {
                continue;
            }
            if let (Some(after_gen), Some(after_path)) = (after_generation, after_path) {
                if record.generation < after_gen
                    || (record.generation == after_gen && record.path.as_str() <= after_path)
                {
                    continue;
                }
            }
            if changes.len() >= limit {
                truncated = true;
                break;
            }
            changes.push(record.clone());
        }
        (changes, truncated, false)
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

fn generation_store_path() -> Result<medousa_store::StorePath, VaultMutationError> {
    medousa_store::StorePath::parse(".medousa/vault/generation")
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))
}

fn load_persisted_generation(files: &StoreRoot) -> Option<u64> {
    let path = generation_store_path().ok()?;
    if !files.is_file(&path).unwrap_or(false) {
        return None;
    }
    let bytes = files.read_limited(&path, 64).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    text.trim().parse().ok()
}

fn persist_generation(
    owner: &VaultIndexOwner,
    generation: u64,
) -> Result<(), VaultMutationError> {
    if owner.persist_generation_fault.load(Ordering::Acquire) {
        return Err(VaultMutationError::Persistence(
            "injected generation persist fault".into(),
        ));
    }
    let path = generation_store_path()?;
    owner
        .files
        .atomic_write(&path, generation.to_string().as_bytes())
        .map_err(VaultMutationError::from)
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
    crate::vault::mutation::recover_all_pending_writes(&owner)?;
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
        assert_eq!(owner.bump_generation().unwrap(), 2);
        owner.set_persist_generation_fault(true);
        assert!(owner.bump_generation().is_err());
        assert_eq!(owner.current_generation(), 2);
        owner.shutdown();
        assert!(!owner.active.load(Ordering::Acquire));
    }
}
