pub mod adapter_ingest;
pub mod agent_runtime;
pub mod channel_delivery;
pub mod recurring_delivery;
pub mod recurring_agent_turn;
pub mod recurring_handlers;
pub mod artifact_chunking;
pub mod artifact_command_runtime;
pub mod artifact_extraction;
pub mod artifact_store;
pub mod media_handlers;
pub mod media_store;
pub mod media_text_extract;
pub mod media_vision;
pub mod model_capability_registry;
pub mod channel_session_store;
pub mod turn_continuation;
pub mod turn_parts;
pub mod turn_slice;
pub mod tool_bootstrap;
pub mod tool_bootstrap_tools;
pub mod tool_history_tools;
pub mod tool_history_index;
pub mod tool_history_handlers;
pub mod grapheme_script;
pub mod grapheme_script_tools;
pub mod grapheme_handlers;
pub mod grapheme_lsp_bridge;
pub mod grapheme_medousa_bridge;
pub mod grapheme_workshop;
pub mod learning_artifacts;
pub mod manuscript_overlay_tools;
pub mod turn_budget_request;
pub mod turn_budget_handlers;
pub mod turn_budget_notify;
pub mod turn_worker_notify;
pub mod turn_control_tools;
pub mod turn_text_heuristics;
pub mod context_pack;
pub mod capability_catalog;
pub mod mcp_daemon_handlers;
pub mod mcp_gateway;
pub mod openshell_handoff;
pub mod openshell_sandbox_run;
pub mod openshell_tools;
pub mod mcp_gateway_client;
pub mod mcp_gateway_api;
pub mod mcp_turn_token;
pub mod mcp_policy;
pub mod daemon_api;
pub mod daemon_handlers;
pub mod vault;
pub mod vault_handlers;
pub mod vault_tools;
pub mod workspace;
pub mod workspace_handlers;
pub mod engine_context;
pub mod events;
pub mod execution_policy;
pub mod medousa_tool_loop;
pub mod grapheme_sttp_compaction;
pub mod identity_markdown;
pub mod identity_manuscript;
pub mod manuscript_handlers;
pub mod skill_import;
pub mod skill_execution;
pub mod skill_ingest;
pub mod skill_tools;
pub mod manuscript_tools;
pub mod cognitive_identity;
pub mod cognitive_identity_writer;
pub mod identity_memory;
pub mod identity_store_ext;
pub mod identity_tools;
pub mod identity_write_policy;
pub mod inference_profiles;
pub mod inference_profiles_handlers;
pub mod inference_router;
pub mod stt;
pub mod stt_handlers;
pub mod user_profiles;
pub mod profile_portability;
pub mod locus_handlers;
pub mod locus_memory;
pub mod local_inference;
pub mod local_inference_cli;
pub mod local_inference_handlers;
pub mod memory_tools;
pub mod tool_aliases;
pub mod tool_names;
pub mod adapter_heartbeat;
pub mod paths;
pub mod product_config;
pub mod ingest_stream;
pub mod interactive_turn_runtime;
pub mod pairing;
pub mod pairing_handlers;
pub mod iroh_transport;
pub mod payload_receipt;
pub mod runtime_config_command_runtime;
pub mod reasoning_effort;
pub mod session;
pub mod session_catalog;
pub mod session_active_turn;
pub mod turn_failure;
pub mod turn_ticket;
pub mod service_launch;
pub mod session_mapping;
pub mod session_store;
pub mod session_lifecycle;
pub mod session_retention;
pub mod locus_semantic_tags;
pub mod session_meta_store;
pub mod surreal_config;
pub mod settings_guard;
pub mod stage_route_command_runtime;
pub mod stage_routing;
pub mod bridge_tools;
pub mod runtime;
pub mod runtime_tools;
pub mod workflow_handlers;
pub mod workflow;
pub mod workflow_plan;
pub mod workshop_env;
pub mod tools;
pub mod tui;
pub mod verification_store;
pub mod verifier;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::infrastructure::runtime::http_webhook_event_publisher::HttpWebhookTransportPublisher;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::ports::outbound::runtime::delivery_endpoint_store::DeliveryEndpointStore;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, StasisRuntimeBuilder};
use stasis::runtime_prelude_ext::InMemoryDeliveryEndpointStore;

pub use daemon_api::{
    ArtifactCommandRequest, ArtifactCommandResponse, ArtifactCommandSpec,
    ArtifactVerificationPolicyInput, DaemonStatsResponse, EnqueueAskRequest,
    EnqueuePromptRequest, EnqueueReportRequest, EnqueueResponse,
    HealthResponse, HeartbeatDeliveryMetricsResponse, HeartbeatDeliveryPolicyResponse,
    HeartbeatPolicyResponse, HeartbeatStatusResponse,
    IngestRequest, IngestResponse, IngestAttachment,
    DeliverPollResponse, DeliveryHealthResponse, ContinuationStatusResponse,
    TurnContinuationLineageResponse, ReplayAndResumeResponse,
    InteractiveTurnRequest, InteractiveTurnResponse, InteractiveTurnStreamEvent,
    MediaRef, MediaUploadResponse, TurnSurfaceContext,
    IdentityContextRequest, JobCitationResponse, JobEvidenceReportResponse,
    ArchiveAskJobRequest, ArchiveAskJobResponse, AskJobCompleteActionsRequest,
    AskJobCompleteActionsResponse, JobReportResponse, RegisterRecurringPromptRequest,
    JobResultResponse,
    RuntimeConfigCommandRequest, RuntimeConfigCommandResponse, RuntimeConfigCommandSpec,
    RuntimeVerifyPolicyState,
    SessionAppendTurnRequest, SessionAppendTurnResponse, SessionHistoryListRequest,
    SessionHistoryListResponse, SessionHistoryResponse, SessionSetDisplayNameRequest,
    SessionSetDisplayNameResponse,
    RegisterRecurringResponse, StageRouteCommandRequest, StageRouteCommandResponse,
    StageRouteCommandSpec, resolve_daemon_url,
};
pub use capability_catalog::{
    CapabilityBinding, CapabilityDefinition, CapabilityImplementations, CapabilityListEntry,
    CapabilityListResponse, CapabilityManifest, CapabilityManifestBindings,
    CapabilityManifestEntry, CapabilityRecommendation, CapabilityRegistry,
    CapabilityResolveRequest, CapabilityResolveResponse, CapabilitySearchMatch,
    CapabilitySearchRequest, CapabilitySearchResponse, CapabilitySource, GraphemeCapabilityBindingSpec,
    McpCapabilityBindingSpec, McpCatalogSyncEntry, McpCatalogSyncResponse,
    CapabilityReindexResponse, capabilities_manifest_path, embedded_capability_manifest,
    load_capability_manifest,
};
pub use mcp_gateway_api::{
    McpAdminStatusResponse, McpDiscoverRequest, McpDiscoverResponse, McpEffectClass,
    McpGatewayHealthResponse, McpInvokeError, McpInvokeRequest, McpInvokeResponse,
    McpPolicyDecision, McpPolicyEvaluateRequest, McpPolicyEvaluateResponse, McpServerSummary,
    McpServersResponse, McpToolCatalogEntry, McpTurnContext, McpTurnLane,
    resolve_mcp_gateway_url, DEFAULT_MCP_GATEWAY_BIND, DEFAULT_MCP_GATEWAY_URL,
};
pub use product_config::{
    ProductConfig, load_product_config, save_product_config, ingest_sender_allowed,
    apply_adapter_env, apply_daemon_env, apply_surreal_env, apply_surreal_env_from_fields,
    parse_u64_csv, parse_i64_csv, format_u64_csv, format_i64_csv, SurrealProductConfig,
    migrate_from_onboard_profile,
};
pub use surreal_config::{
    resolve_daemon_launch_backend, resolve_surreal_connection_settings, sync_profile_daemon_backend,
};
pub use ingest_stream::{build_ingest_stream_url, consume_ingest_stream, render_stream_body};
pub use workshop_env::apply_workshop_llm_env;
pub use adapter_ingest::{
    AdapterDeliveryOutcome, default_delivery_timeout, fetch_job_result, format_ingest_ack,
    should_send_immediate_ingest_reply, wait_for_ask_delivery, ADAPTER_COMMAND_HINT,
};
pub use agent_runtime::{
    AgentStreamEvent, AgentTurnRequest, MedousaAgentRuntime, build_agent_runtime,
    build_daemon_agent_runtime, run_agent_turn, run_daemon_interactive_turn,
};
pub use mcp_gateway_client::{McpGatewayClient, gateway_auth_configured};
pub use mcp_gateway::{
    gateway_config_path, install_starter_gateway_config_if_missing, STARTER_MCP_GATEWAY_TOML,
};
pub use openshell_handoff::{
    collect_openshell_doctor_report, install_starter_openshell_policies_if_missing,
    resolve_openshell_gateway_url, DEFAULT_OPENSHELL_GATEWAY_URL, ENV_OPENSHELL_GATEWAY_URL,
};
pub use openshell_sandbox_run::{
    register_openshell_sandbox_run_handler, OpenshellSandboxRunPayload,
    OPENSHELL_SANDBOX_RUN_JOB_TYPE,
};
pub use runtime::{
    MedousaPlatformRuntime, PlatformBuildConfig, TuiPlatformBuildConfig, TuiPlatformMode,
    build_daemon_platform, build_medousa_platform, build_tui_platform, is_daemon_bind_reachable,
    resolve_tui_platform_mode,
};
pub use tools::{TuiRuntime, build_tui_runtime};

const DEFAULT_LLM_MODEL: &str = "gpt-4o-mini";
const DEFAULT_LLM_PROVIDER: &str = "openai";
const DEFAULT_SURREALKV_FILENAME: &str = "runtime.surrealkv";
pub const DEFAULT_MEDOUSA_LOCAL_BASE_URL: &str = "http://127.0.0.1:7421/v1";

fn provider_base_url_env_keys(provider: &str) -> (String, String) {
    let normalized = provider.trim().to_ascii_uppercase().replace('-', "_");
    (
        format!("MEDOUSA_{normalized}_BASE_URL"),
        format!("STASIS_{normalized}_BASE_URL"),
    )
}

struct MockWebSearchTool;

#[async_trait]
impl StasisTool for MockWebSearchTool {
    fn name(&self) -> &'static str {
        "stasis.web.search.mock"
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let query = input
            .get("query")
            .and_then(|value| value.as_str())
            .unwrap_or("general research")
            .to_string();

        Ok(json!({
            "query": query,
            "results": [
                {
                    "title": "Rust ecosystem trends",
                    "snippet": "Growing adoption in platform tooling and backend services.",
                    "source": "mock://rust-trends-1"
                },
                {
                    "title": "Async Rust in production",
                    "snippet": "Tokio-based workloads continue to increase in operational maturity.",
                    "source": "mock://rust-trends-2"
                },
                {
                    "title": "AI infrastructure in Rust",
                    "snippet": "Teams are exploring Rust for inference gateways and orchestration services.",
                    "source": "mock://rust-trends-3"
                }
            ]
        }))
    }
}

pub fn resolve_llm_model(explicit_model: Option<&str>) -> String {
    explicit_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var("MEDOUSA_LLM_MODEL").ok())
        .or_else(|| std::env::var("STASIS_LLM_MODEL").ok())
        .unwrap_or_else(|| DEFAULT_LLM_MODEL.to_string())
}

pub fn resolve_llm_provider(explicit_provider: Option<&str>) -> String {
    explicit_provider
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var("MEDOUSA_LLM_PROVIDER").ok())
        .or_else(|| std::env::var("STASIS_LLM_PROVIDER").ok())
        .unwrap_or_else(|| DEFAULT_LLM_PROVIDER.to_string())
}

pub fn resolve_llm_target(explicit_provider: Option<&str>, explicit_model: Option<&str>) -> String {
    let provider = resolve_llm_provider(explicit_provider);
    let model = resolve_llm_model(explicit_model);
    GenaiChatClient::build_model_target(Some(&provider), &model)
}

pub fn resolve_llm_base_url(
    explicit_provider: Option<&str>,
    explicit_base_url: Option<&str>,
) -> Option<String> {
    if let Some(explicit) = explicit_base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(explicit.to_string());
    }

    let provider = resolve_llm_provider(explicit_provider);
    let (medousa_provider_key, stasis_provider_key) = provider_base_url_env_keys(&provider);

    if provider.eq_ignore_ascii_case("medousa-local") {
        return std::env::var(&medousa_provider_key)
            .ok()
            .or_else(|| std::env::var(&stasis_provider_key).ok())
            .or_else(|| std::env::var("MEDOUSA_LOCAL_ENGINE_BASE_URL").ok())
            .or_else(|| Some(DEFAULT_MEDOUSA_LOCAL_BASE_URL.to_string()));
    }

    std::env::var(&medousa_provider_key)
        .ok()
        .or_else(|| std::env::var(&stasis_provider_key).ok())
        .or_else(|| {
            // Honour the standard Ollama env var so users don't need a separate flag.
            if provider.eq_ignore_ascii_case("ollama") {
                std::env::var("OLLAMA_HOST").ok()
            } else {
                None
            }
        })
        .or_else(|| std::env::var("MEDOUSA_LLM_BASE_URL").ok())
        .or_else(|| std::env::var("STASIS_LLM_BASE_URL").ok())
}

pub async fn build_runtime(
    backend: RuntimeBackend,
    explicit_provider: Option<&str>,
    explicit_model: Option<&str>,
    explicit_base_url: Option<&str>,
) -> Result<RuntimeComposition> {
    ensure_runtime_backend_prerequisites(&backend)?;
    let identity_memory_store =
        identity_memory::build_identity_memory_store_for_backend(&backend).await?;
    build_runtime_with_identity_store(
        backend,
        explicit_provider,
        explicit_model,
        explicit_base_url,
        Some(identity_memory_store),
    )
    .await
}

pub async fn build_runtime_with_identity_store(
    backend: RuntimeBackend,
    explicit_provider: Option<&str>,
    explicit_model: Option<&str>,
    explicit_base_url: Option<&str>,
    identity_memory_store: Option<Arc<dyn IdentityMemoryStore>>,
) -> Result<RuntimeComposition> {
    runtime::stasis_otel::prepare_stasis_otel_from_tui_defaults();
    apply_workshop_llm_env();
    ensure_runtime_backend_prerequisites(&backend)?;

    let provider = resolve_llm_provider(explicit_provider);
    let model = resolve_llm_model(explicit_model);
    let base_url = resolve_llm_base_url(Some(&provider), explicit_base_url);
    let chat_client = Arc::new(GenaiChatClient::from_provider_model_with_base_url(
        Some(&provider),
        &model,
        base_url.as_deref(),
    ));

    grapheme_medousa_bridge::init_medousa_bridge(grapheme_medousa_bridge::MedousaBridgeDeps {
        chat_client: chat_client.clone(),
        identity_store: identity_memory_store.clone(),
        memory_writer: None,
    });

    let workflow_registry = workflow::shared_workflow_registry();
    let prompt_pipeline = PromptExecutionPipeline::new(chat_client.clone());

    let mut builder = StasisRuntimeBuilder::new(backend)
        .with_chat_client(chat_client)
        .with_locus_memory();

    if let Some(store) = identity_memory_store {
        builder = builder.with_identity_memory_store(store);
    }

    builder = workflow::attach_workflow_handler(builder, prompt_pipeline, workflow_registry);
    builder = runtime::stasis_otel::attach_otel_to_builder(builder)?;

    let runtime = builder.with_tool(MockWebSearchTool)?.build().await?;

    Ok(runtime)
}

/// Daemon runtime with Stasis outbox endpoint routing enabled and internal webhook seeded.
pub async fn build_daemon_runtime(
    backend: RuntimeBackend,
    explicit_provider: Option<&str>,
    explicit_model: Option<&str>,
    explicit_base_url: Option<&str>,
    identity_memory_store: Option<Arc<dyn IdentityMemoryStore>>,
    deliver_webhook_url: &str,
) -> Result<RuntimeComposition> {
    runtime::stasis_otel::prepare_stasis_otel_from_tui_defaults();
    apply_workshop_llm_env();
    ensure_runtime_backend_prerequisites(&backend)?;

    let provider = resolve_llm_provider(explicit_provider);
    let model = resolve_llm_model(explicit_model);
    let base_url = resolve_llm_base_url(Some(&provider), explicit_base_url);
    let chat_client = Arc::new(GenaiChatClient::from_provider_model_with_base_url(
        Some(&provider),
        &model,
        base_url.as_deref(),
    ));

    grapheme_medousa_bridge::init_medousa_bridge(grapheme_medousa_bridge::MedousaBridgeDeps {
        chat_client: chat_client.clone(),
        identity_store: identity_memory_store.clone(),
        memory_writer: None,
    });

    let in_memory_endpoint_store = if matches!(backend, RuntimeBackend::InMemory) {
        Some(Arc::new(InMemoryDeliveryEndpointStore::default())
            as Arc<dyn DeliveryEndpointStore>)
    } else {
        None
    };

    let workflow_registry = workflow::shared_workflow_registry();
    let prompt_pipeline = PromptExecutionPipeline::new(chat_client.clone());

    let mut builder = StasisRuntimeBuilder::new(backend)
        .with_chat_client(chat_client)
        .with_locus_memory()
        .with_endpoint_routing_delivery();

    if let Some(token) = channel_delivery::resolve_deliver_webhook_token() {
        builder = builder.with_endpoint_transport_publisher(
            HttpWebhookTransportPublisher::new().with_bearer_token(token),
        );
    }

    if let Some(store) = &in_memory_endpoint_store {
        builder = builder.with_delivery_endpoint_store(store.clone());
    }

    if let Some(store) = identity_memory_store {
        builder = builder.with_identity_memory_store(store);
    }

    builder = workflow::attach_workflow_handler(builder, prompt_pipeline, workflow_registry);
    builder = runtime::stasis_otel::attach_otel_to_builder(builder)?;

    let runtime = builder.with_tool(MockWebSearchTool)?.build().await?;

    channel_delivery::seed_internal_outbox_endpoint_for_runtime(
        &runtime,
        in_memory_endpoint_store,
        deliver_webhook_url,
    )
    .await?;

    Ok(runtime)
}

pub fn parse_backend(value: Option<&str>) -> RuntimeBackend {
    let raw = value.unwrap_or("in-memory").trim();
    let surreal = surreal_config::resolve_surreal_connection_settings(
        &load_product_config(),
        &session::load_tui_defaults(),
    );
    let namespace = surreal_config::resolve_surreal_namespace(&surreal);
    let database = surreal_config::resolve_surreal_database(&surreal);

    if raw.eq_ignore_ascii_case("surreal-mem") {
        return surreal_config::apply_surreal_auth_to_backend(
            RuntimeBackend::surreal_mem(namespace, database),
            &surreal,
        );
    }

    if raw.eq_ignore_ascii_case("surreal-kv") || raw.starts_with("surreal-kv:") {
        let path = parse_surreal_kv_path(raw);
        return surreal_config::apply_surreal_auth_to_backend(
            RuntimeBackend::surreal_kv(path, namespace, database),
            &surreal,
        );
    }

    if raw.starts_with("surreal-ws:") {
        let endpoint = surreal_config::resolve_surreal_ws_endpoint(raw, &surreal);
        let (endpoint, url_auth) = surreal_config::split_endpoint_userinfo(&endpoint);
        let backend = RuntimeBackend::surreal_ws(endpoint, namespace, database);
        if let Some(auth) = surreal_config::resolve_surreal_auth(&surreal).or(url_auth) {
            return backend.with_surreal_auth(auth);
        }
        return backend;
    }

    RuntimeBackend::InMemory
}

fn parse_surreal_kv_path(raw: &str) -> String {
    raw.strip_prefix("surreal-kv:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| std::env::var("MEDOUSA_SURREALKV_PATH").ok())
        .or_else(|| std::env::var("STASIS_SURREALKV_PATH").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(default_surrealkv_path)
}

fn default_surrealkv_path() -> String {
    paths::medousa_data_dir()
        .join(DEFAULT_SURREALKV_FILENAME)
        .to_string_lossy()
        .to_string()
}

pub(crate) fn ensure_runtime_backend_prerequisites(backend: &RuntimeBackend) -> Result<()> {
    if let RuntimeBackend::SurrealKv { path, .. } = backend {
        let path_buf = PathBuf::from(path);
        if let Some(parent) = path_buf.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create SurrealKV runtime directory {}",
                    parent.display()
                )
            })?;
        }

        clear_stale_surrealkv_lock(backend)?;
    }

    Ok(())
}

/// Remove a leftover SurrealKV `LOCK` file when no daemon holds the database.
pub fn clear_stale_surrealkv_lock(backend: &RuntimeBackend) -> Result<()> {
    if let RuntimeBackend::SurrealKv { path, .. } = backend {
        let lock_path = PathBuf::from(path).join("LOCK");
        if !lock_path.exists() {
            return Ok(());
        }

        std::fs::remove_file(&lock_path).with_context(|| {
            format!(
                "failed to remove stale SurrealKV lock at {} — another medousa_daemon may be running. \
                 Stop it with `pkill -x medousa_daemon`, or remove the lock manually if no daemon is running.",
                lock_path.display()
            )
        })?;
    }

    Ok(())
}

/// Path to the SurrealKV lock file for diagnostics (`None` for non-KV backends).
pub fn surrealkv_lock_path(backend: &RuntimeBackend) -> Option<PathBuf> {
    match backend {
        RuntimeBackend::SurrealKv { path, .. } => Some(PathBuf::from(path).join("LOCK")),
        _ => None,
    }
}

/// Remove the SurrealKV lock file for a given backend (used during graceful shutdown).
pub fn remove_surrealkv_lock(backend: &RuntimeBackend) {
    if let RuntimeBackend::SurrealKv { path, .. } = backend {
        let lock_path = PathBuf::from(path).join("LOCK");
        if lock_path.exists() {
            if let Err(err) = std::fs::remove_file(&lock_path) {
                eprintln!(
                    "warning: failed to remove SurrealKV lock file during shutdown {}: {}",
                    lock_path.display(),
                    err
                );
            }
        }
    }
}

pub async fn process_once(runtime: &RuntimeComposition, worker_id: &str) -> Result<Option<String>> {
    let now = Utc::now();
    let result = match runtime {
        RuntimeComposition::InMemory(rt) => rt.process_once("default", worker_id, now).await?,
        RuntimeComposition::Surreal(rt) => rt.process_once("default", worker_id, now).await?,
    };

    Ok(result)
}

pub async fn publish_pending(runtime: &RuntimeComposition, limit: usize) -> Result<usize> {
    let now = Utc::now();
    let published = match runtime {
        RuntimeComposition::InMemory(rt) => rt.publish_pending_events(limit, now).await?,
        RuntimeComposition::Surreal(rt) => rt.publish_pending_events(limit, now).await?,
    };

    Ok(published)
}
