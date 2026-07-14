//! Medousa daemon HTTP client SDK.

mod client;
mod error;
mod local;
pub mod transport;

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
#[cfg(feature = "async")]
mod health;
#[cfg(feature = "async")]
mod http;
#[cfg(feature = "async")]
mod interactive;
#[cfg(feature = "async")]
pub mod reconnect;
#[cfg(all(feature = "async", feature = "sse"))]
pub mod reconnecting_stream;
#[cfg(feature = "async")]
mod jobs;
#[cfg(feature = "async")]
mod mcp_gateway;
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
pub use transport::{HttpTransport, Transport, path_with_query};

#[cfg(feature = "async")]
pub use reconnect::{
    BackoffPolicy, CircuitBreaker, CircuitBreakerConfig, CircuitState, OverlapGuard,
    OverlapPermit, ReconnectPolicy, stream_path_with_since,
};
#[cfg(all(feature = "async", feature = "sse"))]
pub use reconnecting_stream::{apply_stream_seq, ReconnectingInteractiveStream};

#[cfg(feature = "blocking")]
pub use blocking::BlockingMedousaClient;
#[cfg(feature = "blocking")]
pub use blocking::BlockingLocalModelsClient;
