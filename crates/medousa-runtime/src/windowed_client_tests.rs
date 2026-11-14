use super::*;
use genai::chat::{ChatMessage, Tool};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct CountClient(AtomicUsize);
#[async_trait]
impl AiChatClient for CountClient {
    async fn complete(
        &self,
        _request: ChatRequest,
        _options: Option<&ChatOptions>,
    ) -> Result<ChatResponse> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(StasisError::PortFailure("provider_called".into()))
    }
    async fn complete_stream(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
        _chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> Result<ChatResponse> {
        self.complete(request, options).await
    }
}

fn policy(tokens: u32) -> InferenceWindowPolicy {
    InferenceWindowPolicy {
        input_tokens: Some(tokens),
        safety_tokens: 0,
        observation_tokens: 0,
        ..Default::default()
    }
}

#[tokio::test]
async fn smaller_fallback_cannot_dispatch_oversized_original_request() {
    let client = Arc::new(CountClient::default());
    let request = ChatRequest::new(vec![ChatMessage::user("x".repeat(2000))]);
    let large = WindowedChatClient::new(client.clone(), policy(10000));
    assert!(
        large
            .complete(request.clone(), None)
            .await
            .unwrap_err()
            .to_string()
            .contains("provider_called")
    );
    let fallback = WindowedChatClient::new(client.clone(), policy(1000));
    let error = fallback
        .complete_stream(request, None, None)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("context_window_overflow"));
    assert_eq!(client.0.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn post_compile_appends_and_extra_payload_are_checked_at_final_boundary() {
    let client = Arc::new(CountClient::default());
    let guard = WindowedChatClient::new(client.clone(), policy(1000));
    let mut request = ChatRequest::new(vec![ChatMessage::user("current")]);
    assert!(guard.options(&request, None).is_ok());
    request
        .messages
        .push(ChatMessage::system("unaccounted append".repeat(200)));
    assert!(
        guard
            .complete(request, None)
            .await
            .unwrap_err()
            .to_string()
            .contains("context_window_overflow")
    );
    let request = ChatRequest::new(vec![ChatMessage::user("current")]);
    let options =
        ChatOptions::default().with_extra_body(json!({"instructions":"hidden".repeat(500)}));
    assert!(
        guard
            .complete_stream(request, Some(&options), None)
            .await
            .unwrap_err()
            .to_string()
            .contains("extra_body_tokens_estimate")
    );
    assert_eq!(client.0.load(Ordering::SeqCst), 0);
}

#[test]
fn output_reserve_is_enforced_only_where_adapter_supports_it() {
    let client = Arc::new(CountClient::default());
    let request = ChatRequest::new(vec![ChatMessage::user("current")]);
    let guard = WindowedChatClient::new(client.clone(), policy(20000));
    assert_eq!(
        guard.options(&request, None).unwrap().max_tokens,
        Some(4096)
    );
    assert_eq!(
        guard
            .options(&request, Some(&ChatOptions::default().with_max_tokens(128)))
            .unwrap()
            .max_tokens,
        Some(128)
    );
    let bridge = WindowedChatClient::new(
        client,
        InferenceWindowPolicy {
            enforce_output_limit: false,
            ..policy(20000)
        },
    );
    assert_eq!(bridge.options(&request, None).unwrap().max_tokens, None);
}

#[test]
fn hidden_cached_state_and_output_overrides_fail_closed() {
    let guard = WindowedChatClient::new(Arc::new(CountClient::default()), policy(20000));
    let request = ChatRequest::new(vec![ChatMessage::user("current")]);
    let linked = request
        .clone()
        .with_previous_response_id("opaque-server-state");
    assert!(
        guard
            .options(&linked, None)
            .unwrap_err()
            .to_string()
            .contains("unaccounted_cached_state")
    );
    for value in [
        json!({"max_output_tokens":100000}),
        json!({"messages":[]}),
        json!({"previous_response_id":"opaque"}),
    ] {
        assert!(
            guard
                .options(
                    &request,
                    Some(&ChatOptions::default().with_extra_body(value))
                )
                .unwrap_err()
                .to_string()
                .contains("context_window_overflow")
        );
    }
}

#[test]
fn unknown_capacity_and_tool_schema_cost_are_not_assumed_to_fit() {
    let guard = WindowedChatClient::new(
        Arc::new(CountClient::default()),
        InferenceWindowPolicy::default(),
    );
    let small = ChatRequest::new(vec![ChatMessage::user("current")]);
    assert!(guard.options(&small, None).is_ok());
    let large = small.with_tools(vec![
        Tool::new("schema").with_schema(json!({"description":"x".repeat(5000)})),
    ]);
    assert!(
        guard
            .options(&large, None)
            .unwrap_err()
            .to_string()
            .contains("context_window_overflow")
    );
}
