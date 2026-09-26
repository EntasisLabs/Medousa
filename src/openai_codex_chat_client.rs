//! Native ChatGPT-account Responses transport with Medousa loop ownership.

use async_trait::async_trait;
use futures_util::StreamExt;
use genai::chat::{
    ChatOptions, ChatRequest, ChatResponse, ChatStreamEvent, MessageContent, ReasoningEffort, Usage,
};
use genai::resolver::AuthData;
use genai::{Client, Headers};
use stasis::domain::errors::{Result as StasisResult, StasisError};
#[cfg(feature = "full-daemon")]
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta, send_stream_delta};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::chatgpt_oauth::ChatGptOAuthBroker;

const DEFAULT_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const RESPONSES_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSES_READ_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const RESPONSES_MAX_ATTEMPTS: usize = 3;
const RESPONSES_RETRY_BACKOFF: Duration = Duration::from_millis(250);
pub const OPENAI_CODEX_PROVIDER_ID: &str = "openai-codex";

#[derive(Debug)]
enum StreamOnceError {
    Transport {
        error: genai::Error,
        observable_output_delivered: bool,
    },
    IncompleteStream {
        observable_output_delivered: bool,
    },
    Delivery(StasisError),
}

impl From<genai::Error> for StreamOnceError {
    fn from(error: genai::Error) -> Self {
        Self::Transport {
            error,
            observable_output_delivered: false,
        }
    }
}

impl StreamOnceError {
    fn can_retry_before_output(&self) -> bool {
        match self {
            Self::Transport {
                error,
                observable_output_delivered: false,
            } => is_retryable_responses_error(error),
            Self::IncompleteStream {
                observable_output_delivered: false,
            } => true,
            Self::Transport { .. } | Self::IncompleteStream { .. } | Self::Delivery(_) => false,
        }
    }

    fn unauthorized_before_output(&self) -> bool {
        matches!(
            self,
            Self::Transport {
                error,
                observable_output_delivered: false,
            } if is_unauthorized(error)
        )
    }
}

/// Version of the Codex backend contract implemented by this adapter. This is
/// intentionally independent from Medousa's product version: the ChatGPT Codex
/// backend gates newer models on this protocol identity.
/// Astra catalog baseline: https://learn.chatgpt.com/docs/changelog (0.153.4).
pub(crate) const CODEX_COMPAT_VERSION: &str = "0.153.4";
pub(crate) const CODEX_COMPAT_ORIGINATOR: &str = "codex_cli_rs";

pub(crate) fn codex_compat_user_agent() -> String {
    format!("{CODEX_COMPAT_ORIGINATOR}/{CODEX_COMPAT_VERSION}")
}

#[derive(Clone)]
pub struct OpenAiCodexChatClient {
    model: String,
    responses_url: String,
    credentials: ChatGptCredentialSource,
}

#[derive(Clone)]
enum ChatGptCredentialSource {
    #[cfg(feature = "full-daemon")]
    Daemon,
    Broker(std::sync::Arc<ChatGptOAuthBroker>),
}

impl OpenAiCodexChatClient {
    #[cfg(feature = "full-daemon")]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            responses_url: std::env::var("MEDOUSA_CHATGPT_RESPONSES_URL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_RESPONSES_URL.to_string()),
            credentials: ChatGptCredentialSource::Daemon,
        }
    }

    pub fn with_broker(
        model: impl Into<String>,
        broker: std::sync::Arc<ChatGptOAuthBroker>,
    ) -> Self {
        Self {
            model: model.into(),
            responses_url: std::env::var("MEDOUSA_CHATGPT_RESPONSES_URL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_RESPONSES_URL.to_string()),
            credentials: ChatGptCredentialSource::Broker(broker),
        }
    }

    #[cfg(test)]
    fn with_url(model: impl Into<String>, responses_url: impl Into<String>) -> Self {
        struct EmptyStore;
        impl crate::chatgpt_oauth::ChatGptCredentialStore for EmptyStore {
            fn load_bundle(&self) -> Result<Option<String>, String> {
                Ok(None)
            }

            fn save_bundle(&self, _bundle: Option<&str>) -> Result<(), String> {
                Ok(())
            }
        }
        Self {
            model: model.into(),
            responses_url: responses_url.into(),
            credentials: ChatGptCredentialSource::Broker(std::sync::Arc::new(
                ChatGptOAuthBroker::new(std::sync::Arc::new(EmptyStore)),
            )),
        }
    }

    fn client(&self, access_token: &str, account_id: &str) -> Client {
        let headers = request_headers(access_token, account_id);
        let url = self.responses_url.clone();
        Client::builder()
            .with_web_config(responses_web_config())
            .with_auth_resolver_fn(move |_| {
                Ok(Some(AuthData::RequestOverride {
                    url: url.clone(),
                    headers: headers.clone(),
                }))
            })
            .build()
    }

    fn model_target(&self) -> String {
        let (_, model) = ReasoningEffort::from_model_name(self.model.trim());
        format!("openai_resp::{model}")
    }

    fn stream_options(&self, options: Option<&ChatOptions>) -> ChatOptions {
        let mut options = crate::reasoning_effort::model_chat_options(
            OPENAI_CODEX_PROVIDER_ID,
            &self.model,
            options,
        );
        let (_, model) = ReasoningEffort::from_model_name(self.model.trim());
        if model.starts_with("gpt-6-astra")
            || model.starts_with("gpt-6-sol")
            || model.starts_with("gpt-6-luna")
        {
            // GPT-6 reasoning models accept reasoning effort instead of sampling controls.
            // Preserve supported efforts and let an unset effort use its default.
            options.temperature = None;
            options.top_p = None;
        }
        options
            .with_capture_content(true)
            .with_capture_usage(true)
            .with_capture_tool_calls(true)
            .with_capture_reasoning_content(true)
            .with_normalize_reasoning_content(true)
    }

    async fn credentials(&self) -> StasisResult<(String, String)> {
        let result = match &self.credentials {
            #[cfg(feature = "full-daemon")]
            ChatGptCredentialSource::Daemon => crate::chatgpt_oauth::request_credentials().await,
            ChatGptCredentialSource::Broker(broker) => broker.credentials_for_request().await,
        };
        result.map_err(|error| StasisError::PortFailure(error.to_string()))
    }

    async fn refreshed_credentials(
        &self,
        rejected_access_token: &str,
    ) -> StasisResult<(String, String)> {
        let result = match &self.credentials {
            #[cfg(feature = "full-daemon")]
            ChatGptCredentialSource::Daemon => {
                crate::chatgpt_oauth::refresh_request_credentials(rejected_access_token).await
            }
            ChatGptCredentialSource::Broker(broker) => {
                broker
                    .refresh_after_unauthorized(rejected_access_token)
                    .await
            }
        };
        result.map_err(|error| StasisError::PortFailure(error.to_string()))
    }

    async fn stream_once(
        &self,
        credentials: &(String, String),
        request: ChatRequest,
        options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> Result<ChatResponse, StreamOnceError> {
        let stream_options = self.stream_options(options);

        let mut stream_response = self
            .client(&credentials.0, &credentials.1)
            .exec_chat_stream(self.model_target(), request, Some(&stream_options))
            .await?;
        let model_iden = stream_response.model_iden.clone();
        let mut streamed_text = String::new();
        let mut reasoning_text = String::new();
        let mut captured_content: Option<MessageContent> = None;
        let mut captured_reasoning_content: Option<String> = None;
        let mut captured_stop_reason = None;
        let mut captured_response_id = None;
        let mut usage = Usage::default();
        let mut observable_output_delivered = false;

        while let Some(event) = stream_response.stream.next().await {
            let event = event.map_err(|error| StreamOnceError::Transport {
                error,
                observable_output_delivered,
            })?;
            match event {
                ChatStreamEvent::Chunk(chunk) => {
                    if !chunk.content.is_empty() {
                        streamed_text.push_str(&chunk.content);
                        if let Some(tx) = chunk_tx {
                            send_stream_delta(tx, StreamDelta::Content(chunk.content))
                                .await
                                .map_err(StreamOnceError::Delivery)?;
                            observable_output_delivered = true;
                        }
                    }
                }
                ChatStreamEvent::ReasoningChunk(chunk) => {
                    if !chunk.content.is_empty() {
                        reasoning_text.push_str(&chunk.content);
                        if let Some(tx) = chunk_tx {
                            send_stream_delta(tx, StreamDelta::Reasoning(chunk.content))
                                .await
                                .map_err(StreamOnceError::Delivery)?;
                            observable_output_delivered = true;
                        }
                    }
                }
                ChatStreamEvent::ThoughtSignatureChunk(chunk) => {
                    if !chunk.content.is_empty()
                        && let Some(tx) = chunk_tx
                    {
                        send_stream_delta(tx, StreamDelta::ThoughtSignature(chunk.content))
                            .await
                            .map_err(StreamOnceError::Delivery)?;
                        observable_output_delivered = true;
                    }
                }
                ChatStreamEvent::End(end) => {
                    // This client only talks to the Responses API. GenAI emits
                    // an End event at both a terminal response and raw SSE EOF;
                    // only terminal Responses events carry the response ID.
                    let Some(response_id) = end.captured_response_id else {
                        return Err(StreamOnceError::IncompleteStream {
                            observable_output_delivered,
                        });
                    };
                    captured_response_id = Some(response_id);
                    captured_stop_reason = end.captured_stop_reason;
                    captured_content = end.captured_content;
                    captured_reasoning_content = end.captured_reasoning_content;
                    usage = end.captured_usage.unwrap_or_default();
                }
                _ => {}
            }
        }

        let response_id = captured_response_id.ok_or(StreamOnceError::IncompleteStream {
            observable_output_delivered,
        })?;
        let mut content = captured_content.unwrap_or_default();
        if content.first_text().is_none() && !streamed_text.is_empty() {
            content.extend_front(MessageContent::from_text(streamed_text));
        }
        let reasoning_content = captured_reasoning_content
            .or_else(|| (!reasoning_text.trim().is_empty()).then_some(reasoning_text));
        Ok(ChatResponse {
            content,
            reasoning_content,
            model_iden: model_iden.clone(),
            provider_model_iden: model_iden,
            stop_reason: captured_stop_reason,
            usage,
            captured_raw_body: None,
            response_id: Some(response_id),
        })
    }

    async fn stream_with_retries(
        &self,
        credentials: &(String, String),
        request: ChatRequest,
        options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> Result<ChatResponse, StreamOnceError> {
        for attempt in 1..=RESPONSES_MAX_ATTEMPTS {
            match self
                .stream_once(credentials, request.clone(), options, chunk_tx)
                .await
            {
                Ok(response) => return Ok(response),
                Err(error)
                    if attempt < RESPONSES_MAX_ATTEMPTS && error.can_retry_before_output() =>
                {
                    let multiplier = u32::try_from(attempt).unwrap_or(u32::MAX);
                    tokio::time::sleep(RESPONSES_RETRY_BACKOFF.saturating_mul(multiplier)).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("at least one Responses attempt is configured")
    }
}

fn responses_web_config() -> genai::WebConfig {
    // reqwest's read timeout is an idle bound: it covers waiting for the initial
    // response and resets after each successful response-body read. Leave the
    // total request timeout unset so long but active reasoning streams can finish.
    genai::WebConfig {
        read_timeout: Some(RESPONSES_READ_IDLE_TIMEOUT),
        ..genai::WebConfig::default().with_connect_timeout(RESPONSES_CONNECT_TIMEOUT)
    }
}

#[async_trait]
impl AiChatClient for OpenAiCodexChatClient {
    async fn complete(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        let credentials = self.credentials().await?;
        match self
            .stream_with_retries(&credentials, request.clone(), options, None)
            .await
        {
            Ok(response) => Ok(response),
            Err(error) if error.unauthorized_before_output() => {
                let refreshed = self.refreshed_credentials(&credentials.0).await?;
                self.stream_with_retries(&refreshed, request, options, None)
                    .await
                    .map_err(|error| stream_once_error(&self.model, error))
            }
            Err(error) => Err(stream_once_error(&self.model, error)),
        }
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> StasisResult<ChatResponse> {
        let credentials = self.credentials().await?;
        match self
            .stream_with_retries(&credentials, request.clone(), options, chunk_tx)
            .await
        {
            Ok(response) => Ok(response),
            Err(error) if error.unauthorized_before_output() => {
                let refreshed = self.refreshed_credentials(&credentials.0).await?;
                self.stream_with_retries(&refreshed, request.clone(), options, chunk_tx)
                    .await
                    .map_err(|error| stream_once_error(&self.model, error))
            }
            Err(error) => Err(stream_once_error(&self.model, error)),
        }
    }
}

#[cfg(feature = "full-daemon")]
pub enum RoutedChatClient {
    Provider {
        client: GenaiChatClient,
        provider: String,
        model: String,
    },
    ChatGpt(OpenAiCodexChatClient),
}

#[cfg(feature = "full-daemon")]
impl RoutedChatClient {
    pub fn new(provider: &str, model: &str, base_url: Option<&str>) -> Self {
        if provider.eq_ignore_ascii_case(OPENAI_CODEX_PROVIDER_ID) {
            Self::ChatGpt(OpenAiCodexChatClient::new(model))
        } else {
            let (_, bare_model) = ReasoningEffort::from_model_name(model);
            let target = crate::genai_model_target(provider, bare_model, base_url);
            Self::Provider {
                client: GenaiChatClient::from_provider_model_with_base_url(None, &target, base_url),
                provider: provider.into(),
                model: model.into(),
            }
        }
    }
}

#[cfg(feature = "full-daemon")]
#[async_trait]
impl AiChatClient for RoutedChatClient {
    async fn complete(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        match self {
            // Non-stream responses already carry tool_calls + reasoning_content on the
            // body; no capture flags required.
            Self::Provider {
                client,
                provider,
                model,
            } => {
                let options = crate::reasoning_effort::model_chat_options(provider, model, options);
                client.complete(request, Some(&options)).await
            }
            Self::ChatGpt(client) => client.complete(request, options).await,
        }
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> StasisResult<ChatResponse> {
        match self {
            // stasis GenaiChatClient enables content/reasoning capture on stream, but
            // not tool-call capture. Without this, text+tools responses (common for
            // DeepSeek thinking mode) keep the preamble and drop tool_calls — then
            // the tool loop never stores a round-trippable assistant tool turn.
            Self::Provider {
                client,
                provider,
                model,
            } => {
                let options = crate::reasoning_effort::model_chat_options(provider, model, options);
                let options = provider_stream_options(Some(&options));
                client
                    .complete_stream(request, Some(&options), chunk_tx)
                    .await
            }
            Self::ChatGpt(client) => client.complete_stream(request, options, chunk_tx).await,
        }
    }
}

/// Stream options that keep tool calls in `ChatResponse.content` for transcript authority.
#[cfg(feature = "full-daemon")]
fn provider_stream_options(options: Option<&ChatOptions>) -> ChatOptions {
    options
        .cloned()
        .unwrap_or_default()
        .with_capture_tool_calls(true)
}

fn request_headers(access_token: &str, account_id: &str) -> Headers {
    Headers::from([
        ("Authorization", format!("Bearer {access_token}")),
        ("ChatGPT-Account-ID", account_id.to_string()),
        ("Content-Type", "application/json".to_string()),
        ("Accept", "text/event-stream, application/json".to_string()),
        ("Originator", CODEX_COMPAT_ORIGINATOR.to_string()),
        ("User-Agent", codex_compat_user_agent()),
        ("Version", CODEX_COMPAT_VERSION.to_string()),
    ])
}

fn is_unauthorized(error: &genai::Error) -> bool {
    match error {
        genai::Error::HttpError { status, .. } => status.as_u16() == 401,
        genai::Error::WebModelCall {
            webc_error: genai::webc::Error::ResponseFailedStatus { status, .. },
            ..
        } => status.as_u16() == 401,
        genai::Error::WebStream { error, .. } => error
            .downcast_ref::<genai::Error>()
            .is_some_and(|nested| matches!(nested, genai::Error::HttpError { status, .. } if status.as_u16() == 401)),
        _ => false,
    }
}

fn is_retryable_responses_error(error: &genai::Error) -> bool {
    match error {
        genai::Error::HttpError { status, .. } => is_retryable_status(*status),
        genai::Error::WebModelCall { webc_error, .. }
        | genai::Error::WebAdapterCall { webc_error, .. } => {
            is_retryable_web_error(webc_error)
        }
        genai::Error::WebStream { error, .. } => {
            error
                .downcast_ref::<genai_reqwest::Error>()
                .is_some_and(is_retryable_reqwest_error)
                || error.downcast_ref::<genai::Error>().is_some_and(|nested| {
                    matches!(nested, genai::Error::HttpError { status, .. } if is_retryable_status(*status))
                })
        }
        _ => false,
    }
}

fn is_retryable_web_error(error: &genai::webc::Error) -> bool {
    match error {
        genai::webc::Error::ResponseFailedStatus { status, .. } => is_retryable_status(*status),
        genai::webc::Error::Reqwest(error) => is_retryable_reqwest_error(error),
        _ => false,
    }
}

fn is_retryable_status(status: genai_reqwest::StatusCode) -> bool {
    matches!(
        status,
        genai_reqwest::StatusCode::TOO_MANY_REQUESTS
            | genai_reqwest::StatusCode::INTERNAL_SERVER_ERROR
            | genai_reqwest::StatusCode::BAD_GATEWAY
            | genai_reqwest::StatusCode::SERVICE_UNAVAILABLE
            | genai_reqwest::StatusCode::GATEWAY_TIMEOUT
    )
}

fn is_retryable_reqwest_error(error: &genai_reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout() || error.is_body() || error.is_request()
}

fn transport_error(model: &str, operation: &str, error: genai::Error) -> StasisError {
    StasisError::PortFailure(format!(
        "ChatGPT Responses {operation} failed for model '{model}': {error}"
    ))
}

fn stream_once_error(model: &str, error: StreamOnceError) -> StasisError {
    match error {
        StreamOnceError::Transport { error, .. } => transport_error(model, "stream", error),
        StreamOnceError::IncompleteStream { .. } => StasisError::PortFailure(format!(
            "ChatGPT Responses stream ended before a terminal response for model '{model}'"
        )),
        StreamOnceError::Delivery(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use futures_util::stream;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    #[cfg(feature = "full-daemon")]
    fn provider_stream_options_enable_tool_call_capture() {
        let incoming = ChatOptions::default().with_temperature(0.2);
        let options = provider_stream_options(Some(&incoming));
        assert_eq!(options.temperature, Some(0.2));
        assert_eq!(options.capture_tool_calls, Some(true));
    }

    #[test]
    #[cfg(feature = "full-daemon")]
    fn route_selection_keeps_api_key_and_chatgpt_clients_distinct() {
        assert!(matches!(
            RoutedChatClient::new("openai", "gpt-5.6-sol", None),
            RoutedChatClient::Provider { .. }
        ));
        assert!(matches!(
            RoutedChatClient::new(OPENAI_CODEX_PROVIDER_ID, "gpt-5.6-sol", None),
            RoutedChatClient::ChatGpt(_)
        ));
    }

    #[test]
    fn request_headers_have_account_auth_without_api_key_aliases() {
        let headers = request_headers("oauth-secret", "acct_123");
        let headers = headers.iter().collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            headers.get(&"Authorization".to_string()).unwrap().as_str(),
            "Bearer oauth-secret"
        );
        assert_eq!(
            headers
                .get(&"ChatGPT-Account-ID".to_string())
                .unwrap()
                .as_str(),
            "acct_123"
        );
        assert!(!headers.contains_key(&"X-API-Key".to_string()));
        assert_eq!(
            headers.get(&"Originator".to_string()).unwrap().as_str(),
            CODEX_COMPAT_ORIGINATOR
        );
        assert_eq!(
            headers.get(&"Version".to_string()).unwrap().as_str(),
            CODEX_COMPAT_VERSION
        );
        assert_eq!(
            headers.get(&"User-Agent".to_string()).unwrap().as_str(),
            codex_compat_user_agent()
        );
    }

    #[test]
    fn client_uses_responses_adapter_and_exact_transport_url() {
        let client = OpenAiCodexChatClient::with_url("gpt-5.6-sol", "http://localhost/responses");
        assert_eq!(client.model_target(), "openai_resp::gpt-5.6-sol");
        assert_eq!(client.responses_url, "http://localhost/responses");
    }

    #[test]
    fn retry_status_allowlist_excludes_auth_schema_and_protocol_failures() {
        for status in [
            genai_reqwest::StatusCode::TOO_MANY_REQUESTS,
            genai_reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            genai_reqwest::StatusCode::BAD_GATEWAY,
            genai_reqwest::StatusCode::SERVICE_UNAVAILABLE,
            genai_reqwest::StatusCode::GATEWAY_TIMEOUT,
        ] {
            assert!(is_retryable_status(status), "{status}");
        }
        for status in [
            genai_reqwest::StatusCode::BAD_REQUEST,
            genai_reqwest::StatusCode::UNAUTHORIZED,
            genai_reqwest::StatusCode::FORBIDDEN,
            genai_reqwest::StatusCode::NOT_IMPLEMENTED,
            genai_reqwest::StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            genai_reqwest::StatusCode::NETWORK_AUTHENTICATION_REQUIRED,
        ] {
            assert!(!is_retryable_status(status), "{status}");
        }
    }

    #[test]
    fn nested_stream_401_is_refreshable_only_before_any_output() {
        fn nested_unauthorized() -> genai::Error {
            let nested = genai::Error::HttpError {
                status: genai_reqwest::StatusCode::UNAUTHORIZED,
                canonical_reason: "Unauthorized".to_string(),
                body: "expired".to_string(),
            };
            genai::Error::WebStream {
                model_iden: genai::ModelIden::new(
                    genai::adapter::AdapterKind::OpenAIResp,
                    "gpt-5.6-sol",
                ),
                cause: nested.to_string(),
                error: Box::new(nested),
            }
        }

        let error = nested_unauthorized();
        assert!(
            StreamOnceError::Transport {
                error,
                observable_output_delivered: false,
            }
            .unauthorized_before_output()
        );

        let error = nested_unauthorized();
        assert!(
            !StreamOnceError::Transport {
                error,
                observable_output_delivered: true,
            }
            .unauthorized_before_output()
        );
    }

    #[tokio::test]
    async fn sse_fixture_normalizes_text_reasoning_tools_and_usage() {
        assert_sse_fixture("gpt-5.6-sol", None, "gpt-5.6-sol", None).await;
    }

    #[tokio::test]
    async fn gpt6_requests_use_supported_options_and_preserve_stream_content() {
        for (model, effort, expected_effort) in [
            ("gpt-6-astra", None, None),
            ("gpt-6-astra", Some(ReasoningEffort::None), None),
            ("gpt-6-astra", Some(ReasoningEffort::Minimal), None),
            ("gpt-6-astra", Some(ReasoningEffort::Low), Some("low")),
            ("gpt-6-astra", Some(ReasoningEffort::Medium), Some("medium")),
            ("gpt-6-astra", Some(ReasoningEffort::High), Some("high")),
            ("gpt-6-astra", Some(ReasoningEffort::XHigh), Some("xhigh")),
            ("gpt-6-astra", Some(ReasoningEffort::Max), Some("max")),
            ("gpt-6-astra-minimal", None, None),
            ("gpt-6-astra-max", None, Some("max")),
            ("gpt-6-astra-max", Some(ReasoningEffort::High), Some("high")),
            ("gpt-6-sol", None, None),
            ("gpt-6-sol", Some(ReasoningEffort::None), Some("none")),
            ("gpt-6-sol", Some(ReasoningEffort::Low), Some("low")),
            ("gpt-6-luna", None, None),
            ("gpt-6-luna", Some(ReasoningEffort::High), Some("high")),
        ] {
            let mut options = ChatOptions::default().with_temperature(0.2).with_top_p(0.8);
            options.reasoning_effort = effort;
            let expected_model = model
                .strip_suffix("-minimal")
                .or_else(|| model.strip_suffix("-max"))
                .unwrap_or(model);
            assert_sse_fixture(model, Some(options), expected_model, expected_effort).await;
        }
    }

    async fn assert_sse_fixture(
        model: &str,
        options: Option<ChatOptions>,
        expected_model: &str,
        expected_effort: Option<&str>,
    ) {
        #[derive(Clone, Default)]
        struct Capture(Arc<Mutex<Option<(HeaderMap, serde_json::Value)>>>);

        async fn respond(
            State(capture): State<Capture>,
            headers: HeaderMap,
            axum::Json(body): axum::Json<serde_json::Value>,
        ) -> axum::response::Response {
            let completed = serde_json::json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_stream",
                    "status": "completed",
                    "model": body["model"],
                    "output": [],
                    "usage": { "input_tokens": 3, "output_tokens": 4, "total_tokens": 7 }
                }
            });
            *capture.0.lock().unwrap() = Some((headers, body));
            let fixture = concat!(
                "event: response.output_text.delta\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"answer\"}\n\n",
                "event: response.reasoning_summary_text.delta\n",
                "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"thinking\"}\n\n",
                "event: response.output_item.added\n",
                "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"code_read\"}}\n\n",
                "event: response.function_call_arguments.delta\n",
                "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":1,\"delta\":\"{\\\"path\\\":\\\"src/lib.rs\\\"}\"}\n\n"
            );
            let fixture = format!("{fixture}event: response.completed\ndata: {completed}\n\n");
            axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from(fixture))
                .unwrap()
        }

        let capture = Capture::default();
        let router = axum::Router::new()
            .route("/responses", axum::routing::post(respond))
            .with_state(capture.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = OpenAiCodexChatClient::with_url(model, format!("http://{address}/responses"));
        let (tx, mut rx) = mpsc::channel(8);
        let image_base64 = Arc::<str>::from(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==",
        );
        let request = ChatRequest::new(vec![genai::chat::ChatMessage::user(
            MessageContent::from_parts(vec![
                genai::chat::ContentPart::from_text("inspect the image"),
                genai::chat::ContentPart::from_binary_base64(
                    "image/png",
                    image_base64,
                    Some("pixel.png".to_string()),
                ),
            ]),
        )])
        .with_system("use tools");
        let response = client
            .stream_once(
                &("oauth-secret".to_string(), "acct_123".to_string()),
                request,
                options.as_ref(),
                Some(&tx),
            )
            .await
            .unwrap();

        assert_eq!(response.first_text(), Some("answer"));
        assert_eq!(response.reasoning_content.as_deref(), Some("thinking"));
        let calls = response.content.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].fn_name, "code_read");
        assert_eq!(calls[0].fn_arguments["path"], "src/lib.rs");
        assert_eq!(response.usage.total_tokens, Some(7));
        assert!(matches!(rx.try_recv(), Ok(StreamDelta::Content(text)) if text == "answer"));
        assert!(matches!(rx.try_recv(), Ok(StreamDelta::Reasoning(text)) if text == "thinking"));

        let (headers, body) = capture.0.lock().unwrap().take().unwrap();
        assert_eq!(headers.get("authorization").unwrap(), "Bearer oauth-secret");
        assert_eq!(headers.get("chatgpt-account-id").unwrap(), "acct_123");
        assert_eq!(headers.get("version").unwrap(), CODEX_COMPAT_VERSION);
        assert_eq!(body["model"], expected_model);
        assert_eq!(body["reasoning"]["effort"].as_str(), expected_effort);
        assert!(body.get("temperature").is_none());
        assert!(body.get("top_p").is_none());
        assert_eq!(body["instructions"], "use tools");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        let user_content = body["input"]
            .as_array()
            .and_then(|items| {
                items.iter().find_map(|item| {
                    (item["role"] == "user")
                        .then(|| item["content"].as_array())
                        .flatten()
                })
            })
            .expect("user content parts");
        assert_eq!(user_content[0]["type"], "input_text");
        assert_eq!(user_content[0]["text"], "inspect the image");
        assert_eq!(user_content[1]["type"], "input_image");
        assert_eq!(user_content[1]["detail"], "auto");
        assert!(
            user_content[1]["image_url"]
                .as_str()
                .is_some_and(|url| url.starts_with("data:image/png;base64,"))
        );
    }

    fn completed_sse(model: &str, text: &str) -> String {
        let completed = serde_json::json!({
            "type": "response.completed",
            "response": {
                "id": "resp_retry",
                "status": "completed",
                "model": model,
                "output": [],
                "usage": { "input_tokens": 1, "output_tokens": 1, "total_tokens": 2 }
            }
        });
        format!(
            "event: response.output_text.delta\ndata: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\nevent: response.completed\ndata: {}\n\n",
            serde_json::to_string(text).unwrap(),
            completed
        )
    }

    fn test_request() -> ChatRequest {
        ChatRequest::new(vec![genai::chat::ChatMessage::user("hello")])
    }

    #[tokio::test]
    async fn transient_http_failure_retries_then_succeeds_without_exposing_credentials() {
        #[derive(Clone)]
        struct StateData {
            calls: Arc<AtomicUsize>,
            headers: Arc<Mutex<Vec<HeaderMap>>>,
        }

        async fn respond(
            State(state): State<StateData>,
            headers: HeaderMap,
            axum::Json(body): axum::Json<serde_json::Value>,
        ) -> axum::response::Response {
            let call = state.calls.fetch_add(1, Ordering::SeqCst);
            state.headers.lock().unwrap().push(headers);
            if call == 0 {
                return axum::response::Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from("temporary backend outage"))
                    .unwrap();
            }
            axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from(completed_sse(
                    body["model"].as_str().unwrap_or("gpt-5.6-sol"),
                    "recovered",
                )))
                .unwrap()
        }

        let state = StateData {
            calls: Arc::new(AtomicUsize::new(0)),
            headers: Arc::new(Mutex::new(Vec::new())),
        };
        let router = axum::Router::new()
            .route("/responses", axum::routing::post(respond))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let client =
            OpenAiCodexChatClient::with_url("gpt-5.6-sol", format!("http://{address}/responses"));
        let token = "mock-oauth-secret".to_string();
        let account = "mock-account".to_string();
        let response = client
            .stream_with_retries(
                &(token.clone(), account.clone()),
                test_request(),
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(state.calls.load(Ordering::SeqCst), 2);
        assert_eq!(response.first_text(), Some("recovered"));
        let headers = state.headers.lock().unwrap();
        assert_eq!(headers.len(), 2);
        for request_headers in headers.iter() {
            assert_eq!(
                request_headers["authorization"].to_str().unwrap(),
                format!("Bearer {token}")
            );
            assert_eq!(
                request_headers["chatgpt-account-id"].to_str().unwrap(),
                account
            );
            assert!(!request_headers.contains_key("x-api-key"));
        }
    }

    #[tokio::test]
    async fn schema_bad_request_is_not_retried_and_does_not_leak_auth() {
        let calls = Arc::new(AtomicUsize::new(0));
        async fn reject(State(calls): State<Arc<AtomicUsize>>) -> axum::response::Response {
            calls.fetch_add(1, Ordering::SeqCst);
            axum::response::Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("content-type", "application/json")
                .body(axum::body::Body::from("invalid request schema"))
                .unwrap()
        }

        let router = axum::Router::new()
            .route("/responses", axum::routing::post(reject))
            .with_state(calls.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let client =
            OpenAiCodexChatClient::with_url("gpt-5.6-sol", format!("http://{address}/responses"));
        let error = client
            .stream_with_retries(
                &("mock-oauth-secret".to_string(), "mock-account".to_string()),
                test_request(),
                None,
                None,
            )
            .await
            .unwrap_err();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let rendered = format!("{error:?}");
        assert!(rendered.contains("invalid request schema"));
        assert!(!rendered.contains("mock-oauth-secret"));
        assert!(!rendered.contains("mock-account"));
    }

    #[tokio::test]
    async fn exhausted_transient_statuses_stop_after_three_total_attempts() {
        let calls = Arc::new(AtomicUsize::new(0));
        async fn unavailable(State(calls): State<Arc<AtomicUsize>>) -> axum::response::Response {
            calls.fetch_add(1, Ordering::SeqCst);
            axum::response::Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .header("content-type", "application/json")
                .body(axum::body::Body::from("temporary gateway failure"))
                .unwrap()
        }

        let router = axum::Router::new()
            .route("/responses", axum::routing::post(unavailable))
            .with_state(calls.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let client =
            OpenAiCodexChatClient::with_url("gpt-5.6-sol", format!("http://{address}/responses"));
        let error = client
            .stream_with_retries(
                &("mock-oauth-secret".to_string(), "mock-account".to_string()),
                test_request(),
                None,
                None,
            )
            .await
            .unwrap_err();

        assert_eq!(calls.load(Ordering::SeqCst), RESPONSES_MAX_ATTEMPTS);
        assert!(matches!(error, StreamOnceError::Transport { .. }));
    }

    #[tokio::test]
    async fn clean_eof_before_terminal_response_retries_when_no_output_was_delivered() {
        let calls = Arc::new(AtomicUsize::new(0));
        async fn partial_then_complete(
            State(calls): State<Arc<AtomicUsize>>,
            axum::Json(body): axum::Json<serde_json::Value>,
        ) -> axum::response::Response {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            let payload = if call == 0 {
                "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"discarded partial\"}\n\n".to_string()
            } else {
                completed_sse(
                    body["model"].as_str().unwrap_or("gpt-5.6-sol"),
                    "terminal answer",
                )
            };
            axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from(payload))
                .unwrap()
        }

        let router = axum::Router::new()
            .route("/responses", axum::routing::post(partial_then_complete))
            .with_state(calls.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let client =
            OpenAiCodexChatClient::with_url("gpt-5.6-sol", format!("http://{address}/responses"));
        let response = client
            .stream_with_retries(
                &("mock-oauth-secret".to_string(), "mock-account".to_string()),
                test_request(),
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(response.first_text(), Some("terminal answer"));
        assert_eq!(response.response_id.as_deref(), Some("resp_retry"));
        assert_eq!(
            response
                .stop_reason
                .as_ref()
                .map(genai::chat::StopReason::raw),
            Some("completed")
        );
    }

    #[tokio::test]
    async fn pre_output_connection_failures_retry_three_times() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let server_accepted = accepted.clone();
        let server = tokio::spawn(async move {
            for _ in 0..RESPONSES_MAX_ATTEMPTS {
                let (stream, _) = listener.accept().await.unwrap();
                server_accepted.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });

        let client =
            OpenAiCodexChatClient::with_url("gpt-5.6-sol", format!("http://{address}/responses"));
        let error = client
            .stream_with_retries(
                &("mock-oauth-secret".to_string(), "mock-account".to_string()),
                test_request(),
                None,
                None,
            )
            .await
            .unwrap_err();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("client should make exactly three local connection attempts")
            .unwrap();

        assert_eq!(accepted.load(Ordering::SeqCst), RESPONSES_MAX_ATTEMPTS);
        assert!(matches!(
            error,
            StreamOnceError::Transport {
                observable_output_delivered: false,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn clean_eof_after_a_delivered_delta_is_never_replayed() {
        let calls = Arc::new(AtomicUsize::new(0));
        async fn close_after_delta(
            State(calls): State<Arc<AtomicUsize>>,
        ) -> axum::response::Response {
            calls.fetch_add(1, Ordering::SeqCst);
            let events = stream::iter([Ok::<_, std::io::Error>(
                "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n".to_string(),
            )]);
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from_stream(events))
                .unwrap()
        }

        let router = axum::Router::new()
            .route("/responses", axum::routing::post(close_after_delta))
            .with_state(calls.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let client =
            OpenAiCodexChatClient::with_url("gpt-5.6-sol", format!("http://{address}/responses"));
        let (tx, mut rx) = mpsc::channel(8);
        let error = client
            .stream_with_retries(
                &("mock-oauth-secret".to_string(), "mock-account".to_string()),
                test_request(),
                None,
                Some(&tx),
            )
            .await
            .unwrap_err();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(matches!(rx.try_recv(), Ok(StreamDelta::Content(text)) if text == "partial"));
        assert!(matches!(
            error,
            StreamOnceError::IncompleteStream {
                observable_output_delivered: true,
            }
        ));
    }
}
