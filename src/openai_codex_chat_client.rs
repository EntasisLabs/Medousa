//! Native ChatGPT-account Responses transport with Medousa loop ownership.

use async_trait::async_trait;
use futures_util::StreamExt;
use genai::chat::{ChatOptions, ChatRequest, ChatResponse, ChatStreamEvent, MessageContent, Usage};
use genai::resolver::AuthData;
use genai::{Client, Headers};
use stasis::application::runtime::chat_options_resolver::apply_model_reasoning_suffix;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::{
    AiChatClient, StreamDelta, send_stream_delta,
};
use tokio::sync::mpsc;

use crate::inference_router::OPENAI_CODEX_PROVIDER_ID;

const DEFAULT_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";

#[derive(Debug)]
enum StreamOnceError {
    Transport(genai::Error),
    Delivery(StasisError),
}

impl From<genai::Error> for StreamOnceError {
    fn from(error: genai::Error) -> Self {
        Self::Transport(error)
    }
}

/// Version of the Codex backend contract implemented by this adapter. This is
/// intentionally independent from Medousa's product version: the ChatGPT Codex
/// backend gates newer models on this protocol identity.
pub(crate) const CODEX_COMPAT_VERSION: &str = "0.145.0";
pub(crate) const CODEX_COMPAT_ORIGINATOR: &str = "codex_cli_rs";

pub(crate) fn codex_compat_user_agent() -> String {
    format!("{CODEX_COMPAT_ORIGINATOR}/{CODEX_COMPAT_VERSION}")
}

#[derive(Clone)]
pub struct OpenAiCodexChatClient {
    model: String,
    responses_url: String,
}

impl OpenAiCodexChatClient {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            responses_url: std::env::var("MEDOUSA_CHATGPT_RESPONSES_URL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_RESPONSES_URL.to_string()),
        }
    }

    #[cfg(test)]
    fn with_url(model: impl Into<String>, responses_url: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            responses_url: responses_url.into(),
        }
    }

    fn client(&self, access_token: &str, account_id: &str) -> Client {
        let headers = request_headers(access_token, account_id);
        let url = self.responses_url.clone();
        Client::builder()
            .with_auth_resolver_fn(move |_| {
                Ok(Some(AuthData::RequestOverride {
                    url: url.clone(),
                    headers: headers.clone(),
                }))
            })
            .build()
    }

    fn model_target(&self) -> String {
        format!("openai_resp::{}", self.model.trim())
    }

    async fn credentials(&self) -> StasisResult<(String, String)> {
        crate::chatgpt_oauth::request_credentials()
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))
    }

    async fn refreshed_credentials(
        &self,
        rejected_access_token: &str,
    ) -> StasisResult<(String, String)> {
        crate::chatgpt_oauth::refresh_request_credentials(rejected_access_token)
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))
    }

    async fn stream_once(
        &self,
        credentials: &(String, String),
        request: ChatRequest,
        options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> Result<ChatResponse, StreamOnceError> {
        let mut stream_options =
            apply_model_reasoning_suffix(&self.model, options.cloned().unwrap_or_default());
        stream_options = stream_options
            .with_capture_content(true)
            .with_capture_usage(true)
            .with_capture_tool_calls(true)
            .with_capture_reasoning_content(true)
            .with_normalize_reasoning_content(true);

        let mut stream_response = self
            .client(&credentials.0, &credentials.1)
            .exec_chat_stream(self.model_target(), request, Some(&stream_options))
            .await?;
        let model_iden = stream_response.model_iden.clone();
        let mut streamed_text = String::new();
        let mut reasoning_text = String::new();
        let mut captured_content: Option<MessageContent> = None;
        let mut captured_reasoning_content: Option<String> = None;
        let mut usage = Usage::default();

        while let Some(event) = stream_response.stream.next().await {
            match event? {
                ChatStreamEvent::Chunk(chunk) => {
                    if !chunk.content.is_empty() {
                        streamed_text.push_str(&chunk.content);
                        if let Some(tx) = chunk_tx {
                            send_stream_delta(tx, StreamDelta::Content(chunk.content))
                                .await
                                .map_err(StreamOnceError::Delivery)?;
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
                    }
                }
                ChatStreamEvent::End(end) => {
                    captured_content = end.captured_content;
                    captured_reasoning_content = end.captured_reasoning_content;
                    usage = end.captured_usage.unwrap_or_default();
                }
                _ => {}
            }
        }

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
            stop_reason: None,
            usage,
            captured_raw_body: None,
            response_id: None,
        })
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
            .stream_once(&credentials, request.clone(), options, None)
            .await
        {
            Ok(response) => Ok(response),
            Err(StreamOnceError::Transport(error)) if is_unauthorized(&error) => {
                let refreshed = self.refreshed_credentials(&credentials.0).await?;
                self.stream_once(&refreshed, request, options, None)
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
            .stream_once(&credentials, request.clone(), options, chunk_tx)
            .await
        {
            Ok(response) => Ok(response),
            Err(StreamOnceError::Transport(error)) if is_unauthorized(&error) => {
                let refreshed = self.refreshed_credentials(&credentials.0).await?;
                self.stream_once(&refreshed, request.clone(), options, chunk_tx)
                    .await
                    .map_err(|error| stream_once_error(&self.model, error))
            }
            Err(error) => Err(stream_once_error(&self.model, error)),
        }
    }
}

pub enum RoutedChatClient {
    Provider(GenaiChatClient),
    ChatGpt(OpenAiCodexChatClient),
}

impl RoutedChatClient {
    pub fn new(provider: &str, model: &str, base_url: Option<&str>) -> Self {
        if provider.eq_ignore_ascii_case(OPENAI_CODEX_PROVIDER_ID) {
            Self::ChatGpt(OpenAiCodexChatClient::new(model))
        } else {
            let target = crate::genai_model_target(provider, model, base_url);
            Self::Provider(GenaiChatClient::from_provider_model_with_base_url(
                None, &target, base_url,
            ))
        }
    }
}

#[async_trait]
impl AiChatClient for RoutedChatClient {
    async fn complete(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        match self {
            Self::Provider(client) => client.complete(request, options).await,
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
            Self::Provider(client) => client.complete_stream(request, options, chunk_tx).await,
            Self::ChatGpt(client) => client.complete_stream(request, options, chunk_tx).await,
        }
    }
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
        _ => false,
    }
}

fn transport_error(model: &str, operation: &str, error: genai::Error) -> StasisError {
    StasisError::PortFailure(format!(
        "ChatGPT Responses {operation} failed for model '{model}': {error}"
    ))
}

fn stream_once_error(model: &str, error: StreamOnceError) -> StasisError {
    match error {
        StreamOnceError::Transport(error) => transport_error(model, "stream", error),
        StreamOnceError::Delivery(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::HeaderMap;
    use std::sync::{Arc, Mutex};

    #[test]
    fn route_selection_keeps_api_key_and_chatgpt_clients_distinct() {
        assert!(matches!(
            RoutedChatClient::new("openai", "gpt-5.6-sol", None),
            RoutedChatClient::Provider(_)
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

    #[tokio::test]
    async fn sse_fixture_normalizes_text_reasoning_tools_and_usage() {
        #[derive(Clone, Default)]
        struct Capture(Arc<Mutex<Option<(HeaderMap, serde_json::Value)>>>);

        async fn respond(
            State(capture): State<Capture>,
            headers: HeaderMap,
            axum::Json(body): axum::Json<serde_json::Value>,
        ) -> axum::response::Response {
            *capture.0.lock().unwrap() = Some((headers, body));
            let fixture = concat!(
                "event: response.output_text.delta\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"answer\"}\n\n",
                "event: response.reasoning_summary_text.delta\n",
                "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"thinking\"}\n\n",
                "event: response.output_item.added\n",
                "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"code_read\"}}\n\n",
                "event: response.function_call_arguments.delta\n",
                "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":1,\"delta\":\"{\\\"path\\\":\\\"src/lib.rs\\\"}\"}\n\n",
                "event: response.completed\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream\",\"status\":\"completed\",\"model\":\"gpt-5.6-sol\",\"output\":[],\"usage\":{\"input_tokens\":3,\"output_tokens\":4,\"total_tokens\":7}}}\n\n"
            );
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
        let client =
            OpenAiCodexChatClient::with_url("gpt-5.6-sol", format!("http://{address}/responses"));
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
                None,
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
        assert_eq!(body["model"], "gpt-5.6-sol");
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
}
