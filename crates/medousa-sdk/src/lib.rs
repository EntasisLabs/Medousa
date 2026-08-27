//! Medousa daemon HTTP client SDK.

mod client;
mod error;
pub mod generated;
mod local;
mod op;
pub mod transport;

#[cfg(feature = "async")]
mod agents;
#[cfg(feature = "async")]
mod budget;
#[cfg(feature = "async")]
mod calendar;
#[cfg(feature = "async")]
mod capabilities;
#[cfg(feature = "async")]
mod components;
#[cfg(feature = "async")]
mod environment;
#[cfg(feature = "async")]
mod feeds;
mod health;
#[cfg(feature = "async")]
mod http;
#[cfg(feature = "async")]
mod interactive;
#[cfg(feature = "async")]
mod jobs;
#[cfg(feature = "async")]
mod manuscripts;
#[cfg(feature = "async")]
mod mcp_gateway;
#[cfg(feature = "async")]
mod prompt_stashes;
#[cfg(feature = "async")]
pub mod reconnect;
#[cfg(all(feature = "async", feature = "sse"))]
pub mod reconnecting_stream;
#[cfg(feature = "async")]
mod recurring;
#[cfg(feature = "async")]
mod runtime;
#[cfg(feature = "async")]
mod sessions;
#[cfg(feature = "async")]
mod vault;
#[cfg(feature = "async")]
mod workspace;

#[cfg(feature = "sse")]
pub mod streaming;

#[cfg(feature = "blocking")]
pub mod blocking;

pub use client::MedousaClient;
pub use error::SdkError;
pub use generated::ops as operations;
pub use medousa_types::DAEMON_API_CONTRACT_REVISION;
pub use transport::{HttpTransport, Transport, path_with_query};

#[cfg(feature = "async")]
pub use reconnect::{
    BackoffPolicy, CircuitBreaker, CircuitBreakerConfig, CircuitState, OverlapGuard, OverlapPermit,
    ReconnectPolicy, stream_path_with_since,
};
#[cfg(all(feature = "async", feature = "sse"))]
pub use reconnecting_stream::{
    ReconnectingInteractiveStream, ReconnectingInteractiveStreamV2,
    ReconnectingInteractiveStreamV3, ReconnectingTurnStream, apply_stream_seq, apply_stream_seq_v2,
    apply_stream_seq_v3,
};

#[cfg(feature = "blocking")]
pub use blocking::BlockingLocalModelsClient;
#[cfg(feature = "blocking")]
pub use blocking::BlockingMedousaClient;
