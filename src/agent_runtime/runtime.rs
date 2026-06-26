//! Runtime assembly for the shared agent turn engine.
//!
//! Today this wraps `TuiRuntime` while orchestration is extracted from the TUI bin.
//! Long-term: channel-agnostic construction without `TuiEvent` coupling.

use std::sync::Arc;

use anyhow::Result;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::prelude::RuntimeComposition;
use crate::runtime_session::runtime_bootstrap_session_id;
use tokio::sync::mpsc;

use crate::runtime::memory_bundle::MemoryAdapterBundle;
use crate::tools::TuiRuntime;
use crate::tui::runtime_services::assemble_tui_runtime;

pub type MedousaAgentRuntime = TuiRuntime;

/// Build the shared agent runtime (TUI and offline fallback).
pub use crate::tools::build_tui_runtime as build_agent_runtime;

/// Build daemon agent tooling on an existing runtime composition (single SurrealKV connection).
pub async fn build_daemon_agent_runtime_from_composition(
    runtime: Arc<RuntimeComposition>,
    _identity_memory_store: Arc<dyn IdentityMemoryStore>,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
) -> Result<MedousaAgentRuntime> {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    tokio::spawn(async move {
        while event_rx.recv().await.is_some() {}
    });

    let memory = MemoryAdapterBundle::from_runtime_shell(runtime.as_ref()).await?;

    let agent_runtime = assemble_tui_runtime(
        runtime,
        memory.identity_store.clone(),
        memory.memory_reader.clone(),
        memory.memory_writer.clone(),
        memory.locus_memory.node_store.clone(),
        memory.locus_memory.semantic_index.clone(),
        memory.memory_operations.clone(),
        provider,
        model,
        base_url,
        allowed_grapheme_modules,
        runtime_bootstrap_session_id(),
        true,
        event_tx,
    )
    .await?;

    if let Ok(catalog) = agent_runtime.mcp_gateway_client.fetch_catalog().await {
        agent_runtime
            .capability_registry
            .write()
            .await
            .apply_mcp_catalog_sync(&catalog);
    }

    Ok(agent_runtime)
}

/// Build the shared agent runtime for daemon-hosted turns (standalone / tests only).
///
/// Prefer [`build_daemon_agent_runtime_from_composition`] when the daemon already owns
/// a [`RuntimeComposition`] so SurrealKV is not opened twice.
pub async fn build_daemon_agent_runtime(
    backend: stasis::prelude::RuntimeBackend,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
) -> Result<MedousaAgentRuntime> {
    use crate::tools::build_tui_runtime;

    let (event_tx, mut event_rx) = mpsc::channel(256);
    tokio::spawn(async move {
        while event_rx.recv().await.is_some() {}
    });

    let runtime = build_tui_runtime(
        backend,
        provider,
        model,
        base_url,
        allowed_grapheme_modules,
        runtime_bootstrap_session_id(),
        true,
        event_tx,
    )
    .await?;

    if let Ok(catalog) = runtime.mcp_gateway_client.fetch_catalog().await {
        runtime
            .capability_registry
            .write()
            .await
            .apply_mcp_catalog_sync(&catalog);
    }

    Ok(runtime)
}
