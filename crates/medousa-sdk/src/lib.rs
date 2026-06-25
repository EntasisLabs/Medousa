//! Medousa daemon HTTP client SDK.

mod client;
mod error;
mod local;
pub mod transport;

#[cfg(feature = "async")]
mod budget;
#[cfg(feature = "async")]
mod health;
#[cfg(feature = "async")]
mod interactive;
#[cfg(feature = "async")]
mod jobs;
#[cfg(feature = "async")]
mod runtime;
#[cfg(feature = "async")]
mod sessions;

pub use client::MedousaClient;
pub use error::SdkError;
pub use transport::{HttpTransport, Transport};

#[cfg(feature = "blocking")]
pub use local::blocking::BlockingLocalModelsClient;
