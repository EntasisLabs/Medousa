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
use std::sync::Arc;
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

const CERTIFIED_PROVIDER: &str = "openai";
const MAX_MODEL_BYTES: usize = 256;
const MAX_BASE_URL_BYTES: usize = 2 * 1024;
const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;
const MAX_STREAM_ACCUMULATED_BYTES: usize = 4 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CredentialedAiChatConfigError {
    #[error("credentialed AI provider is not certified")]
    UnsupportedProvider,
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
        let base_url = base_url.map(normalize_base_url).transpose()?;
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
    config: CredentialedAiChatConfig,
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
            config,
            credentials,
            http_client,
        })
    }

    pub fn config(&self) -> &CredentialedAiChatConfig {
        &self.config
    }

    fn adapter_kind(&self) -> AdapterKind {
        if self.config.base_url.is_none()
            && self.config.model.to_ascii_lowercase().starts_with("gpt-5")
        {
            AdapterKind::OpenAIResp
        } else {
            AdapterKind::OpenAI
        }
    }

    fn model_target(&self) -> String {
        let namespace = match self.adapter_kind() {
            AdapterKind::OpenAIResp => "openai_resp",
            _ => "openai",
        };
        format!("{namespace}::{}", self.config.model)
    }

    async fn load_credential(&self) -> StasisResult<ProviderCredential> {
        self.credentials
            .credential_for(&self.config.provider)
            .await
            .map_err(|error| {
                StasisError::PortFailure(format!(
                    "credential for provider '{}' is unavailable: {error}",
                    self.config.provider
                ))
            })
    }

    fn request_client(&self, credential: ProviderCredential) -> Client {
        let adapter_kind = self.adapter_kind();
        let credential = Arc::new(credential);
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
                Ok(Some(AuthData::from_single(
                    credential.expose_secret().to_string(),
                )))
            });

        if let Some(base_url) = self.config.base_url.clone() {
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

    fn completion_options(&self, options: Option<&ChatOptions>) -> ChatOptions {
        apply_model_reasoning_suffix(&self.model_target(), options.cloned().unwrap_or_default())
    }

    fn stream_options(&self, options: Option<&ChatOptions>) -> ChatOptions {
        self.completion_options(options)
            .with_capture_content(true)
            .with_capture_usage(true)
            .with_capture_tool_calls(true)
            .with_capture_reasoning_content(true)
            .with_normalize_reasoning_content(true)
    }

    async fn complete_with_client(
        &self,
        client: &Client,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        client
            .exec_chat(
                self.model_target(),
                request,
                Some(&self.completion_options(options)),
            )
            .await
            .map_err(|error| self.transport_error("completion", error))
    }

    fn transport_error(&self, operation: &str, _error: genai::Error) -> StasisError {
        StasisError::PortFailure(format!(
            "credentialed AI {operation} failed for provider '{}' and model '{}'",
            self.config.provider, self.config.model
        ))
    }
}

impl fmt::Debug for CredentialedAiChatClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialedAiChatClient")
            .field("config", &self.config)
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
        let credential = self.load_credential().await?;
        let client = self.request_client(credential);
        self.complete_with_client(&client, request, options).await
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> StasisResult<ChatResponse> {
        let credential = self.load_credential().await?;
        let client = self.request_client(credential);
        let fallback_request = request.clone();
        let mut stream_response = client
            .exec_chat_stream(
                self.model_target(),
                request,
                Some(&self.stream_options(options)),
            )
            .await
            .map_err(|error| self.transport_error("stream", error))?;

        let model_iden = stream_response.model_iden.clone();
        let mut streamed_text = String::new();
        let mut reasoning_text = String::new();
        let mut captured_content: Option<MessageContent> = None;
        let mut captured_reasoning_content: Option<String> = None;
        let mut usage = Usage::default();
        let mut stop_reason = None;
        let mut response_id = None;

        while let Some(event) = stream_response.stream.next().await {
            match event.map_err(|error| self.transport_error("stream event", error))? {
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
                .complete_with_client(&client, fallback_request, options)
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

fn normalize_provider(provider: String) -> Result<String, CredentialedAiChatConfigError> {
    let provider = provider.trim();
    if !provider.eq_ignore_ascii_case(CERTIFIED_PROVIDER) {
        return Err(CredentialedAiChatConfigError::UnsupportedProvider);
    }
    Ok(CERTIFIED_PROVIDER.to_string())
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
    if parsed.scheme() != "https" {
        return Err(CredentialedAiChatConfigError::InvalidBaseUrl(
            "https_required",
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
    fn configuration_is_provider_bound_and_https_only() {
        let config = config(" gpt-4.1-mini ", Some("https://gateway.example/v1"));
        assert_eq!(config.provider(), "openai");
        assert_eq!(config.model(), "gpt-4.1-mini");
        assert_eq!(config.base_url(), Some("https://gateway.example/v1/"));
        assert_eq!(
            CredentialedAiChatConfig::new("anthropic", "claude", None),
            Err(CredentialedAiChatConfigError::UnsupportedProvider)
        );
        assert!(matches!(
            CredentialedAiChatConfig::new(
                "openai",
                "gpt-4.1-mini",
                Some("http://gateway.example/v1".to_string())
            ),
            Err(CredentialedAiChatConfigError::InvalidBaseUrl(
                "https_required"
            ))
        ));
        assert!(CredentialedAiChatConfig::new("openai", "other::model", None).is_err());
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

        let custom = client(
            config("gpt-5.6-sol", Some("https://gateway.example/v1")),
            credentials,
        );
        assert_eq!(custom.adapter_kind(), AdapterKind::OpenAI);
        assert_eq!(custom.model_target(), "openai::gpt-5.6-sol");
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
