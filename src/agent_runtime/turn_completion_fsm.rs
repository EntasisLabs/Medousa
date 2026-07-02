//! Explicit turn completion FSM — text-only model rounds.
//!
//! **Prose terminates:** any non-empty assistant message with zero tool calls in that round
//! ends the turn. Mid-turn status uses `cognition_turn_update_user`; heavy work uses
//! `cognition_turn_begin_work`; finals after tool work require `cognition_turn_finish`.

use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::turn_text_heuristics::{
    is_extended_prose, looks_like_clarifying_question, looks_like_interim_status,
    looks_like_planning_prose, looks_like_substantive_final_answer, EXTENDED_PROSE_CHAR_THRESHOLD,
};

/// A non-tool draft at or below this many characters is treated as a brief
/// interim note (alongside `looks_like_interim_status`), eligible for a bounded
/// auto-continue instead of ending the turn.
pub const INTERIM_MAX_CHARS: usize = EXTENDED_PROSE_CHAR_THRESHOLD;

/// Per-turn budget for bounded interim auto-continues (short non-tool notes).
pub fn resolve_interim_continue_cap(max_tool_rounds: usize) -> usize {
    let rounds = max_tool_rounds.max(1);
    ((rounds * 2) / 5).clamp(3, 8)
}

/// What the tool loop should do after a text-only model response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnRoundAction {
    EndTurn {
        termination_reason: &'static str,
    },
    ContinueLoop {
        reason: ContinueReason,
        control_message: String,
        missing_tools: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinueReason {
    /// Model returned no text and no tool calls after tools already ran — nudge one more round.
    EmptyAfterTools,
    /// Model returned a short interim acknowledgment/status (no tool call) — continue
    /// (bounded) so a brief "let me check…" doesn't prematurely end the turn.
    InterimProse,
    /// Long planning/status prose — reloop with full text kept in transcript.
    ExtendedProse,
}

/// Developer-facing turn-control body for `[MEDOUSA_TURN_CONTROL]`.
pub fn continue_control_message(reason: ContinueReason, _missing_tools: &[String]) -> String {
    match reason {
        ContinueReason::EmptyAfterTools => {
            "Turn continues: last model round had no tool calls and no assistant text. \
             Call the tools you still need in this round, then cognition_turn_finish with the \
             complete answer, or cognition_turn_checkpoint for a mid-task handoff. \
             After tools have run, only cognition_turn_finish commits the principal-facing answer."
                .to_string()
        }
        ContinueReason::InterimProse => {
            "Turn continues: call tools for the work you described, or cognition_turn_begin_work(goal, message) \
             to enter the bound workshop for multi-tool execution. Host scheduling prose may continue briefly; \
             use cognition_turn_finish once the principal-facing answer is ready."
                .to_string()
        }
        ContinueReason::ExtendedProse => {
            "Runtime reloop: your last message was kept in history. Next round: call cognition_turn_begin_work \
             for execution work, or the tools you still need on host (memory, vault read, runtime). \
             Check [MEDOUSA_SCRATCH] digests_recent before re-calling tools you already ran."
                .to_string()
        }
    }
}

fn is_short_interim_prose(draft: &str) -> bool {
    let trimmed = draft.trim();
    if trimmed.is_empty() || is_extended_prose(trimmed) {
        return false;
    }
    if looks_like_substantive_final_answer(draft) {
        return false;
    }
    looks_like_interim_status(draft) || trimmed.chars().count() <= INTERIM_MAX_CHARS
}

fn continue_loop(
    reason: ContinueReason,
    missing_tools: Vec<String>,
) -> TurnRoundAction {
    TurnRoundAction::ContinueLoop {
        reason,
        control_message: continue_control_message(reason, &missing_tools),
        missing_tools,
    }
}

fn maybe_continue_prose(
    draft: &str,
    interim_continues_used: usize,
    interim_continue_cap: usize,
    after_tools: bool,
) -> Option<TurnRoundAction> {
    if interim_continues_used >= interim_continue_cap {
        return None;
    }
    if looks_like_substantive_final_answer(draft) {
        return None;
    }
    if is_extended_prose(draft) {
        return Some(continue_loop(ContinueReason::ExtendedProse, vec![]));
    }
    if is_short_interim_prose(draft) {
        return Some(continue_loop(ContinueReason::InterimProse, vec![]));
    }
    if !after_tools && looks_like_planning_prose(draft) {
        return Some(continue_loop(ContinueReason::ExtendedProse, vec![]));
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoToolDebtRoundContext {
    pub draft_text: String,
    pub pending_final_answer: bool,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    /// Interim auto-continues already spent this turn.
    pub interim_continues_used: usize,
    /// Per-turn budget for interim auto-continues (bounded so the loop can't spin).
    pub interim_continue_cap: usize,
    /// Host scheduler: cooperative prose; execution via bound workshop.
    pub host_scheduler_lane: bool,
}

#[derive(Debug, Clone)]
pub struct AfterToolsRoundContext<'a> {
    pub draft_text: String,
    pub pending_final_answer: bool,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub invocations: &'a [ToolInvocation],
    pub workshop_lane: bool,
    /// Interim auto-continues already spent this turn.
    pub interim_continues_used: usize,
    /// Per-turn budget for interim auto-continues (bounded so the loop can't spin).
    pub interim_continue_cap: usize,
    /// Host scheduler: cooperative prose; execution via bound workshop.
    pub host_scheduler_lane: bool,
}

/// Zero tool invocations this turn — any non-empty prose ends the turn.
pub fn decide_no_tool_debt_text_round(ctx: &NoToolDebtRoundContext) -> TurnRoundAction {
    let draft = ctx.draft_text.trim();

    if ctx.pending_final_answer && !draft.is_empty() {
        return TurnRoundAction::EndTurn {
            termination_reason: "prepare_final_then_text",
        };
    }

    if ctx.rounds_executed >= ctx.max_tool_rounds.max(1) {
        return TurnRoundAction::EndTurn {
            termination_reason: "max_rounds_fuse",
        };
    }

    if looks_like_clarifying_question(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "clarifying_question",
        };
    }

    if let Some(action) = maybe_continue_prose(
        &ctx.draft_text,
        ctx.interim_continues_used,
        ctx.interim_continue_cap,
        false,
    ) {
        return action;
    }

    TurnRoundAction::EndTurn {
        termination_reason: "no_tools_prose",
    }
}

/// Tools already ran — non-empty prose without `cognition_turn_finish` ends with a stub body.
pub fn decide_after_tools_text_round(ctx: &AfterToolsRoundContext<'_>) -> TurnRoundAction {
    let draft = ctx.draft_text.trim();

    if ctx.workshop_lane && ctx.pending_final_answer && !draft.is_empty() {
        return TurnRoundAction::EndTurn {
            termination_reason: "workshop_lane_prepare_final",
        };
    }

    if ctx.rounds_executed >= ctx.max_tool_rounds.max(1) {
        return TurnRoundAction::EndTurn {
            termination_reason: "max_rounds_fuse",
        };
    }

    if ctx.pending_final_answer && !draft.is_empty() {
        return TurnRoundAction::EndTurn {
            termination_reason: "prepare_final_then_text",
        };
    }

    if draft.is_empty() {
        return continue_loop(ContinueReason::EmptyAfterTools, vec![]);
    }

    if looks_like_clarifying_question(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "clarifying_question",
        };
    }

    if let Some(action) = maybe_continue_prose(
        &ctx.draft_text,
        ctx.interim_continues_used,
        ctx.interim_continue_cap,
        true,
    ) {
        return action;
    }

    if ctx.host_scheduler_lane && !draft.is_empty() && !ctx.pending_final_answer {
        return continue_loop(ContinueReason::ExtendedProse, vec![]);
    }

    TurnRoundAction::EndTurn {
        termination_reason: "prose_requires_finish",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn ctx(draft: &str) -> NoToolDebtRoundContext {
        NoToolDebtRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 1,
            max_tool_rounds: 10,
            interim_continues_used: 0,
            interim_continue_cap: 2,
            host_scheduler_lane: false,
        }
    }

    fn after_tools<'a>(draft: &str, invocations: &'a [ToolInvocation]) -> AfterToolsRoundContext<'a> {
        AfterToolsRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 3,
            max_tool_rounds: 10,
            invocations,
            workshop_lane: false,
            interim_continues_used: 0,
            interim_continue_cap: 2,
            host_scheduler_lane: false,
        }
    }

    fn tool(name: &str) -> ToolInvocation {
        ToolInvocation {
            tool_name: name.to_string(),
            tool_input: Value::Null,
            tool_output: Value::Null,
        }
    }

    #[test]
    fn interim_before_tools_continues_bounded() {
        let action = decide_no_tool_debt_text_round(&ctx("Let me check that for you."));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::InterimProse,
                ..
            }
        ));
    }

    #[test]
    fn planning_prose_before_tools_continues_extended() {
        let planning = "The environment is confirmed — 11 surfaces, blank canvas, and the full \
                        component toolkit is live. Let's make the first mark. I'm going to build \
                        you a Home dashboard — a persistent component on the home surface. \
                        I'll start with environment_get, then propose a custom surface in the active \
                        preset, then component_create with presentation type and artifactId config.";
        assert!(is_extended_prose(planning));
        let action = decide_no_tool_debt_text_round(&ctx(planning));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::ExtendedProse,
                ..
            }
        ));
    }

    #[test]
    fn interim_before_tools_ends_when_cap_exhausted() {
        let cap = resolve_interim_continue_cap(10);
        let mut round = ctx("Let me check that for you.");
        round.interim_continues_used = cap;
        round.interim_continue_cap = cap;
        let action = decide_no_tool_debt_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "no_tools_prose"
            }
        ));
    }

    #[test]
    fn self_correction_after_tools_continues_as_interim() {
        let invocations = vec![tool("cognition_environment_get")];
        let draft = "Now I see what went wrong before — I was targeting home (builtin), which \
                     silently rejects components. Let me grab the schemas.";
        assert!(crate::turn_text_heuristics::looks_like_interim_status(draft));
        let action = decide_after_tools_text_round(&after_tools(draft, &invocations));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::InterimProse,
                ..
            }
        ));
    }

    #[test]
    fn interim_continue_cap_scales_with_round_budget() {
        assert_eq!(resolve_interim_continue_cap(10), 4);
        assert_eq!(resolve_interim_continue_cap(4), 3);
        assert_eq!(resolve_interim_continue_cap(20), 8);
    }

    #[test]
    fn substantive_no_tool_answer_ends() {
        let answer = "Here is a complete explanation of how the ingester maps channel \
                      sessions to Medousa history without any further steps needed.";
        let action = decide_no_tool_debt_text_round(&ctx(answer));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "no_tools_prose"
            }
        ));
    }

    #[test]
    fn prepare_final_with_text_ends() {
        let mut round = ctx("Here is your answer.");
        round.pending_final_answer = true;
        let action = decide_no_tool_debt_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "prepare_final_then_text"
            }
        ));
    }

    #[test]
    fn interim_prose_after_tools_continues_bounded() {
        let invocations = vec![tool("cognition_memory_context")];
        let action = decide_after_tools_text_round(&after_tools(
            "I'll spin up workers next.",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::InterimProse,
                ..
            }
        ));
    }

    #[test]
    fn celebratory_preamble_after_tools_continues_extended() {
        let preamble = "Yesss! Let's do this — I'll pull up the current context, check what's \
                          resonating in memory, and calibrate to a focused AVEC posture. Boom — \
                          focused preset pulled. Let me lock it in and then call cognition_turn_finish \
                          once the full calibration summary is ready for you to read.";
        assert!(is_extended_prose(preamble));
        let invocations = vec![
            tool("cognition_memory_moods"),
            tool("cognition_memory_calibrate"),
        ];
        let action = decide_after_tools_text_round(&after_tools(preamble, &invocations));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::ExtendedProse,
                ..
            }
        ));
    }

    #[test]
    fn interim_prose_after_tools_ends_when_cap_exhausted() {
        let invocations = vec![tool("cognition_memory_context")];
        let cap = resolve_interim_continue_cap(10);
        let mut round = after_tools("I'll spin up workers next.", &invocations);
        round.interim_continues_used = cap;
        round.interim_continue_cap = cap;
        let action = decide_after_tools_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "prose_requires_finish"
            }
        ));
    }

    #[test]
    fn substantive_prose_after_tools_requires_finish() {
        let invocations = vec![tool("cognition_memory_moods")];
        let action = decide_after_tools_text_round(&after_tools(
            "Focused preset pulled and applied: stability is now 0.95, friction dropped to 0.12, \
             and autonomy holds at 0.80. I stored the calibration summary in Locus for this session.",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "prose_requires_finish"
            }
        ));
    }

    #[test]
    fn clarifying_question_after_tools_commits_prose() {
        let invocations = vec![tool("cognition_memory_context")];
        let action = decide_after_tools_text_round(&after_tools(
            "Which repository should I search — medousa or stasis?",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "clarifying_question"
            }
        ));
    }

    #[test]
    fn empty_after_tools_continues_without_draft() {
        let invocations = vec![tool("cognition_tool_history_summary")];
        let action = decide_after_tools_text_round(&after_tools("", &invocations));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::EmptyAfterTools,
                ..
            }
        ));
    }

    #[test]
    fn workshop_lane_prepare_final_ends() {
        let invocations = vec![tool("cognition_turn_prepare_final")];
        let mut round = after_tools(
            "searching tavily — here are raw results:\n- title one",
            &invocations,
        );
        round.pending_final_answer = true;
        round.workshop_lane = true;
        let action = decide_after_tools_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "workshop_lane_prepare_final"
            }
        ));
    }

    #[test]
    fn interim_prose_control_message_recommends_begin_work() {
        let msg = continue_control_message(ContinueReason::InterimProse, &[]);
        assert!(msg.contains("cognition_turn_begin_work"));
        assert!(msg.contains("cognition_turn_finish"));
    }

    #[test]
    fn extended_prose_control_message_mentions_reloop() {
        let msg = continue_control_message(ContinueReason::ExtendedProse, &[]);
        assert!(msg.contains("Runtime reloop"));
        assert!(msg.contains("cognition_turn_begin_work"));
    }

    #[test]
    fn empty_after_tools_control_message_mentions_prose_rule() {
        let msg = continue_control_message(ContinueReason::EmptyAfterTools, &[]);
        assert!(msg.contains("cognition_turn_finish"));
    }
}
