//! Deprecated — use [`crate::blocking::BlockingMedousaClient`] instead.

#[cfg(feature = "blocking")]
#[allow(dead_code)]
pub type BlockingLocalModelsClient = crate::blocking::BlockingMedousaClient;
