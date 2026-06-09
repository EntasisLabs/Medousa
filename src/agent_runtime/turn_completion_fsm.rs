//! Explicit turn completion FSM — text-only model rounds.
//!
//! Phase 1: no-tool-debt policy + transcript helpers.
//! Phase 2: post-tool-debt policy via receipt checklist.
//! Phase 5: no interim-heuristic continues — tool call = loop; prose-only = EndTurn.
//! Phase 3: centralized continue control messages + ledger reason mapping.

use genai::chat::ChatMessage;
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::agent_runtime::turn_completion::missing_ritual_tools_for_avec;
use crate::agent_runtime::turn_worker_tools::is_spawn_turn_worker_tool_name;
use crate::turn_text_heuristics::{
    draft_implies_pending_spawn, looks_like_clarifying_question, looks_like_substantive_final_answer,
    user_prompt_implies_host_delegation,
};

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
    /// Model prose looks in-progress; expect tools or a complete answer next round.
    AwaitingTools,
    /// AVEC / calibrate receipt checklist still open.
    MissingReceipts,
    /// `prepare_final` was invoked but draft still looks interim.
    PrepareFinalInterim,
    /// Host promised delegation (spawn workers) but `cognition_spawn_turn_worker` has not run yet.
    PendingDelegation,
}

/// Developer-facing turn-control body for `[MEDOUSA_TURN_CONTROL]`.
pub fn continue_control_message(reason: ContinueReason, missing_tools: &[String]) -> String {
    match reason {
        ContinueReason::AwaitingTools => {
            "Turn continues: last message looked in-progress; draft kept in transcript — tools or a complete answer next."
                .to_string()
        }
        ContinueReason::MissingReceipts => format!(
            "Turn continues: ritual receipts still open in this turn — {}.",
            missing_tools.join(", ")
        ),
        ContinueReason::PrepareFinalInterim => {
            "Turn continues: cognition_turn_prepare_final was called but the draft still looks interim — \
             send the complete principal-facing answer next."
                .to_string()
        }
        ContinueReason::PendingDelegation => {
            "Turn continues: delegated work is still open — call cognition_spawn_turn_worker with a complete task \
             (include resolved manuscripts/capabilities from this turn) or finish with cognition_turn_finish when done."
                .to_string()
        }
    }
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
}

#[derive(Debug, Clone)]
pub struct AfterToolsRoundContext<'a> {
    pub user_prompt: &'a str,
    pub draft_text: String,
    pub pending_final_answer: bool,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub invocations: &'a [ToolInvocation],
    pub workshop_lane: bool,
    pub open_gaps: &'a [String],
}

fn spawn_turn_worker_invoked(invocations: &[ToolInvocation]) -> bool {
    invocations
        .iter()
        .any(|inv| is_spawn_turn_worker_tool_name(&inv.tool_name))
}

pub fn should_continue_for_pending_delegation(ctx: &AfterToolsRoundContext<'_>) -> bool {
    if ctx.workshop_lane || spawn_turn_worker_invoked(ctx.invocations) {
        return false;
    }
    if user_prompt_implies_host_delegation(ctx.user_prompt) {
        return true;
    }
    if draft_implies_pending_spawn(&ctx.draft_text) {
        return true;
    }
    ctx.open_gaps.iter().any(|gap| {
        let lower = gap.to_ascii_lowercase();
        lower.contains("spawn") || lower.contains("worker") || lower.contains("delegate")
    })
}

/// Phase 1 policy: zero tool invocations this turn — default is end, not loop.
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

    TurnRoundAction::EndTurn {
        termination_reason: "no_tools_prose",
    }
}

/// Phase 2 policy: tools already ran this turn — end by default; continue for receipts or interim.
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

    if !ctx.workshop_lane {
        let missing_ritual =
            missing_ritual_tools_for_avec(ctx.user_prompt, ctx.invocations);
        if !missing_ritual.is_empty() {
            return continue_loop(ContinueReason::MissingReceipts, missing_ritual);
        }
    }

    if ctx.pending_final_answer && !draft.is_empty() {
        return TurnRoundAction::EndTurn {
            termination_reason: "prepare_final_then_text",
        };
    }

    if should_continue_for_pending_delegation(ctx) {
        return continue_loop(
            ContinueReason::PendingDelegation,
            vec!["cognition_spawn_turn_worker".to_string()],
        );
    }

    if looks_like_substantive_final_answer(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "tool_debt_complete",
        };
    }

    if looks_like_clarifying_question(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "clarifying_question",
        };
    }

    TurnRoundAction::EndTurn {
        termination_reason: "tool_debt_complete",
    }
}

/// Keep assistant prose in the tool-lane transcript before another model round.
pub fn append_assistant_draft_to_tool_lane(messages: &mut Vec<ChatMessage>, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    messages.push(ChatMessage::assistant(trimmed.to_string()));
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
        }
    }

    fn after_tools<'a>(
        user_prompt: &'a str,
        draft: &str,
        invocations: &'a [ToolInvocation],
    ) -> AfterToolsRoundContext<'a> {
        AfterToolsRoundContext {
            user_prompt,
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 3,
            max_tool_rounds: 10,
            invocations,
            workshop_lane: false,
            open_gaps: &[],
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
    fn interim_before_tools_ends_without_loop() {
        let action = decide_no_tool_debt_text_round(&ctx("Let me check that for you."));
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
    fn clarifying_question_ends_without_tools() {
        let action = decide_no_tool_debt_text_round(&ctx(
            "Which session should I calibrate — the default or medousa-home?",
        ));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "clarifying_question"
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
    fn max_rounds_fuse_ends() {
        let mut round = ctx("Still working…");
        round.rounds_executed = 10;
        round.max_tool_rounds = 10;
        let action = decide_no_tool_debt_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "max_rounds_fuse"
            }
        ));
    }

    #[test]
    fn append_assistant_draft_skips_empty() {
        let mut messages = Vec::new();
        append_assistant_draft_to_tool_lane(&mut messages, "   ");
        assert!(messages.is_empty());
        append_assistant_draft_to_tool_lane(&mut messages, "Hello");
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn continue_control_message_matches_reason() {
        let msg = continue_control_message(
            ContinueReason::MissingReceipts,
            &["cognition_memory_calibrate".to_string()],
        );
        assert!(msg.contains("cognition_memory_calibrate"));
        assert!(msg.contains("ritual receipts"));
    }

    #[test]
    fn substantive_answer_after_tools_ends() {
        let invocations = vec![tool("cognition_memory_moods"), tool("cognition_memory_calibrate")];
        let answer = "Your memory profile shows stability at 0.95 and three recent nodes about \
                      the ingester roadmap. I stored the update in Locus.";
        let action = decide_after_tools_text_round(&after_tools(
            "pull focused AVEC and calibrate",
            answer,
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "tool_debt_complete"
            }
        ));
    }

    #[test]
    fn missing_calibrate_receipt_continues() {
        let invocations = vec![tool("cognition_memory_moods")];
        let action = decide_after_tools_text_round(&after_tools(
            "pull focused AVEC and calibrate",
            "Focused preset pulled. Stability 0.95.",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::MissingReceipts,
                ..
            }
        ));
    }

    #[test]
    fn short_ack_after_tools_ends() {
        let invocations = vec![tool("cognition_memory_moods"), tool("cognition_memory_calibrate")];
        let action = decide_after_tools_text_round(&after_tools(
            "pull focused AVEC",
            "Stored.",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "tool_debt_complete"
            }
        ));
    }

    #[test]
    fn prepare_final_with_interim_after_tools_ends() {
        let invocations = vec![tool("cognition_turn_prepare_final")];
        let mut round = after_tools("summarize findings", "Stored.", &invocations);
        round.pending_final_answer = true;
        let action = decide_after_tools_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "prepare_final_then_text"
            }
        ));
    }

    #[test]
    fn workshop_lane_prepare_final_ends_despite_interim() {
        let invocations = vec![tool("cognition_turn_prepare_final")];
        let mut round = after_tools(
            "WORKER_TASK",
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
    fn plan_to_spawn_after_discovery_continues_host_loop() {
        let invocations = vec![
            tool("cognition_manuscript_list"),
            tool("cognition_manuscript_resolve"),
            tool("cognition_capability_search"),
        ];
        let draft = "Perfect — base-researcher is resolved and web_research is available. \
                     I'll spin up five research workers next.";
        let action = decide_after_tools_text_round(&AfterToolsRoundContext {
            user_prompt: "research these topics then spin up workers",
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 3,
            max_tool_rounds: 10,
            invocations: &invocations,
            workshop_lane: false,
            open_gaps: &[],
        });
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PendingDelegation,
                ..
            }
        ));
    }

    #[test]
    fn follow_up_spin_them_up_continues_without_spawn() {
        let invocations = vec![
            tool("cognition_manuscript_list"),
            tool("cognition_manuscript_resolve"),
        ];
        let action = decide_after_tools_text_round(&AfterToolsRoundContext {
            user_prompt: "perfect!! spin them up and lets see what we can get!!",
            draft_text: "On it — I'll delegate to the workshop now.".to_string(),
            pending_final_answer: false,
            rounds_executed: 4,
            max_tool_rounds: 10,
            invocations: &invocations,
            workshop_lane: false,
            open_gaps: &[],
        });
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PendingDelegation,
                ..
            }
        ));
    }

    #[test]
    fn spawn_receipt_allows_turn_to_end() {
        let invocations = vec![
            tool("cognition_manuscript_resolve"),
            tool("cognition_spawn_turn_worker"),
        ];
        let action = decide_after_tools_text_round(&AfterToolsRoundContext {
            user_prompt: "spin them up",
            draft_text: "Working on that in the background.".to_string(),
            pending_final_answer: false,
            rounds_executed: 4,
            max_tool_rounds: 10,
            invocations: &invocations,
            workshop_lane: false,
            open_gaps: &[],
        });
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "tool_debt_complete"
            }
        ));
    }
}
