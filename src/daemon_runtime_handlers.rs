//! Portable Stasis handler set shared by every daemon deployment.

use std::sync::Arc;

use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::application::runtime::agent_session_job_handler::AgentSessionJobHandler;
use stasis::application::runtime::agent_turn_job_handler::AgentTurnJobHandler;
use stasis::application::runtime::concurrent_pattern_job_handler::ConcurrentPatternJobHandler;
use stasis::application::runtime::coordinator_failover_job_handler::CoordinatorFailoverJobHandler;
use stasis::application::runtime::grapheme_echo_job_handler::GraphemeEchoJobHandler;
use stasis::application::runtime::grapheme_healthcheck_job_handler::GraphemeHealthcheckJobHandler;
use stasis::application::runtime::grapheme_job_handler::GraphemeJobHandler;
use stasis::application::runtime::grapheme_textops_job_handler::GraphemeTextOpsJobHandler;
use stasis::application::runtime::handoff_pattern_job_handler::HandoffPatternJobHandler;
use stasis::application::runtime::memory_aggregate_job_handler::MemoryAggregateJobHandler;
use stasis::application::runtime::memory_recall_job_handler::MemoryRecallJobHandler;
use stasis::application::runtime::memory_rollup_job_handler::MemoryRollupJobHandler;
use stasis::application::runtime::memory_schema_job_handler::MemorySchemaJobHandler;
use stasis::application::runtime::memory_transform_job_handler::MemoryTransformJobHandler;
use stasis::application::runtime::orchestrator_pattern_job_handler::OrchestratorPatternJobHandler;
use stasis::application::runtime::prompt_chat_job_handler::PromptChatJobHandler;
use stasis::application::runtime::queue_ownership_rebalance_job_handler::QueueOwnershipRebalanceJobHandler;
use stasis::application::runtime::sequential_pattern_job_handler::SequentialPatternJobHandler;
use stasis::application::runtime::tool_loop_job_handler::ToolLoopJobHandler;
use stasis::ports::outbound::ai_chat_client::AiChatClient;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::ports::outbound::memory::memory_context_reader::MemoryContextReader;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;
use stasis::ports::outbound::runtime::cluster_node_store::ClusterNodeStore;
use stasis::ports::outbound::runtime::thread_store::ThreadStore;
use stasis::ports::outbound::runtime::workflow_engine::WorkflowEngine;

#[allow(clippy::too_many_arguments)]
pub(crate) fn register_daemon_runtime_handlers<R>(
    runtime: &R,
    chat_client: &Arc<dyn AiChatClient>,
    tool_registry: &Arc<dyn ToolRegistry>,
    workflow_engine: &Arc<dyn WorkflowEngine>,
    memory_reader: &Option<Arc<dyn MemoryContextReader>>,
    memory_writer: &Option<Arc<dyn MemoryContextWriter>>,
    identity_store: &Option<Arc<dyn IdentityMemoryStore>>,
    memory_operations: &Option<Arc<dyn MemoryOperations>>,
    thread_store: &Arc<dyn ThreadStore>,
    cluster_store: &Arc<dyn ClusterNodeStore>,
) -> stasis::prelude::Result<()>
where
    R: DaemonRuntimeRegistrar,
{
    runtime.register_daemon_handler(GraphemeJobHandler::new(workflow_engine.clone()))?;
    runtime.register_daemon_handler(GraphemeHealthcheckJobHandler::new(workflow_engine.clone()))?;
    runtime.register_daemon_handler(GraphemeEchoJobHandler::new(workflow_engine.clone()))?;
    runtime.register_daemon_handler(GraphemeTextOpsJobHandler::new(workflow_engine.clone()))?;

    runtime.register_daemon_handler(PromptChatJobHandler::new_with_memory_and_identity(
        chat_client.clone(),
        memory_reader.clone(),
        memory_writer.clone(),
        identity_store.clone(),
    ))?;
    runtime.register_daemon_handler(ToolLoopJobHandler::new_with_memory_and_identity(
        chat_client.clone(),
        tool_registry.clone(),
        memory_reader.clone(),
        memory_writer.clone(),
        identity_store.clone(),
    ))?;
    runtime.register_daemon_handler(AgentTurnJobHandler::new_with_memory_and_identity(
        chat_client.clone(),
        tool_registry.clone(),
        memory_reader.clone(),
        memory_writer.clone(),
        identity_store.clone(),
    ))?;
    runtime.register_daemon_handler(AgentSessionJobHandler::new_with_memory_and_identity(
        chat_client.clone(),
        tool_registry.clone(),
        memory_reader.clone(),
        memory_writer.clone(),
        identity_store.clone(),
    ))?;

    if let Some(reader) = memory_reader.clone() {
        runtime.register_daemon_handler(MemoryRecallJobHandler::new(reader))?;
    }
    if let Some(operations) = memory_operations.clone() {
        runtime.register_daemon_handler(MemoryAggregateJobHandler::new(operations.clone()))?;
        runtime.register_daemon_handler(MemoryTransformJobHandler::new(operations.clone()))?;
        runtime.register_daemon_handler(MemoryRollupJobHandler::new(operations.clone()))?;
        runtime.register_daemon_handler(MemorySchemaJobHandler::new(operations))?;
    }

    runtime.register_daemon_handler(
        ConcurrentPatternJobHandler::new_with_thread_store_and_memory(
            chat_client.clone(),
            tool_registry.clone(),
            Some(thread_store.clone()),
            memory_reader.clone(),
            memory_writer.clone(),
            identity_store.clone(),
        ),
    )?;
    runtime.register_daemon_handler(HandoffPatternJobHandler::new_with_thread_store(
        chat_client.clone(),
        Some(thread_store.clone()),
    ))?;
    runtime.register_daemon_handler(OrchestratorPatternJobHandler::new_with_thread_store(
        chat_client.clone(),
        Some(thread_store.clone()),
    ))?;
    runtime.register_daemon_handler(SequentialPatternJobHandler::new_with_thread_store(
        chat_client.clone(),
        Some(thread_store.clone()),
    ))?;

    runtime.register_daemon_handler(CoordinatorFailoverJobHandler::new(cluster_store.clone()))?;
    runtime.register_daemon_handler(QueueOwnershipRebalanceJobHandler::new(
        cluster_store.clone(),
    ))?;
    Ok(())
}

pub(crate) trait DaemonRuntimeRegistrar {
    fn register_daemon_handler<
        H: stasis::application::runtime::in_memory_runtime::JobHandler + 'static,
    >(
        &self,
        handler: H,
    ) -> stasis::prelude::Result<()>;
}

impl DaemonRuntimeRegistrar for stasis::application::runtime::in_memory_runtime::InMemoryRuntime {
    fn register_daemon_handler<
        H: stasis::application::runtime::in_memory_runtime::JobHandler + 'static,
    >(
        &self,
        handler: H,
    ) -> stasis::prelude::Result<()> {
        self.register_handler(handler)
    }
}

impl DaemonRuntimeRegistrar for stasis::application::runtime::surreal_runtime::SurrealRuntime {
    fn register_daemon_handler<
        H: stasis::application::runtime::in_memory_runtime::JobHandler + 'static,
    >(
        &self,
        handler: H,
    ) -> stasis::prelude::Result<()> {
        self.register_handler(handler)
    }
}
