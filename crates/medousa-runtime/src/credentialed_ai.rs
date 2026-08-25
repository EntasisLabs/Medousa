//! Explicit-credential network inference for daemon deployments.
//!
//! Native embedded deployments cannot safely depend on process-global
//! environment variables for provider credentials. This adapter implements the
//! Stasis inference port with a request-local credential supplied by the
//! daemon's existing secret authority. The deployment host decides how that
//! credential is loaded (Keychain on iOS); this crate never owns a second
//! credential store.
//!
//! Stasis 0.9.3's `GenaiChatClient` resolves authentication only from process
//! environment variables and does not accept an injected `genai::Client` or
//! auth resolver. Keep this adapter at the Stasis port boundary until that
//! constructor exists upstream.

use std::fmt;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use genai::adapter::AdapterKind;
use genai::chat::{ChatOptions, ChatRequest, ChatResponse, ChatStreamEvent, MessageContent, Usage};
use genai::resolver::{AuthData, Endpoint};
use genai::{Client, ServiceTarget, WebConfig};
use stasis::application::runtime::chat_options_resolver::apply_model_reasoning_suffix;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta, send_stream_delta};
use tokio::sync::mpsc;
use zeroize::{Zeroize, Zeroizing};

const MAX_PROVIDER_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_BASE_URL_BYTES: usize = 2 * 1024;
const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;
const MAX_STREAM_ACCUMULATED_BYTES: usize = 4 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(90);
const OPENAI_DEFAULT_BASE_URL: &str = "https://api.openai.com/v1/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CredentialedAiChatConfigError {
    #[error("credentialed AI provider is not supported by this runtime route")]
    UnsupportedProvider,
    #[error("credentialed AI provider is invalid: {0}")]
    InvalidProvider(&'static str),
    #[error("credentialed AI model is invalid: {0}")]
    InvalidModel(&'static str),
    #[error("credentialed AI base URL is invalid: {0}")]
    InvalidBaseUrl(&'static str),
}

#[derive(Clone, PartialEq, Eq)]
pub struct CredentialedAiChatConfig {
    provider: String,
    model: String,
    base_url: Option<String>,
}

impl CredentialedAiChatConfig {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<Self, CredentialedAiChatConfigError> {
        let provider = normalize_provider(provider.into())?;
        let model = normalize_model(model.into())?;
        let base_url = base_url.map(normalize_base_url).transpose()?.filter(|url| {
            !provider.eq_ignore_ascii_case("openai") || url != OPENAI_DEFAULT_BASE_URL
        });
        let adapter = resolve_genai_adapter_kind(&provider, &model, base_url.as_deref())
            .ok_or(CredentialedAiChatConfigError::UnsupportedProvider)?;
        if base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("http://"))
            && adapter.default_key_env_name().is_some()
        {
            return Err(CredentialedAiChatConfigError::InvalidBaseUrl(
                "https_required_for_credentials",
            ));
        }
        Ok(Self {
            provider,
            model,
            base_url,
        })
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
}

impl fmt::Debug for CredentialedAiChatConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialedAiChatConfig")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("has_custom_base_url", &self.base_url.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProviderCredentialError {
    #[error("provider credential is missing")]
    Missing,
    #[error("provider credential store is locked")]
    Locked,
    #[error("provider credential store is unavailable")]
    Unavailable,
    #[error("provider credential is invalid: {0}")]
    Invalid(&'static str),
}

/// Owned provider credential that clears its backing allocation on drop.
pub struct ProviderCredential(Zeroizing<String>);

impl ProviderCredential {
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderCredentialError> {
        let mut value = value.into();
        if value.len() > MAX_CREDENTIAL_BYTES {
            value.zeroize();
            return Err(ProviderCredentialError::Invalid("too_long"));
        }
        let normalized = Zeroizing::new(value.trim().to_string());
        value.zeroize();
        if normalized.is_empty() {
            return Err(ProviderCredentialError::Invalid("empty"));
        }
        if normalized.chars().any(char::is_control) {
            return Err(ProviderCredentialError::Invalid("control_character"));
        }
        Ok(Self(normalized))
    }

    fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ProviderCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProviderCredential(REDACTED)")
    }
}

/// Loads provider credentials from the daemon's configured secret authority.
///
/// Implementations must not infer credentials from a client request or mutate
/// process-global environment variables. The iOS composition binds this port
/// to the existing daemon integration secret stored in Keychain.
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    async fn credential_for(
        &self,
        provider: &str,
    ) -> Result<ProviderCredential, ProviderCredentialError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialedAiChatBuildError {
    #[error("credentialed AI HTTP client initialization failed")]
    HttpClient(#[source] reqwest::Error),
}

#[derive(Clone)]
pub struct CredentialedAiChatClient {
    config: Arc<RwLock<CredentialedAiChatConfig>>,
    credentials: Arc<dyn CredentialProvider>,
    http_client: reqwest::Client,
}

impl CredentialedAiChatClient {
    pub fn new(
        config: CredentialedAiChatConfig,
        credentials: Arc<dyn CredentialProvider>,
    ) -> Result<Self, CredentialedAiChatBuildError> {
        let web_config = WebConfig {
            connect_timeout: Some(CONNECT_TIMEOUT),
            read_timeout: Some(READ_TIMEOUT),
            ..WebConfig::default()
        };
        let http_client = web_config
            .apply_to_builder(reqwest::Client::builder())
            .build()
            .map_err(CredentialedAiChatBuildError::HttpClient)?;
        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            credentials,
            http_client,
        })
    }

    pub fn config(&self) -> CredentialedAiChatConfig {
        self.config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Atomically replace the route used by subsequent inference calls.
    /// In-flight calls retain the snapshot they started with.
    pub fn reconfigure(&self, config: CredentialedAiChatConfig) {
        *self
            .config
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = config;
    }

    fn adapter_kind_for(config: &CredentialedAiChatConfig) -> AdapterKind {
        resolve_genai_adapter_kind(&config.provider, &config.model, config.base_url.as_deref())
            .expect("validated credentialed inference route")
    }

    #[cfg(test)]
    fn adapter_kind(&self) -> AdapterKind {
        Self::adapter_kind_for(&self.config())
    }

    fn model_target_for(config: &CredentialedAiChatConfig) -> String {
        genai_model_target(&config.provider, &config.model, config.base_url.as_deref())
    }

    #[cfg(test)]
    fn model_target(&self) -> String {
        Self::model_target_for(&self.config())
    }

    async fn load_credential(
        &self,
        config: &CredentialedAiChatConfig,
    ) -> StasisResult<Option<ProviderCredential>> {
        if Self::adapter_kind_for(config)
            .default_key_env_name()
            .is_none()
        {
            return Ok(None);
        }
        self.credentials
            .credential_for(&config.provider)
            .await
            .map(Some)
            .map_err(|error| {
                StasisError::PortFailure(format!(
                    "credential for provider '{}' is unavailable: {error}",
                    config.provider
                ))
            })
    }

    fn request_client(
        &self,
        config: &CredentialedAiChatConfig,
        credential: Option<ProviderCredential>,
    ) -> Client {
        let adapter_kind = Self::adapter_kind_for(config);
        let credential = credential.map(Arc::new);
        let mut builder = Client::builder()
            .with_reqwest(self.http_client.clone())
            .with_adapter_kind(adapter_kind)
            .with_auth_resolver_fn(move |model_iden: genai::ModelIden| {
                if model_iden.adapter_kind != adapter_kind {
                    return Err(genai::resolver::Error::Custom(
                        "credentialed AI adapter escaped its configured provider".to_string(),
                    ));
                }
                // genai requires an owned key. The request-local client confines
                // this unavoidable clone to one provider call.
                Ok(credential.as_ref().map(|credential| {
                    AuthData::from_single(credential.expose_secret().to_string())
                }))
            });

        if let Some(base_url) = config.base_url.clone() {
            builder =
                builder.with_service_target_resolver_fn(move |service_target: ServiceTarget| {
                    let ServiceTarget { auth, model, .. } = service_target;
                    Ok(ServiceTarget {
                        endpoint: Endpoint::from_owned(base_url.clone()),
                        auth,
                        model,
                    })
                });
        }
        builder.build()
    }

    fn completion_options_for(
        config: &CredentialedAiChatConfig,
        options: Option<&ChatOptions>,
    ) -> ChatOptions {
        apply_model_reasoning_suffix(
            &Self::model_target_for(config),
            options.cloned().unwrap_or_default(),
        )
    }

    fn stream_options_for(
        config: &CredentialedAiChatConfig,
        options: Option<&ChatOptions>,
    ) -> ChatOptions {
        Self::completion_options_for(config, options)
            .with_capture_content(true)
            .with_capture_usage(true)
            .with_capture_tool_calls(true)
            .with_capture_reasoning_content(true)
            .with_normalize_reasoning_content(true)
    }

    #[cfg(test)]
    fn stream_options(&self, options: Option<&ChatOptions>) -> ChatOptions {
        Self::stream_options_for(&self.config(), options)
    }

    async fn complete_with_client(
        &self,
        config: &CredentialedAiChatConfig,
        client: &Client,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        client
            .exec_chat(
                Self::model_target_for(config),
                request,
                Some(&Self::completion_options_for(config, options)),
            )
            .await
            .map_err(|error| Self::transport_error_for(config, "completion", error))
    }

    fn transport_error_for(
        config: &CredentialedAiChatConfig,
        operation: &str,
        _error: genai::Error,
    ) -> StasisError {
        StasisError::PortFailure(format!(
            "credentialed AI {operation} failed for provider '{}' and model '{}'",
            config.provider, config.model
        ))
    }

    #[cfg(test)]
    fn transport_error(&self, operation: &str, error: genai::Error) -> StasisError {
        Self::transport_error_for(&self.config(), operation, error)
    }
}

impl fmt::Debug for CredentialedAiChatClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialedAiChatClient")
            .field("config", &self.config())
            .field("credential_provider", &"REDACTED")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl AiChatClient for CredentialedAiChatClient {
    async fn complete(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        let config = self.config();
        let credential = self.load_credential(&config).await?;
        let client = self.request_client(&config, credential);
        self.complete_with_client(&config, &client, request, options)
            .await
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> StasisResult<ChatResponse> {
        let config = self.config();
        let credential = self.load_credential(&config).await?;
        let client = self.request_client(&config, credential);
        let fallback_request = request.clone();
        let mut stream_response = client
            .exec_chat_stream(
                Self::model_target_for(&config),
                request,
                Some(&Self::stream_options_for(&config, options)),
            )
            .await
            .map_err(|error| Self::transport_error_for(&config, "stream", error))?;

        let model_iden = stream_response.model_iden.clone();
        let mut streamed_text = String::new();
        let mut reasoning_text = String::new();
        let mut captured_content: Option<MessageContent> = None;
        let mut captured_reasoning_content: Option<String> = None;
        let mut usage = Usage::default();
        let mut stop_reason = None;
        let mut response_id = None;

        while let Some(event) = stream_response.stream.next().await {
            match event
                .map_err(|error| Self::transport_error_for(&config, "stream event", error))?
            {
                ChatStreamEvent::Chunk(chunk) => {
                    if !chunk.content.is_empty() {
                        append_bounded(&mut streamed_text, &chunk.content, "content")?;
                        if let Some(tx) = chunk_tx {
                            send_stream_delta(tx, StreamDelta::Content(chunk.content)).await?;
                        }
                    }
                }
                ChatStreamEvent::ReasoningChunk(chunk) => {
                    if !chunk.content.is_empty() {
                        append_bounded(&mut reasoning_text, &chunk.content, "reasoning")?;
                        if let Some(tx) = chunk_tx {
                            send_stream_delta(tx, StreamDelta::Reasoning(chunk.content)).await?;
                        }
                    }
                }
                ChatStreamEvent::ThoughtSignatureChunk(chunk) => {
                    if !chunk.content.is_empty()
                        && let Some(tx) = chunk_tx
                    {
                        send_stream_delta(tx, StreamDelta::ThoughtSignature(chunk.content)).await?;
                    }
                }
                ChatStreamEvent::End(end) => {
                    captured_content = end.captured_content;
                    captured_reasoning_content = end.captured_reasoning_content;
                    usage = end.captured_usage.unwrap_or_default();
                    stop_reason = end.captured_stop_reason;
                    response_id = end.captured_response_id;
                }
                _ => {}
            }
        }

        let mut content = captured_content.unwrap_or_default();
        if content.first_text().is_none() && !streamed_text.is_empty() {
            content.extend_front(MessageContent::from_text(streamed_text));
        }
        if content.first_text().is_none() && content.tool_calls().is_empty() {
            let fallback = self
                .complete_with_client(&config, &client, fallback_request, options)
                .await?;
            if let (Some(tx), Some(text)) = (chunk_tx, fallback.first_text()) {
                send_stream_delta(tx, StreamDelta::Content(text.to_string())).await?;
            }
            return Ok(fallback);
        }

        let reasoning_content = captured_reasoning_content
            .or_else(|| (!reasoning_text.trim().is_empty()).then_some(reasoning_text));
        Ok(ChatResponse {
            content,
            reasoning_content,
            model_iden: model_iden.clone(),
            provider_model_iden: model_iden,
            stop_reason,
            usage,
            captured_raw_body: None,
            response_id,
        })
    }
}

fn append_bounded(target: &mut String, chunk: &str, lane: &str) -> StasisResult<()> {
    if target.len().saturating_add(chunk.len()) > MAX_STREAM_ACCUMULATED_BYTES {
        return Err(StasisError::PortFailure(format!(
            "credentialed AI {lane} stream exceeded its local accumulation limit"
        )));
    }
    target.push_str(chunk);
    Ok(())
}

/// Resolve a Medousa provider id to the genai protocol used by both full and
/// embedded daemon deployments. Providers with an explicit OpenAI-compatible
/// endpoint use the OpenAI protocol; native genai providers keep their native
/// request and response semantics.
pub fn resolve_genai_adapter_kind(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
) -> Option<AdapterKind> {
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    let has_custom_endpoint = base_url
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    let adapter = match provider.as_str() {
        // ChatGPT-account auth is a distinct daemon adapter, not an API-key
        // route. Medousa Local likewise belongs to the local inference port.
        "openai-codex" | "medousa-local" | "bedrock" => return None,
        "openai" => {
            if !has_custom_endpoint && model.starts_with("gpt-5") {
                AdapterKind::OpenAIResp
            } else {
                AdapterKind::OpenAI
            }
        }
        "google" | "google-gemini" | "gemini" => AdapterKind::Gemini,
        "openrouter" | "open-router" => AdapterKind::OpenRouter,
        "zhipu" => AdapterKind::BigModel,
        "qwen" => AdapterKind::Aliyun,
        // These catalog routes expose OpenAI-compatible HTTP APIs. Their
        // endpoint is required so they can never fall through to OpenAI's URL.
        "custom" | "mistral" | "perplexity" | "azure-openai" | "cerebras" | "hyperbolic"
        | "huggingface" => {
            if !has_custom_endpoint {
                return None;
            }
            AdapterKind::OpenAI
        }
        _ => match AdapterKind::from_lower_str(&provider) {
            Some(adapter) => adapter,
            None if has_custom_endpoint => AdapterKind::OpenAI,
            None => return None,
        },
    };
    Some(adapter)
}

/// Build the provider-qualified model target used by genai.
///
/// Explicitly qualified model ids remain authoritative for the full daemon.
/// Credentialed embedded routes reject those ids at configuration admission.
pub fn genai_model_target(provider: &str, model: &str, base_url: Option<&str>) -> String {
    let model = model.trim();
    if model.contains("::") {
        return model.to_string();
    }
    match resolve_genai_adapter_kind(provider, model, base_url) {
        Some(adapter) => format!("{}::{model}", adapter.as_lower_str()),
        None => format!("{}::{model}", provider.trim()),
    }
}

fn normalize_provider(provider: String) -> Result<String, CredentialedAiChatConfigError> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err(CredentialedAiChatConfigError::InvalidProvider("empty"));
    }
    if provider.len() > MAX_PROVIDER_BYTES {
        return Err(CredentialedAiChatConfigError::InvalidProvider("too_long"));
    }
    if provider.chars().any(char::is_control) {
        return Err(CredentialedAiChatConfigError::InvalidProvider(
            "control_character",
        ));
    }
    if !provider
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        return Err(CredentialedAiChatConfigError::InvalidProvider(
            "invalid_character",
        ));
    }
    Ok(provider.to_ascii_lowercase())
}

fn normalize_model(model: String) -> Result<String, CredentialedAiChatConfigError> {
    let model = model.trim();
    if model.is_empty() {
        return Err(CredentialedAiChatConfigError::InvalidModel("empty"));
    }
    if model.len() > MAX_MODEL_BYTES {
        return Err(CredentialedAiChatConfigError::InvalidModel("too_long"));
    }
    if model.chars().any(char::is_control) {
        return Err(CredentialedAiChatConfigError::InvalidModel(
            "control_character",
        ));
    }
    if model.contains("::") {
        return Err(CredentialedAiChatConfigError::InvalidModel(
            "provider_namespace",
        ));
    }
    Ok(model.to_string())
}

fn normalize_base_url(base_url: String) -> Result<String, CredentialedAiChatConfigError> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        return Err(CredentialedAiChatConfigError::InvalidBaseUrl("empty"));
    }
    if base_url.len() > MAX_BASE_URL_BYTES {
        return Err(CredentialedAiChatConfigError::InvalidBaseUrl("too_long"));
    }
    let mut parsed = url::Url::parse(base_url)
        .map_err(|_| CredentialedAiChatConfigError::InvalidBaseUrl("syntax"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(CredentialedAiChatConfigError::InvalidBaseUrl(
            "http_or_https_required",
        ));
    }
    if parsed.cannot_be_a_base() || parsed.host_str().is_none() {
        return Err(CredentialedAiChatConfigError::InvalidBaseUrl(
            "missing_host",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(CredentialedAiChatConfigError::InvalidBaseUrl(
            "embedded_credentials",
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(CredentialedAiChatConfigError::InvalidBaseUrl(
            "query_or_fragment",
        ));
    }
    if !parsed.path().ends_with('/') {
        let path = format!("{}/", parsed.path());
        parsed.set_path(&path);
    }
    Ok(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct MissingCredentialProvider {
        requested: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl CredentialProvider for MissingCredentialProvider {
        async fn credential_for(
            &self,
            provider: &str,
        ) -> Result<ProviderCredential, ProviderCredentialError> {
            self.requested.lock().unwrap().push(provider.to_string());
            Err(ProviderCredentialError::Missing)
        }
    }

    fn config(model: &str, base_url: Option<&str>) -> CredentialedAiChatConfig {
        CredentialedAiChatConfig::new(
            " OpenAI ",
            model,
            base_url.map(std::string::ToString::to_string),
        )
        .unwrap()
    }

    fn client(
        config: CredentialedAiChatConfig,
        credentials: Arc<dyn CredentialProvider>,
    ) -> CredentialedAiChatClient {
        CredentialedAiChatClient::new(config, credentials).unwrap()
    }

    #[test]
    fn configuration_accepts_portable_provider_routes_and_safe_http_urls() {
        let config = config(" gpt-4.1-mini ", Some("https://gateway.example/v1"));
        assert_eq!(config.provider(), "openai");
        assert_eq!(config.model(), "gpt-4.1-mini");
        assert_eq!(config.base_url(), Some("https://gateway.example/v1/"));
        assert!(CredentialedAiChatConfig::new("anthropic", "claude-sonnet-4-6", None).is_ok());
        assert!(
            CredentialedAiChatConfig::new(
                "ollama",
                "llama3.2",
                Some("http://127.0.0.1:11434".to_string())
            )
            .is_ok()
        );
        assert!(matches!(
            CredentialedAiChatConfig::new(
                "openai",
                "gpt-4.1-mini",
                Some("http://gateway.example/v1".to_string())
            ),
            Err(CredentialedAiChatConfigError::InvalidBaseUrl(
                "https_required_for_credentials"
            ))
        ));
        assert_eq!(
            CredentialedAiChatConfig::new("openai-codex", "gpt-5.6-sol", None),
            Err(CredentialedAiChatConfigError::UnsupportedProvider)
        );
        assert!(CredentialedAiChatConfig::new("not a provider", "model", None).is_err());
        assert!(CredentialedAiChatConfig::new("openai", "other::model", None).is_err());
    }

    #[test]
    fn provider_catalog_routes_use_their_native_or_compatible_adapters() {
        let routes = [
            (
                "anthropic",
                "claude-sonnet-4-6",
                None,
                AdapterKind::Anthropic,
                "anthropic::claude-sonnet-4-6",
            ),
            (
                "google",
                "gemini-3.1-pro-preview",
                None,
                AdapterKind::Gemini,
                "gemini::gemini-3.1-pro-preview",
            ),
            (
                "deepseek",
                "deepseek-v4-flash",
                None,
                AdapterKind::DeepSeek,
                "deepseek::deepseek-v4-flash",
            ),
            (
                "groq",
                "llama-3.3-70b-versatile",
                None,
                AdapterKind::Groq,
                "groq::llama-3.3-70b-versatile",
            ),
            ("xai", "grok-4", None, AdapterKind::Xai, "xai::grok-4"),
            (
                "openrouter",
                "anthropic/claude-sonnet-4-6",
                None,
                AdapterKind::OpenRouter,
                "open_router::anthropic/claude-sonnet-4-6",
            ),
            (
                "mistral",
                "mistral-large-latest",
                Some("https://api.mistral.ai/v1"),
                AdapterKind::OpenAI,
                "openai::mistral-large-latest",
            ),
        ];

        for (provider, model, base_url, expected_adapter, expected_target) in routes {
            let config =
                CredentialedAiChatConfig::new(provider, model, base_url.map(str::to_string))
                    .unwrap();
            let client = client(config, Arc::new(MissingCredentialProvider::default()));
            assert_eq!(
                client.adapter_kind(),
                expected_adapter,
                "provider={provider}"
            );
            assert_eq!(
                client.model_target(),
                expected_target,
                "provider={provider}"
            );
        }
    }

    #[test]
    fn base_url_rejects_credential_and_log_injection_surfaces() {
        for value in [
            "https://secret@gateway.example/v1",
            "https://gateway.example/v1?key=secret",
            "https://gateway.example/v1#secret",
        ] {
            assert!(
                CredentialedAiChatConfig::new("openai", "gpt-4.1-mini", Some(value.to_string()))
                    .is_err()
            );
        }
        let config = config("gpt-4.1-mini", Some("https://gateway.example/secret-path"));
        assert!(!format!("{config:?}").contains("secret-path"));
    }

    #[test]
    fn credentials_are_trimmed_bounded_and_debug_redacted() {
        let credential = ProviderCredential::new("  sk-super-secret\n").unwrap();
        assert_eq!(credential.expose_secret(), "sk-super-secret");
        assert_eq!(format!("{credential:?}"), "ProviderCredential(REDACTED)");
        assert!(ProviderCredential::new("x".repeat(MAX_CREDENTIAL_BYTES + 1)).is_err());
        assert!(ProviderCredential::new(" \n\t ").is_err());
    }

    #[test]
    fn default_gpt5_uses_responses_but_custom_endpoints_use_chat_completions() {
        let credentials = Arc::new(MissingCredentialProvider::default());
        let default = client(config("gpt-5.6-sol", None), credentials.clone());
        assert_eq!(default.adapter_kind(), AdapterKind::OpenAIResp);
        assert_eq!(default.model_target(), "openai_resp::gpt-5.6-sol");

        let explicit_default = client(
            config("gpt-5.6-sol", Some("https://api.openai.com/v1")),
            credentials.clone(),
        );
        assert_eq!(explicit_default.adapter_kind(), AdapterKind::OpenAIResp);

        let custom = client(
            config("gpt-5.6-sol", Some("https://gateway.example/v1")),
            credentials,
        );
        assert_eq!(custom.adapter_kind(), AdapterKind::OpenAI);
        assert_eq!(custom.model_target(), "openai::gpt-5.6-sol");
    }

    #[test]
    fn reconfiguration_is_shared_by_clones_and_applies_to_future_calls() {
        let client = client(
            config("gpt-5.4-mini", None),
            Arc::new(MissingCredentialProvider::default()),
        );
        let runtime_port = client.clone();

        client.reconfigure(config("gpt-4.1-mini", Some("https://gateway.example/v1")));

        assert_eq!(runtime_port.config().model(), "gpt-4.1-mini");
        assert_eq!(runtime_port.adapter_kind(), AdapterKind::OpenAI);
        assert_eq!(runtime_port.model_target(), "openai::gpt-4.1-mini");
    }

    #[test]
    fn streaming_options_capture_text_reasoning_usage_and_tools() {
        let client = client(
            config("gpt-4.1-mini", None),
            Arc::new(MissingCredentialProvider::default()),
        );
        let options = client.stream_options(None);
        assert_eq!(options.capture_content, Some(true));
        assert_eq!(options.capture_usage, Some(true));
        assert_eq!(options.capture_tool_calls, Some(true));
        assert_eq!(options.capture_reasoning_content, Some(true));
        assert_eq!(options.normalize_reasoning_content, Some(true));
    }

    #[tokio::test]
    async fn missing_credentials_fail_before_network_and_name_only_the_provider() {
        let credentials = Arc::new(MissingCredentialProvider::default());
        let client = client(config("gpt-4.1-mini", None), credentials.clone());
        let error = client
            .complete(ChatRequest::default(), None)
            .await
            .expect_err("missing key must fail before transport");
        let message = error.to_string();
        assert!(message.contains("openai"));
        assert!(message.contains("missing"));
        assert_eq!(credentials.requested.lock().unwrap().as_slice(), ["openai"]);
    }

    #[tokio::test]
    async fn keyless_ollama_route_does_not_consult_the_credential_store() {
        let credentials = Arc::new(MissingCredentialProvider::default());
        let config = CredentialedAiChatConfig::new(
            "ollama",
            "llama3.2",
            Some("http://127.0.0.1:9".to_string()),
        )
        .unwrap();
        let client = client(config, credentials.clone());
        let error = client
            .complete(ChatRequest::default(), None)
            .await
            .expect_err("closed local endpoint must fail");
        assert!(error.to_string().contains("ollama"));
        assert!(credentials.requested.lock().unwrap().is_empty());
    }

    #[test]
    fn client_debug_does_not_render_the_credential_provider_or_endpoint() {
        let client = client(
            config("gpt-4.1-mini", Some("https://gateway.example/secret-path")),
            Arc::new(MissingCredentialProvider::default()),
        );
        let debug = format!("{client:?}");
        assert!(debug.contains("credential_provider: \"REDACTED\""));
        assert!(!debug.contains("secret-path"));
    }

    #[test]
    fn transport_errors_do_not_render_provider_bodies_or_endpoints() {
        let client = client(
            config("gpt-4.1-mini", Some("https://gateway.example/secret-route")),
            Arc::new(MissingCredentialProvider::default()),
        );
        let error = client.transport_error(
            "completion",
            genai::Error::HttpError {
                status: reqwest::StatusCode::UNAUTHORIZED,
                canonical_reason: "Unauthorized".to_string(),
                body: "sk-provider-secret".to_string(),
            },
        );
        let rendered = error.to_string();
        assert!(rendered.contains("provider 'openai'"));
        assert!(rendered.contains("model 'gpt-4.1-mini'"));
        assert!(!rendered.contains("secret-route"));
        assert!(!rendered.contains("sk-provider-secret"));
    }

    #[test]
    fn credentialed_client_implements_the_stasis_runtime_port() {
        fn assert_port<T: AiChatClient>() {}
        assert_port::<CredentialedAiChatClient>();
    }

    #[test]
    fn stream_accumulation_is_bounded_without_rendering_content_in_errors() {
        let mut accumulated = "x".repeat(MAX_STREAM_ACCUMULATED_BYTES - 1);
        append_bounded(&mut accumulated, "y", "content").unwrap();
        let error = append_bounded(&mut accumulated, "secret", "content").unwrap_err();
        assert!(!error.to_string().contains("secret"));
    }
}
