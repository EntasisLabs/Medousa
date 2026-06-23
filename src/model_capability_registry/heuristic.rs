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
        pricing: None,
        source: "heuristic.fallback".to_string(),
        fetched_at: Utc::now(),
    }
}

pub fn infer_supports_vision(provider: &str, model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    if model.is_empty() {
        return false;
    }

    if model.contains("vision") || model.contains("llava") || model.contains("bakllava") {
        return true;
    }

    match provider {
        "openai" | "azure-openai" | "azure_openai" | "openrouter" => {
            model.starts_with("gpt-4o")
                || model.starts_with("gpt-4.1")
                || model.starts_with("gpt-4-turbo")
                || model.starts_with("gpt-5")
                || model.contains("vision")
                || model.contains("openai/gpt-4o")
                || model.contains("openai/gpt-4.1")
                || model.contains("openai/gpt-5")
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
        _ => false,
    }
}
