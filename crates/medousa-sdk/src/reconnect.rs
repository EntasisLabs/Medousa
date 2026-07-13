//! Client-side reconnect discipline for interactive SSE streams.
//!
//! Mirrors the server `medousa::comms` backoff / circuit-breaker / overlap-guard
//! semantics so SDK consumers get bounded, jittered reattach with `?since=<seq>`
//! against the daemon's disk-backed turn spine.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::transport::path_with_query;

/// Policy for reconnecting an interactive turn SSE stream after a drop.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct ReconnectPolicy {
    pub backoff: BackoffPolicy,
    pub breaker: CircuitBreakerConfig,
}


/// Exponential backoff with cap and deterministic pseudo-jitter (no `rand` dep).
#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub base: Duration,
    pub factor: f64,
    pub max: Duration,
    pub max_attempts: Option<u32>,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(500),
            factor: 2.0,
            max: Duration::from_secs(30),
            max_attempts: Some(10),
        }
    }
}

impl BackoffPolicy {
    pub fn delay(&self, attempt: u32) -> Duration {
        let cap_ms = self.max.as_millis() as f64;
        let base_ms = self.base.as_millis() as f64;
        let factor = if self.factor < 1.0 { 1.0 } else { self.factor };
        let raw = base_ms * factor.powi(attempt.min(64) as i32);
        let clamped = raw.min(cap_ms).max(0.0);
        // Pseudo-jitter in `[0.5, 1.0]` of computed delay.
        let jitter = 0.5 + ((attempt as u64).wrapping_mul(7919) % 500) as f64 / 1000.0;
        Duration::from_millis((clamped * jitter) as u64)
    }

    pub fn may_retry(&self, attempt: u32) -> bool {
        match self.max_attempts {
            Some(max) => attempt < max,
            None => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub cooldown: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            cooldown: Duration::from_secs(15),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    consecutive_failures: u32,
    state: CircuitState,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            consecutive_failures: 0,
            state: CircuitState::Closed,
        }
    }

    pub fn state(&self) -> CircuitState {
        self.state
    }

    pub fn allow(&mut self) -> bool {
        self.state == CircuitState::Closed
    }

    pub fn on_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = CircuitState::Closed;
    }

    pub fn on_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures >= self.config.failure_threshold {
            self.state = CircuitState::Open;
        }
    }
}

/// Rejects a second concurrent reconnect on the same guard.
#[derive(Debug, Clone)]
pub struct OverlapGuard {
    active: Arc<AtomicBool>,
}

impl Default for OverlapGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlapGuard {
    pub fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn try_enter(&self) -> Option<OverlapPermit> {
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            Some(OverlapPermit {
                active: Arc::clone(&self.active),
            })
        } else {
            None
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

pub struct OverlapPermit {
    active: Arc<AtomicBool>,
}

impl Drop for OverlapPermit {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

/// Strip any existing query string and append `?since=<seq>` for spine replay.
pub fn stream_path_with_since(path: &str, since: u64) -> String {
    let base = path.split('?').next().unwrap_or(path);
    if since == 0 {
        return path_with_query(base, &[]);
    }
    path_with_query(base, &[("since", since.to_string())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_path_appends_since() {
        assert_eq!(
            stream_path_with_since("/v1/interactive/turn/t1/stream", 42),
            "/v1/interactive/turn/t1/stream?since=42"
        );
    }

    #[test]
    fn stream_path_replaces_existing_query() {
        assert_eq!(
            stream_path_with_since("/v1/interactive/turn/t1/stream?since=1", 99),
            "/v1/interactive/turn/t1/stream?since=99"
        );
    }

    #[test]
    fn backoff_caps_and_grows() {
        let policy = BackoffPolicy::default();
        assert!(policy.delay(0) >= Duration::from_millis(250));
        assert!(policy.delay(5) <= policy.max);
    }

    #[test]
    fn overlap_guard_admits_one() {
        let guard = OverlapGuard::new();
        let _a = guard.try_enter().expect("first");
        assert!(guard.try_enter().is_none());
    }
}
