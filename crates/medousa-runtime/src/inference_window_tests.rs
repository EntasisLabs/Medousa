use super::*;
use crate::ports::{
    PerceptionEvidencePort, PerceptionEvidenceRequest, PersistedPerceptionEvidence,
};
use genai::chat::{ChatMessage, MessageContent, Tool, ToolCall, ToolResponse};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

fn policy(tokens: u32) -> InferenceWindowPolicy {
    InferenceWindowPolicy {
        target: format!("test:{tokens}"),
        input_tokens: Some(tokens),
        safety_tokens: 0,
        observation_tokens: 0,
        ..Default::default()
    }
}

fn compile(
    request: &mut ChatRequest,
    limit: u32,
    systems: usize,
    prefix: usize,
) -> Result<InferenceWindowReport, ContextWindowOverflow> {
    ContextCoordinator::compile_inference(
        request,
        &policy(limit),
        systems,
        prefix,
        None,
        &mut ToolPerceptionGovernor::default(),
    )
}

fn history() -> ChatRequest {
    ChatRequest::new(vec![
        ChatMessage::system("governing policy: identifiers remain exact"),
        ChatMessage::user("old question ".repeat(120)),
        ChatMessage::assistant("old answer ".repeat(120)),
        ChatMessage::user("recent question"),
        ChatMessage::assistant("recent answer"),
        ChatMessage::user("current exact-id: α🚀/deadbeef"),
        ChatMessage::system("protected sticky: do not cross scope auth:42"),
    ])
    .with_tools(vec![
        Tool::new("read").with_schema(json!({"type":"object"})),
    ])
}

#[test]
fn soft_working_window_is_separate_from_hard_provider_limits() {
    assert_eq!(policy(1_000_000).input_budget(), 1_000_000);
    assert_eq!(policy(1_000_000).working_budget(), WORKING_INPUT_TOKENS);
    assert_eq!(policy(32_000).input_budget(), 32_000);
    let shared = InferenceWindowPolicy {
        input_tokens: None,
        shared_tokens: Some(272_000),
        ..Default::default()
    };
    assert_eq!(shared.input_budget(), 266_880);
    assert_eq!(
        InferenceWindowPolicy {
            input_tokens: Some(16_000),
            ..shared
        }
        .input_budget(),
        14_976
    );
}

#[test]
fn deferred_history_route_preserves_chat_scope_and_never_relabels_work_scope() {
    use crate::context::{
        ContextContribution, ContextCost, ContextOwner, ContextProvenance, Retention,
    };
    use medousa_types::context_selection::{PointerRepresentation, PointerRetrieval};
    for owner in [ContextOwner::Chat, ContextOwner::UnitOfWork] {
        let mut request = history();
        let contribution = ContextContribution {
            id: "prior_conversation".into(),
            owner,
            provenance: ContextProvenance {
                source: "prior_conversation".into(),
                scope: Some("native-session-exact".into()),
            },
            retention: Retention::Flexible,
            decision: crate::context::ContextDecision::Included,
            cost: ContextCost::default(),
            budget_chars: None,
        };
        let mut coordinator = crate::context::ContextCoordinator::default();
        coordinator.include(contribution);
        let prepared = coordinator.report(crate::context::ContextCapacity::default());
        let report = ContextCoordinator::compile_inference(
            &mut request,
            &policy(1100),
            1,
            5,
            Some(&prepared),
            &mut ToolPerceptionGovernor::default(),
        )
        .unwrap();
        assert_eq!(report.deferred_history_messages, 2);
        if owner == ContextOwner::Chat {
            assert_eq!(
                report.deferred_pointers[0].retrieval,
                PointerRetrieval::ChatHistory {
                    session_id: "native-session-exact".into()
                }
            );
            assert_eq!(
                report.deferred_pointers[0].representation,
                PointerRepresentation::Deferred
            );
        } else {
            assert!(report.deferred_pointers.is_empty());
        }
    }
}

#[test]
fn separate_shared_unknown_and_zero_limits_are_distinct() {
    assert_eq!(policy(10000).input_budget(), 10000); // separate input limit
    assert_eq!(
        InferenceWindowPolicy {
            shared_tokens: Some(10000),
            ..policy(10000)
        }
        .input_budget(),
        5904
    );
    assert_eq!(InferenceWindowPolicy::default().input_budget(), 2880);
    assert_eq!(policy(0).input_budget(), 0);
    assert_eq!(
        InferenceWindowPolicy {
            shared_tokens: Some(100),
            ..policy(10000)
        }
        .input_budget(),
        0
    );
    assert!(policy(1_000_000).input_budget() > policy(1000).input_budget());
}

#[test]
fn utf8_and_tools_are_counted_without_chars_div_four_undercount() {
    let plain = ChatRequest::new(vec![ChatMessage::user("a")]);
    let utf8 = ChatRequest::new(vec![ChatMessage::user("🚀")]);
    assert_eq!(
        estimate(&utf8).total_tokens - estimate(&plain).total_tokens,
        3
    );
    let signature = ChatRequest::new(vec![ChatMessage::assistant(MessageContent::from_parts(
        vec![ContentPart::ThoughtSignature(
            "opaque-replay-signature".repeat(1000),
        )],
    ))]);
    assert!(estimate(&signature).text_and_protocol_tokens >= 22000);
    assert_eq!(estimate(&signature).media_parts, 0);
    let tools = plain
        .clone()
        .with_tools(vec![Tool::new("identifiers_are_exact").with_schema(
            json!({"type":"object","properties":{"path":{"type":"string"}}}),
        )]);
    assert!(estimate(&tools).total_tokens > estimate(&plain).total_tokens + 50);
}

#[test]
fn unknown_media_is_an_explicit_estimate_not_zero_or_base64_text() {
    let mut request = ChatRequest::new(vec![ChatMessage::user(MessageContent::from_parts(vec![
        ContentPart::from_binary_base64("image/png", "aGVsbG8=", None),
    ]))]);
    let measured = estimate(&request);
    assert_eq!(measured.media_parts, 1);
    assert_eq!(measured.media_tokens, 8192);
    let overflow = ContextCoordinator::compile_inference(
        &mut request,
        &InferenceWindowPolicy::default(),
        0,
        1,
        None,
        &mut ToolPerceptionGovernor::default(),
    )
    .unwrap_err();
    assert_eq!(overflow.report.estimate.media_tokens, 8192);
    assert!(!overflow.report.decisions.complete_token_cost_known);
    assert!(
        overflow
            .report
            .accounting
            .contains("not_provider_tokenization")
    );
}

#[test]
fn deterministic_exchange_selection_preserves_roles_chronology_and_protected_state() {
    let mut first = history();
    let mut second = first.clone();
    let a = compile(&mut first, 1100, 1, 5).unwrap();
    let b = compile(&mut second, 1100, 1, 5).unwrap();
    assert_eq!(a.deferred_history_messages, 2);
    assert_eq!(a.request_messages_digest, b.request_messages_digest);
    assert!(a.reconciled);
    assert!(a.estimate.total_tokens <= a.input_budget);
    let roles: Vec<_> = first.messages.iter().map(|m| m.role.clone()).collect();
    assert_eq!(
        roles,
        vec![
            ChatRole::System,
            ChatRole::User,
            ChatRole::Assistant,
            ChatRole::User,
            ChatRole::System
        ]
    );
    assert_eq!(
        first.messages[3].content.texts().concat(),
        "current exact-id: α🚀/deadbeef"
    );
    assert_eq!(
        first.messages[4].content.texts().concat(),
        "protected sticky: do not cross scope auth:42"
    );
}

#[test]
fn fallback_shrinkage_and_larger_resume_recompile_authoritative_history() {
    let original = history();
    let mut large = original.clone();
    let mut fallback = original.clone();
    assert_eq!(
        compile(&mut large, 100_000, 1, 5)
            .unwrap()
            .deferred_history_messages,
        0
    );
    assert_eq!(
        compile(&mut fallback, 1100, 1, 5)
            .unwrap()
            .deferred_history_messages,
        2
    );
    assert!(estimate(&large).total_tokens > 1100);
    assert!(estimate(&fallback).total_tokens <= 1100);
    let mut resumed = original.clone();
    compile(&mut resumed, 100_000, 1, 5).unwrap();
    assert_eq!(
        crate::context::prepared_messages_digest(&resumed.messages),
        crate::context::prepared_messages_digest(&original.messages)
    );
}

#[test]
fn protected_overflow_is_structured_and_does_not_truncate_policy_user_or_sticky() {
    let original = ChatRequest::new(vec![
        ChatMessage::system("policy".repeat(100)),
        ChatMessage::user("exact principal identifiers 🚀".repeat(100)),
        ChatMessage::system("protected sticky".repeat(100)),
    ]);
    let mut request = original.clone();
    let error = compile(&mut request, 500, 1, 1).unwrap_err();
    assert_eq!(error.kind, "context_window_overflow");
    assert!(error.estimated_tokens > error.input_budget);
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    assert_eq!(
        serde_json::to_value(error).unwrap()["kind"],
        "context_window_overflow"
    );
}

#[test]
fn codex_proxy_does_not_treat_code_bytes_as_tokens_and_soft_target_is_not_fatal() {
    let policy = InferenceWindowPolicy {
        target: "openai-codex:gpt-6.1-sol".into(),
        input_tokens: Some(475000),
        shared_tokens: Some(475000),
        enforce_output_limit: false,
        ..Default::default()
    };
    let source: String = (0..6000)
        .map(|i| format!("let entry_{i} = read_record(\"source/path/{i}\");\n"))
        .collect();
    let request = ChatRequest::new(vec![ChatMessage::user(source)]);
    assert!(estimate(&request).total_tokens > 250000);
    let measured = policy.estimate(&request);
    assert!(measured.total_tokens < 160000);
    assert_eq!(policy.input_budget(), 469880);
    assert_eq!(policy.working_budget(), 280000);
    assert!(policy.accounting().contains("o200k_proxy"));
    // Exact governing input may exceed the soft target and still be admitted.
    let mut required = ChatRequest::new(vec![ChatMessage::user("z".repeat(285000))]);
    let report = ContextCoordinator::compile_inference(
        &mut required,
        &self::policy(475000),
        0,
        1,
        None,
        &mut ToolPerceptionGovernor::default(),
    )
    .unwrap();
    assert!(report.estimate.total_tokens > report.policy.working_budget());
    assert!(report.estimate.total_tokens < report.input_budget);
}

#[test]
fn tokenizer_proxy_bounds_repetitive_input_and_handles_multibyte_chunk_boundaries() {
    let policy = InferenceWindowPolicy {
        target: "openai-codex:gpt-6.1-sol".into(),
        ..Default::default()
    };
    let repeated = "x".repeat(50000);
    let unicode = format!("{}{}", "a".repeat(2047), "🚀日本語".repeat(1000));
    for text in [&repeated, &unicode] {
        let tokens = policy.text_tokens(text);
        assert!(tokens > 0);
        assert!(tokens < text.len() as u32);
    }
}

#[test]
fn stable_baseline_is_not_reselected_when_tool_lane_grows() {
    let mut original = history();
    let first = ContextCoordinator::compile_inference_for_round(
        &mut original,
        &policy(10000),
        1,
        5,
        None,
        &mut ToolPerceptionGovernor::default(),
        true,
    )
    .unwrap();
    assert_eq!(first.deferred_history_messages, 0);
    let baseline = crate::context::prepared_messages_digest(&original.messages[..6]);
    original
        .messages
        .push(ChatMessage::system("protected tool work".repeat(500)));
    let error = ContextCoordinator::compile_inference_for_round(
        &mut original,
        &policy(10000),
        1,
        5,
        None,
        &mut ToolPerceptionGovernor::default(),
        false,
    )
    .unwrap_err();
    assert_eq!(error.report.deferred_history_messages, 0);
    assert_eq!(
        crate::context::prepared_messages_digest(&original.messages[..6]),
        baseline
    );
}

#[derive(Default)]
struct Evidence(Mutex<Vec<(String, Value)>>);
impl PerceptionEvidencePort for Evidence {
    fn persist(
        &self,
        request: PerceptionEvidenceRequest<'_>,
    ) -> Result<PersistedPerceptionEvidence, String> {
        let mut values = self.0.lock().unwrap();
        let reference = format!("scope-a:evidence:{}", values.len());
        values.push((reference.clone(), request.output.clone()));
        Ok(PersistedPerceptionEvidence {
            receipt: json!({"reference":reference}),
            logical_bytes: request.output.to_string().len() as u64,
            durable_receipt_staged: true,
            receipt_stage_error: None,
        })
    }
}

fn exchange(index: usize, tool: &str, output: Value) -> Vec<ChatMessage> {
    let call = ToolCall {
        call_id: format!("exact-{index}"),
        fn_name: tool.into(),
        fn_arguments: json!({"scope":"scope-a","index":index}),
        thought_signatures: None,
    };
    vec![
        ChatMessage::assistant(MessageContent::from_parts(vec![ContentPart::ToolCall(
            call.clone(),
        )])),
        ChatMessage::from(ToolResponse::from_tool_call(&call, output.to_string())),
    ]
}

#[test]
fn compaction_keeps_pairs_and_scoped_authoritative_nonreplayable_evidence() {
    let output = json!({"ok":true,"output":"x".repeat(25000),"completed":true});
    let evidence = Arc::new(Evidence::default());
    let mut governor = ToolPerceptionGovernor::new(Some(evidence.clone()));
    let mut messages = vec![ChatMessage::user("run in scope-a only")];
    messages.extend(exchange(1, "cognition_coder_shell_run", output.clone()));
    for index in 2..6 {
        messages.extend(exchange(index, "cognition_store_read", json!({"ok":true})));
    }

    let mut request = ChatRequest::new(messages);
    let report = ContextCoordinator::compile_inference(
        &mut request,
        &policy(3500),
        0,
        1,
        None,
        &mut governor,
    )
    .unwrap();
    assert!(report.estimate.total_tokens <= 3500);
    let call = request.messages[1]
        .content
        .tool_calls()
        .into_iter()
        .next()
        .unwrap();
    let response = request.messages[2]
        .content
        .tool_responses()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(call.call_id, response.call_id);
    assert_eq!(call.call_id, "exact-1");
    let compact: Value = serde_json::from_str(&response.content).unwrap();
    let reference = compact["ephemeral_evidence"]["reference"].as_str().unwrap();
    let values = evidence.0.lock().unwrap();
    assert_eq!(
        values.iter().find(|(key, _)| key == reference).unwrap().1,
        output
    );
    assert!(values.iter().all(|(key, _)| key.starts_with("scope-a:")));
}

#[test]
fn failed_and_nonreplayable_results_without_storage_overflow_instead_of_disappearing() {
    for output in [
        json!({"ok":false,"error":"important".repeat(1000)}),
        json!({"ok":true,"output":"non-replayable".repeat(1000)}),
    ] {
        let mut messages = vec![ChatMessage::user("current")];
        messages.extend(exchange(2, "cognition_coder_shell_run", output.clone()));
        let mut request = ChatRequest::new(messages);
        let error = compile(&mut request, 1200, 0, 1).unwrap_err();
        assert_eq!(error.kind, "context_window_overflow");
        assert_eq!(
            request.messages[2]
                .content
                .tool_responses()
                .into_iter()
                .next()
                .unwrap()
                .content,
            output.to_string()
        );
    }
}

#[test]
fn growing_tool_rounds_recheck_every_request_and_never_drop_protocol_pairs() {
    let evidence = Arc::new(Evidence::default());
    let mut governor = ToolPerceptionGovernor::new(Some(evidence));
    let mut authoritative = ChatRequest::new(vec![ChatMessage::user("current exact identifier")]);
    let mut successful = 0;
    for index in 0..30 {
        authoritative.messages.extend(exchange(
            index,
            "cognition_coder_shell_run",
            json!({"ok":true,"output":"x".repeat(4000),"completed":true}),
        ));
        let mut request = authoritative.clone();
        match ContextCoordinator::compile_inference(
            &mut request,
            &policy(24000),
            0,
            1,
            None,
            &mut governor,
        ) {
            Ok(report) => {
                successful += 1;
                assert!(report.estimate.total_tokens <= report.input_budget);
                assert!(report.reconciled);
                let calls: Vec<_> = request
                    .messages
                    .iter()
                    .flat_map(|m| m.content.tool_calls())
                    .map(|c| c.call_id.clone())
                    .collect();
                let results: Vec<_> = request
                    .messages
                    .iter()
                    .flat_map(|m| m.content.tool_responses())
                    .map(|r| r.call_id.clone())
                    .collect();
                assert_eq!(calls, results);
                assert_eq!(calls.len(), index + 1);
            }
            Err(overflow) => {
                assert_eq!(overflow.kind, "context_window_overflow");
                break;
            }
        }
    }
    assert!(
        successful >= 5,
        "must survive productive growth via evidence compaction"
    );
}
