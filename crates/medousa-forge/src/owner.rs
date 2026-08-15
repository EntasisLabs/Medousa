//! Per-item Forge owners and in-memory tail authority (H06.2).
//!
//! Production Forge/reconcile mutations must commit through
//! [`ForgeItemRegistry`] / [`append_owned`]. Direct `FsWorkStore::append` is
//! reserved for store-level tests and fixtures.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use medousa_store::{CommitReceipt, DurabilityLevel, StoreKind};
use sha2::{Digest as _, Sha256};

use crate::error::{ForgeError, Result};
use crate::events::{EventPayload, TransitionEvent};
use crate::execution::{MAX_OWNER_HANDLES, MAX_OWNER_PROJECTION_BYTES, OWNER_IDLE_TTL};
use crate::fold::apply_payload;
use crate::model::{ActorRef, WorkId, WorkItem};
use crate::store::{FsWorkStore, TailMeta};

#[derive(Debug, Clone)]
pub struct ForgeCommitReceipt {
    pub work_id: WorkId,
    pub item_generation: u64,
    pub first_seq: u64,
    pub last_seq: u64,
    pub log_offset: u64,
    pub durability: DurabilityLevel,
    pub operation_generation: Option<u64>,
    pub persistence: CommitReceipt,
}

#[derive(Debug, Clone)]
pub struct ItemOwnerState {
    pub next_seq: u64,
    pub last_offset: u64,
    pub last_hash: [u8; 32],
    pub item_generation: u64,
    pub lease_generation: u64,
    pub operation_generation: u64,
    pub snapshot_seq: u64,
    pub folded: Option<WorkItem>,
    pub last_used: Instant,
    pub projection_bytes: usize,
    pub dirty: bool,
    pub active: bool,
}

impl ItemOwnerState {
    fn from_tail(tail: TailMeta, folded: Option<WorkItem>, snapshot_seq: u64) -> Self {
        let projection_bytes = folded.as_ref().map(estimate_projection_bytes).unwrap_or(0);
        Self {
            next_seq: tail.last_seq.saturating_add(1),
            last_offset: tail.last_offset,
            last_hash: tail.last_hash,
            item_generation: tail.last_seq,
            lease_generation: tail.lease_acquisitions,
            operation_generation: tail.operations_started,
            snapshot_seq,
            folded,
            last_used: Instant::now(),
            projection_bytes,
            dirty: false,
            active: false,
        }
    }

    pub(crate) fn sync_projection(&mut self, item: WorkItem) -> Result<()> {
        let bytes = estimate_projection_bytes(&item);
        if bytes > MAX_OWNER_PROJECTION_BYTES {
            return Err(ForgeError::Overloaded(format!(
                "owner projection exceeds {MAX_OWNER_PROJECTION_BYTES} bytes"
            )));
        }
        self.projection_bytes = bytes;
        self.folded = Some(item);
        Ok(())
    }
}

fn estimate_projection_bytes(item: &WorkItem) -> usize {
    serde_json::to_vec(item)
        .map(|bytes| bytes.len())
        .unwrap_or(0)
}

struct ActiveGuard {
    flag: *mut bool,
}

impl ActiveGuard {
    fn enter(state: &mut ItemOwnerState) -> Self {
        state.active = true;
        Self {
            flag: std::ptr::addr_of_mut!(state.active),
        }
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        // SAFETY: flag points at ItemOwnerState::active while the owner mutex
        // guard that created this ActiveGuard remains live.
        unsafe {
            *self.flag = false;
        }
    }
}

#[derive(Default)]
pub struct ForgeItemRegistry {
    owners: Mutex<HashMap<WorkId, Arc<Mutex<ItemOwnerState>>>>,
}

impl ForgeItemRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn live_count(&self) -> usize {
        self.owners.lock().map(|owners| owners.len()).unwrap_or(0)
    }

    /// Single-flight open: one live owner Arc per work id.
    pub fn get_or_open(
        &self,
        store: &FsWorkStore,
        work_id: &WorkId,
    ) -> Result<Arc<Mutex<ItemOwnerState>>> {
        {
            let owners = self
                .owners
                .lock()
                .map_err(|_| ForgeError::Store("item registry poisoned".into()))?;
            if let Some(existing) = owners.get(work_id) {
                return Ok(Arc::clone(existing));
            }
        }
        let tail = store.recover_tail(work_id)?;
        let snapshot = store.read_snapshot(work_id)?;
        let snapshot_seq = snapshot
            .as_ref()
            .map(|envelope| envelope.applied_seq)
            .unwrap_or(0);
        let folded = snapshot.and_then(|envelope| {
            if envelope.applied_seq == tail.last_seq {
                Some(envelope.item)
            } else {
                None
            }
        });
        let state = Arc::new(Mutex::new(ItemOwnerState::from_tail(
            tail,
            folded,
            snapshot_seq,
        )));
        let mut owners = self
            .owners
            .lock()
            .map_err(|_| ForgeError::Store("item registry poisoned".into()))?;
        if let Some(existing) = owners.get(work_id) {
            return Ok(Arc::clone(existing));
        }
        if owners.len() >= MAX_OWNER_HANDLES {
            self.evict_idle_locked(&mut owners);
        }
        if owners.len() >= MAX_OWNER_HANDLES {
            return Err(ForgeError::Overloaded(
                "forge owner registry is full".into(),
            ));
        }
        owners.insert(work_id.clone(), Arc::clone(&state));
        Ok(state)
    }

    /// Evict only idle, clean, unreferenced owners that no longer hold a projection.
    pub fn evict_idle(&self) {
        if let Ok(mut owners) = self.owners.lock() {
            self.evict_idle_locked(&mut owners);
        }
    }

    fn evict_idle_locked(&self, owners: &mut HashMap<WorkId, Arc<Mutex<ItemOwnerState>>>) {
        let now = Instant::now();
        for handle in owners.values() {
            if Arc::strong_count(handle) != 1 {
                continue;
            }
            let Ok(mut state) = handle.try_lock() else {
                continue;
            };
            if state.active || state.dirty {
                continue;
            }
            if now.duration_since(state.last_used) < OWNER_IDLE_TTL {
                continue;
            }
            // Drop projection only when the owner is idle, clean, and sole-held.
            state.folded = None;
            state.projection_bytes = 0;
        }
        owners.retain(|_, handle| {
            if Arc::strong_count(handle) > 1 {
                return true;
            }
            let Ok(state) = handle.try_lock() else {
                return true;
            };
            if state.active || state.dirty || state.folded.is_some() || state.projection_bytes > 0 {
                return true;
            }
            now.duration_since(state.last_used) < OWNER_IDLE_TTL
        });
    }

    pub fn force_mark_idle_for_test(&self, work_id: &WorkId, idle: bool) {
        let Ok(owners) = self.owners.lock() else {
            return;
        };
        if let Some(handle) = owners.get(work_id)
            && let Ok(mut state) = handle.lock()
        {
            if idle {
                state.last_used = Instant::now()
                    .checked_sub(OWNER_IDLE_TTL + std::time::Duration::from_secs(1))
                    .unwrap_or_else(Instant::now);
                state.dirty = false;
                state.active = false;
                state.folded = None;
                state.projection_bytes = 0;
            } else {
                state.last_used = Instant::now();
            }
        }
    }
}

/// Append under the per-item owner. Durability on the receipt is whatever the
/// store actually achieved (`Synced` today because `append_at` calls `sync_all`).
pub fn append_owned(
    store: &FsWorkStore,
    registry: &ForgeItemRegistry,
    work_id: &WorkId,
    actor: &ActorRef,
    payload: EventPayload,
    expected_item_generation: Option<u64>,
) -> Result<(TransitionEvent, ForgeCommitReceipt)> {
    let handle = registry.get_or_open(store, work_id)?;
    let mut owner = handle
        .lock()
        .map_err(|_| ForgeError::Store("item owner poisoned".into()))?;
    let _active = ActiveGuard::enter(&mut owner);
    if let Some(expected) = expected_item_generation
        && owner.item_generation != expected
    {
        return Err(ForgeError::Conflict(format!(
            "stale item generation: expected {expected}, have {}",
            owner.item_generation
        )));
    }
    let seq = owner.next_seq;
    let event = store.append_at(work_id, actor, payload, seq)?;
    // append_at always sync_all before returning — report that honestly.
    let durability = DurabilityLevel::Synced;
    let bytes = serde_json::to_vec(&event).map(|v| v.len()).unwrap_or(0);
    owner.next_seq = seq.saturating_add(1);
    owner.last_offset = store
        .events_path(work_id)
        .metadata()
        .map(|meta| meta.len())
        .unwrap_or(owner.last_offset);
    owner.last_hash = hash_event(&event);
    owner.item_generation = owner.item_generation.saturating_add(1);
    if matches!(event.payload, EventPayload::LeaseAcquired { .. }) {
        owner.lease_generation = owner.lease_generation.saturating_add(1);
    }
    if matches!(event.payload, EventPayload::OperationStarted { .. }) {
        owner.operation_generation = owner.operation_generation.saturating_add(1);
    }
    match &mut owner.folded {
        Some(item) => {
            apply_payload(item, &event)?;
            let bytes = estimate_projection_bytes(item);
            if bytes > MAX_OWNER_PROJECTION_BYTES {
                owner.folded = None;
                owner.projection_bytes = 0;
                return Err(ForgeError::Overloaded(format!(
                    "owner projection exceeds {MAX_OWNER_PROJECTION_BYTES} bytes"
                )));
            }
            owner.projection_bytes = bytes;
        }
        None => {
            if let EventPayload::ItemRegistered { item } = &event.payload {
                owner.sync_projection((**item).clone())?;
            }
        }
    }
    owner.dirty = true;
    owner.last_used = Instant::now();
    let receipt = ForgeCommitReceipt {
        work_id: work_id.clone(),
        item_generation: owner.item_generation,
        first_seq: seq,
        last_seq: seq,
        log_offset: owner.last_offset,
        durability,
        operation_generation: Some(owner.operation_generation),
        persistence: CommitReceipt::new(
            StoreKind::Forge,
            work_id.as_str(),
            owner.item_generation,
            durability,
            bytes,
        ),
    };
    Ok((event, receipt))
}

pub fn mark_projection_clean(owner: &mut ItemOwnerState, snapshot_seq: u64) {
    owner.dirty = false;
    owner.snapshot_seq = snapshot_seq;
}

pub fn next_lease_generation(owner: &ItemOwnerState) -> u64 {
    owner.lease_generation.saturating_add(1)
}

fn hash_event(event: &TransitionEvent) -> [u8; 32] {
    let encoded = serde_json::to_vec(event).unwrap_or_default();
    let digest = Sha256::digest(&encoded);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventPayload;
    use crate::model::{
        ActorKind, ActorRef, GitOid, GitWorkTarget, WorkItem, WorkState, WorkTarget,
    };
    use std::sync::Barrier;
    use std::thread;
    use tempfile::TempDir;

    fn actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "owner-test".into(),
        }
    }

    fn item(title: &str) -> WorkItem {
        WorkItem::new(
            title,
            "brief",
            WorkTarget::Git(GitWorkTarget {
                repo_path: std::path::PathBuf::from("/tmp/owner-repo"),
                base_ref: "main".into(),
                base_oid: GitOid::new("a".repeat(40)),
            }),
            "user-1",
        )
    }

    fn registered(work: &WorkItem) -> EventPayload {
        EventPayload::ItemRegistered {
            item: Box::new(work.clone()),
        }
    }

    #[test]
    fn stale_item_generation_fence_rejects_concurrent_writer() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let work = item("fence");
        let (_, receipt) = append_owned(
            &store,
            &registry,
            &work.id,
            &actor(),
            registered(&work),
            None,
        )
        .unwrap();
        let err = append_owned(
            &store,
            &registry,
            &work.id,
            &actor(),
            EventPayload::StateChanged {
                from: WorkState::Draft,
                to: WorkState::Ready,
                reason: None,
            },
            Some(receipt.item_generation.saturating_sub(1)),
        )
        .unwrap_err();
        assert!(matches!(err, ForgeError::Conflict(_)));
    }

    #[test]
    fn same_item_appends_are_serialized_and_monotonic() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(FsWorkStore::open(tmp.path()).unwrap());
        let registry = Arc::new(ForgeItemRegistry::new());
        let work = item("serial");
        append_owned(
            &store,
            &registry,
            &work.id,
            &actor(),
            registered(&work),
            None,
        )
        .unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = Vec::new();
        for i in 0..8 {
            let store = Arc::clone(&store);
            let registry = Arc::clone(&registry);
            let work_id = work.id.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                append_owned(
                    &store,
                    &registry,
                    &work_id,
                    &actor(),
                    EventPayload::StateChanged {
                        from: WorkState::Draft,
                        to: WorkState::Ready,
                        reason: Some(format!("t{i}")),
                    },
                    None,
                )
                .map(|(event, _)| event.seq)
            }));
        }
        let mut seqs: Vec<u64> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().unwrap())
            .collect();
        seqs.sort_unstable();
        assert_eq!(seqs, vec![2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn unrelated_item_progresses_while_other_owner_is_held() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let left = item("left");
        let right = item("right");
        append_owned(
            &store,
            &registry,
            &left.id,
            &actor(),
            registered(&left),
            None,
        )
        .unwrap();
        append_owned(
            &store,
            &registry,
            &right.id,
            &actor(),
            registered(&right),
            None,
        )
        .unwrap();
        let left_handle = registry.get_or_open(&store, &left.id).unwrap();
        let left_guard = left_handle.lock().unwrap();
        let (event, _) = append_owned(
            &store,
            &registry,
            &right.id,
            &actor(),
            EventPayload::StateChanged {
                from: WorkState::Draft,
                to: WorkState::Ready,
                reason: None,
            },
            None,
        )
        .unwrap();
        assert_eq!(event.seq, 2);
        drop(left_guard);
    }

    #[test]
    fn eviction_skips_referenced_active_dirty_or_projected_owners() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let work = item("evict");
        append_owned(
            &store,
            &registry,
            &work.id,
            &actor(),
            registered(&work),
            None,
        )
        .unwrap();
        let held = registry.get_or_open(&store, &work.id).unwrap();
        registry.force_mark_idle_for_test(&work.id, true);
        // Outstanding reference blocks eviction even if marked idle/clean.
        registry.evict_idle();
        assert_eq!(registry.live_count(), 1);
        drop(held);

        // Dirty blocks eviction.
        {
            let handle = registry.get_or_open(&store, &work.id).unwrap();
            let mut state = handle.lock().unwrap();
            state.dirty = true;
            state.folded = None;
            state.projection_bytes = 0;
            state.last_used = Instant::now()
                .checked_sub(OWNER_IDLE_TTL + std::time::Duration::from_secs(1))
                .unwrap_or_else(Instant::now);
        }
        registry.evict_idle();
        assert_eq!(registry.live_count(), 1);

        // Clean, unreferenced, no projection, idle → evicted.
        registry.force_mark_idle_for_test(&work.id, true);
        registry.evict_idle();
        assert_eq!(registry.live_count(), 0);
    }

    #[test]
    fn receipt_durability_is_synced_not_caller_labeled() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let work = item("durability");
        let (_, receipt) = append_owned(
            &store,
            &registry,
            &work.id,
            &actor(),
            registered(&work),
            None,
        )
        .unwrap();
        assert_eq!(receipt.durability, DurabilityLevel::Synced);
        assert_eq!(receipt.persistence.durability, DurabilityLevel::Synced);
    }

    #[test]
    fn get_or_open_returns_same_owner_arc() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let work = item("one");
        let a = registry.get_or_open(&store, &work.id).unwrap();
        let b = registry.get_or_open(&store, &work.id).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }
}
