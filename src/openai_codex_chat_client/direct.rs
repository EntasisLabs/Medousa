//! Public Responses wire format for the Sign in with ChatGPT OSS preview.
use super::*;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use genai::chat::{BinarySource, ChatRole, ContentPart, ToolCall, ToolChoice, ToolName};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

fn invalid(message: &str) -> StreamOnceError {
    StreamOnceError::Failure {
        status: 400,
        body: json!({"error": {"code": "invalid_request", "message": message}}),
        request_id: None,
        observable_output_delivered: false,
    }
}

fn signature_item(signature: &str) -> Value {
    serde_json::from_str::<Value>(signature)
        .ok()
        .filter(|item| item["type"] == "reasoning")
        .unwrap_or_else(
            || json!({"type": "reasoning", "encrypted_content": signature, "summary": []}),
        )
}

pub(super) fn request_body(
    model: &str,
    request: ChatRequest,
    options: &ChatOptions,
) -> Result<Value, StreamOnceError> {
    let mut input = Vec::new();
    for message in request.messages {
        let role = match message.role {
            ChatRole::System => "developer",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool => "user",
        };
        let mut parts = Vec::new();
        let mut seen_signatures = HashSet::new();
        let flush = |parts: &mut Vec<Value>, input: &mut Vec<Value>| {
            if !parts.is_empty() {
                input.push(json!({"role": role, "content": std::mem::take(parts)}));
            }
        };
        for part in message.content.parts() {
            match part {
                ContentPart::Text(text) => parts.push(json!({"type": if role == "assistant" { "output_text" } else { "input_text" }, "text": text})),
                ContentPart::Binary(binary) => {
                    if binary.content_type.starts_with("audio/") || binary.content_type.starts_with("video/") { return Err(invalid("ChatGPT plan usage does not support audio/video input")); }
                    let data = match &binary.source { BinarySource::Base64(b) => format!("data:{};base64,{b}", binary.content_type), BinarySource::Url(u) => u.clone() };
                    parts.push(if binary.content_type.starts_with("image/") { json!({"type": "input_image", "image_url": data, "detail": "auto"}) }
                        else if matches!(binary.source, BinarySource::Url(_)) { json!({"type": "input_file", "file_url": data}) }
                        else { json!({"type": "input_file", "file_data": data, "filename": binary.name.as_deref().unwrap_or("attachment")}) });
                }
                ContentPart::ToolCall(call) => {
                    flush(&mut parts, &mut input);
                    for signature in call.thought_signatures.iter().flatten() {
                        if seen_signatures.insert(signature.clone()) { input.push(signature_item(signature)); }
                    }
                    input.push(json!({"type": "function_call", "call_id": call.call_id, "name": call.fn_name, "namespace": "medousa", "arguments": call.fn_arguments.to_string()}));
                }
                ContentPart::ToolResponse(response) => {
                    flush(&mut parts, &mut input);
                    input.push(json!({"type": "function_call_output", "call_id": response.call_id, "output": response.content}));
                }
                ContentPart::ThoughtSignature(signature) => {
                    flush(&mut parts, &mut input);
                    if seen_signatures.insert(signature.clone()) { input.push(signature_item(signature)); }
                }
                ContentPart::ReasoningContent(_) => {} // Summary text is display-only; encrypted items carry replay state.
                ContentPart::Custom(_) => return Err(invalid("Unsupported ChatGPT input part")),
            }
        }
        flush(&mut parts, &mut input);
    }
    let mut body = json!({"model": model, "input": input, "stream": true, "store": false});
    if crate::reasoning_effort::reasoning_capability(OPENAI_CODEX_PROVIDER_ID, model).kind
        == "effort"
    {
        body["reasoning"] = json!({"summary": "detailed"});
        body["include"] = json!(["reasoning.encrypted_content"]);
    }
    if let Some(instructions) = request.system {
        body["instructions"] = instructions.into();
    }
    if let Some(effort) = &options.reasoning_effort {
        body["reasoning"]["effort"] = effort.to_string().to_lowercase().into();
    }
    let mut functions = Vec::new();
    let mut tools = Vec::new();
    for tool in request.tools.unwrap_or_default() {
        match tool.name {
            ToolName::Custom(name) => functions.push(json!({"type": "function", "name": name,
                "description": tool.description.unwrap_or_default(), "parameters": tool.schema.unwrap_or(json!({"type":"object", "properties":{}})), "strict": tool.strict.unwrap_or(false)})),
            ToolName::WebSearch => tools.push(json!({"type": "web_search"})),
        }
    }
    if !functions.is_empty() {
        tools.push(json!({"type": "namespace", "name": "medousa", "description": "Medousa tools executed by the workshop", "tools": functions}));
    }
    if !tools.is_empty() {
        body["tools"] = tools.into();
    }
    if let Some(choice) = &options.tool_choice {
        body["tool_choice"] = match choice {
            ToolChoice::Auto => "auto".into(),
            ToolChoice::None => "none".into(),
            ToolChoice::Required => "required".into(),
            ToolChoice::Tool { name } => {
                json!({"type": "function", "namespace": "medousa", "name": name})
            }
        };
    }
    if let Some(format) = &options.response_format {
        body["text"]["format"] = match format {
            genai::chat::ChatResponseFormat::JsonMode => json!({"type":"json_object"}),
            genai::chat::ChatResponseFormat::JsonSpec(spec) => {
                json!({"type":"json_schema", "name":spec.name, "schema":spec.schema, "strict":false})
            }
        };
    }
    if let Some(verbosity) = &options.verbosity {
        body["text"]["verbosity"] = verbosity.to_string().to_lowercase().into();
    }
    // Never merge extra_body, extra_headers, continuation IDs, or sampling options:
    // they can reintroduce fields disallowed by this route or override its credentials.
    Ok(body)
}

pub(super) async fn stream_once(
    client: &OpenAiCodexChatClient,
    credentials: &(String, String),
    request: ChatRequest,
    options: Option<&ChatOptions>,
    chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
) -> Result<ChatResponse, StreamOnceError> {
    let options = client.stream_options(options);
    let (_, model) = ReasoningEffort::from_model_name(client.model.trim());
    let body = request_body(model, request, &options)?;
    let model_iden = genai::ModelIden::new(genai::adapter::AdapterKind::OpenAIResp, model);
    let transport_error =
        |error: genai_reqwest::Error, observable_output_delivered| StreamOnceError::Transport {
            error: genai::Error::WebStream {
                model_iden: model_iden.clone(),
                cause: "Responses transport failed".into(),
                error: Box::new(error.without_url()),
            },
            observable_output_delivered,
        };
    let response = genai_reqwest::Client::builder()
        .connect_timeout(RESPONSES_CONNECT_TIMEOUT)
        .read_timeout(RESPONSES_READ_IDLE_TIMEOUT)
        .redirect(genai_reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| transport_error(e, false))?
        .post(&client.responses_url)
        .bearer_auth(&credentials.0)
        .header("Accept", "text/event-stream")
        .json(&body)
        .send()
        .await
        .map_err(|e| transport_error(e, false))?;
    let status = response.status().as_u16();
    let request_id = response
        .headers()
        .get("x-request-id")
        .or_else(|| response.headers().get("openai-request-id"))
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    if !(200..300).contains(&status) {
        let text = response
            .text()
            .await
            .map_err(|e| transport_error(e, false))?;
        let body = serde_json::from_str(&text).unwrap_or(json!({"detail": text}));
        return Err(StreamOnceError::Failure {
            status,
            body,
            request_id,
            observable_output_delivered: false,
        });
    }
    let mut events = response.bytes_stream().eventsource();
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut items = BTreeMap::<u64, Value>::new();
    let mut observable_output_delivered = false;
    while let Some(event) = events.next().await {
        let event = match event {
            Ok(event) => event,
            Err(eventsource_stream::EventStreamError::Transport(e)) => {
                return Err(transport_error(e, observable_output_delivered));
            }
            Err(_) => return Err(invalid("Invalid Responses event stream")),
        };
        if event.data == "[DONE]" {
            break;
        }
        let value: Value =
            serde_json::from_str(&event.data).map_err(|_| invalid("Invalid Responses event"))?;
        match value["type"].as_str().unwrap_or(&event.event) {
            "response.output_text.delta"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_text.delta" => {
                let delta = value["delta"].as_str().unwrap_or_default().to_string();
                let is_text = value["type"] == "response.output_text.delta";
                if is_text {
                    text.push_str(&delta);
                } else {
                    reasoning.push_str(&delta);
                }
                if !delta.is_empty()
                    && let Some(tx) = chunk_tx
                {
                    send_stream_delta(
                        tx,
                        if is_text {
                            StreamDelta::Content(delta)
                        } else {
                            StreamDelta::Reasoning(delta)
                        },
                    )
                    .await
                    .map_err(StreamOnceError::Delivery)?;
                    observable_output_delivered = true;
                }
            }
            "response.output_item.added" | "response.output_item.done" => {
                items.insert(
                    value["output_index"].as_u64().unwrap_or_default(),
                    value["item"].clone(),
                );
            }
            "response.function_call_arguments.delta" => {
                let item = items
                    .entry(value["output_index"].as_u64().unwrap_or_default())
                    .or_insert(json!({}));
                let arguments = format!(
                    "{}{}",
                    item["arguments"].as_str().unwrap_or_default(),
                    value["delta"].as_str().unwrap_or_default()
                );
                item["arguments"] = arguments.into();
            }
            "response.failed" | "error" | "response.incomplete" => {
                let body = if value["response"].is_object() {
                    value["response"].clone()
                } else {
                    value.clone()
                };
                let code = body["error"]["code"]
                    .as_str()
                    .or(body["code"].as_str())
                    .unwrap_or_default();
                let status = match code {
                    "subscription_sharing_usage_limit_exceeded" => 429,
                    "subscription_sharing_usage_unavailable"
                    | "subscription_sharing_user_unavailable" => 503,
                    "subscription_sharing_invalid_user" => 401,
                    _ => 400,
                };
                return Err(StreamOnceError::Failure {
                    status,
                    body,
                    request_id,
                    observable_output_delivered,
                });
            }
            "response.completed" => {
                let response = &value["response"];
                let id = response["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| invalid("Completed response has no ID"))?;
                if response["status"]
                    .as_str()
                    .is_some_and(|s| s != "completed")
                {
                    return Err(invalid("Response did not complete"));
                }
                if let Some(output) = response["output"].as_array()
                    && !output.is_empty()
                {
                    items = output
                        .iter()
                        .enumerate()
                        .map(|(i, item)| (i as u64, item.clone()))
                        .collect();
                }
                let mut parts = Vec::new();
                let mut signatures = Vec::new();
                let capture_text = text.is_empty();
                let capture_reasoning = reasoning.is_empty();
                for item in items.values() {
                    if capture_reasoning && item["type"] == "reasoning" {
                        for part in item["summary"].as_array().into_iter().flatten() {
                            if let Some(summary) = part["text"].as_str() {
                                reasoning.push_str(summary);
                            }
                        }
                    }
                    match item["type"].as_str() {
                        Some("reasoning") if item["encrypted_content"].is_string() => signatures
                            .push(
                                item["encrypted_content"]
                                    .as_str()
                                    .expect("encrypted reasoning")
                                    .to_string(),
                            ),
                        Some("message") if capture_text => {
                            for p in item["content"].as_array().into_iter().flatten() {
                                if let Some(t) = p["text"].as_str() {
                                    text.push_str(t);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if !text.is_empty() {
                    parts.push(ContentPart::Text(text));
                }
                parts.extend(
                    signatures
                        .iter()
                        .cloned()
                        .map(ContentPart::ThoughtSignature),
                );
                for item in items
                    .values()
                    .filter(|item| item["type"] == "function_call")
                {
                    if item["namespace"].as_str().is_some_and(|n| n != "medousa") {
                        return Err(invalid("Unexpected tool namespace"));
                    }
                    let call = ToolCall {
                        call_id: item["call_id"]
                            .as_str()
                            .ok_or_else(|| invalid("Missing tool call ID"))?
                            .into(),
                        fn_name: item["name"]
                            .as_str()
                            .ok_or_else(|| invalid("Missing tool name"))?
                            .into(),
                        fn_arguments: serde_json::from_str(
                            item["arguments"].as_str().unwrap_or("{}"),
                        )
                        .map_err(|_| invalid("Invalid tool arguments"))?,
                        thought_signatures: (!signatures.is_empty()).then(|| signatures.clone()),
                    };
                    parts.push(ContentPart::ToolCall(call));
                }
                let wire_usage = &response["usage"];
                let usage = serde_json::from_value(json!({"prompt_tokens": wire_usage["input_tokens"], "completion_tokens": wire_usage["output_tokens"],
                    "total_tokens": wire_usage["total_tokens"], "prompt_tokens_details": wire_usage["input_tokens_details"], "completion_tokens_details": wire_usage["output_tokens_details"]})).unwrap_or_default();
                return Ok(ChatResponse {
                    content: MessageContent::from_parts(parts),
                    reasoning_content: (!reasoning.is_empty()).then_some(reasoning),
                    model_iden: model_iden.clone(),
                    provider_model_iden: model_iden,
                    stop_reason: Some(genai::chat::StopReason::from("completed".to_string())),
                    usage,
                    response_id: Some(id.into()),
                    captured_raw_body: None,
                });
            }
            _ => {}
        }
        if text.len() + reasoning.len() + items.values().map(|i| i.to_string().len()).sum::<usize>()
            > 16 * 1024 * 1024
        {
            return Err(invalid("Responses stream exceeded capture limit"));
        }
    }
    Err(StreamOnceError::IncompleteStream {
        observable_output_delivered,
    })
}
