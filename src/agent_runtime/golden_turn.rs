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
//! * plain reply (no tool calls) — terminates on prose,
//! * tool round then `cognition_turn_finish` — terminal commit + tool slicing,
//! * checkpoint / worker-ack handoff termination reasons,
//! * interim-prose bounded auto-continue,
//! * max-rounds fuse,
//! * streamed content deltas reaching the sink.
//!
//! The terminal *delivery* mapping (which sink method + persisted body a given
//! termination reason produces) is locked separately in `sink_golden` against
//! `InteractiveTurnStreamSink`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use genai::adapter::AdapterKind;
use genai::chat::{ChatOptions, ChatRequest, ChatResponse, MessageContent, ToolCall};
use genai::ModelIden;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline,
};
use stasis::application::orchestration::tool_loop_pipeline::{
    ToolCallMode, ToolLoopExecutionRequest,
};
use stasis::application::orchestration::tool_registry::{InMemoryToolRegistry, StasisTool};
use stasis::domain::errors::Result as StasisResult;
use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta};

use crate::agent_runtime::stream_sink::{AgentStreamSink, SharedAgentStreamSink};
use crate::agent_runtime::turn_completion::ToolLoopCompletionGate;
use crate::medousa_tool_loop::MedousaToolLoopPipeline;
use crate::payload_receipt::ArtifactReceiptMeta;
use crate::turn_control_tools::{
    CognitionTurnBeginWorkTool, CognitionTurnCheckpointTool, CognitionTurnFinishTool,
    CognitionTurnUpdateUserTool,
};

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
/// response; once the script is exhausted it saturates on the final step so the
/// loop's internal stream→non-stream retry (which issues an extra call for a
/// text-only streamed round) observes identical output rather than diverging.
struct ScriptedClient {
    steps: Vec<ChatResponse>,
    idx: Mutex<usize>,
}

impl ScriptedClient {
    fn new(steps: Vec<ChatResponse>) -> Self {
        assert!(!steps.is_empty(), "scripted client needs at least one step");
        Self {
            steps,
            idx: Mutex::new(0),
        }
    }

    fn next(&self) -> ChatResponse {
        let mut idx = self.idx.lock().unwrap();
        let pick = (*idx).min(self.steps.len() - 1);
        *idx += 1;
        self.steps[pick].clone()
    }
}

#[async_trait]
impl AiChatClient for ScriptedClient {
    async fn complete(
        &self,
        _request: ChatRequest,
        _options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        Ok(self.next())
    }

    async fn complete_stream(
        &self,
        _request: ChatRequest,
        _options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::UnboundedSender<StreamDelta>>,
    ) -> StasisResult<ChatResponse> {
        let response = self.next();
        if let (Some(tx), Some(text)) = (chunk_tx, response.first_text()) {
            let _ = tx.send(StreamDelta::Content(text.to_string()));
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

// ── Recording sink ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ev {
    ToolStarted { tool: String, round: usize },
    ToolFinished { tool: String, round: usize },
    Progress(String),
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

    let pipeline = MedousaToolLoopPipeline::new(
        PromptExecutionPipeline::new(Arc::new(ScriptedClient::new(steps))),
        Arc::new(registry),
    );

    let sink_concrete = Arc::new(CapturingSink::default());
    let sink: SharedAgentStreamSink = sink_concrete.clone();
    let mut gate = ToolLoopCompletionGate::new_for_execution(1, None, Some(sink.clone()), max_rounds);

    let request = ToolLoopExecutionRequest {
        user_prompt: user_prompt.to_string(),
        system_prompt: None,
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: ToolCallMode::Auto,
    };

    // Bridge StreamDelta → content_chunk exactly like the daemon's execute_local_turn.
    let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<StreamDelta>();
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
    let response = pipeline
        .execute_with_stream_prior_messages_max_rounds(
            request,
            Vec::new(),
            chunk_tx_ref,
            max_rounds,
            Some(&mut gate),
            None,
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
    }
}

// ── Golden cases ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn golden_plain_reply_terminates_on_prose() {
    let answer = "Here is a complete explanation of how the ingester maps channel \
                  sessions to Medousa history without any further steps needed.";
    let outcome = run_golden(
        "explain the ingester mapping",
        vec![text_response(answer)],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "no_tools_prose");
    assert_eq!(outcome.text, answer);
    assert_eq!(outcome.rounds_executed, 1);
    assert!(outcome.tool_invocations.is_empty());
    // No tool work, no progress, no scratch reset on a single-round plain reply.
    assert!(outcome.event_kinds.is_empty(), "events: {:?}", outcome.events);
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
        vec!["data_probe".to_string(), "cognition_turn_finish".to_string()]
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
async fn golden_interim_prose_continues_then_finishes() {
    let outcome = run_golden(
        "kick off some work",
        vec![
            // Round 1: a short interim acknowledgment with no tool call.
            text_response("Let me check that for you."),
            // Round 2: model commits the real answer via finish.
            tool_response(vec![tool_call(
                "cognition_turn_finish",
                json!({ "message": "Done — here is the result." }),
            )]),
        ],
        10,
        false,
    )
    .await;

    assert_eq!(outcome.termination_reason, "cognition_turn_finish");
    assert_eq!(outcome.text, "Done — here is the result.");
    assert_eq!(outcome.rounds_executed, 2);
    // The interim note is surfaced to the principal as a non-terminal progress line.
    assert!(
        outcome
            .events
            .iter()
            .any(|ev| matches!(ev, Ev::Progress(msg) if msg.contains("Let me check that"))),
        "expected interim progress, got: {:?}",
        outcome.events
    );
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
    // Use a genuinely substantive answer so it terminates in a single round
    // (a short note would be treated as interim prose and bounded-continue).
    let answer = "Here is a complete explanation of how the ingester maps channel \
                  sessions to Medousa history without any further steps needed.";
    let outcome = run_golden("stream me an answer", vec![text_response(answer)], 10, true).await;

    assert_eq!(outcome.termination_reason, "no_tools_prose");
    assert_eq!(outcome.rounds_executed, 1);
    // The streamed token reaches the sink once (the loop's non-streaming retry
    // does not re-stream); content_chunk is the only sink event.
    assert_eq!(outcome.streamed, vec![answer.to_string()]);
    assert_eq!(outcome.event_kinds, vec!["content".to_string()]);
}
