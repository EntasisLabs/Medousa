use std::sync::Arc;

use anyhow::{Context, Result};
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::prelude::RuntimeBackend;
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::runtime::stasis_surreal_schema::ensure_stasis_runtime_schema;
use crate::runtime::vault_surreal_schema::ensure_vault_surreal_schema;
use crate::runtime::stasis_wire::{DaemonStasisWireConfig, build_daemon_stasis_composition};
use crate::artifact_store;
use crate::channel_session_store;
use crate::turn_continuation;
use crate::recurring_agent_turn;
use crate::openshell_sandbox_run;
use crate::recurring_delivery;
use crate::session_meta_store;
use crate::session_store;
use crate::verification_store;
use crate::tools::TuiRuntime;
use crate::tui::runtime_services::assemble_tui_runtime;

/// Single root handle for daemon and offline runtimes: Stasis composition + agent tooling.
pub struct MedousaPlatformRuntime {
    agent: Arc<TuiRuntime>,
}

impl MedousaPlatformRuntime {
    pub fn agent(&self) -> &TuiRuntime {
        &self.agent
    }

    pub fn agent_handle(&self) -> Arc<TuiRuntime> {
        self.agent.clone()
    }

    pub fn composition(&self) -> &stasis::prelude::RuntimeComposition {
        self.agent.runtime.as_ref()
    }

    pub fn identity_store(&self) -> Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore> {
        self.agent.identity_memory_store.clone()
    }

    pub fn identity_service(&self) -> Arc<IdentityMemoryService> {
        Arc::new(IdentityMemoryService::new(self.identity_store()))
    }
}

#[derive(Clone, Debug)]
pub struct PlatformBuildConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub deliver_webhook_url: String,
    pub allowed_grapheme_modules: Vec<String>,
    pub session_id: String,
    pub backend_label: String,
}

impl PlatformBuildConfig {
    pub fn daemon_defaults(deliver_webhook_url: impl Into<String>) -> Self {
        Self {
            provider: None,
            model: None,
            base_url: None,
            deliver_webhook_url: deliver_webhook_url.into(),
            allowed_grapheme_modules: Vec::new(),
            session_id: "daemon-agent-runtime".to_string(),
            backend_label: "in-memory".to_string(),
        }
    }
}

/// Build the full platform for offline/TUI-local use.
pub async fn build_medousa_platform(
    backend: RuntimeBackend,
    config: PlatformBuildConfig,
    event_tx: mpsc::Sender<TuiEvent>,
) -> Result<Arc<MedousaPlatformRuntime>> {
    build_platform_inner(backend, config, event_tx).await
}

/// Build the daemon platform: one DB connection, shared memory, agent tools registered once.
pub async fn build_daemon_platform(
    backend: RuntimeBackend,
    config: PlatformBuildConfig,
) -> Result<Arc<MedousaPlatformRuntime>> {
    let (event_tx, mut event_rx) = mpsc::channel(256);
    tokio::spawn(async move {
        while event_rx.recv().await.is_some() {}
    });
    build_platform_inner(backend, config, event_tx).await
}

async fn build_platform_inner(
    backend: RuntimeBackend,
    config: PlatformBuildConfig,
    event_tx: mpsc::Sender<TuiEvent>,
) -> Result<Arc<MedousaPlatformRuntime>> {
    crate::runtime::stasis_otel::prepare_stasis_otel_from_tui_defaults();
    crate::ensure_runtime_backend_prerequisites(&backend)?;

    let wire_config = DaemonStasisWireConfig {
        backend: backend.clone(),
        provider: config.provider.as_deref(),
        model: config.model.as_deref(),
        base_url: config.base_url.as_deref(),
        deliver_webhook_url: &config.deliver_webhook_url,
    };

    let (composition, memory) = build_daemon_stasis_composition(wire_config)
        .await
        .context("failed to build stasis daemon composition")?;

    eprintln!("medousa-daemon: ensuring Stasis runtime schema…");
    ensure_stasis_runtime_schema(&composition)
        .await
        .context("failed to ensure Stasis SurrealDB runtime tables")?;
    ensure_vault_surreal_schema(&composition)
        .await
        .context("failed to ensure vault SurrealDB tables")?;

    eprintln!("medousa-daemon: initializing session and delivery stores…");
    session_store::init_session_store_with_runtime(&composition).await;
    session_meta_store::init_session_meta_store_with_runtime(&composition).await;
    verification_store::init_verification_store_with_runtime(&composition).await;
    crate::session_catalog::init_session_catalog_with_runtime(&composition).await;
    channel_session_store::init_channel_session_store_with_runtime(&composition).await;
    artifact_store::init_artifact_store_with_runtime(&composition).await;
    turn_continuation::init_turn_continuation_store_with_runtime(&composition).await;
    recurring_delivery::init_recurring_delivery_store_with_runtime(&composition).await;

    eprintln!("medousa-daemon: assembling agent runtime…");
    let agent = assemble_tui_runtime(
        Arc::new(composition),
        memory.identity_store.clone(),
        memory.memory_reader.clone(),
        memory.memory_writer.clone(),
        memory.locus_store.clone(),
        config.provider.as_deref(),
        config.model.as_deref(),
        config.base_url.as_deref(),
        config.allowed_grapheme_modules,
        &config.session_id,
        true,
        event_tx,
    )
    .await
    .context("failed to assemble agent runtime on platform composition")?;

    sync_mcp_catalog(&agent).await;

    let agent = Arc::new(agent);
    agent
        .worker_scheduler
        .attach_runtime(agent.clone())
        .await;
    recurring_agent_turn::register_recurring_agent_turn_handler(
        agent.runtime.as_ref(),
        agent.clone(),
        config.backend_label.clone(),
    )
    .await
    .context("failed to register recurring agent-turn job handler")?;
    openshell_sandbox_run::register_openshell_sandbox_run_handler(agent.runtime.as_ref())
        .await
        .context("failed to register openshell sandbox run job handler")?;
    crate::agent_runtime::turn_worker_job::register_turn_worker_job_handler(
        agent.runtime.as_ref(),
        agent.clone(),
    )
    .await
    .context("failed to register durable turn worker job handler")?;
    crate::agent_runtime::turn_worker_job::reconcile_durable_turn_workers(
        agent.runtime.as_ref(),
        agent.clone(),
    )
    .await;

    Ok(Arc::new(MedousaPlatformRuntime { agent }))
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
