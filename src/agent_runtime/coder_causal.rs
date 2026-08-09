//! Stable causal projections over the Coder activity ledger and sealed Forge
//! experiments. These workflows reconstruct explicit operational evidence;
//! they never infer private reasoning or automatically replay side effects.

use std::collections::BTreeSet;

use genai::chat::Tool;
use medousa_forge::forge::Forge;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::StasisError;
use stasis::prelude::Result;

use super::coder_activity::{CoderActivityEvent, CoderActivityKind};
use super::coder_mode::CoderEntryContext;

pub const COGNITION_CODER_CAUSAL_QUERY: &str = "cognition_coder_causal_query";

pub fn tool_definition() -> Tool {
    Tool::new(COGNITION_CODER_CAUSAL_QUERY)
        .with_description(
            "Query stable Coder traces with why, observation-only replay, regression, or sealed counterfactual workflows. This never exposes hidden reasoning or automatically reruns a tool.",
        )
        .with_schema(json!({
            "type": "object",
            "properties": {
                "workflow": {
                    "type": "string",
                    "enum": ["why", "replay", "regression", "counterfactual"]
                },
                "target": {
                    "type": "string",
                    "description": "engineering:call, engineering:event, or engineering:trace id for why/replay/regression"
                },
                "compare_to": {
                    "type": "string",
                    "description": "Second stable engineering id for regression"
                },
                "attempt_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 2,
                    "maxItems": 4,
                    "uniqueItems": true,
                    "description": "Optional exact sealed attempts for a counterfactual comparison"
                }
            },
            "required": ["workflow"]
        }))
}

#[derive(Debug, Clone, Serialize)]
struct CausalTransition {
    id: String,
    event_id: String,
    revision: u64,
    from_state: String,
    to_state: String,
    occurred_at_utc: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CausalTrace {
    id: String,
    kind: &'static str,
    work_id: String,
    call_id: Option<String>,
    agent_id: String,
    session_id: String,
    turn_id: String,
    attempt_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    declared_intent: Option<String>,
    targets: Vec<String>,
    initial_revision: u64,
    final_revision: u64,
    final_state: String,
    transitions: Vec<CausalTransition>,
    evidence_refs: Vec<String>,
}

pub async fn invoke_causal_query(
    forge: &Forge,
    registry: &dyn ToolRegistry,
    entry: &CoderEntryContext,
    events: &[CoderActivityEvent],
    input: &Value,
) -> Result<Value> {
    let workflow = required_string(input, "workflow")?;
    match workflow {
        "why" => {
            let target = required_string(input, "target")?;
            let trace = trace_for_target(events, target)?;
            Ok(json!({
                "ok": true,
                "workflow": causal_workflow_id("why", &[trace.id.as_str()]),
                "query": "why",
                "trace": trace,
                "explanation": trace_explanation(&trace),
                "limits": {
                    "basis": "explicit_engineering_activity_only",
                    "hidden_reasoning_inferred": false,
                    "raw_tool_payload_replayed": false,
                }
            }))
        }
        "replay" => {
            let target = required_string(input, "target")?;
            let trace = trace_for_target(events, target)?;
            let current_head = forge
                .git()
                .head_oid(&entry.worktree)
                .map_err(|error| input_error(format!("cannot observe Coder HEAD: {error}")))?
                .to_string();
            Ok(json!({
                "ok": true,
                "workflow": causal_workflow_id("replay", &[trace.id.as_str(), current_head.as_str()]),
                "query": "replay",
                "trace": trace,
                "replay": {
                    "kind": "observation_only",
                    "historical_trace_id": trace.id,
                    "observed_head_oid": current_head,
                    "tool": trace.tool,
                    "declared_intent": trace.declared_intent,
                    "targets": trace.targets,
                    "input_payload_available": false,
                    "side_effects_executed": false,
                    "automatic_replay_allowed": false,
                    "fresh_action_required": true,
                    "reason": "The durable activity ledger proves lifecycle and bounded effects, not a complete still-valid invocation. Revalidate current authority and issue a new explicit tool call if the action remains necessary."
                }
            }))
        }
        "regression" => {
            let target = required_string(input, "target")?;
            let compare_to = required_string(input, "compare_to")?;
            let left = trace_for_target(events, target)?;
            let right = trace_for_target(events, compare_to)?;
            Ok(regression_projection(&left, &right))
        }
        "counterfactual" => {
            let comparison =
                super::coder_experiments::compare_sealed_candidates(forge, registry, entry, input)
                    .await?;
            let identities = comparison
                .get("candidates")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|candidate| {
                    Some(format!(
                        "{}@{}",
                        candidate.get("attempt_id")?.as_str()?,
                        candidate.get("sealed_head_oid")?.as_str()?
                    ))
                })
                .collect::<Vec<_>>();
            let refs = identities.iter().map(String::as_str).collect::<Vec<_>>();
            Ok(json!({
                "ok": true,
                "workflow": causal_workflow_id("counterfactual", &refs),
                "query": "counterfactual",
                "comparison": comparison,
                "side_effects_executed": false,
                "comparison_boundary": "sealed_attempt_evidence",
            }))
        }
        _ => Err(input_error(
            "workflow must be why, replay, regression, or counterfactual",
        )),
    }
}

fn trace_for_target(events: &[CoderActivityEvent], target: &str) -> Result<CausalTrace> {
    let target = target.trim();
    let target = target
        .strip_prefix("engineering:trace:event:")
        .map(|event_id| format!("engineering:event:{event_id}"))
        .or_else(|| {
            target
                .strip_prefix("engineering:trace:")
                .map(|call_id| format!("engineering:call:{call_id}"))
        })
        .unwrap_or_else(|| target.to_string());
    let detail =
        super::coder_pointers::follow_engineering_pointer(events, &target).map_err(input_error)?;
    let mut causal_events = if let Some(call_id) = detail.primary.call_id.as_deref() {
        events
            .iter()
            .filter(|event| event.call_id.as_deref() == Some(call_id))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        detail.causal_events
    };
    causal_events.sort_by_key(|event| event.revision);
    let first = causal_events
        .first()
        .ok_or_else(|| input_error("causal trace has no events"))?;
    let last = causal_events
        .last()
        .ok_or_else(|| input_error("causal trace has no events"))?;
    let mut targets = causal_events
        .iter()
        .flat_map(|event| event.targets.iter().cloned())
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();
    let call_id = last.call_id.clone();
    let trace_id = call_id
        .as_deref()
        .map(|call_id| format!("engineering:trace:{call_id}"))
        .unwrap_or_else(|| format!("engineering:trace:event:{}", last.event_id));
    let mut state = "absent".to_string();
    let transitions = causal_events
        .iter()
        .map(|event| {
            let next = event_state(event.kind).to_string();
            let transition = CausalTransition {
                id: format!("engineering:transition:{}", event.event_id),
                event_id: event.event_id.clone(),
                revision: event.revision,
                from_state: state.clone(),
                to_state: next.clone(),
                occurred_at_utc: event.occurred_at_utc,
                detail: event.detail.clone(),
            };
            state = next;
            transition
        })
        .collect::<Vec<_>>();
    let evidence_refs = causal_events
        .iter()
        .map(|event| format!("engineering:event:{}", event.event_id))
        .collect();
    Ok(CausalTrace {
        id: trace_id,
        kind: "coder_tool_trace",
        work_id: last.work_id.clone(),
        call_id,
        agent_id: last.agent_id.clone(),
        session_id: last.session_id.clone(),
        turn_id: last.turn_id.clone(),
        attempt_id: last.attempt_id.clone(),
        tool: causal_events.iter().find_map(|event| event.tool.clone()),
        declared_intent: causal_events.iter().find_map(|event| event.intent.clone()),
        targets,
        initial_revision: first.revision,
        final_revision: last.revision,
        final_state: state,
        transitions,
        evidence_refs,
    })
}

fn trace_explanation(trace: &CausalTrace) -> Value {
    json!({
        "cause": trace.declared_intent,
        "actor": {
            "agent_id": trace.agent_id,
            "session_id": trace.session_id,
            "turn_id": trace.turn_id,
            "attempt_id": trace.attempt_id,
        },
        "action": trace.tool,
        "targets": trace.targets,
        "outcome": trace.final_state,
        "evidence_refs": trace.evidence_refs,
        "statement": format!(
            "The recorded action was declared for '{}', then transitioned to '{}'.",
            trace.declared_intent.as_deref().unwrap_or("an unspecified operational intent"),
            trace.final_state,
        ),
    })
}

fn regression_projection(left: &CausalTrace, right: &CausalTrace) -> Value {
    let left_targets = left.targets.iter().cloned().collect::<BTreeSet<_>>();
    let right_targets = right.targets.iter().cloned().collect::<BTreeSet<_>>();
    let added_targets = right_targets
        .difference(&left_targets)
        .cloned()
        .collect::<Vec<_>>();
    let removed_targets = left_targets
        .difference(&right_targets)
        .cloned()
        .collect::<Vec<_>>();
    let outcome_changed = left.final_state != right.final_state;
    let possible_regression = matches!(left.final_state.as_str(), "completed")
        && matches!(right.final_state.as_str(), "failed" | "blocked");
    json!({
        "ok": true,
        "workflow": causal_workflow_id("regression", &[left.id.as_str(), right.id.as_str()]),
        "query": "regression",
        "left": left,
        "right": right,
        "comparison": {
            "tool_changed": left.tool != right.tool,
            "intent_changed": left.declared_intent != right.declared_intent,
            "outcome_changed": outcome_changed,
            "possible_regression": possible_regression,
            "added_targets": added_targets,
            "removed_targets": removed_targets,
            "basis": "explicit_trace_state_and_targets",
        },
        "limits": {
            "causation_proven_beyond_ledger": false,
            "side_effects_executed": false,
        }
    })
}

fn event_state(kind: CoderActivityKind) -> &'static str {
    match kind {
        CoderActivityKind::AgentJoined => "agent_joined",
        CoderActivityKind::ToolPlanned => "planned",
        CoderActivityKind::ToolBlocked => "blocked",
        CoderActivityKind::ToolCompleted => "completed",
        CoderActivityKind::ToolFailed => "failed",
        CoderActivityKind::AgentLeft => "agent_left",
    }
}

fn causal_workflow_id(kind: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    digest.update(kind.as_bytes());
    for part in parts {
        digest.update([0]);
        digest.update(part.as_bytes());
    }
    format!("causal:{kind}:sha256:{:x}", digest.finalize())
}

fn required_string<'a>(input: &'a Value, field: &str) -> Result<&'a str> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| input_error(format!("{field} is required")))
}

fn input_error(message: impl Into<String>) -> StasisError {
    StasisError::PortFailure(message.into())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn event(revision: u64, kind: CoderActivityKind, call_id: &str) -> CoderActivityEvent {
        CoderActivityEvent {
            event_id: format!("event-{revision}"),
            revision,
            call_id: Some(call_id.into()),
            work_id: "work-1".into(),
            agent_id: "agent-1".into(),
            session_id: "session-1".into(),
            turn_id: "turn-1".into(),
            attempt_id: "attempt-1".into(),
            kind,
            occurred_at_utc: Utc::now(),
            tool: Some("cognition_code_apply_patch".into()),
            intent: Some("Repair the parser".into()),
            targets: vec!["file://src/parser.rs".into()],
            claims: Vec::new(),
            overlaps: Vec::new(),
            detail: Some(event_state(kind).into()),
        }
    }

    #[test]
    fn trace_ids_and_transitions_are_stable() {
        let events = vec![
            event(1, CoderActivityKind::ToolPlanned, "call-1"),
            event(2, CoderActivityKind::ToolCompleted, "call-1"),
        ];
        let trace = trace_for_target(&events, "engineering:call:call-1").expect("trace");
        assert_eq!(trace.id, "engineering:trace:call-1");
        assert_eq!(trace.final_state, "completed");
        assert_eq!(trace.transitions[0].from_state, "absent");
        assert_eq!(trace.transitions[1].from_state, "planned");
        assert_eq!(trace.transitions[1].to_state, "completed");
    }

    #[test]
    fn regression_marks_completed_to_failed_without_claiming_extra_causation() {
        let events = vec![
            event(1, CoderActivityKind::ToolPlanned, "call-1"),
            event(2, CoderActivityKind::ToolCompleted, "call-1"),
            event(3, CoderActivityKind::ToolPlanned, "call-2"),
            event(4, CoderActivityKind::ToolFailed, "call-2"),
        ];
        let left = trace_for_target(&events, "engineering:call:call-1").expect("left");
        let right = trace_for_target(&events, "engineering:call:call-2").expect("right");
        let comparison = regression_projection(&left, &right);
        assert_eq!(comparison["comparison"]["possible_regression"], true);
        assert_eq!(
            comparison["limits"]["causation_proven_beyond_ledger"],
            false
        );
    }
}
