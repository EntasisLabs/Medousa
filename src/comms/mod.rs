//! Comms / Transport layer (Phase 2).
//!
//! One service ([`service::CommsService`]) owns the connection pool, the LAN /
//! Iroh / HTTP-fallback adapters, route selection, and per-route circuit
//! breakers. The engine reaches it only through [`service::CommsHandle`] over a
//! bounded channel — it never imports a transport crate or branches on route at
//! a call site.
//!
//! Building blocks:
//! - [`transport`] — the common [`transport::Transport`] trait + request/response types.
//! - [`pool`] — the centralized pooled reqwest clients.
//! - [`adapters`] — LAN / HTTP-fallback (reqwest) and feature-gated Iroh.
//! - [`route`] — ordered route selection with a cached, TTL'd decision.
//! - [`backoff`] — bounded exponential backoff, circuit breaker, overlap guard.
//! - [`reconnect`] — a driver combining backoff + breaker + overlap guard.
//! - [`service`] — the channel-driven actor tying it all together.
//! - [`fd_limits`] — `RLIMIT_NOFILE` hardening for the daemon.
//! - [`ports`] — re-exported from [`medousa_engine`] (engine outbound ports).

pub mod adapters;
pub mod backoff;
pub mod fd_limits;
pub mod pool;
pub mod reconnect;
pub mod route;
pub mod service;
pub mod transport;

pub use medousa_engine::ports::{
    ChannelToolSink, StoreError, ToolSinkEvent, ToolSinkPort, TurnStorePort,
    TurnStreamRegistryPort, TurnTicketPort, UpsertOutcome,
};

pub use backoff::{
    BackoffPolicy, CircuitBreaker, CircuitBreakerConfig, CircuitState, OverlapGuard, OverlapPermit,
};
pub use fd_limits::{raise_nofile_limit, NofileLimits, DEFAULT_TARGET_NOFILE};
pub use pool::{ConnectionPool, PoolConfig};
pub use reconnect::{Reconnector, ReconnectError};
pub use route::{first_available, RoutePolicy, RouteSelector};
pub use service::{CommsCommand, CommsConfig, CommsHandle, CommsService};
pub use transport::{
    HttpMethod, ResponseBody, Transport, TransportError, TransportErrorKind, TransportKind,
    TransportRequest, TransportResponse,
};

pub use adapters::HttpAdapter;
#[cfg(feature = "iroh-transport")]
pub use adapters::IrohAdapter;

