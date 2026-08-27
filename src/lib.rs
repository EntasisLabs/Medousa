//! Medousa daemon deployment profiles.
//!
//! Full, embedded, server, and headless builds are deployment compositions of
//! the same daemon product. The embedded profile exposes the existing portable
//! authority, session, turn, and runtime modules without server/desktop hosts.

#[cfg(feature = "full-daemon")]
include!("full_daemon.rs");

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "embedded_agent_runtime.rs"]
pub mod agent_runtime;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub use agent_runtime::execution_context;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub(crate) use agent_runtime::prompt_policy;

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod capability_catalog;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod daemon_api;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod daemon_runtime;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod daemon_runtime_handlers;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod delegated_task;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod delegation;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod delegation_tools;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod embedded_daemon;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod environment_store;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod grapheme_api;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod grapheme_grants;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod grapheme_runtime;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod grapheme_script;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod grapheme_source;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod grapheme_workshop;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod locus_memory;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "runtime/locus_semantic_index_store.rs"]
pub mod locus_semantic_index_store;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod locus_semantic_tags;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod locus_service;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "runtime/locus_surreal_client.rs"]
pub mod locus_surreal_client;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod mcp_gateway_api;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod mcp_gateway_client;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod mcp_policy;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod mobile_tool_registry;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "runtime/persistent_locus.rs"]
pub mod persistent_locus;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod reasoning_effort;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod recurring_schedule;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod request_principal;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod runtime_composition_ext;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod runtime_config_command_runtime;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod runtime_job_spec;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[allow(dead_code)]
pub mod session_storage;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod session_store;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod sse_turn_projection;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "runtime/stasis_surreal_schema.rs"]
pub mod stasis_surreal_schema;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod store_root;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
#[path = "runtime/surreal_startup.rs"]
pub mod surreal_startup;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod tool_error;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_event_channel;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_parts;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_pipeline_output;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_recovery;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_scope;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_stream_registry;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod turn_ticket;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod typed_tools;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod ui_tool_output;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod user_profiles;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod vault;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod web_search_tool;
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub mod workshop_authority;

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
include!("embedded_tool_modules.rs");

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub use product_config::load_product_config;

#[cfg(not(any(feature = "full-daemon", feature = "embedded-daemon")))]
compile_error!("enable either the `full-daemon` or `embedded-daemon` feature");
