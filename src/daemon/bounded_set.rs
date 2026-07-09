//! A capacity-bounded dedup set for insert-only daemon state.
//!
//! Several long-lived `AppState` collections are used purely for membership
//! checks (idempotency keys, cancellation tombstones) and would otherwise grow
//! forever because entries are inserted but never removed. [`BoundedDedupSet`]
//! keeps the same `HashSet`-style semantics but evicts the oldest inserted
//! entry once a configured capacity is exceeded (FIFO), so memory stays bounded
//! regardless of how long the daemon runs.

use std::collections::{HashSet, VecDeque};

#[derive(Debug)]
pub struct BoundedDedupSet {
    capacity: usize,
    set: HashSet<String>,
    order: VecDeque<String>,
}

impl BoundedDedupSet {
    /// Create a set that retains at most `capacity` entries (minimum 1).
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            set: HashSet::new(),
            order: VecDeque::new(),
        }
    }

    /// Insert a value, evicting the oldest entry when over capacity.
    ///
    /// Returns `true` if the value was newly inserted and `false` if it was
    /// already present — matching [`std::collections::HashSet::insert`], so
    /// existing idempotency checks (`if !set.insert(id) { .. }`) keep working.
    pub fn insert(&mut self, value: String) -> bool {
        if self.set.contains(&value) {
            return false;
        }
        self.order.push_back(value.clone());
        self.set.insert(value);
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.set.remove(&oldest);
            }
        }
        true
    }

    /// Returns `true` if the value is currently retained.
    pub fn contains(&self, value: &str) -> bool {
        self.set.contains(value)
    }

    /// Remove a value if present, returning whether it existed.
    pub fn remove(&mut self, value: &str) -> bool {
        if self.set.remove(value) {
            if let Some(pos) = self.order.iter().position(|entry| entry == value) {
                self.order.remove(pos);
            }
            true
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_reports_new_vs_existing() {
        let mut set = BoundedDedupSet::new(8);
        assert!(set.insert("a".to_string()));
        assert!(!set.insert("a".to_string()));
        assert!(set.contains("a"));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn evicts_oldest_past_capacity() {
        let mut set = BoundedDedupSet::new(3);
        for id in ["a", "b", "c", "d"] {
            set.insert(id.to_string());
        }
        assert_eq!(set.len(), 3);
        assert!(!set.contains("a"), "oldest entry should be evicted");
        assert!(set.contains("b"));
        assert!(set.contains("c"));
        assert!(set.contains("d"));
    }

    #[test]
    fn reinsert_does_not_grow_or_reorder() {
        let mut set = BoundedDedupSet::new(2);
        set.insert("a".to_string());
        set.insert("b".to_string());
        // Re-inserting an existing key is a no-op and must not evict "a".
        assert!(!set.insert("a".to_string()));
        assert!(set.contains("a"));
        assert!(set.contains("b"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn remove_frees_a_slot() {
        let mut set = BoundedDedupSet::new(2);
        set.insert("a".to_string());
        set.insert("b".to_string());
        assert!(set.remove("a"));
        assert!(!set.contains("a"));
        // After removal a fresh insert should not evict the surviving entry.
        set.insert("c".to_string());
        assert!(set.contains("b"));
        assert!(set.contains("c"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn capacity_is_at_least_one() {
        let mut set = BoundedDedupSet::new(0);
        assert_eq!(set.capacity(), 1);
        set.insert("a".to_string());
        set.insert("b".to_string());
        assert_eq!(set.len(), 1);
        assert!(set.contains("b"));
    }
}
