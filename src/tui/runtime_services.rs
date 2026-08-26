use std::sync::Arc;

use crate::identity_store_ext::MedousaIdentityMemoryStore;
use locus_core_rs::NodeStore;
use medousa_runtime::MedousaToolLoopPipeline;
use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::ports::outbound::ai_chat_client::AiChatClient;
use stasis::prelude::RuntimeBackend;
use stasis::prelude_ext::{MemoryContextReader, MemoryContextWriter};
use tokio::sync::mpsc;

use crate::capability_catalog::CapabilityRegistry;
use crate::client_tools::{ClientRegistry, ClientToolRegistry};
use crate::engine_context::EngineExecutionLane;
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::GraphemeCompactionModelTarget;
use crate::mcp_gateway_client::McpGatewayClient;
use crate::runtime::stasis_wire::{LocalStasisWireConfig, build_local_stasis_composition};
use crate::tools::{PolicyAwareToolRegistry, TuiRuntime};
use crate::typed_tools::{ToolCatalogHandle, ToolRegistrar};
use crate::workflow;
use tokio::sync::RwLock;

pub(crate) fn build_tool_loop_pipeline_for_target(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
    tool_registry: Arc<dyn ToolRegistry>,
) -> MedousaToolLoopPipeline {
    let resolved_provider = crate::resolve_llm_provider(Some(provider));
    let resolved_model = crate::resolve_llm_model(Some(model));
    let resolved_base_url = crate::resolve_llm_base_url(Some(&resolved_provider), base_url);
    let chat_client: Arc<dyn AiChatClient> = Arc::new(crate::build_genai_chat_client(
        &resolved_provider,
        &resolved_model,
        resolved_base_url.as_deref(),
    ));
    let prompt_pipeline = PromptExecutionPipeline::new(chat_client);
    MedousaToolLoopPipeline::new(prompt_pipeline, tool_registry)
        .with_parallel_execution_settings_provider(Arc::new(
            crate::execution_policy::load_parallel_execution_settings,
        ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn build_tui_runtime_services(
    backend: RuntimeBackend,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
    session_id: &str,
    workshop_operator_identity: bool,
    event_tx: mpsc::Sender<TuiEvent>,
) -> anyhow::Result<TuiRuntime> {
    let wire_config = LocalStasisWireConfig {
        backend,
        provider,
        model,
        base_url,
    };
    let (composition, memory) = build_local_stasis_composition(wire_config).await?;
    crate::session_store::init_session_store_with_runtime(&composition).await?;
    crate::artifact_store::init_artifact_store_with_runtime(&composition).await;
    crate::component_store::init_component_store_with_runtime(&composition).await;
    crate::integration_connection::init_integration_connection_from_runtime(&composition).await;
    crate::component_runtime_store::init_component_runtime_with_runtime(&composition).await;
    crate::verification_store::init_verification_store_with_runtime(&composition).await;
    crate::turn_continuation::init_turn_continuation_store_with_runtime(&composition).await;

    assemble_tui_runtime(
        Arc::new(composition),
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
        session_id,
        workshop_operator_identity,
        ClientRegistry::new(),
        event_tx,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
/// Assemble agent/TUI tooling on top of an existing runtime composition (no new DB connection).
pub(crate) async fn assemble_tui_runtime(
    runtime: Arc<stasis::prelude::RuntimeComposition>,
    identity_memory_store: Arc<MedousaIdentityMemoryStore>,
    memory_reader: Arc<dyn MemoryContextReader>,
    memory_writer: Arc<dyn MemoryContextWriter>,
    locus_store: Arc<dyn NodeStore>,
    semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    memory_operations: Arc<
        dyn stasis::ports::outbound::memory::memory_operations::MemoryOperations,
    >,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
    session_id: &str,
    workshop_operator_identity: bool,
    client_registry: ClientRegistry,
    event_tx: mpsc::Sender<TuiEvent>,
) -> anyhow::Result<TuiRuntime> {
    let resolved_provider = crate::resolve_llm_provider(provider);
    let resolved_model = crate::resolve_llm_model(model);
    let resolved_base_url = crate::resolve_llm_base_url(Some(&resolved_provider), base_url);

    let chat_client: Arc<dyn AiChatClient> = Arc::new(crate::build_genai_chat_client(
        &resolved_provider,
        &resolved_model,
        resolved_base_url.as_deref(),
    ));

    let workflow_registry = workflow::shared_workflow_registry();
    let catalog_handle = ToolCatalogHandle::default();
    let mut tool_registry = ToolRegistrar::new(crate::tool_catalog::first_party_placement_index());
    let turn_scope = crate::agent_runtime::execution_context::TurnScopeAccess::default();
    let compaction_target = GraphemeCompactionModelTarget {
        provider: resolved_provider.clone(),
        model: resolved_model.clone(),
        base_url: resolved_base_url.clone(),
    };
    let identity_service = Arc::new(IdentityMemoryService::new(identity_memory_store.clone()
        as Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>));
    let worker_scheduler = Arc::new(crate::agent_runtime::turn_worker::TurnWorkerScheduler::new(
        crate::agent_runtime::turn_worker::turn_worker_store(),
    ));
    let capability_registry = Arc::new(RwLock::new(CapabilityRegistry::with_loaded_manifest()));
    let mcp_gateway_client = Arc::new(McpGatewayClient::from_env());
    let shared_tools = crate::tool_registration_groups::SharedToolRegistrationBindings {
        runtime: runtime.clone(),
        event_tx: event_tx.clone(),
        turn_scope: turn_scope.clone(),
        workflow_registry: workflow_registry.clone(),
        identity_service,
        identity_store: identity_memory_store.clone(),
        memory_reader: memory_reader.clone(),
        memory_writer: memory_writer.clone(),
        locus_store: locus_store.clone(),
        semantic_index: semantic_index.clone(),
        memory_operations: memory_operations.clone(),
        session_id: session_id.to_string(),
        workshop_operator_identity,
        compaction_target,
        catalog_handle: catalog_handle.clone(),
        worker_scheduler: worker_scheduler.clone(),
        capability_registry: capability_registry.clone(),
        mcp_gateway_client: mcp_gateway_client.clone(),
    };
    crate::tool_registration_groups::register_shared_foundation_tools(
        &mut tool_registry,
        &shared_tools,
    )?;
    crate::tool_registration_groups::register_desktop_openshell_tools(
        &mut tool_registry,
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    )?;
    crate::tool_registration_groups::register_shared_secret_tools(
        &mut tool_registry,
        &shared_tools,
    )?;
    crate::tool_registration_groups::register_desktop_coding_tools(
        &mut tool_registry,
        runtime.clone(),
    )?;
    crate::tool_registration_groups::register_shared_interactive_tools(
        &mut tool_registry,
        &shared_tools,
    )?;

    crate::tool_registration_groups::register_desktop_coder_catalog_adapters(&mut tool_registry)?;
    let (tool_registry, tool_catalog) = tool_registry.finish();
    crate::tool_catalog::compile_general_surface(&tool_catalog)?;
    catalog_handle.initialize(tool_catalog.clone())?;

    let prompt_pipeline = PromptExecutionPipeline::new(chat_client);
    let base_registry: Arc<dyn ToolRegistry> = Arc::new(tool_registry);
    let client_tool_registry: Arc<dyn ToolRegistry> = Arc::new(ClientToolRegistry::new(
        base_registry,
        client_registry.clone(),
        turn_scope.clone(),
    ));
    let guarded_registry: Arc<dyn ToolRegistry> = Arc::new(PolicyAwareToolRegistry::new(
        client_tool_registry,
        allowed_grapheme_modules,
        EngineExecutionLane::Interactive,
    ));
    let tool_loop_pipeline =
        MedousaToolLoopPipeline::new(prompt_pipeline, guarded_registry.clone())
            .with_parallel_execution_settings_provider(Arc::new(
                crate::execution_policy::load_parallel_execution_settings,
            ));

    Ok(TuiRuntime {
        runtime,
        tool_loop_pipeline,
        tool_registry: guarded_registry,
        tool_catalog,
        capability_registry,
        mcp_gateway_client,
        workflow_registry,
        locus_store,
        semantic_index,
        medousa_identity_store: identity_memory_store.clone(),
        identity_memory_store: identity_memory_store.clone()
            as Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>,
        memory_reader,
        memory_writer,
        memory_operations,
        client_registry,
        execution_registry: crate::agent_runtime::execution_context::TurnExecutionRegistry::default(
        ),
        worker_scheduler,
    })
}
