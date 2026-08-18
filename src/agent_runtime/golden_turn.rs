//! Phase 0 — golden-turn characterization tests.
//!
//! These lock the *observable* turn semantics of the real
//! [`MedousaToolLoopPipeline`] FSM so the Phase 1 hexagonal extraction is
//! provably behavior-preserving. Determinism comes from a scripted
//! [`AiChatClient`] (there is no scripted model provider in the tree) feeding
//! the genuine tool loop + completion gate + [`AgentStreamSink`] port — i.e. we
//! exercise the production decision code, not a reimplementation of it.
//!
//! What is locked here (the cases the plan calls out):
//! * two consecutive non-tool replies commit (`content_pack_merged`),
//! * announcement before tools holds, then a tool round can proceed,
//! * tool round then `cognition_turn_finish` — terminal commit + tool slicing,
//! * checkpoint / worker-ack handoff termination reasons,
//! * event-driven prose completion before and after tool use,
//! * max-rounds fuse,
//! * streamed content deltas reaching the sink.
//!
//! The terminal *delivery* mapping (which sink method + persisted body a given
//! termination reason produces) is locked separately in `sink_golden` against
//! `InteractiveTurnStreamSink`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use genai::ModelIden;
use genai::adapter::AdapterKind;
use genai::chat::{ChatOptions, ChatRequest, ChatResponse, MessageContent, Tool, ToolCall};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline,
};
use stasis::application::orchestration::tool_loop_pipeline::{
    ToolCallMode, ToolLoopExecutionRequest,
};
use stasis::application::orchestration::tool_registry::{
    InMemoryToolRegistry, StasisTool, ToolRegistry,
};
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta};

use crate::agent_runtime::execution_context::{
    ProviderRoute, SurfaceCapabilities, TurnExecutionContext, with_turn_execution_context,
};
use crate::agent_runtime::stream_sink::{AgentStreamSink, SharedAgentStreamSink};
use crate::agent_runtime::turn_completion::ToolLoopCompletionGate;
use crate::medousa_tool_loop::MedousaToolLoopPipeline;
use crate::payload_receipt::ArtifactReceiptMeta;
use crate::request_principal::{RequestPrincipal, TransportClass};
use crate::session_storage::SessionId;
use crate::turn_continuation::TurnContinuationScope;
use crate::turn_control_tools::{CognitionTurnCheckpointTool, CognitionTurnFinishTool};

// ── Scripted model provider ──────────────────────────────────────────────────

fn mock_iden() -> ModelIden {
    ModelIden::from_static(AdapterKind::OpenAI, "golden-mock")
}

fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        content: MessageContent::from(text.to_string()),
        reasoning_content: None,
        model_iden: mock_iden(),
        provider_model_iden: mock_iden(),
        stop_reason: None,
        usage: Default::default(),
        captured_raw_body: None,
        response_id: None,
    }
}

fn tool_call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        call_id: format!("call-{name}"),
        fn_name: name.to_string(),
        fn_arguments: args,
        thought_signatures: None,
    }
}

fn tool_response(calls: Vec<ToolCall>) -> ChatResponse {
    ChatResponse {
        content: MessageContent::from_tool_calls(calls),
        reasoning_content: None,
        model_iden: mock_iden(),
        provider_model_iden: mock_iden(),
        stop_reason: None,
        usage: Default::default(),
        captured_raw_body: None,
        response_id: None,
    }
}

/// Deterministic scripted chat client. Each model round pops the next scripted
/// response; once the script is exhausted it saturates on the final step.
struct ScriptedClient {
    steps: Vec<ChatResponse>,
    idx: Mutex<usize>,
    requests: Mutex<Vec<ChatRequest>>,
}

impl ScriptedClient {
    fn new(steps: Vec<ChatResponse>) -> Self {
        assert!(!steps.is_empty(), "scripted client needs at least one step");
        Self {
            steps,
            idx: Mutex::new(0),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn next(&self) -> ChatResponse {
        let mut idx = self.idx.lock().unwrap();
        let pick = (*idx).min(self.steps.len() - 1);
        *idx += 1;
        self.steps[pick].clone()
    }

    fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl AiChatClient for ScriptedClient {
    async fn complete(
        &self,
        request: ChatRequest,
        _options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        self.requests.lock().unwrap().push(request);
        Ok(self.next())
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        _options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> StasisResult<ChatResponse> {
        self.requests.lock().unwrap().push(request);
        let response = self.next();
        if let (Some(tx), Some(text)) = (chunk_tx, response.first_text()) {
            tx.send(StreamDelta::Content(text.to_string()))
                .await
                .map_err(|_| StasisError::StreamClosed)?;
        }
        Ok(response)
    }
}

// ── Generic data tool (stands in for any non-control tool) ───────────────────

struct DataProbeTool;

#[async_trait]
impl StasisTool for DataProbeTool {
    fn name(&self) -> &'static str {
        "data_probe"
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        Ok(json!({ "ok": true, "echo": input }))
    }
}

struct LargeDataProbeTool;

#[async_trait]
impl StasisTool for LargeDataProbeTool {
    fn name(&self) -> &'static str {
        "large_data_probe"
    }

    async fn invoke(&self, _input: Value) -> StasisResult<Value> {
        Ok(json!({
            "ok": true,
            "path": "large.log",
            "orientation": {"next": "query a narrower diagnostic range"},
            "content": "x".repeat(100_000),
        }))
    }
}

struct OneShotRoundContext {
    emitted: AtomicBool,
}

impl OneShotRoundContext {
    fn new() -> Self {
        Self {
            emitted: AtomicBool::new(false),
        }
    }
}

impl super::turn_context::ToolRoundContextProvider for OneShotRoundContext {
    fn context_for_next_round(&self) -> StasisResult<Option<String>> {
        Ok((!self.emitted.swap(true, Ordering::SeqCst))
            .then(|| "[TEST_ENGINEERING_DELTA] revision=2".to_string()))
    }
}

struct RevealingRegistry {
    revealed: AtomicBool,
}

#[async_trait]
impl ToolRegistry for RevealingRegistry {
    async fn list_tools(&self) -> StasisResult<Vec<Tool>> {
        let mut tools = vec![Tool::new("discover")];
        if self.revealed.load(Ordering::SeqCst) {
            tools.push(Tool::new("revealed_tool"));
        }
        Ok(tools)
    }

    async fn invoke_tool(&self, tool_name: &str, _input: Value) -> StasisResult<Value> {
        if tool_name == "discover" {
            self.revealed.store(true, Ordering::SeqCst);
        }
        Ok(json!({ "ok": true, "tool": tool_name }))
    }
}

// ── Recording sink ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ev {
    ToolStarted { tool: String, round: usize },
    ToolFinished { tool: String, round: usize },
    Progress(String),
    PackHold(String),
    ScratchReset,
    Content(String),
}

#[derive(Default)]
struct CapturingSink {
    events: Mutex<Vec<Ev>>,
}

impl CapturingSink {
    fn snapshot(&self) -> Vec<Ev> {
        self.events.lock().unwrap().clone()
    }

    fn kinds(&self) -> Vec<String> {
        self.snapshot()
            .into_iter()
            .map(|ev| match ev {
                Ev::ToolStarted { tool, .. } => format!("tool_started:{tool}"),
                Ev::ToolFinished { tool, .. } => format!("tool_finished:{tool}"),
                Ev::Progress(_) => "progress".to_string(),
                Ev::PackHold(_) => "pack_hold".to_string(),
                Ev::ScratchReset => "scratch_reset".to_string(),
                Ev::Content(_) => "content".to_string(),
            })
            .collect()
    }

    fn push(&self, ev: Ev) {
        self.events.lock().unwrap().push(ev);
    }
}

#[async_trait]
impl AgentStreamSink for CapturingSink {
    async fn content_chunk(&self, _turn_id: u64, delta: String) {
        self.push(Ev::Content(delta));
    }

    async fn reasoning_chunk(&self, _turn_id: u64, _delta: String) {}

    async fn agent_response(&self, _turn_id: u64, _text: String, _tool_names: Vec<String>) {}

    async fn agent_turn_progress(&self, _turn_id: u64, message: String, _tool_names: Vec<String>) {
        self.push(Ev::Progress(message));
    }

    async fn agent_pack_hold(
        &self,
        _turn_id: u64,
        fragments: Vec<String>,
        _tool_names: Vec<String>,
    ) {
        self.push(Ev::PackHold(fragments.join("\n\n")));
    }

    async fn agent_error(&self, _turn_id: u64, _message: String) {}

    async fn notice(&self, _message: String) {}

    async fn scratch_reset(&self, _turn_id: u64) {
        self.push(Ev::ScratchReset);
    }

    async fn tool_invoked(&self, _tool_name: String, _input_summary: String) {}

    async fn tool_run_started(
        &self,
        _tool_run_id: String,
        tool_name: String,
        _input_summary: String,
        _input_params: Vec<medousa_types::daemon_api::ToolInputParam>,
        tool_round: usize,
    ) {
        self.push(Ev::ToolStarted {
            tool: tool_name,
            round: tool_round,
        });
    }

    async fn tool_run_finished(
        &self,
        _tool_run_id: String,
        tool_name: String,
        _status: String,
        _input_summary: String,
        _output_summary: Option<String>,
        _tool_input: Value,
        _tool_output: Value,
        _input_receipt: Option<ArtifactReceiptMeta>,
        _output_receipt: Option<ArtifactReceiptMeta>,
        tool_round: usize,
    ) {
        self.push(Ev::ToolFinished {
            tool: tool_name,
            round: tool_round,
        });
    }

    async fn tool_payload(
        &self,
        _tool_name: String,
        _tool_input: Value,
        _tool_output: Value,
        _input_receipt: Option<ArtifactReceiptMeta>,
        _output_receipt: Option<ArtifactReceiptMeta>,
    ) {
    }
}

// ── Harness ──────────────────────────────────────────────────────────────────

struct GoldenOutcome {
    text: String,
    termination_reason: String,
    rounds_executed: usize,
    tool_invocations: Vec<String>,
    events: Vec<Ev>,
    event_kinds: Vec<String>,
    streamed: Vec<String>,
    request_count: usize,
}

fn golden_execution_context() -> Arc<TurnExecutionContext> {
    let turn_id = "golden-turn";
    let scope = TurnContinuationScope {
        turn_correlation_id: turn_id.to_string(),
        session_id: "golden-session".to_string(),
        identity_user_id: None,
        original_prompt: "golden fixture".to_string(),
        delivery_target: None,
        provider: "golden-provider".to_string(),
        model: "golden-model".to_string(),
        response_depth_mode: "standard".to_string(),
        supports_ui_artifacts: false,
        supports_liquid_markdown: false,
        supports_browser_host: false,
        channel_surface: None,
    };
    Arc::new(TurnExecutionContext::new(
        turn_id,
        turn_id,
        SessionId::parse("golden-session").unwrap(),
        RequestPrincipal::anonymous(TransportClass::Loopback),
        ProviderRoute::new("golden-provider", "golden-model"),
        SurfaceCapabilities::default(),
        CancellationToken::new(),
        Instant::now() + Duration::from_secs(60),
        scope,
    ))
}

/// Run the real tool loop against a scripted model and capture the observable
/// sink + outcome. `stream` toggles the streaming code path (and the bridge
/// that forwards `StreamDelta::Content` to `content_chunk`, mirroring
/// `execute_local_turn`).
async fn run_golden(
    user_prompt: &str,
    steps: Vec<ChatResponse>,
    max_rounds: usize,
    stream: bool,
) -> GoldenOutcome {
    let registry = InMemoryToolRegistry::default();
    registry.register_tool(DataProbeTool).unwrap();
    registry.register_tool(CognitionTurnFinishTool).unwrap();
    registry.register_tool(CognitionTurnCheckpointTool).unwrap();

    let client = Arc::new(ScriptedClient::new(steps));
    let pipeline = MedousaToolLoopPipeline::new(
        PromptExecutionPipeline::new(client.clone()),
        Arc::new(registry),
    );

    let sink_concrete = Arc::new(CapturingSink::default());
    let sink: SharedAgentStreamSink = sink_concrete.clone();
    let mut gate =
        ToolLoopCompletionGate::new_for_execution(1, None, Some(sink.clone()), max_rounds);

    let request = ToolLoopExecutionRequest {
        user_prompt: user_prompt.to_string(),
        system_prompt: None,
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: ToolCallMode::Auto,
    };

    // Bridge StreamDelta → content_chunk exactly like the daemon's execute_local_turn.
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<StreamDelta>(32);
    let streamed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let bridge = if stream {
        let bridge_sink = sink.clone();
        let collected = streamed.clone();
        Some(tokio::spawn(async move {
            while let Some(delta) = chunk_rx.recv().await {
                if let StreamDelta::Content(text) = &delta {
                    collected.lock().unwrap().push(text.clone());
                }
                match delta {
                    StreamDelta::Content(text) => bridge_sink.content_chunk(1, text).await,
                    StreamDelta::Reasoning(text) | StreamDelta::ThoughtSignature(text) => {
                        bridge_sink.reasoning_chunk(1, text).await
                    }
                }
            }
        }))
    } else {
        None
    };

    let chunk_tx_ref = if stream { Some(&chunk_tx) } else { None };
    let response = with_turn_execution_context(
        golden_execution_context(),
        pipeline.execute_with_stream_prior_messages_max_rounds(
            request,
            Vec::new(),
            chunk_tx_ref,
            max_rounds,
            Some(&mut gate),
            None,
        ),
    )
    .await
    .expect("golden tool loop should not error");

    drop(chunk_tx);
    if let Some(handle) = bridge {
        let _ = handle.await;
    }

    GoldenOutcome {
        text: response.text,
        termination_reason: response.termination_reason,
        rounds_executed: response.rounds_executed,
        tool_invocations: response
            .tool_invocations
            .iter()
            .map(|inv| inv.tool_name.clone())
            .collect(),
        events: sink_concrete.snapshot(),
        event_kinds: sink_concrete.kinds(),
        streamed: streamed.lock().unwrap().clone(),
        request_count: client.requests().len(),
    }
}

// ── Golden cases ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn golden_round_context_is_injected_before_the_next_inference() {
    let registry = InMemoryToolRegistry::default();
    registry.register_tool(DataProbeTool).unwrap();
    registry.register_tool(CognitionTurnFinishTool).unwrap();
    let client = Arc::new(ScriptedClient::new(vec![
        tool_response(vec![tool_call("data_probe", json!({ "q": "state" }))]),
        tool_response(vec![tool_call(
            "cognition_turn_finish",
            json!({ "message": "Done after observing the delta." }),
        )]),
    ]));
    let pipeline = MedousaToolLoopPipeline::new(
        PromptExecutionPipeline::new(client.clone()),
        Arc::new(registry),
    );
    let mut gate = ToolLoopCompletionGate::new_for_execution(1, None, None, 4);
    gate.round_context_provider = Some(Arc::new(OneShotRoundContext::new()));
    let request = ToolLoopExecutionRequest {
        user_prompt: "probe then finish".to_string(),
        system_prompt: None,
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: ToolCallMode::Auto,
    };

    let response = with_turn_execution_context(
        golden_execution_context(),
        pipeline.execute_with_stream_prior_messages_max_rounds(
            request,
            Vec::new(),
            None,
            4,
            Some(&mut gate),
            None,
        ),
    )
    .await
    .expect("tool loop");
    assert_eq!(response.termination_reason, "cognition_turn_finish");
    let requests = client.requests();
    assert!(requests.len() >= 2);
    assert!(requests[1].messages.iter().any(|message| {
        message
            .content
            .first_text()
            .is_some_and(|text| text.contains("[TEST_ENGINEERING_DELTA]"))
    }));
}

#[tokio::test]
async fn golden_large_tool_output_is_bounded_for_model_but_preserved_in_receipt() {
    let registry = InMemoryToolRegistry::default();
    registry.register_tool(LargeDataProbeTool).unwrap();
    registry.register_tool(CognitionTurnFinishTool).unwrap();
    let client = Arc::new(ScriptedClient::new(vec![
        tool_response(vec![tool_call("large_data_probe", json!({}))]),
        tool_response(vec![tool_call(
            "cognition_turn_finish",
            json!({ "message": "Done after the focused observation." }),
        )]),
    ]));
    let pipeline = MedousaToolLoopPipeline::new(
        PromptExecutionPipeline::new(client.clone()),
        Arc::new(registry),
    );
    let request = ToolLoopExecutionRequest {
        user_prompt: "inspect the large probe".to_string(),
        system_prompt: None,
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: ToolCallMode::Auto,
    };

    let response = with_turn_execution_context(
        golden_execution_context(),
        pipeline.execute_with_stream_prior_messages_max_rounds(
            request,
            Vec::new(),
            None,
            4,
            None,
            None,
        ),
    )
    .await
    .expect("tool loop");
    assert_eq!(
        response.tool_invocations[0].tool_output["content"]
            .as_str()
            .map(str::len),
        Some(100_000)
    );
    let requests = client.requests();
    let second_request = format!("{:?}", requests[1].messages);
    assert!(second_request.contains("perception_status"));
    assert!(second_request.contains("bounded"));
    assert!(second_request.contains("query a narrower diagnostic range"));
    assert!(!second_request.contains(&"x".repeat(60_000)));
}

#[tokio::test]
async fn golden_model_visible_tools_refresh_after_a_tool_round() {
    let client = Arc::new(ScriptedClient::new(vec![
        tool_response(vec![tool_call("discover", json!({}))]),
        tool_response(vec![tool_call("revealed_tool", json!({}))]),
        text_response("done"),
        text_response("done"),
    ]));
    let pipeline = MedousaToolLoopPipeline::new(
        PromptExecutionPipeline::new(client.clone()),
        Arc::new(RevealingRegistry {
            revealed: AtomicBool::new(false),
        }),
    );
    let request = ToolLoopExecutionRequest {
        user_prompt: "discover then use the revealed tool".to_string(),
        system_prompt: None,
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: ToolCallMode::Auto,
    };
    with_turn_execution_context(
        golden_execution_context(),
        pipeline.execute_with_stream_prior_messages_max_rounds(
            request,
            Vec::new(),
            None,
            4,
            None,
            None,
        ),
    )
    .await
    .expect("tool loop");
    let requests = client.requests();
    assert!(requests.len() >= 2);
    assert!(
        requests[0]
            .tools
            .as_ref()
            .expect("first tools")
            .iter()
            .all(|tool| tool.name.as_str() != "revealed_tool")
    );
    assert!(
        requests[1]
            .tools
            .as_ref()
            .expect("second tools")
            .iter()
            .any(|tool| tool.name.as_str() == "revealed_tool")
    );
}

#[tokio::test]
async fn golden_plain_reply_commits_on_two_consecutive_non_tool_responses() {
    let first = "Here is a complete explanation of how the ingester maps channel \
                 sessions to Medousa history without any further steps needed.";
    let second = "That mapping is the whole answer; nothing else to inspect.";
    let outcome = run_golden(
        "explain the ingester mapping",
        vec![text_response(first), text_response(second)],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "content_pack_merged");
    assert_eq!(outcome.text, format!("{first}\n\n{second}"));
    assert_eq!(outcome.rounds_executed, 2);
    assert!(outcome.tool_invocations.is_empty());
    assert_eq!(outcome.event_kinds, vec!["pack_hold".to_string()]);
}

#[tokio::test]
async fn golden_tool_round_then_finish_commits_terminal_body() {
    let outcome = run_golden(
        "look something up then answer",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "ingest" }))]),
            tool_response(vec![tool_call(
                "cognition_turn_finish",
                json!({ "message": "Final answer grounded in the probe." }),
            )]),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_finish");
    assert_eq!(outcome.text, "Final answer grounded in the probe.");
    assert_eq!(outcome.rounds_executed, 2);
    assert_eq!(
        outcome.tool_invocations,
        vec![
            "data_probe".to_string(),
            "cognition_turn_finish".to_string()
        ]
    );
    // Tooling slices: probe runs in round 1; the finish tool runs in round 2.
    // (Scratch reset between rounds only fires on the streaming path; this case
    // is non-streaming, locked separately in the streaming golden.)
    assert_eq!(
        outcome.event_kinds,
        vec![
            "tool_started:data_probe".to_string(),
            "tool_finished:data_probe".to_string(),
            "tool_started:cognition_turn_finish".to_string(),
            "tool_finished:cognition_turn_finish".to_string(),
        ],
        "events: {:?}",
        outcome.events
    );
}

#[tokio::test]
async fn golden_finish_appends_to_held_non_tool_response() {
    let held = "The pager is only the visible symptom; the PTY command has no completion boundary.";
    let finish =
        "The fix is a scoped noninteractive environment plus an explicit completion sentinel.";
    let outcome = run_golden(
        "diagnose the pager problem",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "pty" }))]),
            text_response(held),
            tool_response(vec![tool_call(
                "cognition_turn_finish",
                json!({ "message": finish }),
            )]),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_finish");
    assert_eq!(outcome.text, format!("{held}\n\n{finish}"));
    assert_eq!(outcome.rounds_executed, 3);
}

#[tokio::test]
async fn golden_tool_call_resets_the_held_non_tool_response() {
    let stale = "I found a possible cause and need one more probe.";
    let held = "The second probe confirmed the missing PTY completion boundary.";
    let final_text = "The pager environment and sentinel fix are ready to implement.";
    let outcome = run_golden(
        "keep diagnosing the pager problem",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "first" }))]),
            text_response(stale),
            tool_response(vec![tool_call("data_probe", json!({ "q": "second" }))]),
            text_response(held),
            text_response(final_text),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "content_pack_merged");
    assert_eq!(outcome.text, format!("{held}\n\n{final_text}"));
    assert!(!outcome.text.contains(stale));
    assert_eq!(outcome.rounds_executed, 5);
}

#[tokio::test]
async fn golden_checkpoint_handoff_terminates_as_checkpoint() {
    let outcome = run_golden(
        "do partial work and hand back",
        vec![tool_response(vec![tool_call(
            "cognition_turn_checkpoint",
            json!({ "message": "Found three blockers — your call on scope." }),
        )])],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_checkpoint");
    assert_eq!(outcome.text, "Found three blockers — your call on scope.");
    assert_eq!(outcome.rounds_executed, 1);
}

#[tokio::test]
async fn golden_non_tool_announcement_before_tools_holds_then_tools_proceed() {
    let held = "The probe confirmed the mapping is owned by the ingest adapter.";
    let final_text = "No further inspection is required; that is the answer.";
    let outcome = run_golden(
        "kick off some work",
        vec![
            text_response("Let me check that for you."),
            tool_response(vec![tool_call("data_probe", json!({ "q": "ingest" }))]),
            text_response(held),
            text_response(final_text),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "content_pack_merged");
    assert_eq!(outcome.text, format!("{held}\n\n{final_text}"));
    assert!(!outcome.text.contains("Let me check that for you."));
    assert_eq!(outcome.rounds_executed, 4);
    assert_eq!(outcome.tool_invocations, vec!["data_probe".to_string()]);
}

#[tokio::test]
async fn golden_foreground_announcement_tools_and_two_prose_final() {
    let first_final = "The completion policy is now separate from the execution lane. Coder keeps \
                       its announcement alive, executes the requested probe, and holds this completed \
                       principal-facing prose for one bounded resolution round.";
    let second_final = "The focused regression passed, so these two prose responses now commit together as one answer.";
    let expected = format!("{first_final}\n\n{second_final}");
    let outcome = run_golden(
        "inspect the runtime and report back",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "completion" }))]),
            text_response(first_final),
            text_response(second_final),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "content_pack_merged");
    assert_eq!(outcome.text, expected);
    assert_eq!(outcome.rounds_executed, 3);
    assert_eq!(outcome.tool_invocations, vec!["data_probe".to_string()]);
}

#[tokio::test]
async fn golden_max_rounds_fuse_terminates() {
    // A text reply on the final permitted round trips the max-rounds fuse.
    let outcome = run_golden(
        "answer immediately",
        vec![text_response("partial")],
        1,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "max_rounds_fuse");
    assert_eq!(outcome.rounds_executed, 1);
}

#[tokio::test]
async fn golden_streamed_content_reaches_sink() {
    let first = "Here is a complete explanation of how the ingester maps channel \
                 sessions to Medousa history without any further steps needed.";
    let second = "That mapping is the whole answer; nothing else to inspect.";
    let outcome = run_golden(
        "stream me an answer",
        vec![text_response(first), text_response(second)],
        10,
        true,
    )
    .await;

    assert_eq!(outcome.termination_reason, "content_pack_merged");
    assert_eq!(outcome.rounds_executed, 2);
    assert_eq!(outcome.request_count, 2);
    assert_eq!(
        outcome.streamed,
        vec![first.to_string(), second.to_string()]
    );
    assert_eq!(
        outcome
            .event_kinds
            .iter()
            .filter(|kind| kind.as_str() == "content")
            .count(),
        2
    );
    assert!(outcome.event_kinds.iter().any(|kind| kind == "pack_hold"));
}
