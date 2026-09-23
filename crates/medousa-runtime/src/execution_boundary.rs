//! Portable cancellation/deadline boundary for provider and tool leaves.
//!
//! Runtime compositions may attach an opaque host context. The foreground loop
//! only retains and re-enters that context across spawned tasks; it never
//! interprets host identity, filesystem, transport, or delivery authority.

use std::any::Any;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct TurnExecutionBoundary {
    cancellation: CancellationToken,
    deadline: Option<Instant>,
    host_context: Option<Arc<dyn Any + Send + Sync>>,
}

impl TurnExecutionBoundary {
    /// Interactive work can omit a total execution deadline. Individual
    /// transports/tools still own their stall timeouts, and cancellation is
    /// always enforced. Jobs with an explicit budget retain an absolute limit.
    pub fn new(cancellation: CancellationToken, deadline: impl Into<Option<Instant>>) -> Self {
        Self {
            cancellation,
            deadline: deadline.into(),
            host_context: None,
        }
    }

    pub fn with_host_context<T>(mut self, context: Arc<T>) -> Self
    where
        T: Any + Send + Sync,
    {
        self.host_context = Some(context);
        self
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn host_context<T>(&self) -> Option<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        self.host_context.as_ref()?.clone().downcast::<T>().ok()
    }
}

impl fmt::Debug for TurnExecutionBoundary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TurnExecutionBoundary")
            .field("cancelled", &self.cancellation.is_cancelled())
            .field("deadline", &self.deadline)
            .field("has_host_context", &self.host_context.is_some())
            .finish()
    }
}

tokio::task_local! {
    static ACTIVE_TURN_EXECUTION_BOUNDARY: Arc<TurnExecutionBoundary>;
}

static MISSING_TURN_EXECUTION_BOUNDARY_INVOCATIONS: AtomicU64 = AtomicU64::new(0);

pub async fn with_turn_execution_boundary<F>(
    boundary: Arc<TurnExecutionBoundary>,
    future: F,
) -> F::Output
where
    F: Future,
{
    ACTIVE_TURN_EXECUTION_BOUNDARY.scope(boundary, future).await
}

pub fn active_turn_execution_boundary() -> Option<Arc<TurnExecutionBoundary>> {
    ACTIVE_TURN_EXECUTION_BOUNDARY.try_with(Arc::clone).ok()
}

pub fn missing_turn_execution_boundary_invocations() -> u64 {
    MISSING_TURN_EXECUTION_BOUNDARY_INVOCATIONS.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnExecutionBoundaryError {
    MissingContext,
    Cancelled,
    DeadlineExceeded,
}

impl fmt::Display for TurnExecutionBoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingContext => formatter.write_str("turn execution context is missing"),
            Self::Cancelled => formatter.write_str("turn cancelled"),
            Self::DeadlineExceeded => formatter.write_str("turn execution deadline exceeded"),
        }
    }
}

impl std::error::Error for TurnExecutionBoundaryError {}

/// Await one provider/tool leaf under the active turn's cancellation root and
/// optional absolute deadline. An unscoped leaf fails closed before it is polled.
pub async fn await_turn_boundary<F, T>(future: F) -> Result<T, TurnExecutionBoundaryError>
where
    F: Future<Output = T>,
{
    let Some(boundary) = active_turn_execution_boundary() else {
        MISSING_TURN_EXECUTION_BOUNDARY_INVOCATIONS.fetch_add(1, Ordering::Relaxed);
        return Err(TurnExecutionBoundaryError::MissingContext);
    };
    let cancellation = boundary.cancellation().clone();
    tokio::pin!(future);
    tokio::select! {
        biased;
        () = cancellation.cancelled() => Err(TurnExecutionBoundaryError::Cancelled),
        () = wait_for_turn_deadline(boundary.deadline()) => {
            cancellation.cancel();
            Err(TurnExecutionBoundaryError::DeadlineExceeded)
        }
        output = &mut future => Ok(output),
    }
}

/// Wait for an explicitly configured total execution limit. With no limit this
/// branch stays pending, leaving completion and cancellation to the caller.
pub async fn wait_for_turn_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::*;

    fn boundary(cancellation: CancellationToken) -> Arc<TurnExecutionBoundary> {
        Arc::new(TurnExecutionBoundary::new(
            cancellation,
            Instant::now() + Duration::from_secs(60),
        ))
    }

    #[tokio::test]
    async fn unscoped_leaf_fails_closed_without_polling() {
        let polled = Arc::new(AtomicBool::new(false));
        let observed = polled.clone();
        let result = await_turn_boundary(async move {
            observed.store(true, Ordering::Relaxed);
        })
        .await;

        assert_eq!(result, Err(TurnExecutionBoundaryError::MissingContext));
        assert!(!polled.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_scoped_leaf() {
        let cancellation = CancellationToken::new();
        let active = boundary(cancellation.clone());
        cancellation.cancel();

        let result = with_turn_execution_boundary(active, async {
            await_turn_boundary(std::future::pending::<()>()).await
        })
        .await;

        assert_eq!(result, Err(TurnExecutionBoundaryError::Cancelled));
    }

    #[tokio::test]
    async fn deadline_interrupts_and_cancels_the_root() {
        let cancellation = CancellationToken::new();
        let active = Arc::new(TurnExecutionBoundary::new(
            cancellation.clone(),
            Instant::now() - Duration::from_millis(1),
        ));

        let result = with_turn_execution_boundary(active, async {
            await_turn_boundary(std::future::pending::<()>()).await
        })
        .await;

        assert_eq!(result, Err(TurnExecutionBoundaryError::DeadlineExceeded));
        assert!(cancellation.is_cancelled());
    }

    #[tokio::test(start_paused = true)]
    async fn interactive_execution_can_outlive_previous_turn_caps() {
        let cancellation = CancellationToken::new();
        let active = Arc::new(TurnExecutionBoundary::new(cancellation.clone(), None));
        let started = tokio::time::Instant::now();

        with_turn_execution_boundary(active, async {
            // Model and tool leaves share cancellation, not a dwindling budget
            // from the start of the conversation turn.
            for _ in 0..3 {
                assert_eq!(
                    await_turn_boundary(async {
                        tokio::time::sleep(Duration::from_secs(3_000)).await;
                        "completed"
                    })
                    .await,
                    Ok("completed")
                );
            }
        })
        .await;

        assert!(started.elapsed() > Duration::from_secs(2 * 60 * 60));
        assert!(!cancellation.is_cancelled());
    }

    #[tokio::test(start_paused = true)]
    async fn execution_without_deadline_still_stops_on_user_cancellation() {
        let cancellation = CancellationToken::new();
        let active = Arc::new(TurnExecutionBoundary::new(cancellation.clone(), None));
        let cancel = async {
            tokio::time::sleep(Duration::from_secs(181)).await;
            cancellation.cancel();
        };
        let (_, result) = tokio::join!(
            cancel,
            with_turn_execution_boundary(active, async {
                await_turn_boundary(std::future::pending::<()>()).await
            })
        );

        assert_eq!(result, Err(TurnExecutionBoundaryError::Cancelled));
    }

    #[tokio::test(start_paused = true)]
    async fn explicit_deadline_still_bounds_multiple_successful_leaves() {
        let cancellation = CancellationToken::new();
        let active = Arc::new(TurnExecutionBoundary::new(
            cancellation.clone(),
            (tokio::time::Instant::now() + Duration::from_secs(180)).into_std(),
        ));
        let result = with_turn_execution_boundary(active, async {
            await_turn_boundary(tokio::time::sleep(Duration::from_secs(120)))
                .await
                .unwrap();
            await_turn_boundary(tokio::time::sleep(Duration::from_secs(120))).await
        })
        .await;

        assert_eq!(result, Err(TurnExecutionBoundaryError::DeadlineExceeded));
        assert!(cancellation.is_cancelled());
    }

    #[tokio::test]
    async fn explicit_reentry_preserves_opaque_host_context() {
        #[derive(Debug, PartialEq, Eq)]
        struct HostMarker(&'static str);

        let active = Arc::new(
            TurnExecutionBoundary::new(
                CancellationToken::new(),
                Instant::now() + Duration::from_secs(60),
            )
            .with_host_context(Arc::new(HostMarker("daemon"))),
        );
        let child_boundary = active.clone();
        let observed = with_turn_execution_boundary(active, async move {
            tokio::spawn(with_turn_execution_boundary(child_boundary, async {
                active_turn_execution_boundary()
                    .and_then(|boundary| boundary.host_context::<HostMarker>())
                    .map(|marker| marker.0)
            }))
            .await
            .unwrap()
        })
        .await;

        assert_eq!(observed, Some("daemon"));
    }
}
