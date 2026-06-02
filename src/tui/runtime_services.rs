use std::sync::Arc;

use locus_core_rs::NodeStore;
use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use crate::medousa_tool_loop::MedousaToolLoopPipeline;
use stasis::application::orchestration::tool_registry::{InMemoryToolRegistry, ToolRegistry};
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::AiChatClient;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::prelude::RuntimeBackend;
use stasis::prelude_ext::{MemoryContextReader, MemoryContextWriter};
use tokio::sync::mpsc;

use crate::engine_context::EngineExecutionLane;
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::GraphemeCompactionModelTarget;
use crate::runtime::stasis_wire::{LocalStasisWireConfig, build_local_stasis_composition};
use crate::runtime_tools::{
    CognitionRuntimeDeliveryStatusTool, CognitionRuntimeJobsCancelTool,
    CognitionRuntimeJobsListTool, CognitionRuntimeRecurringCancelTool,
    CognitionRuntimeRecurringListTool, CognitionRuntimeRecurringPauseTool,
    CognitionRuntimeRecurringDoctorTool, CognitionRuntimeRecurringRegisterTool,
    CognitionRuntimeWorkflowCancelTool, CognitionRuntimeWorkflowPlanTool,
    CognitionRuntimeWorkflowRunTool, CognitionRuntimeWorkflowScheduleTool,
    CognitionRuntimeWorkflowStatusTool,
};
use crate::tools::{
    CognitionCapabilityListTool, CognitionCapabilityResolveTool, CognitionCapabilitySearchTool,
    CognitionGraphemeCliRunTool, CognitionGraphemeExamplesTool, CognitionGraphemeModulesInfoTool,
    CognitionGraphemeModulesOpsTool, CognitionGraphemeModulesSearchTool,
    CognitionGraphemePromoteLastRunToRecurringTool, CognitionGraphemePromoteToJobTool,
    CognitionGraphemePromoteToRecurringTool, CognitionGraphemeRunTool, CognitionJobEnqueueTool,
    CognitionMcpDiscoverTool, CognitionMcpInvokeTool, CognitionMcpServersTool, CognitionMemoryRecallTool, CognitionMemoryStoreTool,
    CognitionRuntimeJobStatusTool, CognitionRuntimeRecurringPreviewTool,
    CognitionUtilityDayOfWeekTool, CognitionUtilityTimeNowTool, CognitionUtilityUuidTool,
    PolicyAwareToolRegistry, TuiRuntime,
};
use crate::bridge_tools::{
    CognitionCapabilityInvokeTool, CognitionGraphemeTemplateRunTool,
    CognitionMcpPromoteToJobTool,
};
use crate::capability_catalog::CapabilityRegistry;
use crate::mcp_gateway_client::McpGatewayClient;
use crate::tool_aliases::ToolNameAlias;
use crate::turn_continuation::TurnContinuationScope;
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
    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&resolved_provider),
            &resolved_model,
            resolved_base_url.as_deref(),
        ),
    );
    let prompt_pipeline = PromptExecutionPipeline::new(chat_client);
    MedousaToolLoopPipeline::new(prompt_pipeline, tool_registry)
}

pub(crate) async fn build_tui_runtime_services(
    backend: RuntimeBackend,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
    session_id: &str,
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
    crate::verification_store::init_verification_store_with_runtime(&composition).await;
    crate::turn_continuation::init_turn_continuation_store_with_runtime(&composition).await;

    assemble_tui_runtime(
        Arc::new(composition),
        memory.identity_store.clone(),
        memory.memory_reader.clone(),
        memory.memory_writer.clone(),
        memory.locus_store.clone(),
        provider,
        model,
        base_url,
        allowed_grapheme_modules,
        session_id,
        event_tx,
    )
    .await
}

/// Assemble agent/TUI tooling on top of an existing runtime composition (no new DB connection).
pub(crate) async fn assemble_tui_runtime(
    runtime: Arc<stasis::prelude::RuntimeComposition>,
    identity_memory_store: Arc<dyn IdentityMemoryStore>,
    memory_reader: Arc<dyn MemoryContextReader>,
    memory_writer: Arc<dyn MemoryContextWriter>,
    locus_store: Arc<dyn NodeStore>,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
    session_id: &str,
    event_tx: mpsc::Sender<TuiEvent>,
) -> anyhow::Result<TuiRuntime> {
    let resolved_provider = crate::resolve_llm_provider(provider);
    let resolved_model = crate::resolve_llm_model(model);
    let resolved_base_url = crate::resolve_llm_base_url(Some(&resolved_provider), base_url);

    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&resolved_provider),
            &resolved_model,
            resolved_base_url.as_deref(),
        ),
    );

    let workflow_registry = workflow::shared_workflow_registry();
    let tool_registry = InMemoryToolRegistry::default();
    let turn_scope = Arc::new(RwLock::new(None::<TurnContinuationScope>));
    let compaction_target = GraphemeCompactionModelTarget {
        provider: resolved_provider.clone(),
        model: resolved_model.clone(),
        base_url: resolved_base_url.clone(),
    };
    tool_registry.register_tool(CognitionJobEnqueueTool::new(
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemeRunTool::new(
        runtime.clone(),
        event_tx.clone(),
        session_id.to_string(),
        compaction_target.clone(),
        turn_scope.clone(),
    ))?;
    tool_registry.register_tool(CognitionMemoryStoreTool::new(
        memory_writer.clone(),
        session_id.to_string(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionMemoryRecallTool::new(
        memory_reader.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemeModulesSearchTool::new(event_tx.clone()))?;
    tool_registry.register_tool(CognitionGraphemeModulesInfoTool::new(event_tx.clone()))?;
    tool_registry.register_tool(CognitionGraphemeModulesOpsTool::new(event_tx.clone()))?;
    tool_registry.register_tool(CognitionGraphemeExamplesTool::new(event_tx.clone()))?;
    tool_registry.register_tool(CognitionGraphemeCliRunTool::new(
        runtime.clone(),
        event_tx.clone(),
        session_id.to_string(),
        compaction_target,
    ))?;
    tool_registry.register_tool(CognitionGraphemePromoteToJobTool::new(
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemePromoteToRecurringTool::new(
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemePromoteLastRunToRecurringTool::new(
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    tool_registry.register_tool(CognitionUtilityTimeNowTool)?;
    tool_registry.register_tool(CognitionUtilityDayOfWeekTool)?;
    tool_registry.register_tool(CognitionUtilityUuidTool)?;
    tool_registry.register_tool(CognitionRuntimeRecurringPreviewTool::new(event_tx.clone()))?;
    tool_registry.register_tool(CognitionRuntimeJobStatusTool::new(runtime.clone()))?;
    tool_registry.register_tool(CognitionRuntimeJobsListTool::new(runtime.clone()))?;
    tool_registry.register_tool(CognitionRuntimeJobsCancelTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionRuntimeRecurringListTool::new(runtime.clone()))?;
    tool_registry.register_tool(CognitionRuntimeRecurringDoctorTool::new(runtime.clone()))?;
    tool_registry.register_tool(CognitionRuntimeRecurringRegisterTool::new(
        runtime.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    tool_registry.register_tool(CognitionRuntimeRecurringPauseTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionRuntimeRecurringCancelTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionRuntimeDeliveryStatusTool::new(runtime.clone()))?;
    tool_registry.register_tool(CognitionRuntimeWorkflowRunTool::new(
        runtime.clone(),
        workflow_registry.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    tool_registry.register_tool(CognitionRuntimeWorkflowScheduleTool::new(
        runtime.clone(),
        workflow_registry.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    tool_registry.register_tool(CognitionRuntimeWorkflowStatusTool::new(
        runtime.clone(),
        workflow_registry.clone(),
    ))?;
    tool_registry.register_tool(CognitionRuntimeWorkflowCancelTool::new(
        runtime.clone(),
        workflow_registry.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionRuntimeWorkflowPlanTool::new(event_tx.clone()))?;

    let capability_registry = Arc::new(RwLock::new(CapabilityRegistry::with_loaded_manifest()));
    let mcp_gateway_client = Arc::new(McpGatewayClient::from_env());
    tool_registry.register_tool(CognitionCapabilityResolveTool::new(
        capability_registry.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(ToolNameAlias::new(
        "cognition_capability_resolve",
        CognitionCapabilityResolveTool::new(capability_registry.clone(), event_tx.clone()),
    ))?;
    tool_registry.register_tool(CognitionCapabilityListTool::new(capability_registry.clone()))?;
    tool_registry.register_tool(ToolNameAlias::new(
        "cognition_capability_list",
        CognitionCapabilityListTool::new(capability_registry.clone()),
    ))?;
    tool_registry.register_tool(CognitionCapabilitySearchTool::new(
        capability_registry.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(ToolNameAlias::new(
        "cognition_capability_search",
        CognitionCapabilitySearchTool::new(capability_registry.clone(), event_tx.clone()),
    ))?;
    tool_registry.register_tool(CognitionMcpDiscoverTool::new(
        mcp_gateway_client.clone(),
        session_id.to_string(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(ToolNameAlias::new(
        "cognition_mcp_discover",
        CognitionMcpDiscoverTool::new(
            mcp_gateway_client.clone(),
            session_id.to_string(),
            event_tx.clone(),
        ),
    ))?;
    tool_registry.register_tool(CognitionMcpInvokeTool::new(
        mcp_gateway_client.clone(),
        session_id.to_string(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(ToolNameAlias::new(
        "cognition_mcp_invoke",
        CognitionMcpInvokeTool::new(
            mcp_gateway_client.clone(),
            session_id.to_string(),
            event_tx.clone(),
        ),
    ))?;
    tool_registry.register_tool(CognitionMcpServersTool::new(mcp_gateway_client.clone()))?;
    tool_registry.register_tool(ToolNameAlias::new(
        "cognition_mcp_servers",
        CognitionMcpServersTool::new(mcp_gateway_client.clone()),
    ))?;
    tool_registry.register_tool(CognitionCapabilityInvokeTool::new(
        capability_registry.clone(),
        runtime.clone(),
        mcp_gateway_client.clone(),
        session_id.to_string(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionMcpPromoteToJobTool::new(
        runtime.clone(),
        workflow_registry.clone(),
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemeTemplateRunTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;

    let prompt_pipeline = PromptExecutionPipeline::new(chat_client);
    let base_registry: Arc<dyn ToolRegistry> = Arc::new(tool_registry);
    let guarded_registry: Arc<dyn ToolRegistry> = Arc::new(PolicyAwareToolRegistry::new(
        base_registry,
        allowed_grapheme_modules,
        EngineExecutionLane::Interactive,
    ));
    let tool_loop_pipeline = MedousaToolLoopPipeline::new(prompt_pipeline, guarded_registry.clone());

    Ok(TuiRuntime {
        runtime,
        tool_loop_pipeline,
        tool_registry: guarded_registry,
        capability_registry,
        mcp_gateway_client,
        workflow_registry,
        locus_store,
        identity_memory_store,
        memory_reader,
        memory_writer,
        turn_scope,
    })
}