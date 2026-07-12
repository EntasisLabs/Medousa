//! Output-side turn completion: receipt checklist + optional gatekeeper model.

use genai::chat::{ChatMessage, ChatRequest};
use serde_json::Value;
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline,
};
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use super::prompt_prep::truncate_text_for_budget;
use super::stream_sink::SharedAgentStreamSink;
use super::turn_budget::{TurnBudget, TurnOrchestrationState, try_consume_gatekeeper_budget};
use std::sync::Arc;

use super::turn_context::{TurnScratchpad, WorkerHandoffCapsule};
use super::worker_continuity::HostContinuityBundle;
use stasis::ports::outbound::memory::memory_models::MemoryAvecState;
use crate::turn_text_heuristics::looks_like_interim_status;

/// Host context for completion gatekeeper calls from the tool loop.
pub struct ToolLoopCompletionGate<'a> {
    pub stream_turn_id: u64,
    pub session_id: Option<String>,
    pub sink: Option<SharedAgentStreamSink>,
    pub orchestration: Option<&'a mut TurnOrchestrationState>,
    pub budget: Option<&'a TurnBudget>,
    /// Configured model-round budget for this tool-loop execution.
    pub max_tool_rounds: usize,
    /// Consecutive text-only continues without new tools before the turn stops.
    pub max_text_only_stuck_continues: usize,
    /// Latest scratchpad snapshot from the tool loop (for failure explanation / debugging).
    pub scratch_out: Option<&'a mut Option<TurnScratchpad>>,
    /// Shared slot updated each host tool round; consumed at worker spawn.
    pub host_handoff_slot: Option<Arc<tokio::sync::RwLock<Option<WorkerHandoffCapsule>>>>,
    pub parent_turn_correlation_id: Option<String>,
    /// Seeds worker tool-loop scratch from host handoff (Tier C).
    pub initial_worker_scratch: Option<TurnScratchpad>,
    /// Raw user message for handoff (without tool-policy appendix).
    pub handoff_parent_user_prompt: Option<String>,
    pub handoff_vibe_signature: Option<String>,
    pub handoff_model_avec: Option<MemoryAvecState>,
    pub handoff_continuity_bundle: Option<HostContinuityBundle>,
    /// Workshop lane (research/general worker): skip host memory AVEC ritual receipt checks.
    pub skip_avec_ritual_check: bool,
    /// Origin channel for operator notifications (`tui`, `telegram`, `home-desktop`, …).
    pub channel: Option<String>,
    /// Full delivery target for channel push (Telegram chat id, Home session, …).
    pub delivery_target: Option<crate::turn_continuation::StoredDeliveryTarget>,
    /// Hard ceiling for silent tool-round extension (host bus cap).
    pub tool_round_budget_ceiling: usize,
    /// When true, `cognition_turn_request_more_rounds` pauses for operator approval.
    pub require_operator_budget_gate: bool,
    /// Host scheduler lane: prose-terminates after tools; bounded pre-tool continues.
    pub host_scheduler_lane: bool,
    /// Poll turn-worker store each round; end loop when status is cancelled.
    pub cancel_poll_work_id: Option<String>,
    /// Drain steer inbox each round and inject `[MEDOUSA_WORKSHOP_STEER]`.
    pub steer_poll_work_id: Option<String>,
}

impl ToolLoopCompletionGate<'_> {
    pub fn new_for_execution(
        stream_turn_id: u64,
        session_id: Option<String>,
        sink: Option<SharedAgentStreamSink>,
        max_tool_rounds: usize,
    ) -> Self {
        let max_tool_rounds = max_tool_rounds.max(1);
        Self {
            stream_turn_id,
            session_id,
            sink,
            orchestration: None,
            budget: None,
            max_tool_rounds,
            max_text_only_stuck_continues:
                crate::agent_runtime::turn_ledger::resolve_max_text_only_stuck_continues(
                    max_tool_rounds,
                ),
            scratch_out: None,
            host_handoff_slot: None,
            parent_turn_correlation_id: None,
            initial_worker_scratch: None,
            handoff_parent_user_prompt: None,
            handoff_vibe_signature: None,
            handoff_model_avec: None,
            handoff_continuity_bundle: None,
            skip_avec_ritual_check: false,
            channel: None,
            delivery_target: None,
            tool_round_budget_ceiling: max_tool_rounds,
            require_operator_budget_gate: false,
            host_scheduler_lane: false,
            cancel_poll_work_id: None,
            steer_poll_work_id: None,
        }
    }
}

impl ToolLoopCompletionGate<'_> {
    pub async fn reset_scratch(&self, streaming_enabled: bool) {
        if !streaming_enabled {
            return;
        }
        if let Some(sink) = self.sink.as_ref() {
            sink.scratch_reset(self.stream_turn_id).await;
        }
    }
}

const GATEKEEPER_MAX_PROMPT_CHARS: usize = 2_000;
const GATEKEEPER_MAX_DRAFT_CHARS: usize = 2_400;
const GATEKEEPER_CONFIDENCE_MIN: f32 = 0.55;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnCompletionDecision {
    EndTurn,
    Continue,
}

#[derive(Debug, Clone)]
pub struct TurnCompletionVerdict {
    pub decision: TurnCompletionDecision,
    pub confidence: f32,
    pub reason: String,
    pub source: &'static str,
    pub missing_tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TurnCompletionDocket {
    pub user_prompt: String,
    pub draft_text: String,
    pub pending_final_answer: bool,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub heuristic_would_finalize: bool,
    pub tools_invoked: Vec<String>,
    pub missing_ritual_tools: Vec<String>,
    pub stutter_detected: bool,
    /// Workshop worker (research/general): host receipt + interim finalize rules do not apply.
    pub workshop_lane: bool,
}

/// Lines that carry continuity metadata (model_avec, vibe) — not operator calibrate intent.
fn is_avec_ritual_metadata_line(line: &str) -> bool {
    let t = line.trim().to_ascii_lowercase();
    t.starts_with("model_avec=")
        || t.starts_with("user_avec=")
        || t.starts_with("vibe_signature=")
        || t.contains("compression_avec:")
}

fn prompt_text_for_avec_ritual_scan(prompt: &str) -> String {
    prompt
        .lines()
        .filter(|line| !is_avec_ritual_metadata_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// True when `avec` appears as its own token (e.g. "pull AVEC"), not inside `model_avec`.
fn contains_standalone_avec_token(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|token| token.eq_ignore_ascii_case("avec"))
}

pub fn user_prompt_has_avec_ritual_intent(prompt: &str) -> bool {
    let scanned = prompt_text_for_avec_ritual_scan(prompt);
    let lower = scanned.to_ascii_lowercase();
    if ["calibrat", "mood"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return true;
    }
    if contains_standalone_avec_token(&lower) {
        return true;
    }
    ["focused", "focus"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn tool_was_invoked(invocations: &[ToolInvocation], needles: &[&str]) -> bool {
    invocations.iter().any(|inv| {
        let name = inv.tool_name.to_ascii_lowercase();
        needles.iter().any(|needle| name.contains(needle))
    })
}

/// Code-only checklist for AVEC + calibrate flows.
pub fn missing_ritual_tools_for_avec(user_prompt: &str, invocations: &[ToolInvocation]) -> Vec<String> {
    if !user_prompt_has_avec_ritual_intent(user_prompt) {
        return Vec::new();
    }

    let lower = user_prompt.to_ascii_lowercase();
    let mut missing = Vec::new();

    if (lower.contains("pull") || lower.contains("preset"))
        && !tool_was_invoked(
            invocations,
            &[
                "cognition_memory_moods",
                "memory_moods",
                "cognition_memory_context",
                "memory_context",
            ],
        ) {
            missing.push("cognition_memory_moods".to_string());
        }

    if !tool_was_invoked(
        invocations,
        &["cognition_memory_calibrate", "memory_calibrate", "calibrate"],
    ) {
        missing.push("cognition_memory_calibrate".to_string());
    }
    missing
}

pub fn collect_tool_names(invocations: &[ToolInvocation]) -> Vec<String> {
    invocations
        .iter()
        .map(|inv| inv.tool_name.clone())
        .collect()
}

fn normalize_draft(text: &str) -> String {
    text.to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn drafts_look_similar(previous: &str, current: &str) -> bool {
    let prev = normalize_draft(previous);
    let curr = normalize_draft(current);
    if prev.len() < 48 || curr.len() < 48 {
        return false;
    }
    if prev == curr {
        return true;
    }
    let shared = prev
        .chars()
        .zip(curr.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let min_len = prev.len().min(curr.len());
    shared * 100 / min_len.max(1) >= 72
}

pub fn build_turn_completion_docket(
    user_prompt: &str,
    draft_text: &str,
    invocations: &[ToolInvocation],
    pending_final_answer: bool,
    rounds_executed: usize,
    max_tool_rounds: usize,
    heuristic_would_finalize: bool,
    previous_draft: Option<&str>,
    skip_avec_ritual_check: bool,
) -> TurnCompletionDocket {
    let missing_ritual_tools = if skip_avec_ritual_check {
        Vec::new()
    } else {
        missing_ritual_tools_for_avec(user_prompt, invocations)
    };
    let stutter_detected = previous_draft
        .filter(|prev| drafts_look_similar(prev, draft_text))
        .is_some();

    TurnCompletionDocket {
        user_prompt: user_prompt.to_string(),
        draft_text: draft_text.to_string(),
        pending_final_answer,
        rounds_executed,
        max_tool_rounds,
        heuristic_would_finalize,
        tools_invoked: collect_tool_names(invocations),
        missing_ritual_tools,
        stutter_detected,
        workshop_lane: skip_avec_ritual_check,
    }
}

/// After `prepare_final`, workshop workers deliver an internal result for synthesis — not operator interim chat.
pub fn workshop_lane_finalize_allowed(docket: &TurnCompletionDocket) -> bool {
    docket.workshop_lane && docket.pending_final_answer && !docket.draft_text.trim().is_empty()
}

pub fn should_invoke_completion_gatekeeper(docket: &TurnCompletionDocket) -> bool {
    if workshop_lane_finalize_allowed(docket) {
        return false;
    }
    if docket.pending_final_answer {
        return true;
    }
    if !docket.missing_ritual_tools.is_empty() && docket.heuristic_would_finalize {
        return true;
    }
    if docket.stutter_detected {
        return true;
    }
    if docket.heuristic_would_finalize
        && docket.rounds_executed + 1 >= docket.max_tool_rounds.saturating_sub(1)
    {
        return true;
    }
    false
}

pub fn receipt_checklist_verdict(docket: &TurnCompletionDocket) -> Option<TurnCompletionVerdict> {
    if docket.workshop_lane {
        return None;
    }
    if !docket.missing_ritual_tools.is_empty() {
        return Some(TurnCompletionVerdict {
            decision: TurnCompletionDecision::Continue,
            confidence: 1.0,
            reason: format!(
                "ritual incomplete; missing: {}",
                docket.missing_ritual_tools.join(", ")
            ),
            source: "receipt_checklist",
            missing_tools: docket.missing_ritual_tools.clone(),
        });
    }

    if docket.pending_final_answer && looks_like_interim_status(&docket.draft_text) {
        return Some(TurnCompletionVerdict {
            decision: TurnCompletionDecision::Continue,
            confidence: 0.9,
            reason: "prepare_final called but draft still looks like in-progress status".to_string(),
            source: "receipt_checklist",
            missing_tools: Vec::new(),
        });
    }

    None
}

pub async fn classify_turn_completion_with_gatekeeper(
    pipeline: &PromptExecutionPipeline,
    docket: &TurnCompletionDocket,
) -> Option<TurnCompletionVerdict> {
    let bounded_prompt = truncate_text_for_budget(&docket.user_prompt, GATEKEEPER_MAX_PROMPT_CHARS);
    let bounded_draft = truncate_text_for_budget(&docket.draft_text, GATEKEEPER_MAX_DRAFT_CHARS);
    let tools = if docket.tools_invoked.is_empty() {
        "(none)".to_string()
    } else {
        docket.tools_invoked.join(", ")
    };
    let missing = if docket.missing_ritual_tools.is_empty() {
        "(none)".to_string()
    } else {
        docket.missing_ritual_tools.join(", ")
    };

    let messages = vec![
        ChatMessage::system(
            "You are a turn-completion gatekeeper for Medousa. Decide whether the assistant turn \
             should END (publish draft to user) or CONTINUE (more tools or another model round). \
             Return strict JSON only: {\"decision\":\"end_turn\"|\"continue\",\"confidence\":0-1,\"reason\":\"...\"}. \
             CONTINUE if ritual tools are missing, draft repeats prior content without new receipts, \
             or text promises work not yet done (e.g. \"let me calibrate\" without calibrate receipt). \
             END when the draft fully answers the user request with no pending work, OR when the draft \
             is a concise clarifying question that should go to the user instead of more tool rounds."
                .to_string(),
        ),
        ChatMessage::user(format!(
            "USER_REQUEST:\n{bounded_prompt}\n\n\
             TOOLS_INVOKED:\n{tools}\n\n\
             MISSING_RITUAL_TOOLS:\n{missing}\n\n\
             PREPARE_FINAL_SIGNAL:\n{}\n\n\
             HEURISTIC_WOULD_FINALIZE:\n{}\n\n\
             STUTTER_DETECTED:\n{}\n\n\
             ROUNDS:\n{}/{}\n\n\
             DRAFT_TEXT:\n{bounded_draft}\n\n\
             Should this turn end now?",
            docket.pending_final_answer,
            docket.heuristic_would_finalize,
            docket.stutter_detected,
            docket.rounds_executed,
            docket.max_tool_rounds,
        )),
    ];

    let completion = pipeline
        .complete_chat(
            ChatRequest::new(messages),
            PromptExecutionContext::default(),
        )
        .await
        .ok()?;

    let raw = completion
        .response
        .into_first_text()
        .map(|value| value.trim().to_string())?;

    let parsed: Value = serde_json::from_str(&raw).ok()?;
    let decision_raw = parsed
        .get("decision")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())?;
    let confidence = parsed
        .get("confidence")
        .and_then(|value| value.as_f64())
        .map(|value| value as f32)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let reason = parsed
        .get("reason")
        .and_then(|value| value.as_str())
        .map(|value| truncate_text_for_budget(value, 160))
        .unwrap_or_else(|| "none".to_string());

    let decision = match decision_raw.as_str() {
        "end_turn" | "end" | "finalize" => TurnCompletionDecision::EndTurn,
        _ => TurnCompletionDecision::Continue,
    };

    Some(TurnCompletionVerdict {
        decision,
        confidence,
        reason,
        source: "gatekeeper_model",
        missing_tools: docket.missing_ritual_tools.clone(),
    })
}

pub async fn resolve_turn_completion(
    pipeline: &PromptExecutionPipeline,
    docket: &TurnCompletionDocket,
    sink: Option<&SharedAgentStreamSink>,
    orchestration: Option<&mut TurnOrchestrationState>,
    budget: Option<&TurnBudget>,
) -> TurnCompletionVerdict {
    if workshop_lane_finalize_allowed(docket) {
        let verdict = TurnCompletionVerdict {
            decision: TurnCompletionDecision::EndTurn,
            confidence: 1.0,
            reason: "workshop_lane prepare_final delivery (synthesis-bound)".to_string(),
            source: "workshop_lane",
            missing_tools: Vec::new(),
        };
        emit_gatekeeper_notice(sink, &verdict).await;
        return verdict;
    }

    if let Some(verdict) = receipt_checklist_verdict(docket) {
        emit_gatekeeper_notice(sink, &verdict).await;
        return verdict;
    }

    if !should_invoke_completion_gatekeeper(docket) {
        return TurnCompletionVerdict {
            decision: if docket.heuristic_would_finalize {
                TurnCompletionDecision::EndTurn
            } else {
                TurnCompletionDecision::Continue
            },
            confidence: 1.0,
            reason: "heuristic_only".to_string(),
            source: "heuristic",
            missing_tools: Vec::new(),
        };
    }

    let can_call_model = match (orchestration, budget, sink) {
        (Some(state), Some(budget), Some(sink)) => {
            try_consume_gatekeeper_budget(sink, state, budget).await
        }
        _ => false,
    };

    if can_call_model {
        if let Some(verdict) = classify_turn_completion_with_gatekeeper(pipeline, docket).await
            && (verdict.confidence >= GATEKEEPER_CONFIDENCE_MIN
                || verdict.decision == TurnCompletionDecision::Continue)
            {
                emit_gatekeeper_notice(sink, &verdict).await;
                return verdict;
            }
        if let Some(sink) = sink {
            sink.notice(
                "◈ completion gatekeeper skipped: low confidence; using heuristic".to_string(),
            )
            .await;
        }
    }

    TurnCompletionVerdict {
        decision: if (docket.workshop_lane && docket.pending_final_answer
            && !docket.draft_text.trim().is_empty())
            || (docket.heuristic_would_finalize
                && !looks_like_interim_status(&docket.draft_text))
        {
            TurnCompletionDecision::EndTurn
        } else {
            TurnCompletionDecision::Continue
        },
        confidence: 1.0,
        reason: "heuristic_fallback".to_string(),
        source: "heuristic",
        missing_tools: Vec::new(),
    }
}

async fn emit_gatekeeper_notice(sink: Option<&SharedAgentStreamSink>, verdict: &TurnCompletionVerdict) {
    let Some(sink) = sink else {
        return;
    };
    let decision = match verdict.decision {
        TurnCompletionDecision::EndTurn => "end_turn",
        TurnCompletionDecision::Continue => "continue",
    };
    sink.notice(format!(
        "◈ completion gatekeeper decision={decision} confidence={:.2} source={} reason={}",
        verdict.confidence, verdict.source, verdict.reason
    ))
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ritual_detects_missing_calibrate() {
        let invocations = vec![ToolInvocation {
            tool_name: "cognition_memory_moods".to_string(),
            tool_input: Value::Null,
            tool_output: Value::Null,
        }];
        let missing = missing_ritual_tools_for_avec("pull focused AVEC and calibrate", &invocations);
        assert!(missing.iter().any(|t| t.contains("calibrate")));
    }

    #[test]
    fn ritual_detects_missing_moods_on_pull() {
        let invocations = vec![ToolInvocation {
            tool_name: "cognition_memory_calibrate".to_string(),
            tool_input: Value::Null,
            tool_output: Value::Null,
        }];
        let missing = missing_ritual_tools_for_avec("pull focused AVEC preset", &invocations);
        assert!(missing.iter().any(|t| t.contains("moods")));
    }

    #[test]
    fn workshop_lane_allows_finalize_after_prepare_final_with_interim_phrasing() {
        let docket = TurnCompletionDocket {
            user_prompt: "WORKER_TASK".to_string(),
            draft_text: "searching tavily — here are raw results:\n- title one".to_string(),
            pending_final_answer: true,
            rounds_executed: 10,
            max_tool_rounds: 30,
            heuristic_would_finalize: true,
            tools_invoked: vec!["cognition_turn_prepare_final".to_string()],
            missing_ritual_tools: Vec::new(),
            stutter_detected: false,
            workshop_lane: true,
        };
        assert!(workshop_lane_finalize_allowed(&docket));
        assert!(receipt_checklist_verdict(&docket).is_none());
        assert!(!should_invoke_completion_gatekeeper(&docket));
    }

    #[test]
    fn continuity_model_avec_does_not_trigger_ritual() {
        let invocations = vec![ToolInvocation {
            tool_name: "cognition_grapheme_run".to_string(),
            tool_input: Value::Null,
            tool_output: Value::Null,
        }];
        let prompt = "[HOST_CONTINUITY]\nmodel_avec=stability=0.89 friction=0.25 logic=0.94 autonomy=0.82\n\
             WORKER_TASK:\nRun Tavily search for fun agentic projects";
        assert!(!user_prompt_has_avec_ritual_intent(prompt));
        let missing = missing_ritual_tools_for_avec(prompt, &invocations);
        assert!(missing.is_empty());
    }

    #[test]
    fn stutter_detects_similar_drafts() {
        let a = "Focused AVEC pulled. Stability 0.95. Let me calibrate.";
        let b = "Focused AVEC pulled. Stability 0.95. Let me calibrate to it.";
        assert!(drafts_look_similar(a, b));
    }
}
