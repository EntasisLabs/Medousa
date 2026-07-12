use stasis::application::orchestration::prompt_pipeline::PromptExecutionContext;

/// Stored/display value when provider-native reasoning is left unset.
pub const REASONING_EFFORT_DEFAULT: &str = "default";

/// Normalize user-facing reasoning effort to an optional provider keyword.
/// Empty, `default`, `auto`, and `none` mean "do not set reasoning_effort on the request".
pub fn normalize_reasoning_effort(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "default" | "auto" | "none" => None,
        other => Some(other.to_string()),
    }
}

pub fn normalize_reasoning_effort_value(value: &str) -> String {
    normalize_reasoning_effort(value).unwrap_or_else(|| REASONING_EFFORT_DEFAULT.to_string())
}

pub fn reasoning_effort_hint(mode: &str) -> &'static str {
    match mode.trim().to_ascii_lowercase().as_str() {
        "minimal" => "lightest provider reasoning",
        "low" => "fast reasoning, lower cost",
        "medium" => "balanced reasoning depth",
        "high" => "deeper reasoning",
        "xhigh" => "extra-high reasoning (OpenAI-class)",
        "max" => "maximum reasoning (Anthropic-class)",
        REASONING_EFFORT_DEFAULT | "auto" | "none" | "" => "provider default reasoning",
        other if other.starts_with("budget:") => "custom thinking token budget",
        _ => "provider-native reasoning intensity",
    }
}

pub fn prompt_execution_context(model: &str, reasoning_effort: Option<&str>) -> PromptExecutionContext {
    let model_hint = model.trim();
    PromptExecutionContext {
        model_hint: (!model_hint.is_empty()).then(|| model_hint.to_string()),
        reasoning_effort: reasoning_effort.and_then(normalize_reasoning_effort),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sentinels_map_to_none() {
        assert_eq!(normalize_reasoning_effort("default"), None);
        assert_eq!(normalize_reasoning_effort("none"), None);
        assert_eq!(normalize_reasoning_effort("high"), Some("high".to_string()));
    }

    #[test]
    fn prompt_context_omits_default_effort() {
        let ctx = prompt_execution_context("gpt-5.4-mini", Some("default"));
        assert!(ctx.reasoning_effort.is_none());
        assert_eq!(ctx.model_hint.as_deref(), Some("gpt-5.4-mini"));
    }
}
