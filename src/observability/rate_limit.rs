//! Rate-limit repetitive log lines so heartbeat/proxy noise cannot fill disks.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;

const DEFAULT_INTERVAL: Duration = Duration::from_secs(60);
const DEFAULT_BURST: u32 = 3;

static LIMITER: Lazy<Mutex<RateLimiter>> = Lazy::new(|| Mutex::new(RateLimiter::default()));

#[derive(Debug)]
struct Window {
    window_start: Instant,
    emitted: u32,
    suppressed: u32,
}

#[derive(Debug)]
struct RateLimiter {
    windows: HashMap<String, Window>,
    interval: Duration,
    burst: u32,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            windows: HashMap::new(),
            interval: DEFAULT_INTERVAL,
            burst: DEFAULT_BURST,
        }
    }
}

/// Emit a `warn!` at most `burst` times per minute per key.
pub fn rate_limited_warn(key: &str, message: impl FnOnce() -> String) {
    let mut guard = LIMITER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let interval = guard.interval;
    let burst = guard.burst;
    let now = Instant::now();
    let window = guard.windows.entry(key.to_string()).or_insert_with(|| Window {
        window_start: now,
        emitted: 0,
        suppressed: 0,
    });

    if now.duration_since(window.window_start) >= interval {
        let suppressed = window.suppressed;
        window.window_start = now;
        window.emitted = 0;
        window.suppressed = 0;
        if suppressed > 0 {
            tracing::debug!(
                rate_limit_key = key,
                suppressed_in_window = suppressed,
                "prior repetitive warnings were suppressed"
            );
        }
    }

    if window.emitted < burst {
        window.emitted += 1;
        tracing::warn!("{}", message());
    } else {
        window.suppressed += 1;
    }
}

/// Emit an `error!` at most `burst` times per minute per key.
pub fn rate_limited_error(key: &str, message: impl FnOnce() -> String) {
    let mut guard = LIMITER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let interval = guard.interval;
    let burst = guard.burst;
    let now = Instant::now();
    let window = guard.windows.entry(key.to_string()).or_insert_with(|| Window {
        window_start: now,
        emitted: 0,
        suppressed: 0,
    });

    if now.duration_since(window.window_start) >= interval {
        let suppressed = window.suppressed;
        window.window_start = now;
        window.emitted = 0;
        window.suppressed = 0;
        if suppressed > 0 {
            tracing::debug!(
                rate_limit_key = key,
                suppressed_in_window = suppressed,
                "prior repetitive errors were suppressed"
            );
        }
    }

    if window.emitted < burst {
        window.emitted += 1;
        tracing::error!("{}", message());
    } else {
        window.suppressed += 1;
    }
}

/// Emit a `debug!` at most `burst` times per minute per key.
pub fn rate_limited_debug(key: &str, message: impl FnOnce() -> String) {
    let mut guard = LIMITER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let interval = guard.interval;
    let burst = guard.burst;
    let now = Instant::now();
    let window = guard.windows.entry(key.to_string()).or_insert_with(|| Window {
        window_start: now,
        emitted: 0,
        suppressed: 0,
    });

    if now.duration_since(window.window_start) >= interval {
        window.window_start = now;
        window.emitted = 0;
        window.suppressed = 0;
    }

    if window.emitted < burst {
        window.emitted += 1;
        tracing::debug!("{}", message());
    } else {
        window.suppressed += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_then_suppresses() {
        for _ in 0..3 {
            rate_limited_warn("test-key", || "one".to_string());
        }
        rate_limited_warn("test-key", || "suppressed".to_string());
        let guard = LIMITER.lock().unwrap();
        let window = guard.windows.get("test-key").expect("window");
        assert_eq!(window.emitted, 3);
        assert_eq!(window.suppressed, 1);
    }
}
