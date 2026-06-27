#[cfg(feature = "blocking")]
mod client;

#[cfg(feature = "blocking")]
pub use client::BlockingMedousaClient;

/// Deprecated alias for [`BlockingMedousaClient`].
#[cfg(feature = "blocking")]
pub type BlockingLocalModelsClient = BlockingMedousaClient;
