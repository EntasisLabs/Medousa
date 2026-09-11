use stasis::application::orchestration::prompt_pipeline::PromptExecutionContext;

/// Stored/display value when provider-native reasoning is left unset.
pub const REASONING_EFFORT_DEFAULT: &str = "default";

/// Normalize user-facing reasoning effort to an optional provider keyword.
/// Empty, `default`, and `auto` mean "do not set reasoning_effort on the request".
pub fn normalize_reasoning_effort(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "default" | "auto" => None,
        other => Some(other.to_string()),
    }
}

pub fn normalize_reasoning_effort_value(value: &str) -> String {
    normalize_reasoning_effort(value).unwrap_or_else(|| REASONING_EFFORT_DEFAULT.to_string())
}

pub fn reasoning_effort_hint(mode: &str) -> &'static str {
    match mode.trim().to_ascii_lowercase().as_str() {
        "none" => "reasoning disabled",
        "minimal" => "lightest provider reasoning",
        "low" => "fast reasoning, lower cost",
        "medium" => "balanced reasoning depth",
        "high" => "deeper reasoning",
        "xhigh" => "extra-high reasoning (OpenAI-class)",
        "max" => "maximum supported reasoning depth",
        REASONING_EFFORT_DEFAULT | "auto" | "" => "provider default reasoning",
        other if other.starts_with("budget:") => "custom thinking token budget",
        _ => "provider-native reasoning intensity",
    }
}

pub fn prompt_execution_context(
    model: &str,
    reasoning_effort: Option<&str>,
) -> PromptExecutionContext {
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
        assert_eq!(normalize_reasoning_effort("none"), Some("none".into()));
        assert_eq!(normalize_reasoning_effort("high"), Some("high".to_string()));
    }

    #[test]
    fn prompt_context_omits_default_effort() {
        let ctx = prompt_execution_context("gpt-5.4-mini", Some("default"));
        assert!(ctx.reasoning_effort.is_none());
        assert_eq!(ctx.model_hint.as_deref(), Some("gpt-5.4-mini"));
    }
}

/// Options supported by both the model and Medousa's installed genai adapter.
/// Unknown routes deliberately offer only the provider default.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningCapability {
    pub kind: String,
    pub levels: Vec<String>,
    pub budget_min: Option<u32>,
    pub budget_max: Option<u32>,
    pub default_level: Option<String>,
    pub source: String,
}

impl ReasoningCapability {
    fn effort(levels: &[&str]) -> Self {
        Self {
            kind: "effort".into(),
            levels: levels.iter().map(|s| (*s).into()).collect(),
            budget_min: None,
            budget_max: None,
            default_level: None,
            source: "verified_adapter".into(),
        }
    }

    fn budget(min: u32, max: u32) -> Self {
        Self {
            kind: "budget".into(),
            budget_min: Some(min),
            budget_max: Some(max),
            ..Self::effort(&[])
        }
    }

    pub fn accepts(&self, value: &str) -> bool {
        if normalize_reasoning_effort(value).is_none() {
            return true;
        }
        if let Some(budget) = value
            .strip_prefix("budget:")
            .and_then(|s| s.parse::<u32>().ok())
        {
            return self.kind == "budget"
                && self
                    .budget_min
                    .zip(self.budget_max)
                    .is_some_and(|(min, max)| (min..=max).contains(&budget));
        }
        self.levels.iter().any(|level| level == value)
    }

    pub fn advertised(levels: &[String], default: Option<String>) -> Self {
        let mut capability = Self::effort(&[]);
        for level in levels {
            if genai::chat::ReasoningEffort::from_keyword(level).is_some()
                && !capability.levels.contains(level)
            {
                capability.levels.push(level.clone());
            }
        }
        capability.kind = if !levels.is_empty() && capability.levels.is_empty() {
            "unknown"
        } else if capability.levels.is_empty() {
            "unsupported"
        } else {
            "effort"
        }
        .into();
        capability.default_level = default.filter(|level| capability.levels.contains(level));
        capability.source = "provider_catalog".into();
        capability
    }
}

// Keep these exact families bounded: a new model is not evidence of support.
// Sources: OpenAI model pages, Anthropic effort docs, Gemini Generate Content
// thinking docs; intersected with genai 0.6.5's adapter mappings.
pub fn reasoning_capability(provider: &str, model: &str) -> ReasoningCapability {
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    let (_, model) = genai::chat::ReasoningEffort::from_model_name(&model);
    let is = |names: &[&str]| {
        names.iter().any(|name| {
            model == *name
                || model.strip_prefix(name).is_some_and(|suffix| {
                    suffix.starts_with('-')
                        && suffix[1..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_digit())
                })
        })
    };
    match provider.as_str() {
        "openai" | "openai-codex" => {
            if is(&["gpt-6-astra"]) {
                return ReasoningCapability::effort(&["low", "medium", "high", "xhigh", "max"]);
            }
            if is(&["gpt-5.6-sol", "gpt-5.6-luna", "gpt-5.6-terra", "gpt-5.6"]) {
                return ReasoningCapability::effort(if provider == "openai-codex" {
                    &["low", "medium", "high", "xhigh", "max"]
                } else {
                    &["none", "low", "medium", "high", "xhigh", "max"]
                });
            }
            if is(&[
                "gpt-5.4",
                "gpt-5.4-mini",
                "gpt-5.4-nano",
                "gpt-5.5",
                "gpt-5.2",
            ]) {
                return ReasoningCapability::effort(&["none", "low", "medium", "high", "xhigh"]);
            }
            if is(&["gpt-5.3-codex", "gpt-5.3-codex-spark", "gpt-5.2-codex"]) {
                return ReasoningCapability::effort(&["low", "medium", "high", "xhigh"]);
            }
            if is(&["gpt-5", "gpt-5-mini", "gpt-5-nano"]) {
                return ReasoningCapability::effort(&["minimal", "low", "medium", "high"]);
            }
            if is(&["o3", "o3-mini", "o4-mini", "o1"]) {
                return ReasoningCapability::effort(&["low", "medium", "high"]);
            }
            if is(&[
                "gpt-4o",
                "gpt-4o-mini",
                "gpt-4.1",
                "gpt-4.1-mini",
                "gpt-4.1-nano",
            ]) {
                return ReasoningCapability {
                    kind: "unsupported".into(),
                    ..ReasoningCapability::effort(&[])
                };
            }
        }
        "anthropic" => {
            if is(&["claude-opus-4-7", "claude-opus-4.7"]) {
                return ReasoningCapability::effort(&["low", "medium", "high", "xhigh", "max"]);
            }
            if is(&["claude-opus-4-6"]) {
                return ReasoningCapability::effort(&["low", "medium", "high", "max"]);
            }
            if is(&["claude-sonnet-4-6", "claude-opus-4-5"]) {
                return ReasoningCapability::effort(&["low", "medium", "high"]);
            }
            if is(&[
                "claude-sonnet-4-5",
                "claude-haiku-4-5",
                "claude-sonnet-4-0",
                "claude-3-7-sonnet",
            ]) {
                return ReasoningCapability::budget(1024, 63999);
            }
            if is(&["claude-opus-4-0", "claude-opus-4-1"]) {
                return ReasoningCapability::budget(1024, 31999);
            }
        }
        "google" | "google-gemini" | "gemini" => {
            if is(&["gemini-3-pro-preview"]) {
                return ReasoningCapability::effort(&["low", "high"]);
            }
            if is(&["gemini-3.1-pro-preview"]) {
                return ReasoningCapability::effort(&["low", "medium", "high"]);
            }
            if is(&["gemini-3-flash-preview"]) {
                return ReasoningCapability::effort(&["minimal", "low", "medium", "high"]);
            }
            if is(&["gemini-2.5-pro"]) {
                return ReasoningCapability::budget(128, 32768);
            }
            if is(&["gemini-2.5-flash"]) {
                return ReasoningCapability::budget(0, 24576);
            }
        }
        "medousa-local" => {
            return ReasoningCapability {
                kind: "unsupported".into(),
                ..ReasoningCapability::effort(&[])
            };
        }
        _ => {}
    }
    ReasoningCapability {
        kind: "unknown".into(),
        source: "unknown".into(),
        ..ReasoningCapability::effort(&[])
    }
}

/// Enforce the same capability contract that the composer displays, including
/// inherited effort and legacy model suffixes, before reaching the adapter.
pub fn model_chat_options(
    provider: &str,
    model: &str,
    options: Option<&genai::chat::ChatOptions>,
) -> genai::chat::ChatOptions {
    use genai::chat::ReasoningEffort;
    let mut options =
        stasis::application::runtime::chat_options_resolver::apply_model_reasoning_suffix(
            model,
            options.cloned().unwrap_or_default(),
        );
    let (_, bare_model) = ReasoningEffort::from_model_name(model);
    let capability = crate::model_capability_registry::registry().reasoning(provider, bare_model);
    if let Some(effort) = &options.reasoning_effort {
        let value = match effort {
            ReasoningEffort::Budget(tokens) => format!("budget:{tokens}"),
            other => other.as_keyword().unwrap_or_default().to_string(),
        };
        if !capability.accepts(&value)
            || (provider.trim().eq_ignore_ascii_case("anthropic")
                && matches!(effort, ReasoningEffort::Budget(tokens) if options.max_tokens.is_some_and(|max| *tokens >= max)))
        {
            options.reasoning_effort = None;
        }
    }
    options
}

#[cfg(test)]
mod capability_tests {
    use super::*;
    use genai::chat::{ChatOptions, ReasoningEffort};

    #[test]
    fn profiles_respect_model_and_adapter_boundaries() {
        assert!(!reasoning_capability("openai-codex", "gpt-6-astra").accepts("minimal"));
        assert!(reasoning_capability("openai", "gpt-5.6-sol").accepts("none"));
        assert!(!reasoning_capability("anthropic", "claude-sonnet-4-6").accepts("max"));
        assert!(reasoning_capability("anthropic", "claude-opus-4-6").accepts("max"));
        assert!(!reasoning_capability("anthropic", "claude-opus-4-6").accepts("xhigh"));
        assert!(!reasoning_capability("gemini", "gemini-3.1-pro-preview").accepts("minimal"));
        assert!(reasoning_capability("gemini", "gemini-3.1-pro-preview").accepts("medium"));
        assert_eq!(
            reasoning_capability("openai", "gpt-4.1").kind,
            "unsupported"
        );
        assert_eq!(
            reasoning_capability("custom", "gpt-5.6-sol").kind,
            "unknown"
        );
        assert_eq!(
            reasoning_capability("openai", "gpt-5.6-future").kind,
            "unknown"
        );
    }

    #[test]
    fn budgets_are_checked_and_zero_is_explicit() {
        let flash = reasoning_capability("gemini", "gemini-2.5-flash");
        assert_eq!(flash, reasoning_capability("google", "gemini-2.5-flash"));
        assert_eq!(
            flash,
            reasoning_capability("google-gemini", "gemini-2.5-flash")
        );
        assert!(flash.accepts("budget:0"));
        assert!(flash.accepts("budget:24576"));
        assert!(!flash.accepts("budget:24577"));
        assert!(!reasoning_capability("gemini", "gemini-2.5-pro").accepts("budget:0"));
    }

    #[test]
    fn provider_metadata_filters_unknown_keywords_without_inventing_levels() {
        let capability = ReasoningCapability::advertised(
            &["high".into(), "high".into(), "future".into()],
            Some("low".into()),
        );
        assert_eq!(capability.levels, ["high"]);
        assert!(capability.default_level.is_none());
    }

    #[test]
    fn request_validation_preserves_off_and_drops_invalid_suffixes() {
        let options = ChatOptions::default().with_reasoning_effort(ReasoningEffort::None);
        assert!(matches!(
            model_chat_options("openai", "gpt-5.6-sol", Some(&options)).reasoning_effort,
            Some(ReasoningEffort::None)
        ));
        assert!(
            model_chat_options("openai-codex", "gpt-6-astra-minimal", None)
                .reasoning_effort
                .is_none()
        );
        assert!(matches!(
            model_chat_options("openai-codex", "gpt-6-astra-max", None).reasoning_effort,
            Some(ReasoningEffort::Max)
        ));
    }
}
