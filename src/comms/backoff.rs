//! Reconnect discipline primitives shared by the comms service and any client
//! that reattaches to the daemon: bounded exponential backoff with jitter, a
//! circuit breaker, and an overlap guard.
//!
//! These replace the historical cap-less 500ms reconnect loop. All timing is
//! injectable (`Instant` is passed in) so the logic is unit-testable without
//! sleeping.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Exponential backoff with a hard cap and optional full-jitter.
#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub base: Duration,
    pub factor: f64,
    /// Hard cap — backoff never exceeds this regardless of attempt count.
    pub max: Duration,
    /// When true, the returned delay is uniformly sampled in `[0, computed]`
    /// (AWS "full jitter") to avoid reconnect thundering herds.
    pub jitter: bool,
    /// Optional maximum number of attempts before the caller should give up and
    /// open the breaker / surface a hard failure. `None` = retry forever
    /// (still capped per-attempt).
    pub max_attempts: Option<u32>,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(500),
            factor: 2.0,
            max: Duration::from_secs(30),
            jitter: true,
            max_attempts: Some(10),
        }
    }
}

impl BackoffPolicy {
    /// Deterministic (no-jitter) delay for a 0-based attempt index, saturating
    /// at `max`. Attempt 0 yields `base`.
    pub fn base_delay(&self, attempt: u32) -> Duration {
        let cap_ms = self.max.as_millis() as f64;
        let base_ms = self.base.as_millis() as f64;
        let factor = if self.factor < 1.0 { 1.0 } else { self.factor };
        // factor.powi can overflow to inf; min with cap keeps it bounded.
        let raw = base_ms * factor.powi(attempt.min(64) as i32);
        let clamped = raw.min(cap_ms).max(0.0);
        Duration::from_millis(clamped as u64)
    }

    /// Delay for an attempt, applying jitter if configured. `rand_unit` must be
    /// in `[0, 1)`; callers pass `rand::random::<f64>()` in production and a
    /// fixed value in tests.
    pub fn delay_with(&self, attempt: u32, rand_unit: f64) -> Duration {
        let base = self.base_delay(attempt);
        if !self.jitter {
            return base;
        }
        let unit = rand_unit.clamp(0.0, 1.0);
        let ms = (base.as_millis() as f64 * unit) as u64;
        Duration::from_millis(ms)
    }

    /// Production helper: jittered delay using the thread RNG.
    pub fn delay(&self, attempt: u32) -> Duration {
        self.delay_with(attempt, rand::random::<f64>())
    }

    /// Whether another attempt is permitted under `max_attempts`.
    pub fn may_retry(&self, attempt: u32) -> bool {
        match self.max_attempts {
            Some(max) => attempt < max,
            None => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Requests flow normally.
    Closed,
    /// Tripped — requests are rejected until the cooldown elapses.
    Open,
    /// Cooldown elapsed; a single probe request is allowed through.
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Consecutive failures that trip the breaker from `Closed` -> `Open`.
    pub failure_threshold: u32,
    /// How long the breaker stays `Open` before allowing a half-open probe.
    pub cooldown: Duration,
    /// Consecutive successes in `HalfOpen` required to fully close again.
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            cooldown: Duration::from_secs(15),
            success_threshold: 1,
        }
    }
}

/// A time-injectable circuit breaker. Call [`CircuitBreaker::allow`] (passing
/// "now") before a request; record the result with
/// [`CircuitBreaker::on_success`] / [`CircuitBreaker::on_failure`].
#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitState,
    consecutive_failures: u32,
    consecutive_successes: u32,
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: CircuitState::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            opened_at: None,
        }
    }

    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Returns whether a request should be allowed at time `now`, transitioning
    /// `Open` -> `HalfOpen` when the cooldown has elapsed.
    pub fn allow(&mut self, now: Instant) -> bool {
        match self.state {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Open => {
                let elapsed = self
                    .opened_at
                    .map(|at| now.saturating_duration_since(at))
                    .unwrap_or(self.config.cooldown);
                if elapsed >= self.config.cooldown {
                    self.state = CircuitState::HalfOpen;
                    self.consecutive_successes = 0;
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn on_success(&mut self) {
        self.consecutive_failures = 0;
        match self.state {
            CircuitState::HalfOpen => {
                self.consecutive_successes += 1;
                if self.consecutive_successes >= self.config.success_threshold {
                    self.state = CircuitState::Closed;
                    self.consecutive_successes = 0;
                    self.opened_at = None;
                }
            }
            CircuitState::Closed => {}
            CircuitState::Open => {
                // A success while nominally open (probe raced) closes us.
                self.state = CircuitState::Closed;
                self.opened_at = None;
            }
        }
    }

    pub fn on_failure(&mut self, now: Instant) {
        self.consecutive_successes = 0;
        match self.state {
            CircuitState::HalfOpen => {
                self.trip(now);
            }
            CircuitState::Closed => {
                self.consecutive_failures += 1;
                if self.consecutive_failures >= self.config.failure_threshold {
                    self.trip(now);
                }
            }
            CircuitState::Open => {}
        }
    }

    fn trip(&mut self, now: Instant) {
        self.state = CircuitState::Open;
        self.opened_at = Some(now);
        self.consecutive_failures = 0;
        self.consecutive_successes = 0;
    }
}

/// Guard that ensures at most one reconnect / reattach runs at a time. A new
/// attempt that arrives while one is in flight is rejected (returns `None`)
/// rather than queued, so concurrent triggers (visibility + network + timer)
/// collapse into a single reconnect.
#[derive(Clone, Default)]
pub struct OverlapGuard {
    active: Arc<AtomicBool>,
}

impl OverlapGuard {
    pub fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Acquire the single slot. Returns `None` if an attempt is already running.
    /// Drop the returned permit to release the slot.
    pub fn try_enter(&self) -> Option<OverlapPermit> {
        self.active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| OverlapPermit {
                active: Arc::clone(&self.active),
            })
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

/// RAII permit; releases the [`OverlapGuard`] slot on drop.
pub struct OverlapPermit {
    active: Arc<AtomicBool>,
}

impl Drop for OverlapPermit {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        let policy = BackoffPolicy {
            base: Duration::from_millis(500),
            factor: 2.0,
            max: Duration::from_secs(8),
            jitter: false,
            max_attempts: Some(10),
        };
        assert_eq!(policy.base_delay(0), Duration::from_millis(500));
        assert_eq!(policy.base_delay(1), Duration::from_millis(1000));
        assert_eq!(policy.base_delay(2), Duration::from_millis(2000));
        assert_eq!(policy.base_delay(3), Duration::from_millis(4000));
        // Caps at 8s and never exceeds it.
        assert_eq!(policy.base_delay(4), Duration::from_secs(8));
        assert_eq!(policy.base_delay(50), Duration::from_secs(8));
    }

    #[test]
    fn jitter_stays_within_base_delay() {
        let policy = BackoffPolicy {
            base: Duration::from_millis(1000),
            factor: 2.0,
            max: Duration::from_secs(30),
            jitter: true,
            max_attempts: None,
        };
        let full = policy.base_delay(2);
        assert!(policy.delay_with(2, 0.0) <= full);
        assert!(policy.delay_with(2, 0.999) <= full);
        assert_eq!(policy.delay_with(2, 0.0), Duration::ZERO);
    }

    #[test]
    fn max_attempts_caps_retries() {
        let policy = BackoffPolicy {
            max_attempts: Some(3),
            ..BackoffPolicy::default()
        };
        assert!(policy.may_retry(0));
        assert!(policy.may_retry(2));
        assert!(!policy.may_retry(3));
        assert!(!policy.may_retry(9));
    }

    #[test]
    fn breaker_trips_after_threshold_and_recovers() {
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            cooldown: Duration::from_secs(10),
            success_threshold: 1,
        });
        let t0 = Instant::now();
        assert!(breaker.allow(t0));
        breaker.on_failure(t0);
        breaker.on_failure(t0);
        assert_eq!(breaker.state(), CircuitState::Closed);
        breaker.on_failure(t0);
        assert_eq!(breaker.state(), CircuitState::Open);
        // Still open before cooldown.
        assert!(!breaker.allow(t0 + Duration::from_secs(5)));
        // Cooldown elapsed -> half-open probe allowed.
        let t1 = t0 + Duration::from_secs(10);
        assert!(breaker.allow(t1));
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
        // A success closes it.
        breaker.on_success();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn half_open_failure_reopens() {
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            cooldown: Duration::from_secs(5),
            success_threshold: 1,
        });
        let t0 = Instant::now();
        breaker.on_failure(t0);
        assert_eq!(breaker.state(), CircuitState::Open);
        let t1 = t0 + Duration::from_secs(5);
        assert!(breaker.allow(t1));
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
        breaker.on_failure(t1);
        assert_eq!(breaker.state(), CircuitState::Open);
        // Re-armed cooldown from t1.
        assert!(!breaker.allow(t1 + Duration::from_secs(4)));
        assert!(breaker.allow(t1 + Duration::from_secs(5)));
    }

    #[test]
    fn overlap_guard_admits_one_at_a_time() {
        let guard = OverlapGuard::new();
        let permit = guard.try_enter().expect("first enters");
        assert!(guard.is_active());
        assert!(guard.try_enter().is_none(), "second is rejected while in flight");
        drop(permit);
        assert!(!guard.is_active());
        assert!(guard.try_enter().is_some(), "slot reusable after release");
    }
}
