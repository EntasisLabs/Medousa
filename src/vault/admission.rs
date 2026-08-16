//! Bounded vault I/O admission (H07.1a).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::vault::contracts::VaultMutationError;

const GLOBAL_MUTATION_LIMIT: usize = 64;
const PER_ROOT_MUTATION_LIMIT: usize = 8;
const BLOCKING_JOB_LIMIT: usize = 8;

#[derive(Debug, Default)]
pub struct VaultAdmission {
    global_mutations: AtomicUsize,
    blocking_jobs: AtomicUsize,
}

pub struct AdmissionPermit {
    kind: PermitKind,
    admission: Arc<VaultAdmission>,
}

#[derive(Debug, Clone, Copy)]
enum PermitKind {
    Mutation,
    Blocking,
}

impl VaultAdmission {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn admit_mutation(self: &Arc<Self>) -> Result<AdmissionPermit, VaultMutationError> {
        loop {
            let current = self.global_mutations.load(Ordering::Acquire);
            if current >= GLOBAL_MUTATION_LIMIT {
                return Err(VaultMutationError::Overloaded);
            }
            if self
                .global_mutations
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(AdmissionPermit {
                    kind: PermitKind::Mutation,
                    admission: Arc::clone(self),
                });
            }
        }
    }

    pub fn admit_blocking(self: &Arc<Self>) -> Result<AdmissionPermit, VaultMutationError> {
        loop {
            let current = self.blocking_jobs.load(Ordering::Acquire);
            if current >= BLOCKING_JOB_LIMIT {
                return Err(VaultMutationError::Overloaded);
            }
            if self
                .blocking_jobs
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(AdmissionPermit {
                    kind: PermitKind::Blocking,
                    admission: Arc::clone(self),
                });
            }
        }
    }

    pub fn mutation_in_flight(&self) -> usize {
        self.global_mutations.load(Ordering::Relaxed)
    }

    pub const fn per_root_limit() -> usize {
        PER_ROOT_MUTATION_LIMIT
    }
}

impl Drop for AdmissionPermit {
    fn drop(&mut self) {
        match self.kind {
            PermitKind::Mutation => {
                self.admission
                    .global_mutations
                    .fetch_sub(1, Ordering::AcqRel);
            }
            PermitKind::Blocking => {
                self.admission.blocking_jobs.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_admission_bounds_and_releases() {
        let admission = VaultAdmission::new();
        let mut permits = Vec::new();
        for _ in 0..GLOBAL_MUTATION_LIMIT {
            permits.push(admission.admit_mutation().unwrap());
        }
        assert!(matches!(
            admission.admit_mutation(),
            Err(VaultMutationError::Overloaded)
        ));
        drop(permits);
        assert!(admission.admit_mutation().is_ok());
    }
}
