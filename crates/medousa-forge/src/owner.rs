//! Per-item Forge owners and in-memory tail authority (H06.2A).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use medousa_store::{CommitReceipt, DurabilityLevel, StoreKind};
use sha2::{Digest as _, Sha256};

use crate::error::{ForgeError, Result};
use crate::events::{EventPayload, TransitionEvent};
use crate::execution::{MAX_OWNER_HANDLES, MAX_OWNER_PROJECTION_BYTES, OWNER_IDLE_TTL};
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
}

impl ItemOwnerState {
    fn from_tail(tail: TailMeta, folded: Option<WorkItem>, snapshot_seq: u64) -> Self {
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
            projection_bytes: 0,
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
        let snapshot_seq = snapshot.as_ref().map(|envelope| envelope.applied_seq).unwrap_or(0);
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
        if owners.len() >= MAX_OWNER_HANDLES && !owners.contains_key(work_id) {
            self.evict_idle_locked(&mut owners);
        }
        if owners.len() >= MAX_OWNER_HANDLES && !owners.contains_key(work_id) {
            return Err(ForgeError::Overloaded("forge owner registry is full".into()));
        }
        Ok(owners
            .entry(work_id.clone())
            .or_insert_with(|| Arc::clone(&state))
            .clone())
    }

    fn evict_idle_locked(&self, owners: &mut HashMap<WorkId, Arc<Mutex<ItemOwnerState>>>) {
        let now = Instant::now();
        owners.retain(|_, handle| {
            handle
                .lock()
                .ok()
                .is_none_or(|state| now.duration_since(state.last_used) < OWNER_IDLE_TTL)
        });
        let _ = MAX_OWNER_PROJECTION_BYTES;
    }
}

pub fn append_owned(
    store: &FsWorkStore,
    registry: &ForgeItemRegistry,
    work_id: &WorkId,
    actor: &ActorRef,
    payload: EventPayload,
    durability: DurabilityLevel,
) -> Result<(TransitionEvent, ForgeCommitReceipt)> {
    let handle = registry.get_or_open(store, work_id)?;
    let mut owner = handle
        .lock()
        .map_err(|_| ForgeError::Store("item owner poisoned".into()))?;
    let seq = owner.next_seq;
    let event = store.append_at(work_id, actor, payload, seq)?;
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
            0,
        ),
    };
    Ok((event, receipt))
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
