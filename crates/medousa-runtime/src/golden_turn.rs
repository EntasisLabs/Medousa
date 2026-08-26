//! Golden-turn characterization tests for the portable production loop.
//!
//! These lock the *observable* turn semantics of the real
//! [`MedousaToolLoopPipeline`] FSM so the Phase 1 hexagonal extraction is
//! provably behavior-preserving. Determinism comes from a scripted
//! [`AiChatClient`] (there is no scripted model provider in the tree) feeding
//! the genuine tool loop + completion gate + runtime presentation ports — i.e.
//! we exercise the production decision code, not a reimplementation of it.
//!
//! What is locked here (the cases the plan calls out):
//! * Direct prose commits immediately, independent of wording,
//! * prose after tools remains chronological ActiveWork until typed terminal,
//! * tool round then `cognition_turn_finish` — terminal commit + tool slicing,
//! * checkpoint / worker-ack handoff termination reasons,
//! * event-driven prose completion before and after tool use,
//! * max-rounds fuse,
//! * streamed content deltas reaching the sink.
//!
//! The terminal *delivery* mapping (which sink method + persisted body a given
//! termination reason produces) is locked separately in `sink_golden` against
//! `InteractiveTurnStreamSink`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use genai::ModelIden;
use genai::adapter::AdapterKind;
use genai::chat::{
    ChatOptions, ChatRequest, ChatResponse, ContentPart, MessageContent, Tool, ToolCall,
};
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

use crate::execution_boundary::{TurnExecutionBoundary, with_turn_execution_boundary};
use crate::loop_gate::ToolLoopCompletionGate;
use crate::ports::{
    RuntimePortFuture, RuntimePorts, ToolRunEventPort, ToolRunFinish, ToolRunStart,
    TurnPresentationPort,
};
use crate::tool_loop::MedousaToolLoopPipeline;
use crate::turn_control::COGNITION_TURN;

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

fn finish_call(message: &str) -> ToolCall {
    tool_call(
        COGNITION_TURN,
        json!({ "action": "turn.finish", "message": message }),
    )
}

fn checkpoint_call(message: &str) -> ToolCall {
    tool_call(
        COGNITION_TURN,
        json!({ "action": "turn.checkpoint", "message": message }),
    )
}

fn request_input_call(message: &str) -> ToolCall {
    tool_call(
        COGNITION_TURN,
        json!({ "action": "turn.request_input", "message": message }),
    )
}

fn register_golden_turn_tool(registry: &InMemoryToolRegistry) {
    registry.register_tool(GoldenTurnTool).unwrap();
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

fn prose_and_tool_response(text: &str, call: ToolCall) -> ChatResponse {
    ChatResponse {
        content: MessageContent::from_parts(vec![
            ContentPart::Text(text.to_string()),
            ContentPart::ToolCall(call),
        ]),
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

struct GoldenTurnTool;

#[async_trait]
impl StasisTool for GoldenTurnTool {
    fn name(&self) -> &'static str {
        COGNITION_TURN
    }

    async fn invoke(&self, _input: Value) -> StasisResult<Value> {
        Ok(json!({ "ok": true }))
    }
}

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

// ── Recording runtime ports ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ev {
    ToolStarted { tool: String, round: usize },
    ToolFinished { tool: String, round: usize },
    Progress(String),
    PackHold(String),
    ScratchReset,
    Content(String),
}

#[derive(Clone, Default)]
struct CapturingPorts {
    events: Arc<Mutex<Vec<Ev>>>,
    next_tool_run_id: Arc<AtomicU64>,
}

impl CapturingPorts {
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

impl ToolRunEventPort for CapturingPorts {
    fn started(&self, event: ToolRunStart) -> RuntimePortFuture<String> {
        let tool_run_id = self.next_tool_run_id.fetch_add(1, Ordering::Relaxed);
        self.push(Ev::ToolStarted {
            tool: event.tool_name,
            round: event.tool_round,
        });
        Box::pin(async move { format!("golden-tool-run-{tool_run_id}") })
    }

    fn finished(&self, event: ToolRunFinish) -> RuntimePortFuture<()> {
        self.push(Ev::ToolFinished {
            tool: event.invocation.tool_name,
            round: event.tool_round,
        });
        Box::pin(async {})
    }
}

impl TurnPresentationPort for CapturingPorts {
    fn notice(&self, _message: String) -> RuntimePortFuture<()> {
        Box::pin(async {})
    }

    fn scratch_reset(&self, _stream_turn_id: u64) -> RuntimePortFuture<()> {
        self.push(Ev::ScratchReset);
        Box::pin(async {})
    }

    fn turn_progress(
        &self,
        _stream_turn_id: u64,
        message: String,
        _tool_names: Vec<String>,
    ) -> RuntimePortFuture<()> {
        self.push(Ev::Progress(message));
        Box::pin(async {})
    }

    fn pack_hold(
        &self,
        _stream_turn_id: u64,
        fragments: Vec<String>,
        _tool_names: Vec<String>,
    ) -> RuntimePortFuture<()> {
        self.push(Ev::PackHold(fragments.join("\n\n")));
        Box::pin(async {})
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

fn golden_execution_boundary() -> Arc<TurnExecutionBoundary> {
    Arc::new(TurnExecutionBoundary::new(
        CancellationToken::new(),
        Instant::now() + Duration::from_secs(60),
    ))
}

/// Run the real tool loop against a scripted model and capture the observable
/// port events + outcome. `stream` toggles the streaming code path and the
/// host-side delta bridge used by the full daemon.
async fn run_golden(
    user_prompt: &str,
    steps: Vec<ChatResponse>,
    max_rounds: usize,
    stream: bool,
) -> GoldenOutcome {
    let registry = InMemoryToolRegistry::default();
    registry.register_tool(DataProbeTool).unwrap();
    register_golden_turn_tool(&registry);

    let client = Arc::new(ScriptedClient::new(steps));
    let pipeline = MedousaToolLoopPipeline::new(
        PromptExecutionPipeline::new(client.clone()),
        Arc::new(registry),
    );

    let capturing_ports = Arc::new(CapturingPorts::default());
    let runtime_ports = RuntimePorts::new()
        .with_tool_run_events(capturing_ports.clone())
        .with_turn_presentation(capturing_ports.clone());
    let mut gate = ToolLoopCompletionGate::new_for_execution(1, runtime_ports, max_rounds);

    let request = ToolLoopExecutionRequest {
        user_prompt: user_prompt.to_string(),
        system_prompt: None,
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: ToolCallMode::Auto,
    };

    // Bridge StreamDelta through the same host-side presentation boundary as the daemon.
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<StreamDelta>(32);
    let streamed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let bridge = if stream {
        let bridge_ports = capturing_ports.clone();
        let collected = streamed.clone();
        Some(tokio::spawn(async move {
            while let Some(delta) = chunk_rx.recv().await {
                match delta {
                    StreamDelta::Content(text) => {
                        collected.lock().unwrap().push(text.clone());
                        bridge_ports.push(Ev::Content(text));
                    }
                    StreamDelta::Reasoning(_) | StreamDelta::ThoughtSignature(_) => {}
                }
            }
        }))
    } else {
        None
    };

    let chunk_tx_ref = if stream { Some(&chunk_tx) } else { None };
    let response = with_turn_execution_boundary(
        golden_execution_boundary(),
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
        events: capturing_ports.snapshot(),
        event_kinds: capturing_ports.kinds(),
        streamed: streamed.lock().unwrap().clone(),
        request_count: client.requests().len(),
    }
}

// ── Golden cases ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn golden_round_context_is_injected_before_the_next_inference() {
    let registry = InMemoryToolRegistry::default();
    registry.register_tool(DataProbeTool).unwrap();
    register_golden_turn_tool(&registry);
    let client = Arc::new(ScriptedClient::new(vec![
        tool_response(vec![tool_call("data_probe", json!({ "q": "state" }))]),
        tool_response(vec![finish_call("Done after observing the delta.")]),
    ]));
    let pipeline = MedousaToolLoopPipeline::new(
        PromptExecutionPipeline::new(client.clone()),
        Arc::new(registry),
    );
    let mut gate = ToolLoopCompletionGate::new_for_execution(1, RuntimePorts::new(), 4);
    gate.round_context_provider = Some(Arc::new(OneShotRoundContext::new()));
    let request = ToolLoopExecutionRequest {
        user_prompt: "probe then finish".to_string(),
        system_prompt: None,
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: ToolCallMode::Auto,
    };

    let response = with_turn_execution_boundary(
        golden_execution_boundary(),
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
    register_golden_turn_tool(&registry);
    let client = Arc::new(ScriptedClient::new(vec![
        tool_response(vec![tool_call("large_data_probe", json!({}))]),
        tool_response(vec![finish_call("Done after the focused observation.")]),
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

    let response = with_turn_execution_boundary(
        golden_execution_boundary(),
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
    with_turn_execution_boundary(
        golden_execution_boundary(),
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
async fn golden_plain_reply_commits_directly() {
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

    assert_eq!(outcome.termination_reason, "direct_prose");
    assert_eq!(outcome.text, first);
    assert_eq!(outcome.rounds_executed, 1);
    assert!(outcome.tool_invocations.is_empty());
    assert!(outcome.event_kinds.is_empty());
}

#[tokio::test]
async fn golden_tool_round_then_finish_commits_terminal_body() {
    let outcome = run_golden(
        "look something up then answer",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "ingest" }))]),
            tool_response(vec![finish_call("Final answer grounded in the probe.")]),
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
        vec!["data_probe".to_string(), "cognition_turn".to_string()]
    );
    // Tooling slices: probe runs in round 1; the finish tool runs in round 2.
    // (Scratch reset between rounds only fires on the streaming path; this case
    // is non-streaming, locked separately in the streaming golden.)
    assert_eq!(
        outcome.event_kinds,
        vec![
            "tool_started:data_probe".to_string(),
            "tool_finished:data_probe".to_string(),
            "tool_started:cognition_turn".to_string(),
            "tool_finished:cognition_turn".to_string(),
        ],
        "events: {:?}",
        outcome.events
    );
}

#[tokio::test]
async fn golden_finish_message_is_fallback_not_a_prose_merge() {
    let progress =
        "The pager is only the visible symptom; the PTY command has no completion boundary.";
    let finish =
        "The fix is a scoped noninteractive environment plus an explicit completion sentinel.";
    let outcome = run_golden(
        "diagnose the pager problem",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "pty" }))]),
            text_response(progress),
            tool_response(vec![finish_call(finish)]),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_finish");
    assert_eq!(outcome.text, finish);
    assert_eq!(outcome.rounds_executed, 3);
}

#[tokio::test]
async fn golden_active_work_prose_never_merges_into_terminal_fallback() {
    let first_progress = "I found a possible cause and need one more probe.";
    let second_progress = "The second probe confirmed the missing PTY completion boundary.";
    let final_text = "The pager environment and sentinel fix are ready to implement.";
    let outcome = run_golden(
        "keep diagnosing the pager problem",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "first" }))]),
            text_response(first_progress),
            tool_response(vec![tool_call("data_probe", json!({ "q": "second" }))]),
            text_response(second_progress),
            tool_response(vec![finish_call(final_text)]),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_finish");
    assert_eq!(outcome.text, final_text);
    assert_eq!(outcome.rounds_executed, 5);
}

#[tokio::test]
async fn golden_checkpoint_handoff_terminates_as_checkpoint() {
    let outcome = run_golden(
        "do partial work and hand back",
        vec![tool_response(vec![checkpoint_call(
            "Found three blockers — your call on scope.",
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
async fn golden_request_input_is_a_distinct_typed_terminal() {
    let question = "Which repository should I inspect?";
    let outcome = run_golden(
        "inspect the repository",
        vec![tool_response(vec![request_input_call(question)])],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_request_input");
    assert_eq!(outcome.text, question);
    assert_eq!(outcome.rounds_executed, 1);
}

#[tokio::test]
async fn golden_terminal_mixed_with_ordinary_action_is_ignored() {
    let outcome = run_golden(
        "probe then finish honestly",
        vec![
            tool_response(vec![
                tool_call("data_probe", json!({ "q": "ingest" })),
                finish_call("premature"),
            ]),
            tool_response(vec![finish_call("Grounded final answer.")]),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_finish");
    assert_eq!(outcome.text, "Grounded final answer.");
    assert_eq!(outcome.rounds_executed, 2);
    assert_eq!(
        outcome.tool_invocations,
        vec![
            "data_probe".to_string(),
            "cognition_turn".to_string(),
            "cognition_turn".to_string(),
        ]
    );
}

#[tokio::test]
async fn golden_finish_prefers_same_response_prose_over_message_fallback() {
    let prose = "The probe confirms the ingest adapter owns the mapping.";
    let outcome = run_golden(
        "probe then answer",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "ingest" }))]),
            prose_and_tool_response(prose, finish_call("fallback should not win")),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_finish");
    assert_eq!(outcome.text, prose);
    assert_eq!(outcome.rounds_executed, 2);
}

#[tokio::test]
async fn golden_non_tool_announcement_without_action_ends_directly() {
    let outcome = run_golden(
        "kick off some work",
        vec![
            text_response("Let me check that for you."),
            tool_response(vec![tool_call("data_probe", json!({ "q": "ingest" }))]),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "direct_prose");
    assert_eq!(outcome.text, "Let me check that for you.");
    assert_eq!(outcome.rounds_executed, 1);
    assert!(outcome.tool_invocations.is_empty());
}

#[tokio::test]
async fn golden_active_work_prose_continues_until_finish() {
    let progress = "The completion policy is now separate from the execution lane. The probe passed; I am reconciling the receipt now.";
    let final_text = "The focused regression passed and the receipt matches the change.";
    let outcome = run_golden(
        "inspect the runtime and report back",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "completion" }))]),
            text_response(progress),
            tool_response(vec![finish_call(final_text)]),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_finish");
    assert_eq!(outcome.text, final_text);
    assert_eq!(outcome.rounds_executed, 3);
    assert_eq!(
        outcome.tool_invocations,
        vec!["data_probe".to_string(), "cognition_turn".to_string()]
    );
}

#[tokio::test]
async fn golden_max_rounds_fuse_terminates() {
    // Direct prose wins even on the final permitted round. Enter ActiveWork
    // first to exercise the fuse.
    let outcome = run_golden(
        "answer immediately",
        vec![
            tool_response(vec![tool_call("data_probe", json!({ "q": "partial" }))]),
            text_response("partial"),
        ],
        2,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "max_rounds_fuse");
    assert_eq!(outcome.rounds_executed, 2);
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

    assert_eq!(outcome.termination_reason, "direct_prose");
    assert_eq!(outcome.rounds_executed, 1);
    assert_eq!(outcome.request_count, 1);
    assert_eq!(outcome.streamed, vec![first.to_string()]);
    assert_eq!(
        outcome
            .event_kinds
            .iter()
            .filter(|kind| kind.as_str() == "content")
            .count(),
        1
    );
    assert!(!outcome.event_kinds.iter().any(|kind| kind == "pack_hold"));
}
