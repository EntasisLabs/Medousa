use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, Result};
use stasis::prelude::RuntimeBackend;
use tokio::sync::mpsc;

use crate::artifact_store;
use crate::events::TuiEvent;
use crate::runtime::stasis_surreal_schema::ensure_stasis_runtime_schema;
use crate::runtime::vault_surreal_schema::ensure_vault_surreal_schema;
use crate::runtime::stasis_wire::LocalStasisWireConfig;
use crate::runtime::stasis_wire::build_local_stasis_composition;
use crate::session_meta_store;
use crate::session_store;
use crate::verification_store;
use crate::tools::TuiRuntime;
use crate::tui::runtime_services::assemble_tui_runtime;

/// How the TUI connects to runtime services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiPlatformMode {
    /// Daemon owns persistence; TUI uses an in-memory stub for local tools only.
    ClientStub,
    /// Full local platform (offline or `--local-runtime-only`).
    LocalFull,
}

#[derive(Clone, Debug)]
pub struct TuiPlatformBuildConfig {
    pub backend: RuntimeBackend,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub allowed_grapheme_modules: Vec<String>,
    pub session_id: String,
    pub daemon_url: String,
    pub local_runtime_only: bool,
}

impl TuiPlatformBuildConfig {
    pub fn from_names(
        backend_name: &str,
        provider: Option<&str>,
        model: Option<&str>,
        base_url: Option<&str>,
        allowed_grapheme_modules: Vec<String>,
        session_id: &str,
        daemon_url: &str,
        local_runtime_only: bool,
    ) -> Self {
        Self {
            backend: crate::parse_backend(Some(backend_name)),
            provider: provider.map(str::to_string),
            model: model.map(str::to_string),
            base_url: base_url.map(str::to_string),
            allowed_grapheme_modules,
            session_id: session_id.to_string(),
            daemon_url: daemon_url.to_string(),
            local_runtime_only,
        }
    }
}

/// Resolve backend + mode for TUI startup or settings rebind.
pub fn resolve_tui_platform_mode(config: &TuiPlatformBuildConfig) -> (RuntimeBackend, TuiPlatformMode) {
    if config.local_runtime_only {
        return (config.backend.clone(), TuiPlatformMode::LocalFull);
    }

    if is_daemon_bind_reachable(&config.daemon_url) {
        return (RuntimeBackend::InMemory, TuiPlatformMode::ClientStub);
    }

    (config.backend.clone(), TuiPlatformMode::LocalFull)
}

/// Build the TUI runtime shell: client stub when daemon is up, full local platform otherwise.
pub async fn build_tui_platform(
    config: TuiPlatformBuildConfig,
    event_tx: mpsc::Sender<TuiEvent>,
) -> Result<TuiRuntime> {
    crate::runtime::stasis_otel::prepare_stasis_otel_from_tui_defaults();
    let (backend, mode) = resolve_tui_platform_mode(&config);

    if mode == TuiPlatformMode::ClientStub
        && !matches!(config.backend, RuntimeBackend::InMemory)
    {
        eprintln!(
            "daemon reachable at {} — TUI using in-memory client stub (persistence via daemon API)",
            config.daemon_url
        );
    }

    match mode {
        TuiPlatformMode::ClientStub => {
            build_tui_client_stub(&config, event_tx)
                .await
                .context("failed to build TUI client-stub runtime")
        }
        TuiPlatformMode::LocalFull => {
            build_tui_local_platform(backend, &config, event_tx)
                .await
                .context("failed to build TUI local platform runtime")
        }
    }
}

async fn build_tui_client_stub(
    config: &TuiPlatformBuildConfig,
    event_tx: mpsc::Sender<TuiEvent>,
) -> Result<TuiRuntime> {
    let wire_config = LocalStasisWireConfig {
        backend: RuntimeBackend::InMemory,
        provider: config.provider.as_deref(),
        model: config.model.as_deref(),
        base_url: config.base_url.as_deref(),
    };

    let (composition, memory) = build_local_stasis_composition(wire_config).await?;
    assemble_tui_agent(composition, memory, config, event_tx).await
}

async fn build_tui_local_platform(
    backend: RuntimeBackend,
    config: &TuiPlatformBuildConfig,
    event_tx: mpsc::Sender<TuiEvent>,
) -> Result<TuiRuntime> {
    crate::ensure_runtime_backend_prerequisites(&backend)?;

    let wire_config = LocalStasisWireConfig {
        backend,
        provider: config.provider.as_deref(),
        model: config.model.as_deref(),
        base_url: config.base_url.as_deref(),
    };

    let (composition, memory) = build_local_stasis_composition(wire_config).await?;
    ensure_stasis_runtime_schema(&composition)
        .await
        .context("failed to ensure Stasis SurrealDB runtime tables")?;
    ensure_vault_surreal_schema(&composition)
        .await
        .context("failed to ensure vault SurrealDB tables")?;
    init_local_platform_stores(&composition).await;
    assemble_tui_agent(composition, memory, config, event_tx).await
}

async fn init_local_platform_stores(composition: &stasis::prelude::RuntimeComposition) {
    session_store::init_session_store_with_runtime(composition).await;
    session_meta_store::init_session_meta_store_with_runtime(composition).await;
    artifact_store::init_artifact_store_with_runtime(composition).await;
    crate::component_store::init_component_store_with_runtime(composition).await;
    crate::component_runtime_store::init_component_runtime_with_runtime(composition).await;
    verification_store::init_verification_store_with_runtime(composition).await;
    crate::session_catalog::init_session_catalog_with_runtime(composition).await;
    crate::turn_continuation::init_turn_continuation_store_with_runtime(composition).await;
}

async fn assemble_tui_agent(
    composition: stasis::prelude::RuntimeComposition,
    memory: crate::runtime::memory_bundle::MemoryAdapterBundle,
    config: &TuiPlatformBuildConfig,
    event_tx: mpsc::Sender<TuiEvent>,
) -> Result<TuiRuntime> {
    let agent = assemble_tui_runtime(
        std::sync::Arc::new(composition),
        memory.identity_store.clone(),
        memory.memory_reader.clone(),
        memory.memory_writer.clone(),
        memory.locus_memory.node_store.clone(),
        memory.locus_memory.semantic_index.clone(),
        memory.memory_operations.clone(),
        config.provider.as_deref(),
        config.model.as_deref(),
        config.base_url.as_deref(),
        config.allowed_grapheme_modules.clone(),
        &config.session_id,
        false,
        event_tx,
    )
    .await?;

    sync_mcp_catalog(&agent).await;
    Ok(agent)
}

async fn sync_mcp_catalog(agent: &TuiRuntime) {
    if let Ok(catalog) = agent.mcp_gateway_client.fetch_catalog().await {
        agent
            .capability_registry
            .write()
            .await
            .apply_mcp_catalog_sync(&catalog);
    }
}

/// TCP probe for daemon bind (used before opening a local surreal-kv database).
pub fn is_daemon_bind_reachable(daemon_url: &str) -> bool {
    let host_port = daemon_url
        .strip_prefix("http://")
        .or_else(|| daemon_url.strip_prefix("https://"))
        .unwrap_or(daemon_url)
        .split('/')
        .next()
        .unwrap_or(daemon_url);
    if host_port.is_empty() || !host_port.contains(':') {
        return false;
    }

    let addr = match host_port.to_socket_addrs() {
        Ok(mut addrs) => addrs.next(),
        Err(_) => None,
    };

    match addr {
        Some(addr) => TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok(),
        None => false,
    }
}
