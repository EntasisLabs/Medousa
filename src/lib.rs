pub mod adapter_ingest;
pub mod agent_runtime;
pub mod channel_delivery;
pub mod artifact_chunking;
pub mod artifact_command_runtime;
pub mod artifact_extraction;
pub mod artifact_store;
pub mod context_pack;
pub mod daemon_api;
pub mod daemon_handlers;
pub mod engine_context;
pub mod events;
pub mod grapheme_sttp_compaction;
pub mod identity_memory;
pub mod adapter_heartbeat;
pub mod product_config;
pub mod ingest_stream;
pub mod interactive_turn_runtime;
pub mod payload_receipt;
pub mod runtime_config_command_runtime;
pub mod session;
pub mod session_mapping;
pub mod session_store;
pub mod settings_guard;
pub mod stage_route_command_runtime;
pub mod stage_routing;
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
    DeliverPollResponse, DeliveryHealthResponse,
    InteractiveTurnRequest, InteractiveTurnResponse, InteractiveTurnStreamEvent,
    IdentityContextRequest, JobCitationResponse, JobEvidenceReportResponse,
    JobReportResponse, RegisterRecurringPromptRequest, JobResultResponse,
    RuntimeConfigCommandRequest, RuntimeConfigCommandResponse, RuntimeConfigCommandSpec,
    RuntimeVerifyPolicyState,
    SessionAppendTurnRequest, SessionAppendTurnResponse, SessionHistoryListRequest,
    SessionHistoryListResponse, SessionHistoryResponse,
    RegisterRecurringResponse, StageRouteCommandRequest, StageRouteCommandResponse,
    StageRouteCommandSpec, resolve_daemon_url,
};
pub use product_config::{
    ProductConfig, load_product_config, save_product_config, ingest_sender_allowed,
    apply_adapter_env, apply_daemon_env, parse_u64_csv, parse_i64_csv, format_u64_csv, format_i64_csv,
    migrate_from_onboard_profile,
};
pub use ingest_stream::{build_ingest_stream_url, consume_ingest_stream, render_stream_body};
pub use adapter_ingest::{
    AdapterDeliveryOutcome, default_delivery_timeout, fetch_job_result, format_ingest_ack,
    wait_for_ask_delivery, ADAPTER_COMMAND_HINT,
};
pub use agent_runtime::{
    AgentStreamEvent, AgentTurnRequest, MedousaAgentRuntime, build_agent_runtime,
    build_daemon_agent_runtime, run_daemon_interactive_turn,
};
pub use tools::{TuiRuntime, build_tui_runtime};

const DEFAULT_LLM_MODEL: &str = "gpt-4o-mini";
const DEFAULT_LLM_PROVIDER: &str = "openai";
const DEFAULT_SURREAL_NAMESPACE: &str = "medousa";
const DEFAULT_SURREAL_DATABASE: &str = "runtime";
const DEFAULT_SURREALKV_FILENAME: &str = "runtime.surrealkv";

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
        .map(|value| value.to_string())
        .or_else(|| std::env::var("MEDOUSA_LLM_MODEL").ok())
        .or_else(|| std::env::var("STASIS_LLM_MODEL").ok())
        .unwrap_or_else(|| DEFAULT_LLM_MODEL.to_string())
}

pub fn resolve_llm_provider(explicit_provider: Option<&str>) -> String {
    explicit_provider
        .map(|value| value.to_string())
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
    ensure_runtime_backend_prerequisites(&backend)?;

    let provider = resolve_llm_provider(explicit_provider);
    let model = resolve_llm_model(explicit_model);
    let base_url = resolve_llm_base_url(Some(&provider), explicit_base_url);
    let chat_client = Arc::new(GenaiChatClient::from_provider_model_with_base_url(
        Some(&provider),
        &model,
        base_url.as_deref(),
    ));

    let mut builder = StasisRuntimeBuilder::new(backend)
        .with_chat_client(chat_client)
        .with_locus_memory();

    if let Some(store) = identity_memory_store {
        builder = builder.with_identity_memory_store(store);
    }

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
    ensure_runtime_backend_prerequisites(&backend)?;

    let provider = resolve_llm_provider(explicit_provider);
    let model = resolve_llm_model(explicit_model);
    let base_url = resolve_llm_base_url(Some(&provider), explicit_base_url);
    let chat_client = Arc::new(GenaiChatClient::from_provider_model_with_base_url(
        Some(&provider),
        &model,
        base_url.as_deref(),
    ));

    let in_memory_endpoint_store = if matches!(backend, RuntimeBackend::InMemory) {
        Some(Arc::new(InMemoryDeliveryEndpointStore::default())
            as Arc<dyn DeliveryEndpointStore>)
    } else {
        None
    };

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
    if raw.eq_ignore_ascii_case("surreal-mem") {
        return RuntimeBackend::SurrealMem {
            namespace: resolve_surreal_namespace(),
            database: resolve_surreal_database(),
        };
    }

    if raw.eq_ignore_ascii_case("surreal-kv") || raw.starts_with("surreal-kv:") {
        return parse_surreal_kv_backend(raw);
    }

    if raw.starts_with("surreal-ws:") {
        return parse_surreal_ws_backend(raw);
    }

    RuntimeBackend::InMemory
}

fn resolve_surreal_namespace() -> String {
    std::env::var("MEDOUSA_SURREAL_NAMESPACE")
        .ok()
        .or_else(|| std::env::var("STASIS_SURREAL_NAMESPACE").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SURREAL_NAMESPACE.to_string())
}

fn resolve_surreal_database() -> String {
    std::env::var("MEDOUSA_SURREAL_DATABASE")
        .ok()
        .or_else(|| std::env::var("STASIS_SURREAL_DATABASE").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SURREAL_DATABASE.to_string())
}

fn parse_surreal_kv_backend(raw: &str) -> RuntimeBackend {
    let explicit_path = raw
        .strip_prefix("surreal-kv:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    let path = explicit_path
        .or_else(|| std::env::var("MEDOUSA_SURREALKV_PATH").ok())
        .or_else(|| std::env::var("STASIS_SURREALKV_PATH").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(default_surrealkv_path);

    RuntimeBackend::SurrealKv {
        path,
        namespace: resolve_surreal_namespace(),
        database: resolve_surreal_database(),
    }
}

fn parse_surreal_ws_backend(raw: &str) -> RuntimeBackend {
    let endpoint = raw
        .strip_prefix("surreal-ws:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("ws://127.0.0.1:8000/rpc")
        .to_string();

    RuntimeBackend::SurrealWs {
        endpoint,
        namespace: resolve_surreal_namespace(),
        database: resolve_surreal_database(),
    }
}

fn default_surrealkv_path() -> String {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("medousa")
        .join(DEFAULT_SURREALKV_FILENAME)
        .to_string_lossy()
        .to_string()
}

fn ensure_runtime_backend_prerequisites(backend: &RuntimeBackend) -> Result<()> {
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

        // Remove stale SurrealKV lock file. If another daemon is actually
        // running, it will fail on port bind — not on a leftover LOCK file.
        let lock_path = path_buf.join("LOCK");
        if lock_path.exists() {
            if let Err(err) = std::fs::remove_file(&lock_path) {
                eprintln!(
                    "warning: failed to remove stale SurrealKV lock file {}: {}",
                    lock_path.display(),
                    err
                );
            }
        }
    }

    Ok(())
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
