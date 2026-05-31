//! Runtime assembly for the shared agent turn engine.
//!
//! Today this wraps `TuiRuntime` while orchestration is extracted from the TUI bin.
//! Long-term: channel-agnostic construction without `TuiEvent` coupling.

use anyhow::Result;
use stasis::prelude::RuntimeBackend;
use tokio::sync::mpsc;

use crate::tools::{TuiRuntime, build_tui_runtime};

pub type MedousaAgentRuntime = TuiRuntime;

/// Build the shared agent runtime (TUI and offline fallback).
pub use crate::tools::build_tui_runtime as build_agent_runtime;

/// Build the shared agent runtime for daemon-hosted turns.
///
/// Tool progress events are drained; stream output goes through [`AgentStreamSink`](super::stream_sink::AgentStreamSink).
pub async fn build_daemon_agent_runtime(
    backend: RuntimeBackend,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
) -> Result<MedousaAgentRuntime> {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    tokio::spawn(async move {
        while event_rx.recv().await.is_some() {}
    });

    build_tui_runtime(
        backend,
        provider,
        model,
        base_url,
        allowed_grapheme_modules,
        "daemon-agent-runtime",
        event_tx,
    )
    .await
}
