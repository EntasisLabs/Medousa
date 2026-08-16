//! H07.0 measurement counters.
//!
//! These record today's cost model. Target budgets are attached in later cars;
//! Car 0 must stay green and only asserts that counters are observable.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-wide vault baseline counters for harnesses and tests.
#[derive(Debug, Default)]
pub struct VaultBaselineCounters {
    pub recursive_root_walks: AtomicU64,
    pub files_statted: AtomicU64,
    pub files_read: AtomicU64,
    pub bytes_read: AtomicU64,
    pub bytes_written: AtomicU64,
    pub link_rebuilds: AtomicU64,
    pub index_rewrites: AtomicU64,
    pub ensure_index_fresh_calls: AtomicU64,
    pub mutations: AtomicU64,
    pub conflict_outcomes: AtomicU64,
    pub dual_success_races: AtomicU64,
}

impl VaultBaselineCounters {
    pub fn snapshot(&self) -> VaultBaselineSnapshot {
        VaultBaselineSnapshot {
            recursive_root_walks: self.recursive_root_walks.load(Ordering::Relaxed),
            files_statted: self.files_statted.load(Ordering::Relaxed),
            files_read: self.files_read.load(Ordering::Relaxed),
            bytes_read: self.bytes_read.load(Ordering::Relaxed),
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
            link_rebuilds: self.link_rebuilds.load(Ordering::Relaxed),
            index_rewrites: self.index_rewrites.load(Ordering::Relaxed),
            ensure_index_fresh_calls: self.ensure_index_fresh_calls.load(Ordering::Relaxed),
            mutations: self.mutations.load(Ordering::Relaxed),
            conflict_outcomes: self.conflict_outcomes.load(Ordering::Relaxed),
            dual_success_races: self.dual_success_races.load(Ordering::Relaxed),
        }
    }

    pub fn reset(&self) {
        for counter in [
            &self.recursive_root_walks,
            &self.files_statted,
            &self.files_read,
            &self.bytes_read,
            &self.bytes_written,
            &self.link_rebuilds,
            &self.index_rewrites,
            &self.ensure_index_fresh_calls,
            &self.mutations,
            &self.conflict_outcomes,
            &self.dual_success_races,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VaultBaselineSnapshot {
    pub recursive_root_walks: u64,
    pub files_statted: u64,
    pub files_read: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub link_rebuilds: u64,
    pub index_rewrites: u64,
    pub ensure_index_fresh_calls: u64,
    pub mutations: u64,
    pub conflict_outcomes: u64,
    pub dual_success_races: u64,
}

impl VaultBaselineSnapshot {
    pub fn render_line(&self, label: &str) -> String {
        format!(
            "h07_baseline label={label} walks={} statted={} read_files={} bytes_read={} bytes_written={} link_rebuilds={} index_rewrites={} ensure_fresh={} mutations={} conflicts={} dual_success={}",
            self.recursive_root_walks,
            self.files_statted,
            self.files_read,
            self.bytes_read,
            self.bytes_written,
            self.link_rebuilds,
            self.index_rewrites,
            self.ensure_index_fresh_calls,
            self.mutations,
            self.conflict_outcomes,
            self.dual_success_races
        )
    }
}

static GLOBAL: once_cell::sync::Lazy<Arc<VaultBaselineCounters>> =
    once_cell::sync::Lazy::new(|| Arc::new(VaultBaselineCounters::default()));

pub fn vault_baseline_counters() -> Arc<VaultBaselineCounters> {
    Arc::clone(&GLOBAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_are_observable_without_target_asserts() {
        let counters = VaultBaselineCounters::default();
        counters
            .ensure_index_fresh_calls
            .fetch_add(3, Ordering::Relaxed);
        counters.link_rebuilds.fetch_add(1, Ordering::Relaxed);
        let snap = counters.snapshot();
        assert_eq!(snap.ensure_index_fresh_calls, 3);
        assert_eq!(snap.link_rebuilds, 1);
        let line = snap.render_line("unit");
        assert!(line.contains("ensure_fresh=3"));
        // Intentionally no P06 target assertions in Car 0.
    }
}
