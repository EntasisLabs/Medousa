//! Medousa daemon HTTP client SDK.

mod client;
mod error;
mod local;
pub mod transport;

#[cfg(feature = "async")]
mod health;

pub use client::MedousaClient;
pub use error::SdkError;
pub use transport::{HttpTransport, Transport};

#[cfg(feature = "blocking")]
pub use local::blocking::BlockingLocalModelsClient;
