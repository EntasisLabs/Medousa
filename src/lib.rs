pub mod artifact_chunking;
pub mod artifact_extraction;
pub mod artifact_store;
pub mod context_pack;
pub mod daemon_api;
pub mod engine_context;
pub mod events;
pub mod grapheme_sttp_compaction;
pub mod payload_receipt;
pub mod session;
pub mod settings_guard;
pub mod stage_routing;
pub mod tools;
pub mod tui;
pub mod verification_store;
pub mod verifier;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, StasisRuntimeBuilder};

pub use daemon_api::{
    DaemonStatsResponse, EnqueueAskRequest, EnqueuePromptRequest, EnqueueResponse, HealthResponse,
    RegisterRecurringPromptRequest, RegisterRecurringResponse, resolve_daemon_url,
};
pub use tools::{TuiRuntime, build_tui_runtime};

const DEFAULT_LLM_MODEL: &str = "gpt-4o-mini";
const DEFAULT_LLM_PROVIDER: &str = "openai";

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
    let provider = resolve_llm_provider(explicit_provider);
    let model = resolve_llm_model(explicit_model);
    let base_url = resolve_llm_base_url(Some(&provider), explicit_base_url);
    let chat_client = Arc::new(GenaiChatClient::from_provider_model_with_base_url(
        Some(&provider),
        &model,
        base_url.as_deref(),
    ));

    let runtime = StasisRuntimeBuilder::new(backend)
        .with_chat_client(chat_client)
        .with_tool(MockWebSearchTool)?
        .build()
        .await?;

    Ok(runtime)
}

pub fn parse_backend(value: Option<&str>) -> RuntimeBackend {
    match value.unwrap_or("in-memory") {
        "surreal-mem" => RuntimeBackend::SurrealMem {
            namespace: "medousa".to_string(),
            database: "runtime".to_string(),
        },
        _ => RuntimeBackend::InMemory,
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
