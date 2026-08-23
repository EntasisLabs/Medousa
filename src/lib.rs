//! Medousa daemon deployment profiles.
//!
//! Full, embedded, server, and headless builds are deployment compositions of
//! the same daemon product. The embedded profile exposes the existing portable
//! authority, session, turn, and runtime modules without server/desktop hosts.

#[cfg(feature = "full-daemon")]
include!("full_daemon.rs");

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod embedded_daemon;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "agent_runtime/execution_context.rs"]
pub mod execution_context;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "runtime/locus_semantic_index_store.rs"]
pub mod locus_semantic_index_store;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "runtime/locus_surreal_client.rs"]
pub mod locus_surreal_client;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "runtime/persistent_locus.rs"]
pub mod persistent_locus;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod request_principal;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[allow(dead_code)]
pub mod session_storage;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod session_store;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod sse_turn_projection;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod store_root;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "runtime/surreal_startup.rs"]
pub mod surreal_startup;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_event_channel;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_pipeline_output;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_scope;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_stream_registry;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_ticket;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod workshop_authority;

#[cfg(not(any(feature = "full-daemon", feature = "embedded-daemon")))]
compile_error!("enable either the `full-daemon` or `embedded-daemon` feature");
