use super::types::{Modality, ModelCapabilityRecord};
use chrono::Utc;

/// Offline fallback when the catalog has no entry yet (boot, offline, new model).
pub fn infer_capability(provider: &str, model: &str) -> ModelCapabilityRecord {
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim().to_string();
    let supports_vision = infer_supports_vision(&provider, &model);
    let input_modalities = if supports_vision {
        vec![Modality::Text, Modality::Image]
    } else {
        vec![Modality::Text]
    };

    ModelCapabilityRecord {
        provider: provider.clone(),
        model_id: model.clone(),
        display_name: None,
        input_modalities,
        output_modalities: vec![Modality::Text],
        max_input_tokens: None,
        max_output_tokens: None,
        supports_tool_calling: None,
        supports_vision,
        reasoning: None,
        pricing: None,
        source: "heuristic.fallback".to_string(),
        fetched_at: Utc::now(),
    }
}

/// Preserve the limits advertised by the signed-in account, independently of
/// API-key model catalogs. Codex's effective percentage reserves provider headroom.
pub fn chatgpt_catalog_record(model: &serde_json::Value) -> ModelCapabilityRecord {
    let mut record = infer_capability("openai-codex", model["slug"].as_str().unwrap_or_default());
    record.display_name = model["display_name"].as_str().map(str::to_owned);
    record.source = "chatgpt_account".into();
    let positive = |key: &str| model[key].as_u64().filter(|value| *value > 0);
    let window = match (positive("context_window"), positive("max_context_window")) {
        (Some(window), Some(maximum)) => Some(window.min(maximum)),
        (window, maximum) => window.or(maximum),
    };
    let percent = positive("effective_context_window_percent")
        .filter(|value| *value <= 100)
        .unwrap_or(95);
    record.max_input_tokens = window.map(|window| window.saturating_mul(percent) / 100);
    record.max_output_tokens = positive("max_output_tokens");
    record.reasoning = model["supported_reasoning_levels"]
        .as_array()
        .map(|levels| {
            crate::reasoning_effort::ReasoningCapability::advertised(
                &levels
                    .iter()
                    .filter_map(|level| level["effort"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>(),
                model["default_reasoning_level"].as_str().map(str::to_owned),
            )
        });
    record
}

pub fn infer_supports_vision(provider: &str, model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    if model.is_empty() {
        return false;
    }

    if model.contains("vision")
        || model.contains("llava")
        || model.contains("bakllava")
        || model.contains("-vl")
        || model.contains("_vl")
    {
        return true;
    }

    match provider {
        "openai" | "openai-codex" | "azure-openai" | "azure_openai" | "openrouter" => {
            model.starts_with("gpt-4o")
                || model.starts_with("gpt-4.1")
                || model.starts_with("gpt-4-turbo")
                || model.starts_with("gpt-5")
                || model.starts_with("gpt-6.1-sol")
                || model.starts_with("gpt-6-astra")
                || model.starts_with("gpt-6-sol")
                || model.starts_with("gpt-6-luna")
                || model.contains("vision")
                || model.contains("openai/gpt-4o")
                || model.contains("openai/gpt-4.1")
                || model.contains("openai/gpt-5")
                || model.contains("openai/gpt-6.1-sol")
                || model.contains("openai/gpt-6-astra")
                || model.contains("openai/gpt-6-sol")
                || model.contains("openai/gpt-6-luna")
        }
        "anthropic" => {
            model.contains("claude-3")
                || model.contains("claude-4")
                || model.contains("claude-opus")
                || model.contains("claude-sonnet")
                || model.contains("claude-haiku")
        }
        "google" | "gemini" | "google-gemini" => model.contains("gemini"),
        "groq" => model.contains("vision"),
        "ollama" | "local" | "lmstudio" | "lm-studio" => {
            model.contains("llava")
                || model.contains("vision")
                || model.contains("moondream")
                || model.contains("minicpm-v")
        }
        "medousa-local" => {
            model == "gemma-4-e2b-it-4bit"
                || model == "lfm2.5-vl-1.6b-4bit"
                || model == "ministral-3-3b-instruct-4bit"
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_catalog_preserves_effective_limits_and_reasoning() {
        let record = chatgpt_catalog_record(&serde_json::json!({
            "slug": "gpt-6.1-sol", "display_name": "Sol",
            "context_window": 272000, "max_context_window": 872000,
            "effective_context_window_percent": 95, "max_output_tokens": 16384,
            "supported_reasoning_levels": [{"effort": "low"}, {"effort": "high"}],
            "default_reasoning_level": "high"
        }));
        assert_eq!(record.max_input_tokens, Some(258400));
        assert_eq!(record.max_output_tokens, Some(16384));
        assert_eq!(record.display_name.as_deref(), Some("Sol"));
        assert_eq!(record.source, "chatgpt_account");
        assert!(record.reasoning.is_some());
        assert!(record.supports_vision);
    }

    #[test]
    fn account_catalog_missing_or_invalid_limits_remain_unknown() {
        for value in [
            serde_json::json!({}),
            serde_json::json!({
                "context_window": -1, "max_context_window": 0, "max_output_tokens": "bad"
            }),
        ] {
            let record = chatgpt_catalog_record(&value);
            assert_eq!(record.max_input_tokens, None);
            assert_eq!(record.max_output_tokens, None);
        }
        let record = chatgpt_catalog_record(&serde_json::json!({
            "context_window": 1000000, "max_context_window": 100000,
            "effective_context_window_percent": 100
        }));
        assert_eq!(record.max_input_tokens, Some(100000));
        let record = chatgpt_catalog_record(&serde_json::json!({"max_context_window": 100000}));
        assert_eq!(record.max_input_tokens, Some(95000));
    }

    #[test]
    fn chatgpt_account_gpt_models_support_vision() {
        assert!(infer_supports_vision("openai-codex", "gpt-5.6-sol"));
        assert!(infer_supports_vision("openai-codex", "gpt-5.6-luna"));
        assert!(infer_supports_vision("openai-codex", "gpt-6.1-sol"));
        assert!(infer_supports_vision("openrouter", "openai/gpt-6.1-sol"));
        assert!(infer_supports_vision("openai-codex", "gpt-6-sol"));
        assert!(infer_supports_vision("openai-codex", "gpt-6-luna"));
    }

    #[test]
    fn astra_fallback_preserves_image_input() {
        let record = infer_capability("openai-codex", "gpt-6-astra");
        assert!(record.supports_vision);
        assert_eq!(
            record.input_modalities,
            vec![Modality::Text, Modality::Image]
        );
        assert_eq!(record.output_modalities, vec![Modality::Text]);
        assert!(infer_supports_vision("openai", "gpt-6-astra"));
        assert!(infer_supports_vision("openrouter", "openai/gpt-6-astra"));
        assert!(infer_supports_vision("openai", "gpt-6-sol"));
        assert!(infer_supports_vision("openrouter", "openai/gpt-6-luna"));
        assert!(!infer_supports_vision("openai-codex", "gpt-6-unknown"));
    }

    #[test]
    fn native_ios_catalog_distinguishes_vision_models() {
        assert!(infer_supports_vision(
            "medousa-local",
            "gemma-4-e2b-it-4bit"
        ));
        assert!(infer_supports_vision(
            "medousa-local",
            "lfm2.5-vl-1.6b-4bit"
        ));
        assert!(infer_supports_vision(
            "medousa-local",
            "ministral-3-3b-instruct-4bit"
        ));
        assert!(!infer_supports_vision("medousa-local", "qwen3.5-2b-4bit"));
        assert!(!infer_supports_vision("medousa-local", "lfm2.5-2.6b-4bit"));
    }
}
