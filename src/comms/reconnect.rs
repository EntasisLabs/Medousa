//! A reusable reconnect driver that combines the three discipline primitives —
//! bounded backoff, a circuit breaker, and an overlap guard — into one loop.
//!
//! This is the Rust counterpart to the client-side reconnect logic: it caps
//! retries, backs off exponentially with jitter, trips a breaker after repeated
//! failure, and (via the overlap guard) refuses to start a second reconnect
//! while one is already running.

use std::future::Future;
use std::sync::Mutex;
use std::time::Instant;

use super::backoff::{BackoffPolicy, CircuitBreaker, CircuitBreakerConfig, OverlapGuard};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectError {
    /// Another reconnect was already in flight (overlap guard rejected us).
    AlreadyRunning,
    /// The circuit breaker is open; caller should wait for cooldown.
    CircuitOpen,
    /// Backoff `max_attempts` exhausted without a successful attempt.
    Exhausted,
}

pub struct Reconnector {
    backoff: BackoffPolicy,
    breaker: Mutex<CircuitBreaker>,
    guard: OverlapGuard,
}

impl Reconnector {
    pub fn new(backoff: BackoffPolicy, breaker: CircuitBreakerConfig) -> Self {
        Self {
            backoff,
            breaker: Mutex::new(CircuitBreaker::new(breaker)),
            guard: OverlapGuard::new(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.guard.is_active()
    }

    pub fn circuit_state(&self) -> super::backoff::CircuitState {
        self.breaker.lock().expect("breaker mutex").state()
    }

    /// Drive `attempt` until it succeeds, retrying with backoff. `attempt`
    /// returns `Ok(())` on success or `Err(_)` to retry. Sleeping uses
    /// `tokio::time::sleep`.
    ///
    /// Returns:
    /// - `Ok(attempts_used)` on success,
    /// - `Err(AlreadyRunning)` if a reconnect is already in flight,
    /// - `Err(CircuitOpen)` if the breaker is open and not yet in cooldown,
    /// - `Err(Exhausted)` if all permitted attempts failed.
    pub async fn run<F, Fut, E>(&self, mut attempt: F) -> Result<u32, ReconnectError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<(), E>>,
    {
        let Some(_permit) = self.guard.try_enter() else {
            return Err(ReconnectError::AlreadyRunning);
        };

        // Gate on the breaker before spending any attempts.
        {
            let mut breaker = self.breaker.lock().expect("breaker mutex");
            if !breaker.allow(Instant::now()) {
                return Err(ReconnectError::CircuitOpen);
            }
        }

        let mut attempt_idx: u32 = 0;
        loop {
            match attempt().await {
                Ok(()) => {
                    self.breaker.lock().expect("breaker mutex").on_success();
                    return Ok(attempt_idx + 1);
                }
                Err(_) => {
                    {
                        let mut breaker = self.breaker.lock().expect("breaker mutex");
                        breaker.on_failure(Instant::now());
                    }
                    if !self.backoff.may_retry(attempt_idx + 1) {
                        return Err(ReconnectError::Exhausted);
                    }
                    let delay = self.backoff.delay(attempt_idx);
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                    attempt_idx += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn fast_backoff(max_attempts: Option<u32>) -> BackoffPolicy {
        BackoffPolicy {
            base: Duration::from_millis(1),
            factor: 1.0,
            max: Duration::from_millis(1),
            jitter: false,
            max_attempts,
        }
    }

    #[tokio::test]
    async fn succeeds_after_transient_failures() {
        let reconnector = Reconnector::new(fast_backoff(Some(10)), CircuitBreakerConfig::default());
        let calls = Arc::new(AtomicU32::new(0));
        let calls2 = calls.clone();
        let used = reconnector
            .run(|| {
                let calls = calls2.clone();
                async move {
                    let n = calls.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(())
                    } else {
                        Ok(())
                    }
                }
            })
            .await
            .expect("eventually connects");
        assert_eq!(used, 3);
    }

    #[tokio::test]
    async fn exhausts_after_max_attempts() {
        let reconnector = Reconnector::new(fast_backoff(Some(3)), CircuitBreakerConfig::default());
        let result = reconnector.run(|| async { Err::<(), ()>(()) }).await;
        assert_eq!(result, Err(ReconnectError::Exhausted));
    }

    #[tokio::test]
    async fn rejects_overlapping_reconnect() {
        let reconnector = Arc::new(Reconnector::new(
            fast_backoff(None),
            CircuitBreakerConfig::default(),
        ));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let rx = Arc::new(tokio::sync::Mutex::new(Some(rx)));
        let r1 = reconnector.clone();
        let rx1 = rx.clone();
        // First reconnect blocks inside its attempt until we signal it.
        let first = tokio::spawn(async move {
            r1.run(|| {
                let rx1 = rx1.clone();
                async move {
                    if let Some(rx) = rx1.lock().await.take() {
                        let _ = rx.await;
                    }
                    Ok::<(), ()>(())
                }
            })
            .await
        });

        // Give the first task time to enter the guard.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let second = reconnector.run(|| async { Ok::<(), ()>(()) }).await;
        assert_eq!(second, Err(ReconnectError::AlreadyRunning));

        let _ = tx.send(());
        assert!(first.await.unwrap().is_ok());
    }
}
