use std::sync::Arc;

use genai::chat::ChatMessage;
use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use crate::medousa_tool_loop::MedousaToolLoopPipeline;
use stasis::application::orchestration::tool_loop_pipeline::ToolCallMode;
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::AiChatClient;

use crate::session::ConversationTurn;
use crate::learning_artifacts::{
    build_grapheme_script_recall_block, build_runtime_learnings_block,
    DEFAULT_LEARNING_RECALL_BLOCK_CHARS, DEFAULT_SCRIPT_RECALL_BLOCK_CHARS,
};
use crate::tool_bootstrap::{build_tool_hints_block, DEFAULT_TOOL_HINTS_BLOCK_CHARS};
use crate::turn_slice::{
    build_tool_slices_block, format_cold_history_line, prior_turn_content,
    DEFAULT_SLICE_BLOCK_CHARS, DEFAULT_SLICE_HOT_LINE_CHARS,
};
use crate::stage_routing::StageRoute;
use crate::tools::TuiRuntime;
use crate::tui::settings::RuntimeSettings;

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
}

#[derive(Debug, Clone, Copy)]
pub struct PriorMessageLimits {
    pub max_prior_total_chars: usize,
    pub max_single_prior_message_chars: usize,
    pub hot_window_char_budget: usize,
    pub cold_window_char_budget: usize,
    pub cold_summary_line_chars: usize,
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
    use std::sync::Arc;

    use stasis::application::orchestration::tool_registry::ToolRegistry;

    use crate::agent_runtime::turn_worker::AllowlistToolRegistry;
    use crate::tui::runtime_services::build_tool_loop_pipeline_for_target;

    let registry: Arc<dyn ToolRegistry> = match tool_allowlist {
        Some(allowlist) => Arc::new(AllowlistToolRegistry::new(
            tui_rt.tool_registry.clone(),
            allowlist,
        )),
        None => tui_rt.tool_registry.clone(),
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

pub fn build_prior_messages(
    session_id: &str,
    turns: &[ConversationTurn],
    current_prompt: &str,
    current_user_persisted: bool,
    hot_window_turns: usize,
    cold_window_turns: usize,
    limits: PriorMessageLimits,
) -> PriorMessageBuild {
    let mut selected: Vec<&ConversationTurn> = turns
        .iter()
        .filter(|turn| !crate::turn_failure::is_error_turn_excluded_from_model_context(turn))
        .collect();

    if current_user_persisted
        && let Some(last) = selected.last()
            && last.role == "user" && last.content.trim() == current_prompt.trim() {
                selected.pop();
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
    for turn in &hot_turns {
        if hot_remaining == 0 {
            break;
        }

        let bounded = prior_turn_content(turn, limits.max_single_prior_message_chars);
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
        .filter_map(|turn| {
            format_cold_history_line(turn, limits.cold_summary_line_chars).or_else(|| {
                match turn.role.as_str() {
                    "user" | "assistant" | "agent" => {
                        let line = truncate_text_for_budget(
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
                }
            })
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

    let slice_budget = limits
        .hot_window_char_budget
        .min(DEFAULT_SLICE_BLOCK_CHARS)
        .min(limits.max_prior_total_chars.saturating_sub(total_chars));
    let tool_slices_block = build_tool_slices_block(
        turns,
        &hot_turns,
        slice_budget,
        DEFAULT_SLICE_HOT_LINE_CHARS,
    );
    let tool_slices_chars = tool_slices_block.chars().count();
    if !tool_slices_block.trim().is_empty() {
        total_chars = total_chars.saturating_add(tool_slices_chars);
        accepted.push(ChatMessage::assistant(tool_slices_block));
    }

    let script_budget = limits
        .hot_window_char_budget
        .min(DEFAULT_SCRIPT_RECALL_BLOCK_CHARS)
        .min(limits.max_prior_total_chars.saturating_sub(total_chars));
    let script_recall_block =
        build_grapheme_script_recall_block(current_prompt, script_budget);
    let script_recall_chars = script_recall_block.chars().count();
    if !script_recall_block.trim().is_empty() {
        total_chars = total_chars.saturating_add(script_recall_chars);
        accepted.push(ChatMessage::assistant(script_recall_block));
    }

    let learning_budget = limits
        .hot_window_char_budget
        .min(DEFAULT_LEARNING_RECALL_BLOCK_CHARS)
        .min(limits.max_prior_total_chars.saturating_sub(total_chars));
    let learning_recall_block =
        build_runtime_learnings_block(current_prompt, learning_budget);
    let learning_recall_chars = learning_recall_block.chars().count();
    if !learning_recall_block.trim().is_empty() {
        total_chars = total_chars.saturating_add(learning_recall_chars);
        accepted.push(ChatMessage::assistant(learning_recall_block));
    }

    let hints_budget = limits
        .hot_window_char_budget
        .min(DEFAULT_TOOL_HINTS_BLOCK_CHARS)
        .min(limits.max_prior_total_chars.saturating_sub(total_chars));
    let tool_hints_block =
        build_tool_hints_block(session_id, current_prompt, turns, hints_budget);
    let tool_hints_chars = tool_hints_block.chars().count();
    if !tool_hints_block.trim().is_empty() {
        total_chars = total_chars.saturating_add(tool_hints_chars);
        accepted.push(ChatMessage::assistant(tool_hints_block));
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
        tool_slices_chars,
        script_recall_chars,
        learning_recall_chars,
        tool_hints_chars,
        total_chars,
    }
}

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
    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(Some(provider), model, base_url),
    );
    PromptExecutionPipeline::new(chat_client)
}

pub fn build_intent_classifier_recent_context(
    turns: &[ConversationTurn],
    current_prompt: &str,
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
            && last.role == "user" && last.content.trim() == current_prompt.trim() {
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
        "search",
        "look up",
        "lookup",
        "run ",
        "execute",
        "query",
        "fetch",
        "verify",
        "evidence",
        "grapheme",
        "tool",
        "call",
        "api",
        "latest",
        // memory / AVEC / posture (avoid long-session no-tools misclassification)
        "calibrat",
        "avec",
        "memory",
        "mood",
        "locus",
        "recall",
        "context",
        "schema",
        "store",
        "pull",
        "focus",
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

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use stasis::application::orchestration::tool_loop_pipeline::ToolCallMode;

    use super::{
        PriorMessageLimits, apply_context_compiler_activation_gate, build_intent_classifier_recent_context,
        build_prior_messages, decide_turn_activation, parse_tool_call_mode,
    };
    use crate::session::ConversationTurn;

    fn sample_limits() -> PriorMessageLimits {
        PriorMessageLimits {
            max_prior_total_chars: 24_000,
            max_single_prior_message_chars: 4_000,
            hot_window_char_budget: 14_000,
            cold_window_char_budget: 8_000,
            cold_summary_line_chars: 240,
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
            });
        }

        let built = build_prior_messages(
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
            },
            ConversationTurn {
                role: "agent".to_string(),
                content: "hi there".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
            },
        ];

        let built = build_prior_messages("test-session", &turns, "new prompt", false, 8, 24, sample_limits());
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
            },
            ConversationTurn {
                role: "agent".to_string(),
                content: "earlier answer".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
            },
            ConversationTurn {
                role: "user".to_string(),
                content: "thanks".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
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
        use crate::turn_slice::{TurnSliceSummary, TOOL_SLICES_PREFIX};

        let turns = vec![
            ConversationTurn {
                role: "user".to_string(),
                content: "research these topics".to_string(),
                timestamp: Utc::now(),
                tool_names: vec![],
                answer_state: None,
                parts: None,
                slice_summary: None,
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
            },
        ];

        let built = build_prior_messages("test-session", &turns, "spin them up", false, 8, 24, sample_limits());
        assert!(built.tool_slices_chars > TOOL_SLICES_PREFIX.len());
        assert!(built.total_chars > built.tool_slices_chars);
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
            },
            ConversationTurn {
                role: "user".to_string(),
                content: "ok".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: None,
                slice_summary: None,
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
