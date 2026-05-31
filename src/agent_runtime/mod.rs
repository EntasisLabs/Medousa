//! Shared agent turn orchestration extracted from the TUI local fallback.
//!
//! Phase 1: turn services + runtime type scaffold.
//! Phase 2+: daemon-hosted turn loop and channel-agnostic streaming.

pub mod runtime;
pub mod turn_services;
pub mod types;

pub use runtime::{MedousaAgentRuntime, build_agent_runtime};
pub use types::{AgentStreamEvent, AgentTurnRequest};
