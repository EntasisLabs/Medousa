use std::sync::Arc;

use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use stasis::application::orchestration::tool_loop_pipeline::ToolLoopPipeline;
use stasis::application::orchestration::tool_registry::{InMemoryToolRegistry, ToolRegistry};
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::AiChatClient;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::prelude::{RuntimeBackend, StasisRuntimeBuilder};
use stasis::prelude_ext::{
    LocusContextReader, LocusContextWriter, LocusNodeStoreFactory, MemoryContextReader,
    MemoryContextWriter,
};
use tokio::sync::mpsc;

use crate::engine_context::EngineExecutionLane;
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::GraphemeCompactionModelTarget;
use crate::runtime_tools::{
    CognitionRuntimeDeliveryStatusTool, CognitionRuntimeJobsCancelTool,
    CognitionRuntimeJobsListTool, CognitionRuntimeRecurringCancelTool,
    CognitionRuntimeRecurringListTool, CognitionRuntimeRecurringPauseTool,
    CognitionRuntimeRecurringRegisterTool,     CognitionRuntimeWorkflowCancelTool, CognitionRuntimeWorkflowPlanTool,
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
use crate::workflow;
use tokio::sync::RwLock;

pub(crate) fn build_tool_loop_pipeline_for_target(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
    tool_registry: Arc<dyn ToolRegistry>,
) -> ToolLoopPipeline {
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
    ToolLoopPipeline::new(prompt_pipeline, tool_registry)
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
    let backend_for_identity = backend.clone();

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

    let locus_store = LocusNodeStoreFactory::in_memory().await?;
    let memory_reader: Arc<dyn MemoryContextReader> =
        Arc::new(LocusContextReader::new(locus_store.clone()));
    let memory_writer: Arc<dyn MemoryContextWriter> =
        Arc::new(LocusContextWriter::new(locus_store.clone()));
    let identity_memory_store: Arc<dyn IdentityMemoryStore> =
        crate::identity_memory::build_identity_memory_store_for_backend(&backend_for_identity)
            .await?;

    let workflow_registry = workflow::shared_workflow_registry();
    let prompt_pipeline_for_handler = PromptExecutionPipeline::new(chat_client.clone());

    let runtime_composition = workflow::attach_workflow_handler(
        StasisRuntimeBuilder::new(backend)
            .with_chat_client(chat_client.clone())
            .with_memory_context_reader(memory_reader.clone())
            .with_memory_context_writer(memory_writer.clone())
            .with_identity_memory_store(identity_memory_store.clone()),
        prompt_pipeline_for_handler,
        workflow_registry.clone(),
    )
    .build()
    .await?;

    let runtime = Arc::new(runtime_composition);

    let tool_registry = InMemoryToolRegistry::default();
    let compaction_target = GraphemeCompactionModelTarget {
        provider: resolved_provider.clone(),
        model: resolved_model.clone(),
        base_url: resolved_base_url.clone(),
    };
    tool_registry.register_tool(CognitionJobEnqueueTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemeRunTool::new(
        runtime.clone(),
        event_tx.clone(),
        session_id.to_string(),
        compaction_target.clone(),
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
    ))?;
    tool_registry.register_tool(CognitionGraphemePromoteToRecurringTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemePromoteLastRunToRecurringTool::new(
        runtime.clone(),
        event_tx.clone(),
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
    tool_registry.register_tool(CognitionRuntimeRecurringRegisterTool::new(
        runtime.clone(),
        event_tx.clone(),
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
    ))?;
    tool_registry.register_tool(CognitionRuntimeWorkflowScheduleTool::new(
        runtime.clone(),
        workflow_registry.clone(),
        event_tx.clone(),
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
    tool_registry.register_tool(CognitionCapabilityListTool::new(capability_registry.clone()))?;
    tool_registry.register_tool(CognitionCapabilitySearchTool::new(
        capability_registry.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionMcpDiscoverTool::new(
        mcp_gateway_client.clone(),
        session_id.to_string(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionMcpInvokeTool::new(
        mcp_gateway_client.clone(),
        session_id.to_string(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionMcpServersTool::new(mcp_gateway_client.clone()))?;
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
    let tool_loop_pipeline = ToolLoopPipeline::new(prompt_pipeline, guarded_registry.clone());

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
    })
}
