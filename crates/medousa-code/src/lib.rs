//! Medousa LSP Interoperability Orchestrator.
//!
//! Speaks LSP to many language servers and exposes one client surface so Home,
//! daemon agents, and later Detamu share language intelligence on the workshop
//! disk — without Tauri or `medousa_daemon` owning rust-analyzer.

pub mod backend;
pub mod detamu;
pub mod document;
pub mod registry;
pub mod server;
pub mod session;

pub use registry::{LanguageId, ServerLaunchSpec, ServerRegistry};
pub use server::{OrchestratorConfig, OrchestratorState, serve};
pub use session::SessionPool;

pub const ENGINE_NAME: &str = "medousa-code";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
