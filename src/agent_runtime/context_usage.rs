//! Turn-start context budget telemetry (Cursor-style layer breakdown).

use std::sync::Arc;

use stasis::application::orchestration::tool_registry::ToolRegistry;

use crate::daemon_api::{ContextUsageLayer, ContextUsageReport};
use super::turn_services::PriorMessageBuild;

pub const ESTIMATOR_LABEL: &str = "chars_div_4";

pub fn chars_to_tokens(chars: usize) -> u32 {
    u32::try_from(chars.div_ceil(4)).unwrap_or(u32::MAX)
}

fn layer(id: &str, label: &str, chars: usize) -> ContextUsageLayer {
    let chars = u32::try_from(chars).unwrap_or(u32::MAX);
    ContextUsageLayer {
        id: id.to_string(),
        label: label.to_string(),
        chars,
        tokens_estimate: chars_to_tokens(chars as usize),
    }
}

pub struct ContextUsageInput<'a> {
    pub system_prompt_chars: usize,
    pub user_prompt_chars: usize,
    pub resolved_prompt_chars: usize,
    pub prompt_for_request_chars: usize,
    pub ambient_chars: usize,
    pub prior_build: &'a PriorMessageBuild,
    pub tool_count: usize,
    pub tool_schema_chars: usize,
    pub context_limit_tokens: Option<u32>,
}

pub fn build_context_usage_report(input: ContextUsageInput<'_>) -> ContextUsageReport {
    let injection_chars = input
        .resolved_prompt_chars
        .saturating_sub(input.user_prompt_chars);
    let tool_policy_chars = input
        .prompt_for_request_chars
        .saturating_sub(input.resolved_prompt_chars);

    let prior_hot_chars = input
        .prior_build
        .total_chars
        .saturating_sub(input.prior_build.cold_summary_chars)
        .saturating_sub(input.prior_build.tool_slices_chars)
        .saturating_sub(input.prior_build.tool_hints_chars)
        .saturating_sub(input.prior_build.script_recall_chars)
        .saturating_sub(input.prior_build.learning_recall_chars);

    let mut layers = vec![
        layer("system_prompt", "System prompt", input.system_prompt_chars),
        layer("tool_definitions", "Tool definitions", input.tool_schema_chars),
    ];

    if input.ambient_chars > 0 {
        layers.push(layer("ambient", "Ambient context", input.ambient_chars));
    }

    let other_injection = injection_chars.saturating_sub(input.ambient_chars);
    if other_injection > 0 {
        layers.push(layer(
            "memory_identity",
            "Memory & identity",
            other_injection,
        ));
    }

    if tool_policy_chars > 0 {
        layers.push(layer("tool_policy", "Tool policy", tool_policy_chars));
    }

    if input.prior_build.tool_hints_chars > 0 {
        layers.push(layer(
            "tool_hints",
            "Tool hints",
            input.prior_build.tool_hints_chars,
        ));
    }
    if input.prior_build.tool_slices_chars > 0 {
        layers.push(layer(
            "tool_slices",
            "Prior tool slices",
            input.prior_build.tool_slices_chars,
        ));
    }
    if input.prior_build.script_recall_chars > 0 {
        layers.push(layer(
            "grapheme_scripts",
            "Grapheme scripts",
            input.prior_build.script_recall_chars,
        ));
    }
    if input.prior_build.learning_recall_chars > 0 {
        layers.push(layer(
            "runtime_learnings",
            "Runtime learnings",
            input.prior_build.learning_recall_chars,
        ));
    }
    if input.prior_build.cold_summary_chars > 0 {
        layers.push(layer(
            "cold_history",
            "Summarized history",
            input.prior_build.cold_summary_chars,
        ));
    }
    if prior_hot_chars > 0 {
        layers.push(layer("prior_conversation", "Conversation", prior_hot_chars));
    }

    if input.user_prompt_chars > 0 {
        layers.push(layer("user_message", "Your message", input.user_prompt_chars));
    }

    let total_chars = layers.iter().map(|l| l.chars as usize).sum::<usize>();
    let total_tokens_estimate = chars_to_tokens(total_chars);

    ContextUsageReport {
        layers,
        total_tokens_estimate,
        total_chars: u32::try_from(total_chars).unwrap_or(u32::MAX),
        context_limit_tokens: input.context_limit_tokens,
        tool_count: u32::try_from(input.tool_count).unwrap_or(u32::MAX),
        estimator: ESTIMATOR_LABEL.to_string(),
    }
}

pub async fn estimate_tool_schema_chars(
    registry: &Arc<dyn ToolRegistry>,
) -> (usize, usize) {
    let tools = match registry.list_tools().await {
        Ok(tools) => tools,
        Err(_) => return (0, 0),
    };
    let count = tools.len();
    let chars: usize = tools
        .iter()
        .map(estimate_single_tool_chars)
        .sum();
    (count, chars)
}

fn estimate_single_tool_chars(tool: &genai::chat::Tool) -> usize {
    serde_json::to_string(tool)
        .map(|json| json.len())
        .unwrap_or_else(|_| tool.name.to_string().len() + 64)
}

pub fn operator_summary(report: &ContextUsageReport) -> String {
    let total = format_tokens(report.total_tokens_estimate);
    let limit = report
        .context_limit_tokens
        .map(|limit| format!(" / {}", format_tokens(limit)))
        .unwrap_or_default();
    let system = report
        .layers
        .iter()
        .find(|l| l.id == "system_prompt")
        .map(|l| format_tokens(l.tokens_estimate))
        .unwrap_or_else(|| "?".to_string());
    let tools = report
        .layers
        .iter()
        .find(|l| l.id == "tool_definitions")
        .map(|l| format_tokens(l.tokens_estimate))
        .unwrap_or_else(|| "?".to_string());
    format!("Context {total}{limit} · system {system} · tools {tools}")
}

/// Fill percentage when `context_limit_tokens` is known.
pub fn usage_fill_percent(report: &ContextUsageReport) -> Option<u32> {
    let limit = report.context_limit_tokens?;
    if limit == 0 {
        return None;
    }
    let pct = (report.total_tokens_estimate as f64 / limit as f64) * 100.0;
    Some(pct.round().clamp(0.0, 100.0) as u32)
}

/// Human-readable multiline breakdown for TUI, WhatsApp, Telegram, etc.
pub fn format_context_usage_text(report: &ContextUsageReport) -> String {
    let mut lines = Vec::new();
    if let Some(pct) = usage_fill_percent(report) {
        let total = format_tokens(report.total_tokens_estimate);
        let limit = report
            .context_limit_tokens
            .map(format_tokens)
            .unwrap_or_else(|| "?".to_string());
        lines.push(format!("Context {pct}% full · ~{total} / {limit} tokens"));
    } else {
        lines.push(format!(
            "Context ~{} tokens (estimator: {})",
            format_tokens(report.total_tokens_estimate),
            report.estimator
        ));
    }
    lines.push(String::new());
    for layer in &report.layers {
        lines.push(format!(
            "  {} — {}",
            layer.label,
            format_tokens(layer.tokens_estimate)
        ));
    }
    lines.join("\n")
}

fn format_tokens(n: u32) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_sum_to_total() {
        let prior = PriorMessageBuild {
            messages: Vec::new(),
            hot_turns_included: 0,
            cold_turns_summarized: 0,
            cold_summary_chars: 100,
            tool_slices_chars: 200,
            script_recall_chars: 0,
            learning_recall_chars: 0,
            tool_hints_chars: 50,
            total_chars: 500,
        };
        let report = build_context_usage_report(ContextUsageInput {
            system_prompt_chars: 16_000,
            user_prompt_chars: 10,
            resolved_prompt_chars: 1_200,
            prompt_for_request_chars: 1_600,
            ambient_chars: 600,
            prior_build: &prior,
            tool_count: 28,
            tool_schema_chars: 36_000,
            context_limit_tokens: Some(200_000),
        });
        assert!(report.total_tokens_estimate > 0);
        assert_eq!(report.tool_count, 28);
        assert!(report.layers.iter().any(|l| l.id == "system_prompt"));
        assert!(operator_summary(&report).contains("Context"));
    }

    #[test]
    fn format_context_usage_text_includes_layers() {
        let prior = PriorMessageBuild {
            messages: Vec::new(),
            hot_turns_included: 0,
            cold_turns_summarized: 0,
            cold_summary_chars: 0,
            tool_slices_chars: 0,
            script_recall_chars: 0,
            learning_recall_chars: 0,
            tool_hints_chars: 0,
            total_chars: 0,
        };
        let report = build_context_usage_report(ContextUsageInput {
            system_prompt_chars: 2_000,
            user_prompt_chars: 100,
            resolved_prompt_chars: 100,
            prompt_for_request_chars: 100,
            ambient_chars: 0,
            prior_build: &prior,
            tool_count: 5,
            tool_schema_chars: 9_000,
            context_limit_tokens: Some(200_000),
        });
        let text = format_context_usage_text(&report);
        assert!(text.contains("System prompt"));
        assert!(text.contains("Tool definitions"));
        assert!(text.contains('%'));
    }
}
