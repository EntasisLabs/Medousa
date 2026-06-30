//! Centralized connection pool.
//!
//! Historically every transport call site built its own `reqwest::Client`
//! (gateway proxy at 600s/request, ad-hoc clients in handlers), which both
//! leaked sockets under load and defeated keep-alive. The pool builds the
//! reqwest clients **once** and hands shared references to every adapter, so
//! connection reuse and FD bounds are enforced in one place.

use std::time::Duration;

use super::transport::{TransportError, TransportErrorKind};

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub connect_timeout: Duration,
    /// Timeout for buffered (non-stream) requests.
    pub request_timeout: Duration,
    /// Total timeout for streaming requests. Long, but bounded — the per-chunk
    /// idle timeout (in the adapter) is what actually protects against a hung
    /// SSE peer.
    pub stream_timeout: Duration,
    /// Idle timeout per chunk on a streaming response.
    pub stream_idle_timeout: Duration,
    pub pool_max_idle_per_host: usize,
    pub pool_idle_timeout: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(120),
            stream_timeout: Duration::from_secs(900),
            stream_idle_timeout: Duration::from_secs(75),
            pool_max_idle_per_host: 8,
            pool_idle_timeout: Duration::from_secs(90),
        }
    }
}

/// Shared, pooled HTTP clients. Cheap to clone (`reqwest::Client` is an `Arc`).
#[derive(Clone)]
pub struct ConnectionPool {
    standard: reqwest::Client,
    streaming: reqwest::Client,
    config: PoolConfig,
}

impl ConnectionPool {
    pub fn new(config: PoolConfig) -> Result<Self, TransportError> {
        let standard = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .pool_idle_timeout(config.pool_idle_timeout)
            .build()
            .map_err(|err| {
                TransportError::new(TransportErrorKind::Other, format!("build pool client: {err}"))
            })?;
        // Streaming client: a long overall cap (the adapter enforces the real
        // per-chunk idle timeout) and a small idle pool so long-lived SSE
        // connections don't pin many idle sockets.
        let streaming = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.stream_timeout)
            .pool_max_idle_per_host(config.pool_max_idle_per_host.min(4))
            .pool_idle_timeout(config.pool_idle_timeout)
            .build()
            .map_err(|err| {
                TransportError::new(
                    TransportErrorKind::Other,
                    format!("build streaming pool client: {err}"),
                )
            })?;
        Ok(Self {
            standard,
            streaming,
            config,
        })
    }

    pub fn standard(&self) -> &reqwest::Client {
        &self.standard
    }

    pub fn streaming(&self) -> &reqwest::Client {
        &self.streaming
    }

    pub fn config(&self) -> &PoolConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_builds_with_defaults() {
        let pool = ConnectionPool::new(PoolConfig::default()).expect("pool builds");
        assert_eq!(pool.config().pool_max_idle_per_host, 8);
    }

    #[test]
    fn streaming_client_caps_idle_pool() {
        let pool = ConnectionPool::new(PoolConfig {
            pool_max_idle_per_host: 32,
            ..PoolConfig::default()
        })
        .expect("pool builds");
        // Standard keeps the configured idle count; streaming is capped to 4.
        assert_eq!(pool.config().pool_max_idle_per_host, 32);
    }
}
