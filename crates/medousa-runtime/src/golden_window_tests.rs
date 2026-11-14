use super::*;
use crate::checkpoint::{ActiveTurnResumeState, ActiveTurnTranscript, SafeCheckpointBoundary};
use crate::inference_window::{InferenceWindowPolicy, estimate};
use genai::chat::ChatMessage;

struct FreshState;
impl crate::turn_context::ToolRoundContextProvider for FreshState {
    fn context_for_next_round(&self) -> StasisResult<Option<String>> {
        Ok(Some("fresh scoped work pointer".into()))
    }
    fn replaces_previous_context(&self) -> bool {
        true
    }
    fn sticky_context_for_next_round(&self) -> crate::turn_context::StickyContextFuture<'_> {
        Box::pin(async { Ok(Some("fresh protected exact-note-id:scope-a:42".into())) })
    }
}

fn window(tokens: u32) -> InferenceWindowPolicy {
    InferenceWindowPolicy {
        target: format!("scripted:{tokens}"),
        input_tokens: Some(tokens),
        safety_tokens: 0,
        observation_tokens: 0,
        ..Default::default()
    }
}

#[tokio::test]
async fn adaptive_window_rechecks_growing_real_tool_loop_with_streaming_and_reports() {
    for stream in [false, true] {
        let registry = InMemoryToolRegistry::default();
        registry.register_tool(DataProbeTool).unwrap();
        register_golden_turn_tool(&registry);
        let mut steps: Vec<_> = (0..8)
            .map(|round| tool_response(vec![tool_call("data_probe", json!({"round":round}))]))
            .collect();
        steps.push(tool_response(vec![finish_call("done")]));
        let client = Arc::new(ScriptedClient::new(steps));
        let observed = Arc::new(Mutex::new(Vec::new()));
        let captured = observed.clone();
        let policy = window(6000);
        let pipeline = MedousaToolLoopPipeline::new(
            PromptExecutionPipeline::new(client.clone()),
            Arc::new(registry),
        )
        .with_window_policy(policy.clone())
        .with_first_request_context(crate::request_context::FirstRequestContextConfig {
            assembly: None,
            observer: Some(Arc::new(move |footprint| {
                captured.lock().unwrap().push(footprint);
                Box::pin(async {})
            })),
        });
        let ledger = Arc::new(CapturingPorts::default());
        let mut gate = ToolLoopCompletionGate::new_for_execution(
            1,
            RuntimePorts::new().with_ledger_sink(ledger.clone()),
            12,
        );
        gate.round_context_provider = Some(Arc::new(FreshState));
        let request = ToolLoopExecutionRequest {
            user_prompt: "current exact principal".into(),
            system_prompt: Some("policy unchanged".into()),
            context: Default::default(),
            tool_name: String::new(),
            tool_input: Value::Null,
            tool_call_mode: ToolCallMode::Auto,
        };
        let prior = vec![
            ChatMessage::user("old question".repeat(2000)),
            ChatMessage::assistant("old answer"),
        ];
        let (tx, mut rx) = mpsc::channel(128);
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
        let response = with_turn_execution_boundary(
            golden_execution_boundary(),
            pipeline.execute_with_stream_prior_messages_max_rounds(
                request,
                prior,
                stream.then_some(&tx),
                12,
                Some(&mut gate),
                None,
            ),
        )
        .await
        .unwrap();
        drop(tx);
        drain.await.unwrap();
        assert_eq!(response.text, "done");
        let requests = client.requests();
        assert_eq!(observed.lock().unwrap().len(), 1); // phase 1–2 observer contract
        let reports: Vec<_> = ledger
            .ledger
            .lock()
            .unwrap()
            .iter()
            .filter_map(|row| {
                row.inference
                    .as_ref()
                    .map(|inference| inference.request.clone())
            })
            .collect();
        assert_eq!(requests.len(), 9);
        assert_eq!(reports.len(), requests.len());
        for (request, footprint) in requests.iter().zip(reports.iter()) {
            let report = footprint.inference_window.as_ref().unwrap();
            assert!(report.reconciled);
            if let Some(first) = &footprint.first_request_context {
                assert_eq!(first.decisions, report.decisions);
                assert_eq!(first.decisions.capacity.input_tokens, policy.input_tokens);
            }
            assert_eq!(report.deferred_history_messages, 2);
            assert_eq!(report.estimate, estimate(request));
            assert!(report.estimate.total_tokens <= policy.input_budget());
            assert!(
                format!("{:?}", request.messages)
                    .contains("fresh protected exact-note-id:scope-a:42")
            );
        }
    }
}

#[tokio::test]
async fn restored_checkpoint_recompiles_for_smaller_target_and_replaces_stale_stickies() {
    let original = run_golden(
        "current exact principal",
        vec![
            tool_response(vec![tool_call("data_probe", json!({"step":1}))]),
            tool_response(vec![finish_call("initial done")]),
        ],
        4,
        false,
    )
    .await;
    let saved = original
        .checkpoints
        .iter()
        .find(|state| state.boundary == SafeCheckpointBoundary::ToolBatchCompleted)
        .unwrap()
        .clone();
    let mut prefix = vec![
        ChatMessage::user("old question ".repeat(2000)),
        ChatMessage::assistant("old answer"),
    ];
    prefix.extend(saved.user_lane_prefix);
    let mut tool_lane = saved.tool_lane_messages;
    crate::turn_context::replace_sticky_context(
        &mut tool_lane,
        Some("stale protected sticky".into()),
    );
    let resume = ActiveTurnResumeState {
        source_daemon_turn_id: "saved-1".into(),
        restore_turn_budget: true,
        append_current_user_message: false,
        counters: saved.counters,
        transcript: ActiveTurnTranscript {
            user_lane_prefix: prefix.clone(),
            tool_lane_messages: tool_lane,
        },
        invocations: saved.invocations,
        scratch: saved.scratch,
        saved_human_prompt: "current exact principal".into(),
        resume_round_cap: Some(4),
    };
    let registry = InMemoryToolRegistry::default();
    registry.register_tool(DataProbeTool).unwrap();
    register_golden_turn_tool(&registry);
    let client = Arc::new(ScriptedClient::new(vec![tool_response(vec![finish_call(
        "resumed done",
    )])]));
    let pipeline = MedousaToolLoopPipeline::new(
        PromptExecutionPipeline::new(client.clone()),
        Arc::new(registry),
    )
    .with_window_policy(window(6000));
    let checkpoints = CapturingCheckpoints::default();
    let mut gate = ToolLoopCompletionGate::new_for_execution(2, RuntimePorts::new(), 4);
    gate.active_turn_resume = Some(resume);
    gate.active_turn_checkpoint_sink = Some(Arc::new(checkpoints.clone()));
    gate.round_context_provider = Some(Arc::new(FreshState));
    let request = ToolLoopExecutionRequest {
        user_prompt: "current exact principal".into(),
        system_prompt: Some("policy unchanged".into()),
        context: Default::default(),
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
    .unwrap();
    assert_eq!(response.text, "resumed done");
    let requests = client.requests();
    assert_eq!(requests.len(), 1);
    assert!(estimate(&requests[0]).total_tokens <= 6000);
    let rendered = format!("{:?}", requests[0].messages);
    assert!(rendered.contains("fresh protected exact-note-id:scope-a:42"));
    assert!(!rendered.contains("stale protected sticky"));
    assert!(!rendered.contains(&"old question ".repeat(2000)));
    let calls: Vec<_> = requests[0]
        .messages
        .iter()
        .flat_map(|m| m.content.tool_calls())
        .map(|c| c.call_id.clone())
        .collect();
    let results: Vec<_> = requests[0]
        .messages
        .iter()
        .flat_map(|m| m.content.tool_responses())
        .map(|r| r.call_id.clone())
        .collect();
    assert_eq!(calls, results);
    // Budget deferral is a projection, never a destructive checkpoint rewrite.
    let durable = checkpoints.states.lock().unwrap();
    assert_eq!(durable[0].user_lane_prefix.len(), prefix.len());
    assert_eq!(
        durable[0].user_lane_prefix[0].content.first_text(),
        prefix[0].content.first_text()
    );
}
