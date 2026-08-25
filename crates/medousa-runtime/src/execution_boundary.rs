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
    deadline: Instant,
    host_context: Option<Arc<dyn Any + Send + Sync>>,
}

impl TurnExecutionBoundary {
    pub fn new(cancellation: CancellationToken, deadline: Instant) -> Self {
        Self {
            cancellation,
            deadline,
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

    pub fn deadline(&self) -> Instant {
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
/// absolute deadline. An unscoped leaf fails closed before it is polled.
pub async fn await_turn_boundary<F, T>(future: F) -> Result<T, TurnExecutionBoundaryError>
where
    F: Future<Output = T>,
{
    let Some(boundary) = active_turn_execution_boundary() else {
        MISSING_TURN_EXECUTION_BOUNDARY_INVOCATIONS.fetch_add(1, Ordering::Relaxed);
        return Err(TurnExecutionBoundaryError::MissingContext);
    };
    let cancellation = boundary.cancellation().clone();
    let deadline = tokio::time::Instant::from_std(boundary.deadline());
    tokio::pin!(future);
    tokio::select! {
        biased;
        () = cancellation.cancelled() => Err(TurnExecutionBoundaryError::Cancelled),
        () = tokio::time::sleep_until(deadline) => {
            cancellation.cancel();
            Err(TurnExecutionBoundaryError::DeadlineExceeded)
        }
        output = &mut future => Ok(output),
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
