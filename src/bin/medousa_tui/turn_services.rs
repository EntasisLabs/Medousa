use std::sync::Arc;

use genai::chat::ChatMessage;
use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use stasis::application::orchestration::tool_loop_pipeline::{ToolCallMode, ToolLoopPipeline};
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::AiChatClient;

use medousa::TuiRuntime;

use super::{ConversationTurn, RuntimeSettings};

#[derive(Debug, Clone)]
pub(crate) struct TurnActivationDecision {
    pub(crate) turn_class: &'static str,
    pub(crate) tool_call_mode: ToolCallMode,
    pub(crate) max_tool_rounds: usize,
    pub(crate) enforce_no_tools: bool,
    pub(crate) reason: &'static str,
}

#[derive(Debug, Clone)]
pub(crate) struct PriorMessageBuild {
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) hot_turns_included: usize,
    pub(crate) cold_turns_summarized: usize,
    pub(crate) cold_summary_chars: usize,
    pub(crate) total_chars: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PriorMessageLimits {
    pub(crate) max_prior_total_chars: usize,
    pub(crate) max_single_prior_message_chars: usize,
    pub(crate) hot_window_char_budget: usize,
    pub(crate) cold_window_char_budget: usize,
    pub(crate) cold_summary_line_chars: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct IntentContextLimits {
    pub(crate) context_line_chars: usize,
}

pub(crate) struct SelectedTurnPipeline {
    pub(crate) pipeline: ToolLoopPipeline,
    pub(crate) route_dispatch_notice: Option<String>,
}

pub(crate) fn select_pipeline_for_turn(
    tui_rt: &TuiRuntime,
    final_route: Option<&medousa::stage_routing::StageRoute>,
    settings: &RuntimeSettings,
) -> SelectedTurnPipeline {
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
        let pipeline = tui_rt.tool_loop_pipeline_for_target(
            &route.provider,
            &route.model,
            route_base_url.as_deref(),
        );
        return SelectedTurnPipeline {
            pipeline,
            route_dispatch_notice: Some(route_notice),
        };
    }

    SelectedTurnPipeline {
        pipeline: tui_rt.tool_loop_pipeline.clone(),
        route_dispatch_notice: None,
    }
}

pub(crate) fn parse_tool_call_mode(value: &str) -> ToolCallMode {
    if value.trim().eq_ignore_ascii_case("strict") {
        ToolCallMode::Strict
    } else {
        ToolCallMode::Auto
    }
}

pub(crate) fn build_prior_messages(
    turns: &[ConversationTurn],
    current_prompt: &str,
    current_user_persisted: bool,
    hot_window_turns: usize,
    cold_window_turns: usize,
    limits: PriorMessageLimits,
) -> PriorMessageBuild {
    let mut selected: Vec<&ConversationTurn> = turns.iter().collect();

    if current_user_persisted {
        if let Some(last) = selected.last() {
            if last.role == "user" && last.content.trim() == current_prompt.trim() {
                selected.pop();
            }
        }
    }

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

    let mut hot_remaining = limits
        .hot_window_char_budget
        .min(limits.max_prior_total_chars);
    for turn in hot_turns {
        if hot_remaining == 0 {
            break;
        }

        let bounded = truncate_text_for_budget(&turn.content, limits.max_single_prior_message_chars);
        let bounded = truncate_text_for_budget(&bounded, hot_remaining);
        if bounded.trim().is_empty() {
            continue;
        }

        let bounded_chars = bounded.chars().count();
        hot_remaining = hot_remaining.saturating_sub(bounded_chars);
        total_chars = total_chars.saturating_add(bounded_chars);
        match turn.role.as_str() {
            "user" => accepted.push(ChatMessage::user(bounded)),
            "assistant" | "agent" => accepted.push(ChatMessage::assistant(bounded)),
            _ => {}
        }
    }

    let cold_lines = cold_turns
        .iter()
        .rev()
        .filter_map(|turn| match turn.role.as_str() {
            "user" | "assistant" | "agent" => {
                let line = truncate_text_for_budget(&turn.content, limits.cold_summary_line_chars);
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
        })
        .collect::<Vec<_>>();

    let cold_summary = if cold_lines.is_empty() {
        String::new()
    } else {
        let cold_budget = limits
            .cold_window_char_budget
            .min(limits.max_prior_total_chars.saturating_sub(total_chars));
        truncate_text_for_budget(
            &format!("[MEDOUSA_COLD_HISTORY_SUMMARY]\n{}", cold_lines.join("\n")),
            cold_budget,
        )
    };
    let cold_summary_chars = cold_summary.chars().count();
    if !cold_summary.trim().is_empty() {
        total_chars = total_chars.saturating_add(cold_summary_chars);
        accepted.push(ChatMessage::assistant(cold_summary));
    }

    accepted.reverse();
    PriorMessageBuild {
        messages: accepted,
        hot_turns_included: selected.len().min(hot_window_turns),
        cold_turns_summarized: selected
            .len()
            .saturating_sub(hot_window_turns)
            .min(cold_window_turns),
        cold_summary_chars,
        total_chars,
    }
}

pub(crate) fn decide_turn_activation(
    prompt: &str,
    configured_mode: ToolCallMode,
    configured_rounds: usize,
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

    if tool_intent {
        return TurnActivationDecision {
            turn_class: "c",
            tool_call_mode: ToolCallMode::Auto,
            max_tool_rounds: configured_rounds.min(12).max(2),
            enforce_no_tools: false,
            reason: "tool_intent_detected",
        };
    }

    if direct_answer_intent && prompt_chars < direct_answer_max_prompt_chars {
        return TurnActivationDecision {
            turn_class: "a",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: 1,
            enforce_no_tools: true,
            reason: "direct_answer_short_prompt",
        };
    }

    if turn_count > long_session_turn_threshold && prompt_chars < long_session_max_prompt_chars {
        return TurnActivationDecision {
            turn_class: "b",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: 1,
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

pub(crate) fn apply_context_compiler_activation_gate(
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

pub(crate) fn build_prompt_pipeline_for_turn(
    final_route: Option<&medousa::stage_routing::StageRoute>,
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

    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&provider),
            &model,
            base_url.as_deref(),
        ),
    );
    PromptExecutionPipeline::new(chat_client)
}

pub(crate) fn build_intent_classifier_recent_context(
    turns: &[ConversationTurn],
    current_prompt: &str,
    current_user_persisted: bool,
    max_turns: usize,
    max_chars: usize,
    limits: IntentContextLimits,
) -> String {
    let mut selected: Vec<&ConversationTurn> = turns.iter().collect();

    if current_user_persisted {
        if let Some(last) = selected.last() {
            if last.role == "user" && last.content.trim() == current_prompt.trim() {
                selected.pop();
            }
        }
    }

    let mut lines = Vec::new();
    let mut total_chars = 0usize;
    for turn in selected.iter().rev().take(max_turns).rev() {
        let role = match turn.role.as_str() {
            "user" => "user",
            "assistant" | "agent" => "assistant",
            _ => continue,
        };

        let text = truncate_text_for_budget(&turn.content, limits.context_line_chars)
            .replace('\n', " ");
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

fn route_base_url(
    route: &medousa::stage_routing::StageRoute,
    settings: &RuntimeSettings,
) -> Option<String> {
    if route.provider.eq_ignore_ascii_case(settings.provider.trim()) {
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
