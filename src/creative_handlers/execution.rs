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

    #[tokio::test]
    async fn simultaneous_startup_reads_wait_for_capacity_and_execute_once_each() {
        let execution = Arc::new(ForgeExecutionService::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..3 {
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
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(execution.running_commands(), 0);
        assert!(execution.metrics().rejected.load(Ordering::Relaxed) > 0);
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
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, finish) = std::sync::mpsc::channel();
        let owner = tokio::spawn({
            let execution = execution.clone();
            async move {
                execution
                    .run(
                        ExecutionClass::Compaction,
                        MAX_COMPACTION_BUFFER_BYTES,
                        move || {
                            let _ = started.send(());
                            finish.recv().unwrap();
                            Ok(())
                        },
                    )
                    .await
            }
        });
        ready.await.unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let waiter = tokio::spawn(run(execution.clone(), move || {
            count.fetch_add(1, Ordering::SeqCst);
        }));
        while execution.metrics().rejected.load(Ordering::Relaxed) == 0 {
            tokio::task::yield_now().await;
        }
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        release.send(()).unwrap();
        owner.await.unwrap().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn persistent_contention_returns_overload_without_running_work() {
        let execution = Arc::new(ForgeExecutionService::new());
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, finish) = std::sync::mpsc::channel();
        let owner = tokio::spawn({
            let execution = execution.clone();
            async move {
                execution
                    .run(
                        ExecutionClass::Compaction,
                        MAX_COMPACTION_BUFFER_BYTES,
                        move || {
                            let _ = started.send(());
                            finish.recv().unwrap();
                            Ok(())
                        },
                    )
                    .await
            }
        });
        ready.await.unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            run(execution.clone(), move || {
                count.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .await;
        release.send(()).unwrap();
        owner.await.unwrap().unwrap();
        assert!(matches!(result.unwrap(), Err(ForgeError::Overloaded(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(execution.metrics().rejected.load(Ordering::Relaxed), 5);
    }
}
