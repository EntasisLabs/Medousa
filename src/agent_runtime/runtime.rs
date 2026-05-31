//! Runtime assembly for the shared agent turn engine.
//!
//! Today this wraps `TuiRuntime` while orchestration is extracted from the TUI bin.
//! Long-term: channel-agnostic construction without `TuiEvent` coupling.

pub use crate::tools::{TuiRuntime as MedousaAgentRuntime, build_tui_runtime as build_agent_runtime};
