use std::sync::Arc;

use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
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
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::infrastructure::runtime::http_webhook_event_publisher::HttpWebhookTransportPublisher;
use stasis::ports::outbound::ai_chat_client::AiChatClient;
use stasis::ports::outbound::runtime::delivery_endpoint_store::DeliveryEndpointStore;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, RuntimeFactory, StasisRuntimeBuilder};
use stasis::runtime_prelude_ext::InMemoryDeliveryEndpointStore;

use crate::channel_delivery;
use crate::runtime::memory_bundle::MemoryAdapterBundle;
use crate::runtime::stasis_otel::attach_otel_to_builder;
use crate::workflow;

struct MockWebSearchTool;

#[async_trait::async_trait]
impl stasis::application::orchestration::tool_registry::StasisTool for MockWebSearchTool {
    fn name(&self) -> &'static str {
        "stasis.web.search.mock"
    }

    async fn invoke(&self, input: serde_json::Value) -> stasis::prelude::Result<serde_json::Value> {
        let query = input
            .get("query")
            .and_then(|value| value.as_str())
            .unwrap_or("general research")
            .to_string();

        Ok(serde_json::json!({
            "query": query,
            "results": [
                {
                    "title": "Rust ecosystem trends",
                    "snippet": "Growing adoption in platform tooling and backend services.",
                    "source": "mock://rust-trends-1"
                }
            ]
        }))
    }
}

fn surreal_backend_connect_label(backend: &RuntimeBackend) -> Option<String> {
    match backend {
        RuntimeBackend::SurrealWs { endpoint, namespace, database, .. } => {
            let endpoint = endpoint.trim();
            let display = if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
                endpoint.to_string()
            } else {
                format!("ws://{endpoint}")
            };
            Some(format!(
                "SurrealDB {display} (ns={namespace}, db={database})"
            ))
        }
        RuntimeBackend::SurrealKv { path, namespace, database, .. } => Some(format!(
            "SurrealKV {path} (ns={namespace}, db={database})"
        )),
        RuntimeBackend::SurrealMem { namespace, database, .. } => Some(format!(
            "Surreal mem:// (ns={namespace}, db={database})"
        )),
        _ => None,
    }
}

pub struct DaemonStasisWireConfig<'a> {
    pub backend: RuntimeBackend,
    pub provider: Option<&'a str>,
    pub model: Option<&'a str>,
    pub base_url: Option<&'a str>,
    pub deliver_webhook_url: &'a str,
}

/// Build a daemon Stasis composition with explicit shared memory adapters.
/// Returns the composition and the memory bundle wired into Stasis and agent tools.
pub async fn build_daemon_stasis_composition(
    config: DaemonStasisWireConfig<'_>,
) -> anyhow::Result<(RuntimeComposition, MemoryAdapterBundle)> {
    match &config.backend {
        RuntimeBackend::InMemory => {
            let memory = MemoryAdapterBundle::build_in_memory().await?;
            let composition = build_in_memory_daemon_composition(&config, &memory).await?;
            Ok((composition, memory))
        }
        RuntimeBackend::SurrealKv { .. }
        | RuntimeBackend::SurrealWs { .. }
        | RuntimeBackend::SurrealMem { .. } => {
            if let Some(label) = surreal_backend_connect_label(&config.backend) {
                eprintln!("medousa-daemon: connecting to {label}…");
            }
            let shell = RuntimeFactory::build(config.backend.clone()).await?;
            eprintln!("medousa-daemon: surreal runtime connected, initializing memory adapters…");
            let memory = MemoryAdapterBundle::from_runtime_shell(&shell).await?;
            eprintln!("medousa-daemon: wiring job handlers and delivery…");
            let composition = wire_existing_daemon_composition(shell, &config, &memory).await?;
            Ok((composition, memory))
        }
    }
}

async fn build_in_memory_daemon_composition(
    config: &DaemonStasisWireConfig<'_>,
    memory: &MemoryAdapterBundle,
) -> anyhow::Result<RuntimeComposition> {
    let provider = crate::resolve_llm_provider(config.provider);
    let model = crate::resolve_llm_model(config.model);
    let base_url = crate::resolve_llm_base_url(Some(&provider), config.base_url);
    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&provider),
            &model,
            base_url.as_deref(),
        ),
    );

    let in_memory_endpoint_store = Arc::new(InMemoryDeliveryEndpointStore::default())
        as Arc<dyn DeliveryEndpointStore>;

    let workflow_registry = workflow::shared_workflow_registry();
    let prompt_pipeline = PromptExecutionPipeline::new(chat_client.clone());

    let mut builder = StasisRuntimeBuilder::new(config.backend.clone())
        .with_chat_client(chat_client)
        .with_memory_context_reader(memory.memory_reader.clone())
        .with_memory_context_writer(memory.memory_writer.clone())
        .with_memory_operations(memory.memory_operations.clone())
        .with_identity_memory_store(memory.identity_store.clone())
        .with_locus_memory()
        .with_endpoint_routing_delivery()
        .with_delivery_endpoint_store(in_memory_endpoint_store.clone());

    if let Some(token) = channel_delivery::resolve_deliver_webhook_token() {
        builder = builder.with_endpoint_transport_publisher(
            HttpWebhookTransportPublisher::new().with_bearer_token(token),
        );
    }

    builder = workflow::attach_workflow_handler(builder, prompt_pipeline, workflow_registry);
    builder = attach_otel_to_builder(builder)?;
    let runtime = builder.with_tool(MockWebSearchTool)?.build().await?;

    channel_delivery::seed_internal_outbox_endpoint_for_runtime(
        &runtime,
        Some(in_memory_endpoint_store),
        config.deliver_webhook_url,
    )
    .await?;

    Ok(runtime)
}

pub struct LocalStasisWireConfig<'a> {
    pub backend: RuntimeBackend,
    pub provider: Option<&'a str>,
    pub model: Option<&'a str>,
    pub base_url: Option<&'a str>,
}

/// Build a local/TUI Stasis composition (no daemon delivery endpoints, single DB connect on surreal).
pub async fn build_local_stasis_composition(
    config: LocalStasisWireConfig<'_>,
) -> anyhow::Result<(RuntimeComposition, MemoryAdapterBundle)> {
    match &config.backend {
        RuntimeBackend::InMemory => {
            let memory = MemoryAdapterBundle::build_in_memory().await?;
            let composition = build_in_memory_local_composition(&config, &memory).await?;
            Ok((composition, memory))
        }
        RuntimeBackend::SurrealKv { .. }
        | RuntimeBackend::SurrealWs { .. }
        | RuntimeBackend::SurrealMem { .. } => {
            crate::ensure_runtime_backend_prerequisites(&config.backend)?;
            let shell = RuntimeFactory::build(config.backend.clone()).await?;
            let memory = MemoryAdapterBundle::from_runtime_shell(&shell).await?;
            let composition = wire_local_stasis_composition(shell, &config, &memory).await?;
            Ok((composition, memory))
        }
    }
}

async fn build_in_memory_local_composition(
    config: &LocalStasisWireConfig<'_>,
    memory: &MemoryAdapterBundle,
) -> anyhow::Result<RuntimeComposition> {
    let provider = crate::resolve_llm_provider(config.provider);
    let model = crate::resolve_llm_model(config.model);
    let base_url = crate::resolve_llm_base_url(Some(&provider), config.base_url);
    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&provider),
            &model,
            base_url.as_deref(),
        ),
    );

    let workflow_registry = workflow::shared_workflow_registry();
    let prompt_pipeline = PromptExecutionPipeline::new(chat_client.clone());

    let builder = workflow::attach_workflow_handler(
        StasisRuntimeBuilder::new(config.backend.clone())
            .with_chat_client(chat_client)
            .with_memory_context_reader(memory.memory_reader.clone())
            .with_memory_context_writer(memory.memory_writer.clone())
            .with_memory_operations(memory.memory_operations.clone())
            .with_identity_memory_store(memory.identity_store.clone())
            .with_locus_memory(),
        prompt_pipeline,
        workflow_registry,
    );
    let runtime = attach_otel_to_builder(builder)?.build().await?;

    Ok(runtime)
}

async fn wire_local_stasis_composition(
    runtime: RuntimeComposition,
    config: &LocalStasisWireConfig<'_>,
    memory: &MemoryAdapterBundle,
) -> anyhow::Result<RuntimeComposition> {
    let provider = crate::resolve_llm_provider(config.provider);
    let model = crate::resolve_llm_model(config.model);
    let base_url = crate::resolve_llm_base_url(Some(&provider), config.base_url);
    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&provider),
            &model,
            base_url.as_deref(),
        ),
    );

    let workflow_registry = workflow::shared_workflow_registry();
    let prompt_pipeline = PromptExecutionPipeline::new(chat_client.clone());

    let memory_context_reader = Some(memory.memory_reader.clone());
    let memory_context_writer = Some(memory.memory_writer.clone());
    let memory_operations = Some(memory.memory_operations.clone());
    let identity_memory_store = Some(memory.identity_store.clone());
    let workflow_engine = RuntimeFactory::default_workflow_engine();
    let tool_registry = Arc::new(
        stasis::application::orchestration::tool_registry::InMemoryToolRegistry::default(),
    );

    match runtime {
        RuntimeComposition::InMemory(rt) => {
            let thread_store = RuntimeFactory::resolve_thread_store(
                &RuntimeComposition::InMemory(rt.clone()),
                None,
            );
            let cluster_store = RuntimeFactory::resolve_cluster_node_store(
                &RuntimeComposition::InMemory(rt.clone()),
                None,
            );
            register_daemon_handlers(
                &rt,
                &chat_client,
                &tool_registry,
                &workflow_engine,
                &memory_context_reader,
                &memory_context_writer,
                &identity_memory_store,
                &memory_operations,
                &thread_store,
                &cluster_store,
            )?;
            workflow::register_workflow_job_handlers(
                &rt,
                workflow_registry.clone(),
                prompt_pipeline.clone(),
            )?;
            Ok(RuntimeComposition::InMemory(rt))
        }
        RuntimeComposition::Surreal(rt) => {
            let thread_store = RuntimeFactory::resolve_thread_store(
                &RuntimeComposition::Surreal(rt.clone()),
                None,
            );
            let cluster_store = RuntimeFactory::resolve_cluster_node_store(
                &RuntimeComposition::Surreal(rt.clone()),
                None,
            );
            register_daemon_handlers(
                &rt,
                &chat_client,
                &tool_registry,
                &workflow_engine,
                &memory_context_reader,
                &memory_context_writer,
                &identity_memory_store,
                &memory_operations,
                &thread_store,
                &cluster_store,
            )?;
            workflow::register_workflow_job_handlers(
                &rt,
                workflow_registry.clone(),
                prompt_pipeline.clone(),
            )?;
            Ok(RuntimeComposition::Surreal(rt))
        }
    }
}

async fn wire_existing_daemon_composition(
    runtime: RuntimeComposition,
    config: &DaemonStasisWireConfig<'_>,
    memory: &MemoryAdapterBundle,
) -> anyhow::Result<RuntimeComposition> {
    let provider = crate::resolve_llm_provider(config.provider);
    let model = crate::resolve_llm_model(config.model);
    let base_url = crate::resolve_llm_base_url(Some(&provider), config.base_url);
    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&provider),
            &model,
            base_url.as_deref(),
        ),
    );

    let workflow_registry = workflow::shared_workflow_registry();
    let prompt_pipeline = PromptExecutionPipeline::new(chat_client.clone());

    let memory_context_reader = Some(memory.memory_reader.clone());
    let memory_context_writer = Some(memory.memory_writer.clone());
    let memory_operations = Some(memory.memory_operations.clone());
    let identity_memory_store = Some(memory.identity_store.clone());
    let workflow_engine = RuntimeFactory::default_workflow_engine();
    let tool_registry = Arc::new(stasis::application::orchestration::tool_registry::InMemoryToolRegistry::default());
    tool_registry.register_tool(MockWebSearchTool)?;

    let endpoint_transports = if let Some(token) = channel_delivery::resolve_deliver_webhook_token() {
        vec![Arc::new(
            HttpWebhookTransportPublisher::new().with_bearer_token(token),
        ) as Arc<dyn stasis::ports::outbound::runtime::endpoint_transport_publisher::EndpointTransportPublisher>]
    } else {
        Vec::new()
    };

    match runtime {
        RuntimeComposition::InMemory(rt) => {
            let thread_store = RuntimeFactory::resolve_thread_store(
                &RuntimeComposition::InMemory(rt.clone()),
                None,
            );
            let cluster_store = RuntimeFactory::resolve_cluster_node_store(
                &RuntimeComposition::InMemory(rt.clone()),
                None,
            );
            let endpoint_store = RuntimeFactory::resolve_delivery_endpoint_store(
                &RuntimeComposition::InMemory(rt.clone()),
                None,
            );
            let status_store = RuntimeFactory::resolve_endpoint_delivery_status_store(
                &RuntimeComposition::InMemory(rt.clone()),
                None,
            );
            let routing_publisher = RuntimeFactory::build_endpoint_routing_publisher(
                endpoint_store,
                status_store,
                &endpoint_transports,
                None,
            );
            rt.register_event_publisher(routing_publisher)?;
            register_daemon_handlers(
                &rt,
                &chat_client,
                &tool_registry,
                &workflow_engine,
                &memory_context_reader,
                &memory_context_writer,
                &identity_memory_store,
                &memory_operations,
                &thread_store,
                &cluster_store,
            )?;
            workflow::register_workflow_job_handlers(
                &rt,
                workflow_registry.clone(),
                prompt_pipeline.clone(),
            )?;
            Ok(RuntimeComposition::InMemory(rt))
        }
        RuntimeComposition::Surreal(rt) => {
            let thread_store = RuntimeFactory::resolve_thread_store(
                &RuntimeComposition::Surreal(rt.clone()),
                None,
            );
            let cluster_store = RuntimeFactory::resolve_cluster_node_store(
                &RuntimeComposition::Surreal(rt.clone()),
                None,
            );
            let endpoint_store = RuntimeFactory::resolve_delivery_endpoint_store(
                &RuntimeComposition::Surreal(rt.clone()),
                None,
            );
            let status_store = RuntimeFactory::resolve_endpoint_delivery_status_store(
                &RuntimeComposition::Surreal(rt.clone()),
                None,
            );
            let routing_publisher = RuntimeFactory::build_endpoint_routing_publisher(
                endpoint_store.clone(),
                status_store,
                &endpoint_transports,
                None,
            );
            rt.register_event_publisher(routing_publisher)?;
            register_daemon_handlers(
                &rt,
                &chat_client,
                &tool_registry,
                &workflow_engine,
                &memory_context_reader,
                &memory_context_writer,
                &identity_memory_store,
                &memory_operations,
                &thread_store,
                &cluster_store,
            )?;
            workflow::register_workflow_job_handlers(
                &rt,
                workflow_registry.clone(),
                prompt_pipeline.clone(),
            )?;
            let composition = RuntimeComposition::Surreal(rt);
            channel_delivery::seed_internal_outbox_endpoint_for_runtime(
                &composition,
                None,
                config.deliver_webhook_url,
            )
            .await?;
            Ok(composition)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn register_daemon_handlers<R>(
    rt: &R,
    chat_client: &Arc<dyn AiChatClient>,
    tool_registry: &Arc<stasis::application::orchestration::tool_registry::InMemoryToolRegistry>,
    workflow_engine: &Arc<dyn stasis::ports::outbound::runtime::workflow_engine::WorkflowEngine>,
    memory_context_reader: &Option<Arc<dyn stasis::ports::outbound::memory::memory_context_reader::MemoryContextReader>>,
    memory_context_writer: &Option<Arc<dyn stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter>>,
    identity_memory_store: &Option<Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>>,
    memory_operations: &Option<Arc<dyn stasis::ports::outbound::memory::memory_operations::MemoryOperations>>,
    thread_store: &Arc<dyn stasis::ports::outbound::runtime::thread_store::ThreadStore>,
    cluster_store: &Arc<dyn stasis::ports::outbound::runtime::cluster_node_store::ClusterNodeStore>,
) -> stasis::prelude::Result<()>
where
    R: DaemonRuntimeRegistrar,
{
    rt.register_handler(GraphemeJobHandler::new(workflow_engine.clone()))?;
    rt.register_handler(GraphemeHealthcheckJobHandler::new(workflow_engine.clone()))?;
    rt.register_handler(GraphemeEchoJobHandler::new(workflow_engine.clone()))?;
    rt.register_handler(GraphemeTextOpsJobHandler::new(workflow_engine.clone()))?;

    rt.register_handler(PromptChatJobHandler::new_with_memory_and_identity(
        chat_client.clone(),
        memory_context_reader.clone(),
        memory_context_writer.clone(),
        identity_memory_store.clone(),
    ))?;

    rt.register_handler(ToolLoopJobHandler::new_with_memory_and_identity(
        chat_client.clone(),
        tool_registry.clone(),
        memory_context_reader.clone(),
        memory_context_writer.clone(),
        identity_memory_store.clone(),
    ))?;

    rt.register_handler(AgentTurnJobHandler::new_with_memory_and_identity(
        chat_client.clone(),
        tool_registry.clone(),
        memory_context_reader.clone(),
        memory_context_writer.clone(),
        identity_memory_store.clone(),
    ))?;
    rt.register_handler(AgentSessionJobHandler::new_with_memory_and_identity(
        chat_client.clone(),
        tool_registry.clone(),
        memory_context_reader.clone(),
        memory_context_writer.clone(),
        identity_memory_store.clone(),
    ))?;

    if let Some(reader) = memory_context_reader.clone() {
        rt.register_handler(MemoryRecallJobHandler::new(reader))?;
    }
    if let Some(operations) = memory_operations.clone() {
        rt.register_handler(MemoryAggregateJobHandler::new(operations.clone()))?;
        rt.register_handler(MemoryTransformJobHandler::new(operations.clone()))?;
        rt.register_handler(MemoryRollupJobHandler::new(operations.clone()))?;
        rt.register_handler(MemorySchemaJobHandler::new(operations.clone()))?;
    }

    rt.register_handler(ConcurrentPatternJobHandler::new_with_thread_store(
        chat_client.clone(),
        Some(thread_store.clone()),
    ))?;
    rt.register_handler(HandoffPatternJobHandler::new_with_thread_store(
        chat_client.clone(),
        Some(thread_store.clone()),
    ))?;
    rt.register_handler(OrchestratorPatternJobHandler::new_with_thread_store(
        chat_client.clone(),
        Some(thread_store.clone()),
    ))?;
    rt.register_handler(SequentialPatternJobHandler::new_with_thread_store(
        chat_client.clone(),
        Some(thread_store.clone()),
    ))?;

    rt.register_handler(CoordinatorFailoverJobHandler::new(cluster_store.clone()))?;
    rt.register_handler(QueueOwnershipRebalanceJobHandler::new(cluster_store.clone()))?;

    Ok(())
}

trait DaemonRuntimeRegistrar {
    fn register_handler<H: stasis::application::runtime::in_memory_runtime::JobHandler + 'static>(
        &self,
        handler: H,
    ) -> stasis::prelude::Result<()>;

    fn register_event_publisher(
        &self,
        publisher: stasis::infrastructure::runtime::endpoint_routing_event_publisher::EndpointRoutingEventPublisher,
    ) -> stasis::prelude::Result<()>;
}

impl DaemonRuntimeRegistrar for stasis::application::runtime::in_memory_runtime::InMemoryRuntime {
    fn register_handler<H: stasis::application::runtime::in_memory_runtime::JobHandler + 'static>(
        &self,
        handler: H,
    ) -> stasis::prelude::Result<()> {
        self.register_handler(handler)
    }

    fn register_event_publisher(
        &self,
        publisher: stasis::infrastructure::runtime::endpoint_routing_event_publisher::EndpointRoutingEventPublisher,
    ) -> stasis::prelude::Result<()> {
        self.register_event_publisher(publisher)
    }
}

impl DaemonRuntimeRegistrar for stasis::application::runtime::surreal_runtime::SurrealRuntime {
    fn register_handler<H: stasis::application::runtime::in_memory_runtime::JobHandler + 'static>(
        &self,
        handler: H,
    ) -> stasis::prelude::Result<()> {
        self.register_handler(handler)
    }

    fn register_event_publisher(
        &self,
        publisher: stasis::infrastructure::runtime::endpoint_routing_event_publisher::EndpointRoutingEventPublisher,
    ) -> stasis::prelude::Result<()> {
        self.register_event_publisher(publisher)
    }
}
