//! Bounded per-path mutation lanes for H07.1a.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::vault::contracts::{VaultLaneKey, VaultMutationError, VaultRootId};

const DEFAULT_MAX_LANES: usize = 4_096;
const DEFAULT_IDLE_EVICT: Duration = Duration::from_secs(60);

#[derive(Debug)]
struct LaneSlot {
    epoch: u64,
    held: bool,
    waiters: usize,
    last_idle: Instant,
    pending_intent: bool,
}

impl LaneSlot {
    fn new(epoch: u64) -> Self {
        Self {
            epoch,
            held: false,
            waiters: 0,
            last_idle: Instant::now(),
            pending_intent: false,
        }
    }
}

#[derive(Debug)]
struct LaneRegistryInner {
    lanes: HashMap<VaultLaneKey, LaneSlot>,
    root_epochs: HashMap<VaultRootId, u64>,
    admitting: bool,
    max_lanes: usize,
    next_epoch: u64,
}

/// Ordered lane registry with root-switch drain and bounded eviction.
#[derive(Debug)]
pub struct VaultLaneRegistry {
    inner: Mutex<LaneRegistryInner>,
    cvar: Condvar,
}

pub struct VaultLaneGuard {
    registry: Arc<VaultLaneRegistry>,
    keys: Vec<(VaultLaneKey, u64)>,
}

impl VaultLaneRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(LaneRegistryInner {
                lanes: HashMap::new(),
                root_epochs: HashMap::new(),
                admitting: true,
                max_lanes: DEFAULT_MAX_LANES,
                next_epoch: 1,
            }),
            cvar: Condvar::new(),
        })
    }

    pub fn acquire(
        self: &Arc<Self>,
        mut keys: Vec<VaultLaneKey>,
    ) -> Result<VaultLaneGuard, VaultMutationError> {
        keys.sort();
        keys.dedup();
        if keys.is_empty() {
            return Err(VaultMutationError::Invalid("lane set is empty".into()));
        }
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if !guard.admitting {
                return Err(VaultMutationError::Overloaded);
            }
            self.evict_idle_locked(&mut guard);
            let mut ready = true;
            for key in &keys {
                let root_epoch = *guard.root_epochs.get(&key.root_id).unwrap_or(&0);
                if !guard.lanes.contains_key(key) {
                    let epoch = guard.next_epoch.max(root_epoch.saturating_add(1));
                    guard.next_epoch = guard.next_epoch.saturating_add(1);
                    guard.lanes.insert(key.clone(), LaneSlot::new(epoch));
                }
                let slot = guard.lanes.get_mut(key).expect("lane inserted");
                if slot.held {
                    ready = false;
                    slot.waiters = slot.waiters.saturating_add(1);
                }
            }
            if ready {
                let mut held = Vec::with_capacity(keys.len());
                for key in &keys {
                    let slot = guard.lanes.get_mut(key).expect("lane inserted");
                    slot.held = true;
                    held.push((key.clone(), slot.epoch));
                }
                return Ok(VaultLaneGuard {
                    registry: Arc::clone(self),
                    keys: held,
                });
            }
            let wait_result = self.cvar.wait_timeout(guard, Duration::from_secs(30));
            guard = match wait_result {
                Ok((next, _)) => next,
                Err(poisoned) => poisoned.into_inner().0,
            };
            for key in &keys {
                if let Some(slot) = guard.lanes.get_mut(key) {
                    slot.waiters = slot.waiters.saturating_sub(1);
                }
            }
            if !guard.admitting {
                return Err(VaultMutationError::Overloaded);
            }
        }
    }

    pub fn stop_admission(&self) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.admitting = false;
        self.cvar.notify_all();
    }

    pub fn resume_admission(&self) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.admitting = true;
        self.cvar.notify_all();
    }

    /// Root switch / shutdown: bump root epoch so later lanes cannot confuse identity.
    pub fn bump_root_epoch(&self, root_id: &VaultRootId) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let next = guard.next_epoch;
        guard.next_epoch = next.saturating_add(1);
        guard.root_epochs.insert(root_id.clone(), next);
        guard.lanes.retain(|key, slot| {
            if &key.root_id == root_id && !slot.held && !slot.pending_intent && slot.waiters == 0 {
                return false;
            }
            true
        });
        self.cvar.notify_all();
    }

    pub fn mark_pending_intent(&self, key: &VaultLaneKey, pending: bool) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(slot) = guard.lanes.get_mut(key) {
            slot.pending_intent = pending;
        }
    }

    pub fn lane_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .lanes
            .len()
    }

    fn evict_idle_locked(&self, guard: &mut LaneRegistryInner) {
        if guard.lanes.len() < guard.max_lanes {
            return;
        }
        let now = Instant::now();
        let mut ranked: BTreeMap<(u64, String), VaultLaneKey> = BTreeMap::new();
        for (key, slot) in &guard.lanes {
            if slot.held || slot.pending_intent || slot.waiters > 0 {
                continue;
            }
            if now.duration_since(slot.last_idle) < DEFAULT_IDLE_EVICT {
                continue;
            }
            ranked.insert((slot.epoch, key.normalized_path.clone()), key.clone());
        }
        while guard.lanes.len() >= guard.max_lanes {
            let Some((_, key)) = ranked.pop_first() else {
                break;
            };
            guard.lanes.remove(&key);
        }
    }

    fn release(&self, keys: &[(VaultLaneKey, u64)]) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (key, epoch) in keys {
            if let Some(slot) = guard.lanes.get_mut(key) {
                // Ignore release for superseded epochs (root bump / eviction race).
                if slot.epoch != *epoch {
                    continue;
                }
                slot.held = false;
                slot.last_idle = Instant::now();
            }
        }
        self.cvar.notify_all();
    }
}

impl Drop for VaultLaneGuard {
    fn drop(&mut self) {
        self.registry.release(&self.keys);
    }
}

impl VaultLaneGuard {
    pub fn keys(&self) -> impl Iterator<Item = &VaultLaneKey> {
        self.keys.iter().map(|(key, _)| key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::note::VaultNoteSource;
    use std::sync::Barrier;
    use std::thread;

    #[test]
    fn ordered_multi_lane_acquire_is_deadlock_free() {
        let registry = VaultLaneRegistry::new();
        let root = VaultRootId::new("personal");
        let barrier = Arc::new(Barrier::new(2));
        let left = {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            let root = root.clone();
            thread::spawn(move || {
                barrier.wait();
                let keys = vec![
                    VaultLaneKey::new(root.clone(), VaultNoteSource::User, "b.md"),
                    VaultLaneKey::new(root, VaultNoteSource::User, "a.md"),
                ];
                let _guard = registry.acquire(keys).unwrap();
                thread::sleep(Duration::from_millis(20));
            })
        };
        let right = {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let keys = vec![
                    VaultLaneKey::new(root.clone(), VaultNoteSource::User, "a.md"),
                    VaultLaneKey::new(root, VaultNoteSource::User, "b.md"),
                ];
                let _guard = registry.acquire(keys).unwrap();
                thread::sleep(Duration::from_millis(20));
            })
        };
        left.join().unwrap();
        right.join().unwrap();
    }

    #[test]
    fn root_epoch_bump_drops_idle_lanes() {
        let registry = VaultLaneRegistry::new();
        let root = VaultRootId::new("personal");
        let key = VaultLaneKey::new(root.clone(), VaultNoteSource::User, "a.md");
        {
            let _guard = registry.acquire(vec![key.clone()]).unwrap();
        }
        assert_eq!(registry.lane_count(), 1);
        registry.bump_root_epoch(&root);
        assert_eq!(registry.lane_count(), 0);
    }
}
