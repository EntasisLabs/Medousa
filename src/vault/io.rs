//! Process-level vault I/O admission (H07 repair R4).
//!
//! Async handlers validate/parse then `await` this service. Blocking vault work
//! runs on bounded workers after global + class permits are acquired. A full
//! queue never falls back to inline sync work on a Tokio worker.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use once_cell::sync::Lazy;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::vault::contracts::VaultMutationError;

const MAX_QUEUED: usize = 64;
const MAX_MUTATION_WORKERS: usize = 8;
const MAX_SCAN_WORKERS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultIoClass {
    Mutation,
    Scan,
    SearchRebuild,
}

#[derive(Debug, Default)]
pub struct VaultIoMetrics {
    pub admitted: AtomicU64,
    pub rejected: AtomicU64,
    pub completed: AtomicU64,
}

pub struct VaultIoService {
    queue: Arc<Semaphore>,
    mutation: Arc<Semaphore>,
    scan: Arc<Semaphore>,
    search: Arc<Semaphore>,
    pub metrics: VaultIoMetrics,
}

static VAULT_IO: Lazy<VaultIoService> = Lazy::new(VaultIoService::new);

pub fn vault_io() -> &'static VaultIoService {
    &VAULT_IO
}

struct Admission {
    _queue: OwnedSemaphorePermit,
    _class: OwnedSemaphorePermit,
}

impl VaultIoService {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Semaphore::new(MAX_QUEUED)),
            mutation: Arc::new(Semaphore::new(MAX_MUTATION_WORKERS)),
            scan: Arc::new(Semaphore::new(MAX_SCAN_WORKERS)),
            search: Arc::new(Semaphore::new(MAX_SCAN_WORKERS)),
            metrics: VaultIoMetrics::default(),
        }
    }

    fn class_semaphore(&self, class: VaultIoClass) -> Arc<Semaphore> {
        match class {
            VaultIoClass::Mutation => Arc::clone(&self.mutation),
            VaultIoClass::Scan => Arc::clone(&self.scan),
            VaultIoClass::SearchRebuild => Arc::clone(&self.search),
        }
    }

    async fn admit(&self, class: VaultIoClass) -> Result<Admission, VaultMutationError> {
        let queue = Arc::clone(&self.queue)
            .try_acquire_owned()
            .map_err(|_| {
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                VaultMutationError::Overloaded
            })?;
        let class_sem = self.class_semaphore(class);
        let class_permit = class_sem.acquire_owned().await.map_err(|_| {
            self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
            VaultMutationError::Overloaded
        })?;
        self.metrics.admitted.fetch_add(1, Ordering::Relaxed);
        Ok(Admission {
            _queue: queue,
            _class: class_permit,
        })
    }

    /// Admit then run `work` on the blocking pool. Never runs vault I/O inline
    /// on the calling Tokio worker.
    pub async fn run<T, F>(
        &self,
        class: VaultIoClass,
        work: F,
    ) -> Result<T, VaultMutationError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, VaultMutationError> + Send + 'static,
    {
        let admission = self.admit(class).await?;
        let join = tokio::task::spawn_blocking(move || {
            let _admission = admission;
            work()
        })
        .await
        .map_err(|error| VaultMutationError::Invalid(format!("vault worker failed: {error}")))?;
        self.metrics.completed.fetch_add(1, Ordering::Relaxed);
        join
    }

    /// Convenience for `anyhow`-returning vault service methods.
    pub async fn run_anyhow<T, F>(&self, class: VaultIoClass, work: F) -> Result<T, anyhow::Error>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, anyhow::Error> + Send + 'static,
    {
        match self
            .run(class, move || work().map_err(|error| VaultMutationError::Invalid(error.to_string())))
            .await
        {
            Ok(value) => Ok(value),
            Err(VaultMutationError::Overloaded) => Err(anyhow::anyhow!("vault overloaded")),
            Err(other) => Err(anyhow::anyhow!(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::thread;
    use std::time::Duration;

    #[tokio::test]
    async fn full_queue_returns_overloaded_without_inline_fallback() {
        let service = VaultIoService::new();
        // Exhaust the queue with held permits.
        let mut held = Vec::new();
        for _ in 0..MAX_QUEUED {
            held.push(Arc::clone(&service.queue).acquire_owned().await.unwrap());
        }
        let err = service
            .run(VaultIoClass::Scan, || Ok::<_, VaultMutationError>(()))
            .await
            .unwrap_err();
        assert!(matches!(err, VaultMutationError::Overloaded));
        drop(held);
    }

    #[tokio::test]
    async fn work_runs_off_tokio_worker() {
        let service = VaultIoService::new();
        let barrier = Arc::new(Barrier::new(2));
        let barrier_worker = Arc::clone(&barrier);
        let handle = tokio::spawn(async move {
            service
                .run(VaultIoClass::Mutation, move || {
                    barrier_worker.wait();
                    Ok::<_, VaultMutationError>(42u32)
                })
                .await
        });
        // Prove the async task can make progress while blocking work waits.
        tokio::time::sleep(Duration::from_millis(20)).await;
        barrier.wait();
        assert_eq!(handle.await.unwrap().unwrap(), 42);
    }

    #[test]
    fn sync_smoke_constructs() {
        let _ = VaultIoService::new();
        let _ = vault_io();
        let _ = thread::spawn(|| ());
    }
}
