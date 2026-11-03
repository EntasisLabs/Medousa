use super::*;
use async_trait::async_trait;
use genai::chat::{ContentPart, Tool, ToolCall};
use serde_json::{Value, json};
use stasis::application::orchestration::{
    prompt_pipeline::{PromptExecutionContext, PromptExecutionPipeline},
    tool_loop_pipeline::{ToolCallMode, ToolLoopExecutionRequest},
    tool_registry::{InMemoryToolRegistry, ToolRegistry},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

struct ImageLoopClient {
    rounds: AtomicUsize,
    wire: Arc<Mutex<Option<Value>>>,
    parallel: bool,
    live_expected: Option<String>,
}

#[async_trait]
impl AiChatClient for ImageLoopClient {
    async fn complete(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        let first = self.rounds.fetch_add(1, Ordering::SeqCst) == 0;
        let content = if first {
            let calls = if self.parallel { 2 } else { 1 };
            MessageContent::from_parts(
                (0..calls)
                    .map(|index| {
                        ContentPart::ToolCall(ToolCall {
                            call_id: format!("native-image-call-{index}"),
                            fn_name: crate::image_view_tools::COGNITION_IMAGE_VIEW.into(),
                            fn_arguments: json!({"path": format!("terminal-{index}.png")}),
                            thought_signatures: None,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            *self.wire.lock().unwrap() = Some(
                direct::request_body("gpt-6.1-sol", request.clone(), &ChatOptions::default())
                    .unwrap(),
            );
            if let Some(expected) = &self.live_expected {
                let response = OpenAiCodexChatClient::new("gpt-6.1-sol")
                    .complete(request, options)
                    .await?;
                let actual = response.first_text().unwrap_or_default().trim();
                assert_eq!(
                    actual, expected,
                    "live native loop must understand pixels, not just return a receipt"
                );
                eprintln!("live native image understood: {actual}");
                return Ok(response);
            }
            MessageContent::from_parts(vec![ContentPart::Text("finished".into())])
        };
        let model = genai::ModelIden::new(genai::adapter::AdapterKind::OpenAIResp, "gpt-6.1-sol");
        Ok(ChatResponse {
            content,
            reasoning_content: None,
            model_iden: model.clone(),
            provider_model_iden: model,
            stop_reason: None,
            usage: Default::default(),
            captured_raw_body: None,
            response_id: None,
        })
    }
}

// Mirror the Coder registry's per-invocation root binding: Tokio task-local
// roots do not propagate into the native parallel JoinSet on their own.
struct ImageRootRegistry {
    inner: InMemoryToolRegistry,
    root: std::path::PathBuf,
    parallel_barrier: Option<Arc<tokio::sync::Barrier>>,
}

#[async_trait]
impl ToolRegistry for ImageRootRegistry {
    async fn list_tools(&self) -> StasisResult<Vec<Tool>> {
        self.inner.list_tools().await
    }

    async fn invoke_tool(&self, name: &str, input: Value) -> StasisResult<Value> {
        if let Some(barrier) = &self.parallel_barrier {
            tokio::time::timeout(std::time::Duration::from_secs(5), barrier.wait())
                .await
                .expect("both image calls must execute concurrently");
        }
        crate::coding_tools::with_coder_tool_root(
            self.root.clone(),
            self.inner.invoke_tool(name, input),
        )
        .await
    }
}

#[tokio::test]
async fn terminal_file_through_native_loop_reaches_provider_as_tool_pixels() {
    assert_native_image_flow(false, false).await;
}

#[tokio::test]
async fn parallel_terminal_files_reach_provider_as_correlated_tool_pixels() {
    assert_native_image_flow(true, false).await;
}

async fn assert_native_image_flow(parallel: bool, live: bool) {
    use crate::agent_runtime::execution_context::{
        TurnExecutionContext, with_turn_execution_context,
    };
    use crate::request_principal::{RequestPrincipal, TransportClass};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    // Different pixels detect cross-call swaps and avoid intentional same-artifact
    // de-duplication in the runtime hydration budget.
    let (live_pixels, live_expected) = generated_color_challenge();
    let expected_pixels: Vec<_> = (0..2)
        .map(|index| {
            image::RgbaImage::from_fn(9, 5, |x, y| {
                image::Rgba([x as u8 * 20, y as u8 * 40, 100 + index * 80, 255])
            })
        })
        .collect();
    let expected_pixels = if live {
        vec![live_pixels]
    } else {
        expected_pixels
    };
    let prompt = if live {
        "Inspect the attached image. Return only the eight band colors in left-to-right order, lowercase, comma-separated, with no spaces. If no pixels are available return NO_IMAGE. Do not infer colors from metadata."
    } else {
        "inspect the terminal image"
    };
    for (index, pixels) in expected_pixels.iter().enumerate() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels.clone())
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(root.join(format!("terminal-{index}.png")), png.into_inner()).unwrap();
    }
    let session = format!("image-wire-flow-{}", uuid::Uuid::new_v4());
    let scope = crate::turn_scope::TurnContinuationScope {
        turn_correlation_id: "image-wire-turn".into(),
        session_id: session.clone(),
        identity_user_id: None,
        original_prompt: "inspect the terminal image".into(),
        delivery_target: None,
        provider: "openai-codex".into(),
        model: "gpt-6.1-sol".into(),
        response_depth_mode: "standard".into(),
        supports_ui_artifacts: false,
        supports_liquid_markdown: false,
        supports_browser_host: false,
        browser_driver_id: None,
        selected_worlds: vec![],
        channel_surface: None,
    };
    let context = Arc::new(
        TurnExecutionContext::from_scope(
            "image-wire-turn",
            RequestPrincipal::anonymous(TransportClass::Loopback),
            tokio_util::sync::CancellationToken::new(),
            std::time::Instant::now() + std::time::Duration::from_secs(60),
            scope,
        )
        .unwrap(),
    );
    let wire = Arc::new(Mutex::new(None));
    let client = Arc::new(ImageLoopClient {
        rounds: AtomicUsize::new(0),
        wire: wire.clone(),
        parallel,
        live_expected: live.then_some(live_expected),
    });
    let mut registry = InMemoryToolRegistry::default();
    crate::image_view_tools::register_image_view_tool(&mut registry).unwrap();
    let pipeline = medousa_runtime::MedousaToolLoopPipeline::new(
        PromptExecutionPipeline::new(client),
        Arc::new(ImageRootRegistry {
            inner: registry,
            root: root.clone(),
            parallel_barrier: parallel.then(|| Arc::new(tokio::sync::Barrier::new(2))),
        }),
    )
    .with_parallel_execution_settings(
        medousa_runtime::execution_policy::ParallelExecutionSettings {
            // Image tools snapshot artifacts, so the default conservative policy
            // serializes them. Explicitly enable this to exercise JoinSet dispatch.
            allow_mutating_parallel: true,
            ..Default::default()
        },
    );
    let ports = medousa_runtime::RuntimePorts::new().with_tool_observation_hydration(Arc::new(
        crate::agent_runtime::tool_observation_hydration::DaemonToolObservationHydrationPort::new(
            &session,
        ),
    ));
    let mut gate = medousa_runtime::ToolLoopCompletionGate::new_for_execution(1, ports, 2);
    gate.skip_avec_ritual_check = true;
    gate.completion_profile = medousa_runtime::TurnCompletionProfile::WorkerSynthesis;
    with_turn_execution_context(
        context,
        crate::coding_tools::with_coder_tool_root(root, async {
            pipeline
                .execute_with_stream_prior_messages_max_rounds(
                    ToolLoopExecutionRequest {
                        user_prompt: prompt.into(),
                        system_prompt: None,
                        context: PromptExecutionContext::default(),
                        tool_name: String::new(),
                        tool_input: Value::Null,
                        tool_call_mode: ToolCallMode::Auto,
                    },
                    vec![],
                    None,
                    2,
                    Some(&mut gate),
                    None,
                )
                .await
                .unwrap();
        }),
    )
    .await;
    let body = wire
        .lock()
        .unwrap()
        .take()
        .expect("next model request captured");
    let input = body["input"].as_array().unwrap();
    let expected_calls = if parallel { 2 } else { 1 };
    assert_eq!(
        input
            .iter()
            .filter(|item| item["type"] == "function_call_output")
            .count(),
        expected_calls
    );
    for (index, pixels) in expected_pixels.iter().enumerate().take(expected_calls) {
        let call_id = format!("native-image-call-{index}");
        let output = input
            .iter()
            .find(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
            .unwrap();
        assert!(output["output"].is_string(), "tool receipt stays text");
        let observation = input
            .iter()
            .filter(|item| item["role"] == "user")
            .filter_map(|item| item["content"].as_array())
            .find(|parts| parts.iter().any(|part| part["type"] == "input_image"))
            .expect("standard image input message");
        let pair = observation
            .as_chunks::<2>()
            .0
            .iter()
            .find(|pair| {
                pair[0]["text"]
                    .as_str()
                    .is_some_and(|text| text.contains(&format!("source_call_id={call_id}\n")))
            })
            .expect("image retains source call ID");
        let metadata = pair[0]["text"].as_str().unwrap();
        assert!(metadata.contains("trust=untrusted_external_content"));
        let image = &pair[1];
        assert_eq!(image["type"], "input_image");
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(
                image["image_url"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("data:image/png;base64,")
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(&image::load_from_memory(&bytes).unwrap().to_rgba8(), pixels);
    }
    assert_eq!(
        input.iter().filter(|item| item["role"] == "user").count(),
        2
    );
    // One transient observation carries all correlated images, not a durable
    // principal turn. The regular request is unchanged.
    assert!(input.iter().any(|item| item["type"] == "function_call"));
}

/// Explicit opt-in: uses the signed-in account and sends only a generated image.
/// The expected color order is never included in the request or tool receipt.
#[tokio::test]
#[ignore = "requires signed-in ChatGPT account; consumes live provider inference"]
async fn live_generated_image_transport_probe() {
    tokio::time::timeout(
        std::time::Duration::from_secs(120),
        assert_native_image_flow(false, true),
    )
    .await
    .expect("live visual probe timed out");
}

fn generated_color_challenge() -> (image::RgbaImage, String) {
    let colors = [
        ([255, 220, 0, 255], "yellow"),
        ([240, 0, 0, 255], "red"),
        ([0, 40, 255, 255], "blue"),
        ([0, 200, 0, 255], "green"),
    ];
    let seed = uuid::Uuid::new_v4();
    // Alternating parity prevents neighboring bands from merging. The seed and
    // expected color names never enter a tool argument, receipt, or prompt.
    let order: Vec<_> = seed.as_bytes()[..8]
        .iter()
        .enumerate()
        .map(|(i, value)| (i + usize::from(*value) * 2) % 4)
        .collect();
    let pixels = image::RgbaImage::from_fn(800, 320, |x, _| {
        image::Rgba(colors[order[(x / 100) as usize]].0)
    });
    let expected = order
        .iter()
        .map(|index| colors[*index].1)
        .collect::<Vec<_>>()
        .join(",");
    (pixels, expected)
}
