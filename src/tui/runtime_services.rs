use std::sync::Arc;

use crate::identity_store_ext::MedousaIdentityMemoryStore;
use crate::medousa_tool_loop::MedousaToolLoopPipeline;
use locus_core_rs::NodeStore;
use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::ports::outbound::ai_chat_client::AiChatClient;
use stasis::prelude::RuntimeBackend;
use stasis::prelude_ext::{MemoryContextReader, MemoryContextWriter};
use tokio::sync::mpsc;

use crate::bridge_tools::CognitionWebSearchTool;
use crate::capability_catalog::CapabilityRegistry;
use crate::client_tools::{ClientRegistry, ClientToolRegistry};
use crate::engine_context::EngineExecutionLane;
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::GraphemeCompactionModelTarget;
use crate::identity_memory::{
    resolve_identity_channel_id, resolve_identity_persona_id, resolve_tool_identity_user_id,
};
use crate::identity_tools::{
    CognitionIdentityCommitTool, CognitionIdentityContextTool, CognitionIdentityProposeTool,
    CognitionIdentityRecallTool, CognitionIdentityRememberTool,
};
use crate::mcp_gateway_client::McpGatewayClient;
use crate::runtime::stasis_wire::{LocalStasisWireConfig, build_local_stasis_composition};
use crate::tools::{
    CognitionUtilityDayOfWeekTool, CognitionUtilityTimeNowTool, CognitionUtilityUuidTool,
    PolicyAwareToolRegistry, TuiRuntime,
};
use crate::typed_tools::{ToolCatalogHandle, ToolRegistrar, ToolRegistration};
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
    crate::session_store::init_session_store_with_runtime(&composition).await;
    crate::artifact_store::init_artifact_store_with_runtime(&composition).await;
    crate::component_store::init_component_store_with_runtime(&composition).await;
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
    crate::schema_api::register_schema_tools(&mut tool_registry)?;
    let compaction_target = GraphemeCompactionModelTarget {
        provider: resolved_provider.clone(),
        model: resolved_model.clone(),
        base_url: resolved_base_url.clone(),
    };
    crate::runtime_api::register_runtime_api_tools(
        &mut tool_registry,
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
        workflow_registry.clone(),
    )?;
    let identity_service = Arc::new(IdentityMemoryService::new(identity_memory_store.clone()
        as Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>));
    let identity_user_id = resolve_tool_identity_user_id(session_id, workshop_operator_identity);
    let identity_persona_id = resolve_identity_persona_id();
    let identity_channel_id = resolve_identity_channel_id(Some("interactive"));
    tool_registry.register_typed_tool(CognitionIdentityContextTool::new(
        identity_service.clone(),
        identity_user_id.clone(),
        identity_persona_id,
        identity_channel_id,
        workshop_operator_identity,
        event_tx.clone(),
    ))?;
    tool_registry.register_typed_tool(CognitionIdentityProposeTool::new(
        identity_service.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_typed_tool(CognitionIdentityCommitTool::new(
        identity_service,
        Some(memory_writer.clone()),
        event_tx.clone(),
    ))?;
    tool_registry.register_typed_tool(CognitionIdentityRecallTool::new(
        identity_memory_store.clone(),
        identity_user_id.clone(),
        workshop_operator_identity,
        event_tx.clone(),
    ))?;
    tool_registry.register_typed_tool(CognitionIdentityRememberTool::new(
        identity_memory_store.clone(),
        Some(memory_writer.clone()),
        identity_user_id.clone(),
        workshop_operator_identity,
        event_tx.clone(),
    ))?;
    crate::manuscript_tools::register_manuscript_tools(&mut tool_registry)?;
    crate::openshell_tools::register_openshell_tools(
        &mut tool_registry,
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    )?;
    crate::shell_tools::register_shell_tools(&mut tool_registry, runtime.clone())?;
    crate::code_intelligence_tools::register_code_intelligence_tools(&mut tool_registry)?;
    crate::coding_tools::register_coding_tools(&mut tool_registry)?;
    crate::detamu_tools::register_detamu_tools(&mut tool_registry)?;
    crate::ui_present_tools::register_ui_present_tools(&mut tool_registry, turn_scope.clone())?;
    crate::ui_scene_tools::register_ui_scene_tools(&mut tool_registry, turn_scope.clone())?;
    crate::ui_build_tools::register_ui_build_tools(&mut tool_registry, turn_scope.clone())?;
    crate::store_tools::register_store_tools(
        &mut tool_registry,
        event_tx.clone(),
        turn_scope.clone(),
        session_id.to_string(),
    )?;
    crate::artifact_tools::register_artifact_tools(
        &mut tool_registry,
        event_tx.clone(),
        turn_scope.clone(),
    )?;
    crate::skill_tools::register_skill_tools(
        &mut tool_registry,
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    )?;
    crate::vault_tools::register_vault_tools(
        &mut tool_registry,
        event_tx.clone(),
        turn_scope.clone(),
        session_id.to_string(),
    )?;
    crate::calendar_tools::register_calendar_tools(&mut tool_registry, event_tx.clone())?;
    crate::tool_history_tools::register_tool_history_tools(&mut tool_registry, turn_scope.clone())?;
    crate::chat_history_tools::register_chat_history_tools(&mut tool_registry, turn_scope.clone())?;
    crate::grapheme_script_tools::register_grapheme_script_tools(
        &mut tool_registry,
        event_tx.clone(),
    )?;
    crate::manuscript_overlay_tools::register_manuscript_overlay_tools(&mut tool_registry)?;
    crate::tool_bootstrap_tools::register_tool_bootstrap_tools(
        &mut tool_registry,
        turn_scope.clone(),
        catalog_handle.clone(),
    )?;
    crate::environment_tools::register_environment_tools(&mut tool_registry, turn_scope.clone())?;
    crate::custom_view_tools::register_custom_view_tools(
        &mut tool_registry,
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    )?;
    crate::context_pointer_tools::register_context_pointer_tools(
        &mut tool_registry,
        turn_scope.clone(),
    )?;

    crate::memory_api::register_memory_tools(
        &mut tool_registry,
        locus_store.clone(),
        memory_reader.clone(),
        memory_writer.clone(),
        semantic_index.clone(),
        memory_operations.clone(),
        session_id.to_string(),
        workshop_operator_identity,
        turn_scope.clone(),
        event_tx.clone(),
    )?;
    tool_registry.register_typed_tool(CognitionUtilityTimeNowTool)?;
    tool_registry.register_typed_tool(CognitionUtilityDayOfWeekTool)?;
    tool_registry.register_typed_tool(CognitionUtilityUuidTool)?;
    let worker_scheduler = Arc::new(crate::agent_runtime::turn_worker::TurnWorkerScheduler::new(
        crate::agent_runtime::turn_worker::turn_worker_store(),
    ));
    crate::agent_runtime::turn_worker_tools::register_turn_worker_tools(
        &mut tool_registry,
        worker_scheduler.clone(),
    )?;
    crate::turn_api::register_turn_tools(
        &mut tool_registry,
        worker_scheduler.clone(),
        session_id.to_string(),
        turn_scope.clone(),
    )?;

    let capability_registry = Arc::new(RwLock::new(CapabilityRegistry::with_loaded_manifest()));
    let mcp_gateway_client = Arc::new(McpGatewayClient::from_env());
    crate::capability_tools::register_capability_tools(
        &mut tool_registry,
        runtime.clone(),
        event_tx.clone(),
        session_id.to_string(),
        turn_scope.clone(),
        compaction_target,
        capability_registry.clone(),
        mcp_gateway_client.clone(),
    )?;
    crate::feed_tools::register_feed_tools(
        &mut tool_registry,
        capability_registry.clone(),
        turn_scope.clone(),
    )?;
    crate::layout_tools::register_layout_tools(&mut tool_registry)?;
    tool_registry.register_typed_tool(CognitionWebSearchTool::new(
        capability_registry.clone(),
        runtime.clone(),
        mcp_gateway_client.clone(),
        session_id.to_string(),
        turn_scope.clone(),
        event_tx.clone(),
    ))?;
    crate::browser_fetch_tools::register_browser_fetch_tool(
        &mut tool_registry,
        turn_scope.clone(),
        event_tx.clone(),
    )?;
    crate::browser_snapshot_tools::register_browser_snapshot_tool(
        &mut tool_registry,
        turn_scope.clone(),
        event_tx.clone(),
    )?;
    crate::browser_act_tools::register_browser_act_tool(
        &mut tool_registry,
        turn_scope.clone(),
        event_tx.clone(),
    )?;

    crate::agent_runtime::coder_tools::register_catalog_runtime_adapters(&mut tool_registry)?;
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
        MedousaToolLoopPipeline::new(prompt_pipeline, guarded_registry.clone());

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
