//! Shared ingest stream consumption and channel formatting helpers for thin adapters.

pub mod adapter_ingest;
pub mod channel_format;
pub mod ingest_stream;

pub use adapter_ingest::*;
pub use channel_format::*;
pub use ingest_stream::*;
