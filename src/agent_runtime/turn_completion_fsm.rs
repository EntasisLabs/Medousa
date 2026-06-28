//! Explicit turn completion FSM — text-only model rounds.
//!
//! **Prose terminates:** any non-empty assistant message with zero tool calls in that round
//! ends the turn. Progress uses `cognition_turn_begin_work`; finals after tool work require
//! `cognition_turn_finish`. See `architecture/turn-prose-terminates-plan.md`.

use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::turn_text_heuristics::{
    looks_like_clarifying_question, looks_like_interim_status, looks_like_substantive_final_answer,
};

/// A non-tool draft at or below this many characters is treated as a brief
/// interim note (alongside `looks_like_interim_status`), eligible for a bounded
/// auto-continue instead of ending the turn.
pub const INTERIM_MAX_CHARS: usize = 255;

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
}

/// Developer-facing turn-control body for `[MEDOUSA_TURN_CONTROL]`.
pub fn continue_control_message(reason: ContinueReason, _missing_tools: &[String]) -> String {
    match reason {
        ContinueReason::EmptyAfterTools => {
            "Turn continues: last model round had no tool calls and no assistant text. \
             Call the tools you still need in this round, then cognition_turn_finish with the \
             complete answer, or cognition_turn_checkpoint for a mid-task handoff. \
             Note: assistant prose without tools ends this turn — after tools, only \
             cognition_turn_finish commits the final reply."
                .to_string()
        }
        ContinueReason::InterimProse => {
            "Turn continues: that was a brief acknowledgment, not the final answer. Call the \
             tool(s) you need now (or cognition_turn_begin_work to post a visible status line), \
             then cognition_turn_finish with the complete answer once the work is done. Keep any \
             interim note short AND pair it with a tool call — naked prose without a tool will end \
             the turn."
                .to_string()
        }
    }
}

/// Whether a non-empty, non-clarifying draft reads as a brief interim note rather
/// than a committed answer. Substantive answers are explicitly excluded so they
/// still terminate the turn normally.
fn is_interim_prose(draft: &str) -> bool {
    let trimmed = draft.trim();
    if trimmed.is_empty() {
        return false;
    }
    if looks_like_substantive_final_answer(draft) {
        return false;
    }
    looks_like_interim_status(draft) || trimmed.chars().count() < INTERIM_MAX_CHARS
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

    // Bounded interim auto-continue: a short pre-tool acknowledgment ("let me look
    // into that") continues the turn instead of ending it, so the model can do the
    // work it just promised. Capped per turn so it can never spin.
    if ctx.interim_continues_used < ctx.interim_continue_cap && is_interim_prose(&ctx.draft_text) {
        return continue_loop(ContinueReason::InterimProse, vec![]);
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

    // Bounded interim auto-continue after tools: a short status note ("pulling the
    // next batch…") continues so the model can call cognition_turn_finish to commit
    // the real answer, rather than the stub-terminating on naked prose.
    if ctx.interim_continues_used < ctx.interim_continue_cap && is_interim_prose(&ctx.draft_text) {
        return continue_loop(ContinueReason::InterimProse, vec![]);
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
        // A brief pre-tool acknowledgment now continues (bounded) instead of ending,
        // so the model can actually do the work it just promised.
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
    fn interim_before_tools_ends_when_cap_exhausted() {
        let mut round = ctx("Let me check that for you.");
        round.interim_continues_used = round.interim_continue_cap;
        let action = decide_no_tool_debt_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "no_tools_prose"
            }
        ));
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
        // A short status note after tools ("I'll spin up workers next") continues so
        // the model can call cognition_turn_finish to commit the real answer.
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
    fn interim_prose_after_tools_ends_when_cap_exhausted() {
        let invocations = vec![tool("cognition_memory_context")];
        let mut round = after_tools("I'll spin up workers next.", &invocations);
        round.interim_continues_used = round.interim_continue_cap;
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
        // A genuinely substantive answer (>=20 words / outcome-rich) still terminates
        // even under the interim policy — it is not an interim note.
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
    fn empty_after_tools_control_message_mentions_prose_rule() {
        let msg = continue_control_message(ContinueReason::EmptyAfterTools, &[]);
        assert!(msg.contains("cognition_turn_finish"));
        assert!(msg.contains("only"));
    }
}
