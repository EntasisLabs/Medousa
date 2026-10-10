use super::*;
use genai::chat::{ChatMessage, ContentPart, Tool, ToolCall, ToolResponse};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn public_request_namespaces_tools_replays_history_and_omits_preview_fields() {
    let call = ToolCall {
        call_id: "call_1".into(),
        fn_name: "code_read".into(),
        fn_arguments: json!({"path":"src/lib.rs"}),
        thought_signatures: None,
    };
    let reasoning =
        json!({"type":"reasoning","id":"rs_1","encrypted_content":"opaque", "summary":[]});
    let request = ChatRequest::new(vec![
        ChatMessage::system("system instruction"),
        ChatMessage::user("read code"),
        ChatMessage::assistant(MessageContent::from_parts(vec![
            ContentPart::ThoughtSignature(reasoning.to_string()),
            ContentPart::ToolCall(call.clone()),
        ])),
        ChatMessage::from(ToolResponse::from_tool_call(&call, "contents")),
    ])
    .with_tools(vec![Tool::new("code_read").with_schema(
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
    )])
    .with_previous_response_id("old-response")
    .with_store(true);
    let options = ChatOptions::default().with_temperature(0.4).with_max_tokens(10).with_extra_body(json!({"store":true,"stream":false,"metadata":{"secret":"ignored"},"tools":[{"type":"tool_search"}]}));
    let body = direct::request_body("gpt-6.1-sol", request, &options).unwrap();
    assert_eq!(body["store"], false);
    assert_eq!(body["stream"], true);
    assert_eq!(body["tools"][0]["type"], "namespace");
    assert_eq!(body["tools"][0]["name"], "medousa");
    assert_eq!(body["tools"][0]["tools"][0]["name"], "code_read");
    for field in [
        "previous_response_id",
        "temperature",
        "top_p",
        "max_output_tokens",
        "metadata",
        "background",
    ] {
        assert!(body.get(field).is_none(), "{field}");
    }
    assert_eq!(body["input"][0]["role"], "developer");
    assert_eq!(body["input"][2], reasoning);
    assert_eq!(body["input"][3]["namespace"], "medousa");
    assert_eq!(body["input"][4]["type"], "function_call_output");
    assert_eq!(body["input"][4]["output"], "contents");
}

#[test]
fn opaque_reasoning_on_tool_calls_is_replayed_once_before_the_call() {
    let call = ToolCall {
        call_id: "call_opaque".into(),
        fn_name: "code_read".into(),
        fn_arguments: json!({}),
        thought_signatures: Some(vec!["opaque-ciphertext".into()]),
    };
    for parts in [
        vec![ContentPart::ToolCall(call.clone())],
        vec![
            ContentPart::ThoughtSignature("opaque-ciphertext".into()),
            ContentPart::ToolCall(call),
        ],
    ] {
        let body = direct::request_body(
            "gpt-6.1-sol",
            ChatRequest::new(vec![ChatMessage::assistant(MessageContent::from_parts(
                parts,
            ))]),
            &ChatOptions::default(),
        )
        .unwrap();
        assert_eq!(body["input"].as_array().unwrap().len(), 2);
        assert_eq!(body["input"][0]["encrypted_content"], "opaque-ciphertext");
        assert_eq!(body["input"][1]["call_id"], "call_opaque");
    }
}

async fn failure_fixture(
    status: u16,
    body: String,
    stream: bool,
    with_deltas: bool,
) -> (StreamOnceError, usize) {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let router = axum::Router::new().route(
        "/responses",
        axum::routing::post(move || {
            let body = body.clone();
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                axum::response::Response::builder()
                    .status(status)
                    .header("x-request-id", "request_fixture")
                    .header(
                        "content-type",
                        if stream {
                            "text/event-stream"
                        } else {
                            "application/json"
                        },
                    )
                    .body(axum::body::Body::from(body))
                    .unwrap()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/responses", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = OpenAiCodexChatClient::with_url("gpt-6.1-sol", url);
    let (tx, mut rx) = mpsc::channel(8);
    let error = client
        .stream_with_retries(
            &("oauth-fixture".into(), "oaiapp_fixture".into()),
            ChatRequest::new(vec![ChatMessage::user("hello")]),
            None,
            if with_deltas { Some(&tx) } else { None },
        )
        .await
        .unwrap_err();
    if with_deltas {
        assert!(matches!(rx.try_recv(),Ok(StreamDelta::Content(text)) if text=="partial"));
    }
    (error, calls.load(Ordering::SeqCst))
}
#[tokio::test]
async fn usage_limit_before_stream_is_not_retried_and_preserves_diagnostics() {
    let body=json!({"error":{"code":"subscription_sharing_usage_limit_exceeded","param":"model","message":"App limit reached"}}).to_string();
    let (error, calls) = failure_fixture(429, body, false, false).await;
    assert_eq!(calls, 1);
    let message = stream_once_error("gpt-6.1-sol", error).to_string();
    for detail in [
        "429",
        "subscription_sharing_usage_limit_exceeded",
        "model",
        "request_fixture",
        "https://chatgpt.com/settings/usage",
    ] {
        assert!(message.contains(detail));
    }
    assert!(!message.contains("oauth-fixture"));
}
#[tokio::test]
async fn usage_failure_after_deltas_and_incomplete_events_never_succeed() {
    let event = json!({"type":"response.failed","response":{"id":"resp_failed","status":"failed","error":{"code":"subscription_sharing_usage_limit_exceeded","message":"App limit reached"}}});
    let (error,calls)=failure_fixture(200,format!("event: response.output_text.delta\ndata: {{\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}}\n\nevent: response.failed\ndata: {event}\n\n"),true,true).await;
    assert_eq!(calls, 1);
    assert!(!error.can_retry_before_output());
    let event = json!({"type":"response.incomplete","response":{"id":"resp_incomplete","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}}});
    let (_, calls) = failure_fixture(
        200,
        format!("event: response.incomplete\ndata: {event}\n\n"),
        true,
        false,
    )
    .await;
    assert_eq!(calls, 1);
}
#[tokio::test]
async fn direct_admission_detail_shape_is_preserved() {
    let (error, calls) = failure_fixture(
        403,
        json!({"detail":"Serving region restriction"}).to_string(),
        false,
        false,
    )
    .await;
    assert_eq!(calls, 1);
    let message = stream_once_error("gpt-6.1-sol", error).to_string();
    assert!(message.contains("detail"));
    assert!(message.contains("Serving region restriction"));
}
