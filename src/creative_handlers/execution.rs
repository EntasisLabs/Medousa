//! Retry only admission contention, before any Create work has started.
use medousa_forge::{
    ForgeError,
    execution::{ExecutionClass, ForgeExecutionService, MAX_COMPACTION_BUFFER_BYTES},
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) async fn run<T: Send + 'static>(
    execution: Arc<ForgeExecutionService>,
    work: impl FnOnce() -> T + Send + 'static,
) -> medousa_forge::Result<T> {
    // The closure is retained, not cloned or replayed. Forge consumes it only
    // after acquiring capacity; a cancelled blocking job retains its permits.
    let pending = Arc::new(Mutex::new(Some(work)));
    for attempt in 0..5 {
        let task = pending.clone();
        let result = execution
            .run(
                ExecutionClass::Compaction,
                MAX_COMPACTION_BUFFER_BYTES,
                move || {
                    let work = task
                        .lock()
                        .map_err(|_| ForgeError::Store("Create work lock poisoned".into()))?
                        .take()
                        .ok_or_else(|| ForgeError::Store("Create work already started".into()))?;
                    Ok(work())
                },
            )
            .await;
        match result {
            Err(ForgeError::Overloaded(_)) if attempt < 4 => {
                tokio::time::sleep(Duration::from_millis(50 << attempt)).await;
            }
            result => return result,
        }
    }
    unreachable!("the final admission attempt returns")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn occupy_workers(
        execution: Arc<ForgeExecutionService>,
    ) -> (
        Vec<std::sync::mpsc::Sender<()>>,
        Vec<tokio::task::JoinHandle<medousa_forge::Result<()>>>,
    ) {
        let mut releases = Vec::new();
        let mut owners = Vec::new();
        for _ in 0..medousa_forge::execution::MAX_BLOCKING_JOBS {
            let (started, ready) = tokio::sync::oneshot::channel();
            let (release, finish) = std::sync::mpsc::channel();
            releases.push(release);
            let execution = execution.clone();
            owners.push(tokio::spawn(async move {
                execution
                    .run(
                        ExecutionClass::Compaction,
                        MAX_COMPACTION_BUFFER_BYTES,
                        move || {
                            let _ = started.send(());
                            let _ = finish.recv();
                            Ok(())
                        },
                    )
                    .await
            }));
            tokio::time::timeout(Duration::from_secs(5), ready)
                .await
                .expect("worker must start")
                .unwrap();
        }
        (releases, owners)
    }

    async fn wait_for_queued(execution: &ForgeExecutionService, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while execution.queued_commands() < count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("jobs must reach the admission queue");
    }

    #[tokio::test]
    async fn simultaneous_startup_reads_wait_for_capacity_and_execute_once_each() {
        let execution = Arc::new(ForgeExecutionService::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        let jobs = medousa_forge::execution::MAX_BLOCKING_JOBS + 3;
        for _ in 0..jobs {
            let execution = execution.clone();
            let calls = calls.clone();
            tasks.push(tokio::spawn(run(execution, move || {
                assert!(medousa_forge::execution::already_admitted());
                calls.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(25));
                42
            })));
        }
        for task in tasks {
            assert_eq!(task.await.unwrap().unwrap(), 42);
        }
        assert_eq!(calls.load(Ordering::SeqCst), jobs);
        assert_eq!(execution.running_commands(), 0);
        assert_eq!(execution.queued_commands(), 0);
        assert_eq!(execution.metrics().rejected.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn a_store_error_is_returned_once_without_replaying_the_work() {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let result = run(Arc::new(ForgeExecutionService::new()), move || {
            count.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>("revision conflict")
        })
        .await
        .unwrap();
        assert_eq!(result, Err("revision conflict"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_cancelled_waiter_never_runs_its_mutation() {
        let execution = Arc::new(ForgeExecutionService::new());
        let (releases, owners) = occupy_workers(execution.clone()).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let waiter = tokio::spawn(run(execution.clone(), move || {
            count.fetch_add(1, Ordering::SeqCst);
        }));
        wait_for_queued(&execution, 1).await;
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        drop(releases);
        for owner in owners {
            owner.await.unwrap().unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(execution.queued_commands(), 0);
    }

    #[tokio::test]
    async fn persistent_contention_returns_overload_without_running_work() {
        let execution = Arc::new(ForgeExecutionService::new());
        use medousa_forge::execution::{MAX_BLOCKING_JOBS, MAX_QUEUED_COMMANDS};
        let (releases, owners) = occupy_workers(execution.clone()).await;
        let mut queued = Vec::new();
        for _ in 0..MAX_QUEUED_COMMANDS - MAX_BLOCKING_JOBS {
            let execution = execution.clone();
            queued.push(tokio::spawn(async move {
                execution
                    .run(
                        ExecutionClass::Compaction,
                        MAX_COMPACTION_BUFFER_BYTES,
                        || Ok(()),
                    )
                    .await
            }));
        }
        wait_for_queued(&execution, MAX_QUEUED_COMMANDS - MAX_BLOCKING_JOBS).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            run(execution.clone(), move || {
                count.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .await;
        drop(releases);
        for task in owners.into_iter().chain(queued) {
            task.await.unwrap().unwrap();
        }
        assert!(matches!(result.unwrap(), Err(ForgeError::Overloaded(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(execution.metrics().rejected.load(Ordering::Relaxed), 5);
        assert_eq!(execution.running_commands(), 0);
        assert_eq!(execution.queued_commands(), 0);
    }
}
