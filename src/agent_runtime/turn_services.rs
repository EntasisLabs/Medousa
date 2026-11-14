use std::sync::Arc;

use genai::chat::ChatMessage;
use medousa_runtime::MedousaToolLoopPipeline;
use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use stasis::application::orchestration::tool_loop_pipeline::ToolCallMode;
use stasis::ports::outbound::ai_chat_client::AiChatClient;

use super::context_assembly::contribution;
use super::context_assembly::{ChatContext, ToolContext};
use crate::learning_artifacts::{
    DEFAULT_LEARNING_RECALL_BLOCK_CHARS, DEFAULT_SCRIPT_RECALL_BLOCK_CHARS,
};
use crate::runtime::settings::RuntimeSettings;
use crate::session::ConversationTurn;
use crate::stage_routing::StageRoute;
use crate::tool_bootstrap::DEFAULT_TOOL_HINTS_BLOCK_CHARS;
use crate::tools::TuiRuntime;
use crate::turn_slice::{DEFAULT_SLICE_BLOCK_CHARS, format_cold_history_line, prior_turn_content};
use medousa_runtime::context::{ContextCoordinator, ContextCost, ContextOwner, Retention};

pub(crate) fn requested_tool_allowlist(
    tools: Option<&[String]>,
) -> Option<std::collections::HashSet<String>> {
    // Preserve explicit emptiness: None inherits normal selection; Some(empty)
    // is a result-only ceiling, including after whitespace normalization.
    tools.map(|tools| {
        tools
            .iter()
            .map(|tool| tool.trim().to_string())
            .filter(|tool| !tool.is_empty())
            .collect()
    })
}

#[derive(Debug, Clone)]
pub struct TurnActivationDecision {
    pub turn_class: &'static str,
    pub tool_call_mode: ToolCallMode,
    pub max_tool_rounds: usize,
    pub enforce_no_tools: bool,
    pub reason: &'static str,
}

#[derive(Debug, Clone)]
pub struct PriorMessageBuild {
    pub messages: Vec<ChatMessage>,
    pub hot_turns_included: usize,
    pub cold_turns_summarized: usize,
    pub cold_summary_chars: usize,
    pub tool_slices_chars: usize,
    pub script_recall_chars: usize,
    pub learning_recall_chars: usize,
    pub tool_hints_chars: usize,
    pub total_chars: usize,
    /// Actual included spans, distinct from legacy selection counters.
    pub context: ContextCoordinator,
}

#[derive(Debug, Clone, Copy)]
pub struct PriorMessageLimits {
    pub max_prior_total_chars: usize,
    pub max_single_prior_message_chars: usize,
    pub hot_window_char_budget: usize,
    pub cold_window_char_budget: usize,
    pub cold_summary_line_chars: usize,
    pub include_auxiliary_history: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct IntentContextLimits {
    pub context_line_chars: usize,
}

pub struct SelectedTurnPipeline {
    pub pipeline: MedousaToolLoopPipeline,
    pub route_dispatch_notice: Option<String>,
}

pub fn select_pipeline_for_turn(
    tui_rt: &TuiRuntime,
    final_route: Option<&StageRoute>,
    settings: &RuntimeSettings,
) -> SelectedTurnPipeline {
    select_pipeline_for_turn_with_allowlist(tui_rt, final_route, settings, None)
}

pub fn select_pipeline_for_turn_with_allowlist(
    tui_rt: &TuiRuntime,
    final_route: Option<&StageRoute>,
    settings: &RuntimeSettings,
    tool_allowlist: Option<std::collections::HashSet<String>>,
) -> SelectedTurnPipeline {
    select_pipeline_for_turn_with_registry_and_allowlist(
        tui_rt.tool_registry.clone(),
        final_route,
        settings,
        tool_allowlist,
    )
}

pub fn select_pipeline_for_turn_with_registry_and_allowlist(
    tool_registry: Arc<dyn stasis::application::orchestration::tool_registry::ToolRegistry>,
    final_route: Option<&StageRoute>,
    settings: &RuntimeSettings,
    tool_allowlist: Option<std::collections::HashSet<String>>,
) -> SelectedTurnPipeline {
    use std::sync::Arc;

    use stasis::application::orchestration::tool_registry::ToolRegistry;

    use crate::agent_runtime::turn_worker::AllowlistToolRegistry;
    use crate::runtime::agent_services::build_tool_loop_pipeline_for_target;

    let registry: Arc<dyn ToolRegistry> = match tool_allowlist {
        // Explicitly empty means result-only, not an implicit public API grant.
        Some(allowlist) if allowlist.is_empty() => Arc::new(AllowlistToolRegistry::new_exact(
            tool_registry.clone(),
            allowlist,
        )),
        Some(allowlist) => Arc::new(AllowlistToolRegistry::new(tool_registry.clone(), allowlist)),
        None => tool_registry,
    };

    if let Some(route) = final_route {
        let route_base_url = route_base_url(route, settings);
        let route_notice = format!(
            "◈ stage route dispatch final_response target={}:{} base_url={}",
            route.provider,
            route.model,
            route_base_url
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("(auto)"),
        );
        let pipeline = build_tool_loop_pipeline_for_target(
            &route.provider,
            &route.model,
            route_base_url.as_deref(),
            registry,
        );
        return SelectedTurnPipeline {
            pipeline,
            route_dispatch_notice: Some(route_notice),
        };
    }

    let pipeline = build_tool_loop_pipeline_for_target(
        &settings.provider,
        &settings.model,
        (!settings.base_url.trim().is_empty()).then_some(settings.base_url.as_str()),
        registry,
    );
    SelectedTurnPipeline {
        pipeline,
        route_dispatch_notice: None,
    }
}

pub fn parse_tool_call_mode(value: &str) -> ToolCallMode {
    if value.trim().eq_ignore_ascii_case("strict") {
        ToolCallMode::Strict
    } else {
        ToolCallMode::Auto
    }
}

/// In-memory renderers have no retrieval failure; native recall producers pass
/// their observed read outcomes through record_auxiliary_outcome below.
fn record_auxiliary_omission(
    context: &mut ContextCoordinator,
    session_id: &str,
    id: &str,
    rendered: Option<&str>,
    budget: usize,
) {
    use medousa_runtime::context::{ContextDecision, ContextOmissionReason};
    if rendered.is_some_and(|text| !text.trim().is_empty()) {
        return;
    }
    let reason = if budget == 0 {
        ContextOmissionReason::BudgetExcluded
    } else if rendered.is_none() {
        ContextOmissionReason::Unavailable
    } else {
        ContextOmissionReason::NoResult
    };
    let mut item = contribution(
        id,
        ContextOwner::Tool,
        session_id,
        ContextCost::default(),
        Retention::Flexible,
        Some(budget),
    );
    item.decision = ContextDecision::Omitted {
        reason,
        source_text_chars: None,
    };
    context.include(item);
}

/// Keep successfully rendered spans even on partial retrieval failure. Record
/// the unmeasured missing portion separately, never charge invented lengths or
/// relabel successful empty reads as unavailable. Budget precedence is unchanged.
fn record_auxiliary_outcome(
    context: &mut ContextCoordinator,
    session_id: &str,
    id: &str,
    outcome: &crate::learning_artifacts::RecallBlock,
    budget: usize,
) {
    let has_text = !outcome.text.trim().is_empty();
    record_auxiliary_omission(
        context,
        session_id,
        id,
        (outcome.unavailable_reads == 0 || has_text).then_some(outcome.text.as_str()),
        budget,
    );
    if outcome.unavailable_reads > 0 && has_text {
        record_auxiliary_omission(
            context,
            session_id,
            &format!("{id}_unavailable_reads"),
            None,
            budget,
        );
    }
}

#[allow(clippy::too_many_arguments)] // prior-message windows travel together
pub fn build_prior_messages(
    tool_catalog: &crate::typed_tools::ToolCatalog,
    session_id: &str,
    turns: &[ConversationTurn],
    current_prompt: &str,
    current_user_persisted: bool,
    hot_window_turns: usize,
    cold_window_turns: usize,
    limits: PriorMessageLimits,
) -> PriorMessageBuild {
    let selected = ChatContext::prior_turns(turns, current_user_persisted);
    let tool_context = ToolContext {
        catalog: tool_catalog,
        session_id,
        prompt: current_prompt,
        turns,
    };
    let mut context = ContextCoordinator::with_legacy_history_limit(limits.max_prior_total_chars);
    let mut accepted: Vec<ChatMessage> = Vec::new();
    let mut total_chars = 0usize;

    let hot_turns: Vec<&ConversationTurn> = selected
        .iter()
        .rev()
        .take(hot_window_turns)
        .copied()
        .collect();
    let cold_turns: Vec<&ConversationTurn> = selected
        .iter()
        .rev()
        .skip(hot_window_turns)
        .take(cold_window_turns)
        .copied()
        .collect();

    let mut hot_remaining = context.history_allocation(limits.hot_window_char_budget);
    for turn in &hot_turns {
        let content = prior_turn_content(turn, limits.max_single_prior_message_chars);
        if hot_remaining == 0 {
            context.include(
                contribution(
                    "prior_conversation",
                    ContextOwner::Chat,
                    session_id,
                    ContextCost::text(content.chars().count()),
                    Retention::Flexible,
                    Some(0),
                )
                .project_text(0),
            );
            continue;
        }
        let bounded = truncate_text_for_budget(&content, hot_remaining);
        if bounded.trim().is_empty() {
            context.include(
                contribution(
                    "prior_conversation",
                    ContextOwner::Chat,
                    session_id,
                    ContextCost::text(content.chars().count()),
                    Retention::Flexible,
                    Some(hot_remaining),
                )
                .project_text(0),
            );
            continue;
        }

        let bounded_chars = bounded.chars().count();
        hot_remaining = hot_remaining.saturating_sub(bounded_chars);
        total_chars = total_chars.saturating_add(bounded_chars);
        context.account_history_selection(bounded_chars);
        match turn.role.as_str() {
            "user" => accepted.push(ChatMessage::user(bounded)),
            "assistant" | "agent" => accepted.push(ChatMessage::assistant(bounded)),
            _ => {
                context.include(
                    contribution(
                        "unsupported_history_role",
                        ContextOwner::Chat,
                        session_id,
                        ContextCost::text(content.chars().count()),
                        Retention::Flexible,
                        Some(
                            limits
                                .hot_window_char_budget
                                .min(limits.max_prior_total_chars),
                        ),
                    )
                    .project_text(0),
                );
                continue;
            }
        }
        context.include(
            contribution(
                "prior_conversation",
                ContextOwner::Chat,
                session_id,
                ContextCost::text(content.chars().count()),
                Retention::Flexible,
                Some(
                    limits
                        .hot_window_char_budget
                        .min(limits.max_prior_total_chars),
                ),
            )
            .project_text(bounded_chars),
        );
    }

    let cold_lines = cold_turns
        .iter()
        .rev()
        .filter_map(|turn| {
            format_cold_history_line(turn, limits.cold_summary_line_chars).or_else(|| {
                match turn.role.as_str() {
                    "user" | "assistant" | "agent" => {
                        let line =
                            truncate_text_for_budget(&turn.content, limits.cold_summary_line_chars);
                        if line.trim().is_empty() {
                            None
                        } else {
                            let role = if turn.role == "agent" {
                                "assistant"
                            } else {
                                turn.role.as_str()
                            };
                            Some(format!("{}: {}", role, line.replace('\n', " ")))
                        }
                    }
                    _ => None,
                }
            })
        })
        .collect::<Vec<_>>();

    let cold_source = if cold_lines.is_empty() {
        String::new()
    } else {
        format!("[MEDOUSA_COLD_HISTORY_SUMMARY]\n{}", cold_lines.join("\n"))
    };
    let cold_budget = context.history_allocation(limits.cold_window_char_budget);
    let cold_summary = truncate_text_for_budget(&cold_source, cold_budget);
    let cold_summary_chars = cold_summary.chars().count();
    if !cold_summary.trim().is_empty() {
        total_chars = total_chars.saturating_add(cold_summary_chars);
        context.account_history_selection(cold_summary_chars);
        accepted.push(ChatMessage::assistant(cold_summary));
    }
    if !cold_source.is_empty() {
        context.include(
            contribution(
                "cold_history",
                ContextOwner::Chat,
                session_id,
                ContextCost::text(cold_source.chars().count()),
                Retention::Flexible,
                Some(cold_budget),
            )
            .project_text(cold_summary_chars),
        );
    }

    if cold_source.is_empty() {
        for turn in &cold_turns {
            let mut item = contribution(
                "cold_history_candidate",
                ContextOwner::Chat,
                session_id,
                ContextCost::default(),
                Retention::Flexible,
                Some(limits.cold_summary_line_chars),
            );
            item.decision = medousa_runtime::context::ContextDecision::Omitted {
                reason: if limits.cold_summary_line_chars == 0 {
                    medousa_runtime::context::ContextOmissionReason::BudgetExcluded
                } else {
                    medousa_runtime::context::ContextOmissionReason::NoResult
                },
                source_text_chars: Some(turn.content.chars().count()),
            };
            context.include(item);
        }
    }

    let (tool_slices_chars, script_recall_chars, learning_recall_chars, tool_hints_chars) =
        if limits.include_auxiliary_history {
            let slice_budget = context
                .history_allocation(limits.hot_window_char_budget.min(DEFAULT_SLICE_BLOCK_CHARS));
            let tool_slices_block = tool_context.slices(&hot_turns, slice_budget);
            let tool_slices_chars = tool_slices_block.chars().count();
            record_auxiliary_omission(
                &mut context,
                session_id,
                "tool_slices",
                Some(&tool_slices_block),
                slice_budget,
            );
            if !tool_slices_block.trim().is_empty() {
                total_chars = total_chars.saturating_add(tool_slices_chars);
                context.account_history_selection(tool_slices_chars);
                accepted.push(ChatMessage::assistant(tool_slices_block));
                context.include(contribution(
                    "tool_slices",
                    ContextOwner::Tool,
                    session_id,
                    ContextCost::text(tool_slices_chars),
                    Retention::Flexible,
                    Some(slice_budget),
                ));
            }

            let script_budget = context.history_allocation(
                limits
                    .hot_window_char_budget
                    .min(DEFAULT_SCRIPT_RECALL_BLOCK_CHARS),
            );
            let scripts = tool_context.scripts(script_budget);
            record_auxiliary_outcome(
                &mut context,
                session_id,
                "grapheme_scripts",
                &scripts,
                script_budget,
            );
            let script_recall_block = scripts.text;
            let script_recall_chars = script_recall_block.chars().count();
            if !script_recall_block.trim().is_empty() {
                total_chars = total_chars.saturating_add(script_recall_chars);
                context.account_history_selection(script_recall_chars);
                accepted.push(ChatMessage::assistant(script_recall_block));
                context.include(contribution(
                    "grapheme_scripts",
                    ContextOwner::Tool,
                    session_id,
                    ContextCost::text(script_recall_chars),
                    Retention::Flexible,
                    Some(script_budget),
                ));
            }

            let learning_budget = context.history_allocation(
                limits
                    .hot_window_char_budget
                    .min(DEFAULT_LEARNING_RECALL_BLOCK_CHARS),
            );
            let learnings = tool_context.learnings(learning_budget);
            record_auxiliary_outcome(
                &mut context,
                session_id,
                "runtime_learnings",
                &learnings,
                learning_budget,
            );
            let learning_recall_block = learnings.text;
            let learning_recall_chars = learning_recall_block.chars().count();
            if !learning_recall_block.trim().is_empty() {
                total_chars = total_chars.saturating_add(learning_recall_chars);
                context.account_history_selection(learning_recall_chars);
                accepted.push(ChatMessage::assistant(learning_recall_block));
                context.include(contribution(
                    "runtime_learnings",
                    ContextOwner::Tool,
                    session_id,
                    ContextCost::text(learning_recall_chars),
                    Retention::Flexible,
                    Some(learning_budget),
                ));
            }

            let hints_budget = context.history_allocation(
                limits
                    .hot_window_char_budget
                    .min(DEFAULT_TOOL_HINTS_BLOCK_CHARS),
            );
            let tool_hints_block = tool_context.hints(hints_budget);
            let tool_hints_chars = tool_hints_block.chars().count();
            record_auxiliary_omission(
                &mut context,
                session_id,
                "tool_hints",
                Some(&tool_hints_block),
                hints_budget,
            );
            if !tool_hints_block.trim().is_empty() {
                total_chars = total_chars.saturating_add(tool_hints_chars);
                context.account_history_selection(tool_hints_chars);
                accepted.push(ChatMessage::assistant(tool_hints_block));
                context.include(contribution(
                    "tool_hints",
                    ContextOwner::Tool,
                    session_id,
                    ContextCost::text(tool_hints_chars),
                    Retention::Flexible,
                    Some(hints_budget),
                ));
            }
            (
                tool_slices_chars,
                script_recall_chars,
                learning_recall_chars,
                tool_hints_chars,
            )
        } else {
            for id in [
                "tool_slices",
                "grapheme_scripts",
                "runtime_learnings",
                "tool_hints",
            ] {
                let mut item = contribution(
                    id,
                    ContextOwner::Tool,
                    session_id,
                    ContextCost::default(),
                    Retention::Flexible,
                    None,
                );
                item.decision = medousa_runtime::context::ContextDecision::Omitted {
                    reason: medousa_runtime::context::ContextOmissionReason::Disabled,
                    source_text_chars: None,
                };
                context.include(item);
            }
            (0, 0, 0, 0)
        };

    accepted.reverse();
    PriorMessageBuild {
        messages: accepted,
        hot_turns_included: selected.len().min(hot_window_turns),
        cold_turns_summarized: selected
            .len()
            .saturating_sub(hot_window_turns)
            .min(cold_window_turns),
        cold_summary_chars,
        tool_slices_chars,
        script_recall_chars,
        learning_recall_chars,
        tool_hints_chars,
        total_chars,
        context,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn decide_turn_activation(
    prompt: &str,
    configured_mode: ToolCallMode,
    configured_rounds: usize,
    tool_intent_max_rounds: usize,
    short_turn_max_rounds: usize,
    turn_count: usize,
    direct_answer_max_prompt_chars: usize,
    long_session_turn_threshold: usize,
    long_session_max_prompt_chars: usize,
) -> TurnActivationDecision {
    let prompt_trimmed = prompt.trim();
    let prompt_lower = prompt_trimmed.to_ascii_lowercase();
    let prompt_chars = prompt_trimmed.chars().count();

    let tool_intent = contains_tool_intent(&prompt_lower);
    let direct_answer_intent = contains_direct_answer_intent(&prompt_lower);

    let tool_intent_cap = tool_intent_max_rounds.max(2);
    let short_cap = short_turn_max_rounds.max(1);

    if tool_intent {
        return TurnActivationDecision {
            turn_class: "c",
            tool_call_mode: ToolCallMode::Auto,
            max_tool_rounds: configured_rounds.min(tool_intent_cap).max(2),
            enforce_no_tools: false,
            reason: "tool_intent_detected",
        };
    }

    if direct_answer_intent && prompt_chars < direct_answer_max_prompt_chars {
        return TurnActivationDecision {
            turn_class: "a",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: short_cap,
            enforce_no_tools: true,
            reason: "direct_answer_short_prompt",
        };
    }

    if turn_count > long_session_turn_threshold && prompt_chars < long_session_max_prompt_chars {
        return TurnActivationDecision {
            turn_class: "b",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: short_cap,
            enforce_no_tools: true,
            reason: "long_session_short_turn",
        };
    }

    TurnActivationDecision {
        turn_class: "b",
        tool_call_mode: configured_mode,
        max_tool_rounds: configured_rounds,
        enforce_no_tools: false,
        reason: "configured_default",
    }
}

pub fn apply_context_compiler_activation_gate(
    base: TurnActivationDecision,
    allow_no_tools_fallback: bool,
) -> TurnActivationDecision {
    if base.enforce_no_tools && !allow_no_tools_fallback {
        return TurnActivationDecision {
            turn_class: "b",
            tool_call_mode: ToolCallMode::Auto,
            max_tool_rounds: base.max_tool_rounds.max(2),
            enforce_no_tools: false,
            reason: "cheap_recall_first_no_verified_context",
        };
    }

    base
}

pub fn build_prompt_pipeline_for_turn(
    final_route: Option<&StageRoute>,
    settings: &RuntimeSettings,
) -> PromptExecutionPipeline {
    let (provider, model, base_url) = match final_route {
        Some(route) => (
            route.provider.clone(),
            route.model.clone(),
            route_base_url(route, settings),
        ),
        None => {
            let base = settings.base_url.trim();
            (
                settings.provider.clone(),
                settings.model.clone(),
                if base.is_empty() {
                    None
                } else {
                    Some(base.to_string())
                },
            )
        }
    };
    build_prompt_pipeline_for_target(&provider, &model, base_url.as_deref())
}

pub fn build_prompt_pipeline_for_target(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
) -> PromptExecutionPipeline {
    let chat_client: Arc<dyn AiChatClient> =
        Arc::new(crate::build_genai_chat_client(provider, model, base_url));
    PromptExecutionPipeline::new(chat_client)
}

pub fn build_intent_classifier_recent_context(
    turns: &[ConversationTurn],
    _current_prompt: &str,
    current_user_persisted: bool,
    max_turns: usize,
    max_chars: usize,
    limits: IntentContextLimits,
) -> String {
    let mut selected: Vec<&ConversationTurn> = turns
        .iter()
        .filter(|turn| !crate::turn_failure::is_error_turn_excluded_from_model_context(turn))
        .collect();

    if current_user_persisted
        && let Some(last) = selected.last()
        && last.role == "user"
    {
        selected.pop();
    }

    let mut lines = Vec::new();
    let mut total_chars = 0usize;
    for turn in selected.iter().rev().take(max_turns).rev() {
        let role = match turn.role.as_str() {
            "user" => "user",
            "assistant" | "agent" => "assistant",
            _ => continue,
        };

        let text =
            truncate_text_for_budget(&turn.content, limits.context_line_chars).replace('\n', " ");
        if text.trim().is_empty() {
            continue;
        }

        let line = format!("{}: {}", role, text);
        let line_chars = line.chars().count();
        if total_chars.saturating_add(line_chars) > max_chars {
            break;
        }
        total_chars = total_chars.saturating_add(line_chars);
        lines.push(line);
    }

    lines.join("\n")
}

fn contains_tool_intent(prompt_lower: &str) -> bool {
    [
        "search", "look up", "lookup", "run ", "execute", "query", "fetch", "verify", "evidence",
        "grapheme", "tool", "call", "api", "latest",
        // memory / AVEC / posture (avoid long-session no-tools misclassification)
        "calibrat", "avec", "memory", "mood", "locus", "recall", "context", "schema", "store",
        "pull", "focus",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
}

fn contains_direct_answer_intent(prompt_lower: &str) -> bool {
    [
        "explain",
        "summarize",
        "rephrase",
        "clarify",
        "what does",
        "how does",
        "why",
        "help me understand",
        "give me",
        "draft",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
}

fn route_base_url(route: &StageRoute, settings: &RuntimeSettings) -> Option<String> {
    if route
        .provider
        .eq_ignore_ascii_case(settings.provider.trim())
    {
        let candidate = settings.base_url.trim();
        if !candidate.is_empty() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn truncate_text_for_budget(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }

    if max_chars <= 12 {
        return text.chars().take(max_chars).collect();
    }

    let head = max_chars / 2;
    let tail = max_chars.saturating_sub(head + 5);
    let head_part = text.chars().take(head).collect::<String>();
    let tail_part = text
        .chars()
        .skip(total_chars.saturating_sub(tail))
        .collect::<String>();
    format!("{head_part}\n...\n{tail_part}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn explicit_empty_tool_ceiling_is_not_dropped() {
        assert!(super::requested_tool_allowlist(None).is_none());
        assert_eq!(
            super::requested_tool_allowlist(Some(&[])),
            Some(std::collections::HashSet::new())
        );
        assert_eq!(
            super::requested_tool_allowlist(Some(&["   ".into()])),
            Some(std::collections::HashSet::new())
        );
        assert_eq!(
            super::requested_tool_allowlist(Some(&[" cognition_web_search ".into()])),
            Some(std::collections::HashSet::from([
                "cognition_web_search".into()
            ]))
        );
    }
    use chrono::Utc;
    use stasis::application::orchestration::tool_loop_pipeline::ToolCallMode;

    use super::{
        PriorMessageLimits, apply_context_compiler_activation_gate,
        build_intent_classifier_recent_context, build_prior_messages, decide_turn_activation,
        parse_tool_call_mode,
    };
    use crate::session::ConversationTurn;

    fn sample_limits() -> PriorMessageLimits {
        PriorMessageLimits {
            max_prior_total_chars: 24_000,
            max_single_prior_message_chars: 4_000,
            hot_window_char_budget: 14_000,
            cold_window_char_budget: 8_000,
            cold_summary_line_chars: 240,
            include_auxiliary_history: true,
        }
    }

    #[test]
    fn parse_tool_call_mode_respects_strict() {
        assert!(matches!(
            parse_tool_call_mode("strict"),
            ToolCallMode::Strict
        ));
        assert!(matches!(parse_tool_call_mode("auto"), ToolCallMode::Auto));
    }

    #[test]
    fn activation_policy_prefers_no_tools_for_short_explanations() {
        let policy = decide_turn_activation(
            "Explain what this config means",
            ToolCallMode::Auto,
            10,
            12,
            1,
            4,
            320,
            28,
            420,
        );
        assert!(policy.enforce_no_tools);
        assert_eq!(policy.max_tool_rounds, 1);
    }

    #[test]
    fn activation_policy_prefers_tools_for_lookup_intent() {
        let policy = decide_turn_activation(
            "Search latest runtime failures and verify evidence",
            ToolCallMode::Strict,
            30,
            12,
            1,
            8,
            320,
            28,
            420,
        );
        assert!(!policy.enforce_no_tools);
        assert_eq!(policy.tool_call_mode, ToolCallMode::Auto);
    }

    #[test]
    fn activation_policy_prefers_tools_for_avec_calibrate_intent() {
        let policy = decide_turn_activation(
            "yoo can you pull a focused AVEC and calibrate to it?",
            ToolCallMode::Auto,
            10,
            12,
            1,
            40,
            320,
            28,
            420,
        );
        assert!(!policy.enforce_no_tools);
        assert_eq!(policy.reason, "tool_intent_detected");
        assert!(policy.max_tool_rounds >= 2);
    }

    #[test]
    fn activation_gate_blocks_no_tools_when_recall_not_verified() {
        let base = decide_turn_activation(
            "Explain what this config means",
            ToolCallMode::Auto,
            10,
            12,
            1,
            4,
            320,
            28,
            420,
        );
        assert!(base.enforce_no_tools);

        let gated = apply_context_compiler_activation_gate(base, false);
        assert!(!gated.enforce_no_tools);
        assert_eq!(gated.tool_call_mode, ToolCallMode::Auto);
        assert_eq!(gated.reason, "cheap_recall_first_no_verified_context");
    }

    #[test]
    fn baseline_history_assembly_parity_matrix() {
        // Frozen phase-1 reference: the 4def4274 text-selection algorithm.
        // Domain rendering helpers are intentionally unchanged. Auxiliary pool
        // formula parity is separately verified in the portable coordinator.
        fn baseline(
            turns: &[ConversationTurn],
            persisted: bool,
            hot: usize,
            cold: usize,
            limits: PriorMessageLimits,
        ) -> (Vec<genai::chat::ChatMessage>, usize) {
            let mut selected = turns
                .iter()
                .filter(|turn| {
                    !crate::turn_failure::is_error_turn_excluded_from_model_context(turn)
                })
                .collect::<Vec<_>>();
            if persisted && selected.last().is_some_and(|turn| turn.role == "user") {
                selected.pop();
            }
            let hot_turns = selected.iter().rev().take(hot).copied().collect::<Vec<_>>();
            let cold_turns = selected
                .iter()
                .rev()
                .skip(hot)
                .take(cold)
                .copied()
                .collect::<Vec<_>>();
            let mut remaining = limits
                .hot_window_char_budget
                .min(limits.max_prior_total_chars);
            let mut used = 0usize;
            let mut messages = Vec::new();
            for turn in &hot_turns {
                if remaining == 0 {
                    break;
                }
                let content =
                    super::prior_turn_content(turn, limits.max_single_prior_message_chars);
                let bounded = super::truncate_text_for_budget(&content, remaining);
                if bounded.trim().is_empty() {
                    continue;
                }
                let chars = bounded.chars().count();
                remaining = remaining.saturating_sub(chars);
                used = used.saturating_add(chars);
                match turn.role.as_str() {
                    "user" => messages.push(genai::chat::ChatMessage::user(bounded)),
                    "agent" | "assistant" => {
                        messages.push(genai::chat::ChatMessage::assistant(bounded))
                    }
                    _ => {}
                }
            }
            let lines = cold_turns
                .iter()
                .rev()
                .filter_map(|turn| {
                    super::format_cold_history_line(turn, limits.cold_summary_line_chars).or_else(
                        || match turn.role.as_str() {
                            "user" | "assistant" | "agent" => {
                                let line = super::truncate_text_for_budget(
                                    &turn.content,
                                    limits.cold_summary_line_chars,
                                );
                                if line.trim().is_empty() {
                                    None
                                } else {
                                    let role = if turn.role == "agent" {
                                        "assistant"
                                    } else {
                                        turn.role.as_str()
                                    };
                                    Some(format!("{}: {}", role, line.replace('\n', " ")))
                                }
                            }
                            _ => None,
                        },
                    )
                })
                .collect::<Vec<_>>();
            if !lines.is_empty() {
                let summary = super::truncate_text_for_budget(
                    &format!("[MEDOUSA_COLD_HISTORY_SUMMARY]\n{}", lines.join("\n")),
                    limits
                        .cold_window_char_budget
                        .min(limits.max_prior_total_chars.saturating_sub(used)),
                );
                if !summary.trim().is_empty() {
                    used = used.saturating_add(summary.chars().count());
                    messages.push(genai::chat::ChatMessage::assistant(summary));
                }
            }
            messages.reverse();
            (messages, used)
        }
        let turns = ["user", "agent", "system", "assistant", "user"]
            .into_iter()
            .enumerate()
            .map(|(index, role)| ConversationTurn {
                role: role.into(),
                content: format!("turn-{index} 🦀 {}", "x".repeat(32)),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            })
            .collect::<Vec<_>>();
        let catalog = crate::typed_tools::ToolCatalog::default();
        for cap in [0, 1, 5, 12, 13, 64, 240] {
            for single in [0, 12, 40] {
                for hot in [0, 1, 3, 5] {
                    for cold in [0, 2, 5] {
                        for persisted in [false, true] {
                            let limits = PriorMessageLimits {
                                max_prior_total_chars: cap,
                                max_single_prior_message_chars: single,
                                hot_window_char_budget: 96,
                                cold_window_char_budget: 96,
                                cold_summary_line_chars: 24,
                                include_auxiliary_history: false,
                            };
                            let expected = baseline(&turns, persisted, hot, cold, limits);
                            let actual = build_prior_messages(
                                &catalog,
                                "parity-session",
                                &turns,
                                "current",
                                persisted,
                                hot,
                                cold,
                                limits,
                            );
                            assert_eq!(
                                serde_json::to_value(&actual.messages).unwrap(),
                                serde_json::to_value(expected.0).unwrap(),
                                "cap={cap} single={single} hot={hot} cold={cold} persisted={persisted}"
                            );
                            assert_eq!(actual.total_chars, expected.1);
                            let cost =
                                medousa_runtime::context::ContextCost::messages(&actual.messages);
                            assert_eq!(
                                actual
                                    .context
                                    .report(medousa_runtime::context::ContextCapacity::default())
                                    .cost,
                                cost
                            );
                        }
                    }
                }
            }
        }
    }

    fn correction_turn(role: &str, content: &str) -> ConversationTurn {
        ConversationTurn {
            role: role.into(),
            content: content.into(),
            timestamp: Utc::now(),
            tool_names: Vec::new(),
            answer_state: None,
            parts: None,
            slice_summary: None,
            speaker_profile_id: None,
            reactions: Vec::new(),
        }
    }

    #[test]
    fn known_history_candidates_are_recorded_after_zero_or_exhausted_budget() {
        use medousa_runtime::context::{ContextCost, ContextDecision};
        let turns = vec![
            correction_turn("user", "old user"),
            correction_turn("assistant", "old answer"),
            correction_turn("user", "recent user"),
            correction_turn("assistant", "recent answer"),
        ];
        for cap in [0, 1] {
            let mut limits = sample_limits();
            limits.max_prior_total_chars = cap;
            limits.include_auxiliary_history = false;
            let built = build_prior_messages(
                &crate::typed_tools::ToolCatalog::default(),
                "excluded",
                &turns,
                "current",
                false,
                2,
                2,
                limits,
            );
            let hot: Vec<_> = built
                .context
                .contributions
                .iter()
                .filter(|item| item.id == "prior_conversation")
                .collect();
            assert_eq!(hot.len(), 2);
            assert!(hot.iter().filter(|item| matches!(item.decision, ContextDecision::Excluded { source_text_chars } if source_text_chars > 0))
                .count() >= if cap == 0 { 2 } else { 1 });
            let cold = built
                .context
                .contributions
                .iter()
                .find(|item| item.id == "cold_history")
                .unwrap();
            assert!(
                matches!(cold.decision, ContextDecision::Excluded { source_text_chars } if source_text_chars > 0)
            );
            assert_eq!(cold.cost, ContextCost::default());
            assert_eq!(
                built.context.report(Default::default()).cost,
                ContextCost::messages(&built.messages)
            );
            if cap == 0 {
                assert!(built.messages.is_empty());
                assert_eq!(built.total_chars, 0);
            } else {
                assert_eq!(built.messages.len(), 1);
                assert_eq!(built.total_chars, 1);
            }
        }
    }

    #[test]
    fn cold_candidates_survive_zero_line_rendering_budget() {
        use medousa_runtime::context::{ContextCost, ContextDecision, ContextOmissionReason};
        let turns = vec![
            correction_turn("user", "older question"),
            correction_turn("assistant", "older answer"),
        ];
        let mut limits = sample_limits();
        limits.cold_summary_line_chars = 0;
        limits.include_auxiliary_history = false;
        let built = build_prior_messages(
            &crate::typed_tools::ToolCatalog::default(),
            "cold-zero-lines",
            &turns,
            "current",
            false,
            0,
            2,
            limits,
        );
        assert!(built.messages.is_empty());
        let candidates: Vec<_> = built
            .context
            .contributions
            .iter()
            .filter(|item| item.id == "cold_history_candidate")
            .collect();
        assert_eq!(candidates.len(), 2);
        for item in candidates {
            assert_eq!(item.cost, ContextCost::default());
            assert!(
                matches!(item.decision, ContextDecision::Omitted { reason: ContextOmissionReason::BudgetExcluded, source_text_chars: Some(count) } if count > 0)
            );
        }
    }

    #[test]
    fn auxiliary_omissions_distinguish_disabled_budget_unavailable_and_no_result() {
        use medousa_runtime::context::{
            ContextCoordinator, ContextCost, ContextDecision, ContextOmissionReason,
        };
        for enabled in [false, true] {
            let mut limits = sample_limits();
            limits.max_prior_total_chars = 0;
            limits.include_auxiliary_history = enabled;
            let built = build_prior_messages(
                &crate::typed_tools::ToolCatalog::default(),
                "aux-empty",
                &[],
                "current",
                false,
                2,
                2,
                limits,
            );
            for id in [
                "tool_slices",
                "grapheme_scripts",
                "runtime_learnings",
                "tool_hints",
            ] {
                let item = built
                    .context
                    .contributions
                    .iter()
                    .find(|item| item.id == id)
                    .unwrap();
                assert_eq!(item.cost, ContextCost::default());
                assert_eq!(
                    item.decision,
                    ContextDecision::Omitted {
                        reason: if enabled {
                            ContextOmissionReason::BudgetExcluded
                        } else {
                            ContextOmissionReason::Disabled
                        },
                        source_text_chars: None,
                    }
                );
            }
            assert!(built.messages.is_empty());
        }
        let mut context = ContextCoordinator::default();
        super::record_auxiliary_omission(&mut context, "aux-empty", "unavailable", None, 10);
        super::record_auxiliary_omission(&mut context, "aux-empty", "empty", Some(""), 10);
        for (id, reason) in [
            ("unavailable", ContextOmissionReason::Unavailable),
            ("empty", ContextOmissionReason::NoResult),
        ] {
            let item = context
                .contributions
                .iter()
                .find(|item| item.id == id)
                .unwrap();
            assert_eq!(
                item.decision,
                ContextDecision::Omitted {
                    reason,
                    source_text_chars: None
                }
            );
        }
    }

    #[test]
    fn auxiliary_build_reports_partial_native_index_loads() {
        use medousa_runtime::context::{ContextDecision, ContextOmissionReason};
        crate::vault::service::with_temp_vault(|| {
            crate::grapheme_script::store::with_temp_grapheme_scripts(|| {
                use crate::grapheme_script::{GraphemeScriptService, store::GraphemeScriptStore};
                let prompt = "snapshotoutcomeneedle";
                let catalog = crate::typed_tools::ToolCatalog::default();
                let build = || {
                    build_prior_messages(
                        &catalog,
                        "snapshot-outcome",
                        &[],
                        prompt,
                        false,
                        2,
                        2,
                        sample_limits(),
                    )
                };
                GraphemeScriptService::save(
                    Some("good"),
                    "Good",
                    prompt,
                    vec![],
                    vec![],
                    None,
                    None,
                )
                .unwrap();
                let index_path = GraphemeScriptStore::root_dir().join("index.jsonl");
                let valid = std::fs::read_to_string(&index_path).unwrap();
                std::fs::write(&index_path, format!("{valid}\ninvalid-index-entry\n")).unwrap();
                crate::grapheme_script::grapheme_script_store().reload_from_disk();
                let partial = build();
                assert!(
                    partial
                        .context
                        .contributions
                        .iter()
                        .any(|item| item.id == "grapheme_scripts"
                            && item.decision == ContextDecision::Included)
                );
                assert!(partial.context.contributions.iter().any(|item| item.id
                    == "grapheme_scripts_unavailable_reads"
                    && item.decision
                        == ContextDecision::Omitted {
                            reason: ContextOmissionReason::Unavailable,
                            source_text_chars: None
                        }));
                std::fs::write(&index_path, "invalid-index-entry\n").unwrap();
                crate::grapheme_script::grapheme_script_store().reload_from_disk();
                // An undecodable dirty note is skipped by the existing freshness
                // pass, which leaves its certification fence open. No extra scan.
                std::fs::write(
                    crate::vault::path::user_vault_root().join("unreadable.md"),
                    [0xff, 0xfe],
                )
                .unwrap();
                crate::vault::store::PROJECTION.mark_stale_reconciling();
                let unavailable = build();
                for id in ["grapheme_scripts", "runtime_learnings"] {
                    let item = unavailable
                        .context
                        .contributions
                        .iter()
                        .find(|item| item.id == id)
                        .unwrap();
                    assert_eq!(
                        item.decision,
                        ContextDecision::Omitted {
                            reason: ContextOmissionReason::Unavailable,
                            source_text_chars: None
                        }
                    );
                }
            });
        });
    }

    #[test]
    fn auxiliary_build_reports_failed_vault_enumeration_not_successful_empty() {
        use medousa_runtime::context::{ContextDecision, ContextOmissionReason};
        crate::vault::service::with_temp_vault(|| {
            let root = crate::vault::path::user_vault_root();
            let blocked = root.join("not-a-directory");
            std::fs::write(&blocked, "not a vault directory").unwrap();
            crate::vault::roots::set_test_vault_root_override(Some(blocked));
            crate::vault::path::clear_vault_root_capabilities();
            crate::vault::store::PROJECTION.mark_stale_reconciling();
            let built = build_prior_messages(
                &crate::typed_tools::ToolCatalog::default(),
                "enumeration-failure",
                &[],
                "current",
                false,
                2,
                2,
                sample_limits(),
            );
            let item = built
                .context
                .contributions
                .iter()
                .find(|item| item.id == "runtime_learnings")
                .unwrap();
            assert_eq!(
                item.decision,
                ContextDecision::Omitted {
                    reason: ContextOmissionReason::Unavailable,
                    source_text_chars: None
                }
            );
            // Reset fixture authority before its normal teardown scan.
            crate::vault::roots::set_test_vault_root_override(Some(root));
            crate::vault::path::clear_vault_root_capabilities();
        });
    }

    #[test]
    fn auxiliary_producer_adapter_build_preserves_success_partial_and_failed_reads() {
        use crate::grapheme_script::{GraphemeScriptService, store::GraphemeScriptStore};
        use medousa_runtime::context::{ContextCost, ContextDecision, ContextOmissionReason};
        crate::vault::service::with_temp_vault(|| {
            crate::grapheme_script::store::with_temp_grapheme_scripts(|| {
                let prompt = "outcomeidentityneedle";
                let catalog = crate::typed_tools::ToolCatalog::default();
                let mut limits = sample_limits();
                limits.max_prior_total_chars = 20_000;
                limits.hot_window_char_budget = 10_000;
                let build = || {
                    build_prior_messages(&catalog, "outcome-test", &[], prompt, false, 2, 2, limits)
                };
                let omission = |built: &super::PriorMessageBuild, id: &str, reason| {
                    let item = built
                        .context
                        .contributions
                        .iter()
                        .find(|item| item.id == id)
                        .unwrap();
                    assert_eq!(item.cost, ContextCost::default());
                    assert_eq!(
                        item.decision,
                        ContextDecision::Omitted {
                            reason,
                            source_text_chars: None
                        }
                    );
                };
                let empty = build();
                for id in ["runtime_learnings", "grapheme_scripts"] {
                    omission(&empty, id, ContextOmissionReason::NoResult);
                }
                let mut script_paths = Vec::new();
                for (id, name) in [("first", "First"), ("second", "Second")] {
                    let path = format!("learnings/{id}.md");
                    crate::vault::VaultService::write_note(
                        Some(&path),
                        &crate::daemon_api::VaultWriteRequest {
                            path: Some(path.clone()),
                            content: format!(
                                "---\ntags: [runtime-learning]\n---\n# {name}\n{prompt}"
                            ),
                            ..Default::default()
                        },
                        None,
                    )
                    .unwrap();
                    let entry = GraphemeScriptService::save(
                        Some(id),
                        name,
                        prompt,
                        vec![],
                        vec![],
                        None,
                        None,
                    )
                    .unwrap();
                    script_paths.push(GraphemeScriptStore::root_dir().join(entry.body_path));
                }
                let complete = build();
                let scripts = crate::learning_artifacts::build_grapheme_script_recall_block(
                    prompt,
                    crate::learning_artifacts::DEFAULT_SCRIPT_RECALL_BLOCK_CHARS,
                );
                assert!(!scripts.is_empty());
                assert!(
                    complete
                        .messages
                        .iter()
                        .any(|message| message.content.first_text() == Some(scripts.as_str()))
                );
                assert!(
                    !complete
                        .context
                        .contributions
                        .iter()
                        .any(|item| item.id.ends_with("_unavailable_reads"))
                );
                // Delete payloads without refreshing native indexes. This exercises
                // real read errors through producer -> ToolContext -> build, no mock
                // omission helper or second retrieval path.
                for (index, id) in ["second", "first"].into_iter().enumerate() {
                    std::fs::remove_file(
                        crate::vault::path::user_vault_root().join(format!("learnings/{id}.md")),
                    )
                    .unwrap();
                    std::fs::remove_file(&script_paths[1 - index]).unwrap();
                    let built = build();
                    for source in ["runtime_learnings", "grapheme_scripts"] {
                        if index == 0 {
                            assert!(
                                built
                                    .context
                                    .contributions
                                    .iter()
                                    .any(|item| item.id == source
                                        && item.decision == ContextDecision::Included)
                            );
                            omission(
                                &built,
                                &format!("{source}_unavailable_reads"),
                                ContextOmissionReason::Unavailable,
                            );
                        } else {
                            omission(&built, source, ContextOmissionReason::Unavailable);
                            assert!(
                                !built
                                    .context
                                    .contributions
                                    .iter()
                                    .any(|item| item.id == source
                                        && item.decision == ContextDecision::Included)
                            );
                        }
                    }
                    let expected_scripts =
                        crate::learning_artifacts::build_grapheme_script_recall_block(
                            prompt,
                            crate::learning_artifacts::DEFAULT_SCRIPT_RECALL_BLOCK_CHARS,
                        );
                    assert_eq!(
                        built
                            .messages
                            .iter()
                            .filter_map(|message| message.content.first_text())
                            .find(|text| text.starts_with("[MEDOUSA_GRAPHEME_SCRIPTS]")),
                        (!expected_scripts.is_empty()).then_some(expected_scripts.as_str())
                    );
                }
            });
        });
    }

    #[test]
    fn real_auxiliary_rendering_preserves_legacy_order_and_budgets() {
        crate::vault::service::with_temp_vault(|| {
            let prompt = "context-auxiliary-parity";
            let path = "learnings/context-auxiliary-parity.md";
            crate::vault::VaultService::write_note(Some(path), &crate::daemon_api::VaultWriteRequest {
                path: Some(path.into()), content: format!("---\ntags: [runtime-learning]\n---\n# {prompt}\nKeep domain ownership explicit."),
                ..Default::default()
            }, None).unwrap();
            let turns = vec![
                correction_turn("user", "old"),
                correction_turn("assistant", "old answer"),
                correction_turn("user", "recent question"),
                correction_turn("assistant", "recent answer"),
            ];
            let catalog = crate::typed_tools::ToolCatalog::default();
            let limits = sample_limits();
            let actual =
                build_prior_messages(&catalog, "aux-parity", &turns, prompt, false, 2, 2, limits);
            let mut no_aux = limits;
            no_aux.include_auxiliary_history = false;
            let base =
                build_prior_messages(&catalog, "aux-parity", &turns, prompt, false, 2, 2, no_aux);
            let mut expected = base.messages;
            expected.reverse();
            let mut used = base.total_chars;
            let hot = vec![&turns[3], &turns[2]];
            // Frozen pre-coordinator allocation expression. Run the real producers,
            // including a guaranteed nonempty vault learning and tool-slice block.
            let budget = |used: usize, cap: usize| {
                limits
                    .max_prior_total_chars
                    .saturating_sub(used)
                    .min(limits.hot_window_char_budget.min(cap))
            };
            let slices = crate::turn_slice::build_tool_slices_block(
                &turns,
                &hot,
                budget(used, crate::turn_slice::DEFAULT_SLICE_BLOCK_CHARS),
                crate::turn_slice::DEFAULT_SLICE_HOT_LINE_CHARS,
            );
            assert!(!slices.is_empty());
            used += slices.chars().count();
            expected.push(genai::chat::ChatMessage::assistant(slices));
            let scripts = crate::learning_artifacts::build_grapheme_script_recall_block(
                prompt,
                budget(
                    used,
                    crate::learning_artifacts::DEFAULT_SCRIPT_RECALL_BLOCK_CHARS,
                ),
            );
            if !scripts.trim().is_empty() {
                used += scripts.chars().count();
                expected.push(genai::chat::ChatMessage::assistant(scripts));
            }
            let learnings = crate::learning_artifacts::build_runtime_learnings_block(
                prompt,
                budget(
                    used,
                    crate::learning_artifacts::DEFAULT_LEARNING_RECALL_BLOCK_CHARS,
                ),
            );
            assert!(!learnings.is_empty());
            used += learnings.chars().count();
            expected.push(genai::chat::ChatMessage::assistant(learnings));
            let hints = crate::tool_bootstrap::build_tool_hints_block(
                &catalog,
                "aux-parity",
                prompt,
                &turns,
                budget(used, crate::tool_bootstrap::DEFAULT_TOOL_HINTS_BLOCK_CHARS),
            );
            if !hints.trim().is_empty() {
                used += hints.chars().count();
                expected.push(genai::chat::ChatMessage::assistant(hints));
            }
            expected.reverse();
            assert_eq!(
                serde_json::to_value(actual.messages).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
            assert_eq!(actual.total_chars, used);
            assert_eq!(
                actual.context.report(Default::default()).cost.text_chars,
                used
            );
        });
    }

    #[test]
    fn prior_messages_include_cold_history_summary() {
        let mut turns = Vec::new();
        for idx in 0..18 {
            turns.push(ConversationTurn {
                role: if idx % 2 == 0 {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                },
                content: format!("turn-{idx}-{}", "x".repeat(120)),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            });
        }

        let built = build_prior_messages(
            &crate::typed_tools::ToolCatalog::default(),
            "test-session",
            &turns,
            "new prompt",
            false,
            8,
            24,
            sample_limits(),
        );
        assert!(built.hot_turns_included > 0);
        assert!(built.cold_turns_summarized > 0);
        assert!(built.total_chars > 0);
    }

    #[test]
    fn prior_messages_include_agent_role_as_assistant() {
        let turns = vec![
            ConversationTurn {
                role: "user".to_string(),
                content: "hello".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            },
            ConversationTurn {
                role: "agent".to_string(),
                content: "hi there".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            },
        ];

        let built = build_prior_messages(
            &crate::typed_tools::ToolCatalog::default(),
            "test-session",
            &turns,
            "new prompt",
            false,
            8,
            24,
            sample_limits(),
        );
        let has_assistant = built
            .messages
            .iter()
            .any(|message| matches!(message.role, genai::chat::ChatRole::Assistant));
        assert!(has_assistant);
    }

    #[test]
    fn classifier_recent_context_excludes_current_persisted_user_turn() {
        let turns = vec![
            ConversationTurn {
                role: "user".to_string(),
                content: "earlier question".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            },
            ConversationTurn {
                role: "agent".to_string(),
                content: "earlier answer".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            },
            ConversationTurn {
                role: "user".to_string(),
                content: "thanks".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            },
        ];

        let context = build_intent_classifier_recent_context(
            &turns,
            "thanks",
            true,
            4,
            1400,
            super::IntentContextLimits {
                context_line_chars: 260,
            },
        );
        assert!(context.contains("user: earlier question"));
        assert!(context.contains("assistant: earlier answer"));
        assert!(!context.contains("user: thanks"));
    }

    #[test]
    fn prior_messages_include_tool_slices_block() {
        use crate::turn_parts::TurnPart;
        use crate::turn_slice::{TOOL_SLICES_PREFIX, TurnSliceSummary};

        let turns = vec![
            ConversationTurn {
                role: "user".to_string(),
                content: "research these topics".to_string(),
                timestamp: Utc::now(),
                tool_names: vec![],
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            },
            ConversationTurn {
                role: "assistant".to_string(),
                content: "resolved base-researcher".to_string(),
                timestamp: Utc::now(),
                tool_names: vec!["cognition_manuscript_list".to_string()],
                answer_state: None,
                parts: Some(vec![TurnPart::ToolRun {
                    run_id: "r1".to_string(),
                    tool_name: "cognition_manuscript_list".to_string(),
                    status: "succeeded".to_string(),
                    input_summary: "list".to_string(),
                    input_params: vec![],
                    output_summary: Some("base-researcher".to_string()),
                    artifact_refs: vec![],
                    tool_round: Some(1),
                    started_at: Utc::now(),
                    finished_at: None,
                }]),
                slice_summary: Some(TurnSliceSummary {
                    goal: "resolve manuscripts".to_string(),
                    tool_rounds: 1,
                    tools: vec!["cognition_manuscript_list".to_string()],
                    outcomes: vec!["base-researcher".to_string()],
                    ..Default::default()
                }),
                speaker_profile_id: None,
                reactions: Vec::new(),
            },
        ];

        let built = build_prior_messages(
            &crate::typed_tools::ToolCatalog::default(),
            "test-session",
            &turns,
            "spin them up",
            false,
            8,
            24,
            sample_limits(),
        );
        assert!(built.tool_slices_chars > TOOL_SLICES_PREFIX.len());
        assert!(built.total_chars > built.tool_slices_chars);

        let mut focused_limits = sample_limits();
        focused_limits.include_auxiliary_history = false;
        let focused = build_prior_messages(
            &crate::typed_tools::ToolCatalog::default(),
            "test-session",
            &turns,
            "spin them up",
            false,
            6,
            0,
            focused_limits,
        );
        assert_eq!(focused.cold_turns_summarized, 0);
        assert_eq!(focused.tool_slices_chars, 0);
        assert_eq!(focused.script_recall_chars, 0);
        assert_eq!(focused.learning_recall_chars, 0);
        assert_eq!(focused.tool_hints_chars, 0);
    }

    #[test]
    fn classifier_recent_context_normalizes_agent_role() {
        let turns = vec![
            ConversationTurn {
                role: "agent".to_string(),
                content: "done".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            },
            ConversationTurn {
                role: "user".to_string(),
                content: "ok".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
                speaker_profile_id: None,
                reactions: Vec::new(),
            },
        ];

        let context = build_intent_classifier_recent_context(
            &turns,
            "new",
            false,
            4,
            1400,
            super::IntentContextLimits {
                context_line_chars: 260,
            },
        );
        assert!(context.contains("assistant: done"));
        assert!(!context.contains("agent: done"));
    }
}
