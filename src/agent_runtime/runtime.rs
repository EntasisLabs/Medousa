//! Runtime assembly for the shared agent turn engine.
//!
//! Owns the concrete runtime; TUI callers use compatibility aliases, not a second owner.

use std::sync::Arc;

use crate::runtime::event_delivery::HeadlessEventDrain;
use crate::runtime_session::runtime_bootstrap_session_id;
use anyhow::Result;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::prelude::RuntimeBackend;
use stasis::prelude::RuntimeComposition;
use tokio::sync::mpsc;

use crate::capability_catalog::CapabilityRegistry;
use crate::client_tools::ClientRegistry;
use crate::events::RuntimeEvent;
use crate::mcp_gateway_client::McpGatewayClient;
use crate::runtime::agent_services::{assemble_agent_runtime, build_tool_loop_pipeline_for_target};
use crate::runtime::memory_bundle::MemoryAdapterBundle;
use crate::tools::WorkerCodeProjectSetupService;
use locus_core_rs::NodeStore;
use medousa_runtime::MedousaToolLoopPipeline;
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::prelude_ext::{MemoryContextReader, MemoryContextWriter};
use tokio::sync::RwLock;

#[cfg(feature = "full-daemon")]
pub struct MedousaAgentRuntime {
    pub(crate) _event_drain: Option<HeadlessEventDrain>,
    pub runtime: Arc<RuntimeComposition>,
    pub tool_loop_pipeline: MedousaToolLoopPipeline,
    pub tool_registry: Arc<dyn ToolRegistry>,
    pub tool_catalog: Arc<crate::typed_tools::ToolCatalog>,
    pub capability_registry: Arc<RwLock<CapabilityRegistry>>,
    pub mcp_gateway_client: Arc<McpGatewayClient>,
    pub workflow_registry: Arc<crate::workflow::WorkflowRegistry>,
    pub locus_store: Arc<dyn NodeStore>,
    pub semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    pub medousa_identity_store: Arc<crate::identity_store_ext::MedousaIdentityMemoryStore>,
    pub identity_memory_store: Arc<dyn IdentityMemoryStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
    pub memory_writer: Arc<dyn MemoryContextWriter>,
    pub memory_operations:
        Arc<dyn stasis::ports::outbound::memory::memory_operations::MemoryOperations>,
    pub client_registry: crate::client_tools::ClientRegistry,
    pub execution_registry: crate::agent_runtime::execution_context::TurnExecutionRegistry,
    pub worker_scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
    /// Daemon-owned Forge authority is attached after the platform runtime is
    /// assembled. Background workers read this slot at execution time so a
    /// remotely admitted Coder job cannot accidentally use client-local state.
    pub forge_authority: Arc<std::sync::RwLock<Option<Arc<medousa_forge::forge::Forge>>>>,
    /// Destination-owned setup service for projectless remote Coder jobs.
    pub(crate) worker_code_project_setup:
        Arc<std::sync::RwLock<Option<Arc<WorkerCodeProjectSetupService>>>>,
}

#[cfg(feature = "full-daemon")]
impl MedousaAgentRuntime {
    pub fn attach_worker_code_project_setup(&self, service: Arc<WorkerCodeProjectSetupService>) {
        *self
            .worker_code_project_setup
            .write()
            .expect("worker project setup lock poisoned") = Some(service);
    }

    pub async fn ensure_worker_code_project(
        &self,
        session_id: String,
        setup: crate::workshop_contract::WorkerCodeProjectSetup,
    ) -> Result<crate::delegated_task::WorkerCodeProjectRef, String> {
        let service = self
            .worker_code_project_setup
            .read()
            .expect("worker project setup lock poisoned")
            .clone()
            .ok_or_else(|| "destination Coder project setup is unavailable".to_string())?;
        service(session_id, setup).await
    }

    pub fn attach_forge_authority(&self, forge: Arc<medousa_forge::forge::Forge>) {
        *self
            .forge_authority
            .write()
            .expect("Forge authority lock poisoned") = Some(forge);
    }

    pub fn forge_authority(&self) -> Option<Arc<medousa_forge::forge::Forge>> {
        self.forge_authority
            .read()
            .expect("Forge authority lock poisoned")
            .clone()
    }

    pub fn tool_loop_pipeline_for_target(
        &self,
        provider: &str,
        model: &str,
        base_url: Option<&str>,
    ) -> MedousaToolLoopPipeline {
        build_tool_loop_pipeline_for_target(provider, model, base_url, self.tool_registry.clone())
    }
}

/// Build the shared agent runtime (TUI and offline fallback).
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "full-daemon")]
pub async fn build_agent_runtime(
    backend: RuntimeBackend,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
    session_id: &str,
    workshop_operator_identity: bool,
    event_tx: mpsc::Sender<RuntimeEvent>,
) -> anyhow::Result<MedousaAgentRuntime> {
    crate::runtime::agent_services::build_agent_runtime_services(
        backend,
        provider,
        model,
        base_url,
        allowed_grapheme_modules,
        session_id,
        workshop_operator_identity,
        event_tx,
    )
    .await
}

/// Build daemon agent tooling on an existing runtime composition (single SurrealKV connection).
pub async fn build_daemon_agent_runtime_from_composition(
    runtime: Arc<RuntimeComposition>,
    _identity_memory_store: Arc<dyn IdentityMemoryStore>,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
) -> Result<MedousaAgentRuntime> {
    let (event_tx, event_drain) = HeadlessEventDrain::start();

    let memory = MemoryAdapterBundle::from_runtime_shell(runtime.as_ref()).await?;

    let mut agent_runtime = assemble_agent_runtime(
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
        ClientRegistry::new(),
        event_tx,
    )
    .await?;

    agent_runtime._event_drain = Some(event_drain);

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
    let (event_tx, event_drain) = HeadlessEventDrain::start();

    let mut runtime = build_agent_runtime(
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

    runtime._event_drain = Some(event_drain);

    if let Ok(catalog) = runtime.mcp_gateway_client.fetch_catalog().await {
        runtime
            .capability_registry
            .write()
            .await
            .apply_mcp_catalog_sync(&catalog);
    }

    Ok(runtime)
}

#[cfg(test)]
mod ownership_tests {
    use super::MedousaAgentRuntime;
    use std::any::TypeId;
    use std::sync::Arc;

    #[test]
    fn compatibility_handle_is_the_same_runtime_not_another_authority() {
        assert_eq!(
            TypeId::of::<Arc<crate::tools::TuiRuntime>>(),
            TypeId::of::<Arc<MedousaAgentRuntime>>()
        );
    }

    #[test]
    fn terminal_event_compatibility_name_is_the_shared_contract() {
        assert_eq!(
            TypeId::of::<crate::events::TuiEvent>(),
            TypeId::of::<crate::events::RuntimeEvent>()
        );
    }
}
