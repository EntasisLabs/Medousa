use std::time::Duration;

use chrono::Utc;
use reqwest::Client;
use serde_json::Value;

use super::heuristic::infer_supports_vision;
use super::types::{Modality, ModelCapabilityRecord, ModelPricing, ProviderCatalogSnapshot};

const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const ANTHROPIC_MODELS_URL: &str = "https://api.anthropic.com/v1/models";
const MISTRAL_MODELS_URL: &str = "https://api.mistral.ai/v1/models";
const OLLAMA_TAGS_URL: &str = "http://127.0.0.1:11434/api/tags";

pub fn http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|err| err.to_string())
}

pub async fn fetch_provider_catalog(
    client: &Client,
    provider: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
    openrouter_overlay: Option<&[ModelCapabilityRecord]>,
) -> ProviderCatalogSnapshot {
    let provider = provider.trim().to_ascii_lowercase();
    let fetched_at = Utc::now();
    let result = match provider.as_str() {
        "openrouter" => fetch_openrouter(client).await,
        "anthropic" => fetch_anthropic(client, api_key).await,
        "google" | "gemini" | "google-gemini" => fetch_google(client, api_key).await,
        "mistral" => fetch_mistral(client, api_key).await,
        "ollama" | "local" => fetch_ollama(client).await,
        other => fetch_openai_compatible(client, other, api_key, base_url, openrouter_overlay).await,
    };

    match result {
        Ok((source, models)) => ProviderCatalogSnapshot {
            provider: provider.clone(),
            fetched_at,
            source,
            models,
            error: None,
        },
        Err(err) => ProviderCatalogSnapshot {
            provider: provider.clone(),
            fetched_at,
            source: "fetch.failed".to_string(),
            models: Vec::new(),
            error: Some(err),
        },
    }
}

async fn fetch_openrouter(client: &Client) -> Result<(String, Vec<ModelCapabilityRecord>), String> {
    let response = client
        .get(OPENROUTER_MODELS_URL)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("OpenRouter returned HTTP {}", response.status()));
    }
    let payload: Value = response.json().await.map_err(|err| err.to_string())?;
    let models = payload
        .get("data")
        .and_then(|data| data.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(parse_openrouter_model)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(("openrouter.models".to_string(), models))
}

fn parse_openrouter_model(entry: &Value) -> Option<ModelCapabilityRecord> {
    let model_id = entry.get("id")?.as_str()?.trim().to_string();
    if model_id.is_empty() {
        return None;
    }
    let display_name = entry
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let input_modalities = parse_modalities(
        entry
            .get("architecture")
            .and_then(|arch| arch.get("input_modalities")),
    );
    let output_modalities = parse_modalities(
        entry
            .get("architecture")
            .and_then(|arch| arch.get("output_modalities")),
    );
    let supports_vision = input_modalities.contains(&Modality::Image)
        || infer_supports_vision("openrouter", &model_id);
    let max_input_tokens = entry
        .get("context_length")
        .and_then(json_u64)
        .or_else(|| {
            entry
                .get("top_provider")
                .and_then(|top| top.get("context_length"))
                .and_then(json_u64)
        });
    let max_output_tokens = entry
        .get("top_provider")
        .and_then(|top| top.get("max_completion_tokens"))
        .and_then(json_u64);
    let pricing = entry.get("pricing").map(|pricing| ModelPricing {
        prompt_per_token_usd: pricing
            .get("prompt")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok()),
        completion_per_token_usd: pricing
            .get("completion")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok()),
        image_per_unit_usd: pricing
            .get("image")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok()),
    });
    Some(ModelCapabilityRecord {
        provider: "openrouter".to_string(),
        model_id,
        display_name,
        input_modalities,
        output_modalities,
        max_input_tokens,
        max_output_tokens,
        supports_tool_calling: entry
            .get("supported_parameters")
            .and_then(|params| params.as_array())
            .map(|params| params.iter().any(|value| value.as_str() == Some("tools"))),
        supports_vision,
        pricing,
        source: "openrouter.models".to_string(),
        fetched_at: Utc::now(),
    })
}

async fn fetch_anthropic(
    client: &Client,
    api_key: Option<&str>,
) -> Result<(String, Vec<ModelCapabilityRecord>), String> {
    let api_key = api_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Anthropic API key required".to_string())?;
    let response = client
        .get(ANTHROPIC_MODELS_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Anthropic returned HTTP {}", response.status()));
    }
    let payload: Value = response.json().await.map_err(|err| err.to_string())?;
    let models = payload
        .get("data")
        .or_else(|| payload.get("models"))
        .and_then(|data| data.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(parse_anthropic_model)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(("anthropic.models".to_string(), models))
}

fn parse_anthropic_model(entry: &Value) -> Option<ModelCapabilityRecord> {
    let model_id = entry
        .get("id")
        .or_else(|| entry.get("name"))
        .and_then(|value| value.as_str())?
        .trim()
        .to_string();
    if model_id.is_empty() {
        return None;
    }
    let vision_supported = entry
        .get("capabilities")
        .and_then(|caps| caps.get("image_input"))
        .and_then(|value| value.get("supported"))
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| infer_supports_vision("anthropic", &model_id));
    let input_modalities = if vision_supported {
        vec![Modality::Text, Modality::Image]
    } else {
        vec![Modality::Text]
    };
    Some(ModelCapabilityRecord {
        provider: "anthropic".to_string(),
        model_id: model_id.clone(),
        display_name: entry
            .get("display_name")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        input_modalities,
        output_modalities: vec![Modality::Text],
        max_input_tokens: entry.get("max_input_tokens").and_then(json_u64),
        max_output_tokens: entry.get("max_tokens").and_then(json_u64),
        supports_tool_calling: None,
        supports_vision: vision_supported,
        pricing: None,
        source: "anthropic.models".to_string(),
        fetched_at: Utc::now(),
    })
}

async fn fetch_google(
    client: &Client,
    api_key: Option<&str>,
) -> Result<(String, Vec<ModelCapabilityRecord>), String> {
    let api_key = api_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Google API key required".to_string())?;
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={}",
        urlencoding::encode(api_key)
    );
    let response = client.get(url).send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Google returned HTTP {}", response.status()));
    }
    let payload: Value = response.json().await.map_err(|err| err.to_string())?;
    let models = payload
        .get("models")
        .and_then(|models| models.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(parse_google_model)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(("google.models".to_string(), models))
}

fn parse_google_model(entry: &Value) -> Option<ModelCapabilityRecord> {
    let name = entry.get("name")?.as_str()?.trim();
    let model_id = name.trim_start_matches("models/").to_string();
    if model_id.is_empty() {
        return None;
    }
    let methods = entry
        .get("supportedGenerationMethods")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !methods.iter().any(|method| method.contains("generateContent")) {
        return None;
    }
    let supports_vision = model_id.to_ascii_lowercase().contains("gemini");
    Some(ModelCapabilityRecord {
        provider: "google".to_string(),
        model_id: model_id.clone(),
        display_name: entry
            .get("displayName")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        input_modalities: if supports_vision {
            vec![Modality::Text, Modality::Image]
        } else {
            vec![Modality::Text]
        },
        output_modalities: vec![Modality::Text],
        max_input_tokens: entry.get("inputTokenLimit").and_then(json_u64),
        max_output_tokens: entry.get("outputTokenLimit").and_then(json_u64),
        supports_tool_calling: None,
        supports_vision,
        pricing: None,
        source: "google.models".to_string(),
        fetched_at: Utc::now(),
    })
}

async fn fetch_mistral(
    client: &Client,
    api_key: Option<&str>,
) -> Result<(String, Vec<ModelCapabilityRecord>), String> {
    let api_key = api_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Mistral API key required".to_string())?;
    let response = client
        .get(MISTRAL_MODELS_URL)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Mistral returned HTTP {}", response.status()));
    }
    let payload: Value = response.json().await.map_err(|err| err.to_string())?;
    let models = payload
        .get("data")
        .and_then(|data| data.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(parse_mistral_model)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(("mistral.models".to_string(), models))
}

fn parse_mistral_model(entry: &Value) -> Option<ModelCapabilityRecord> {
    let model_id = entry.get("id")?.as_str()?.trim().to_string();
    if model_id.is_empty() {
        return None;
    }
    let vision_supported = entry
        .get("capabilities")
        .and_then(|caps| caps.get("vision"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    Some(ModelCapabilityRecord {
        provider: "mistral".to_string(),
        model_id: model_id.clone(),
        display_name: None,
        input_modalities: if vision_supported {
            vec![Modality::Text, Modality::Image]
        } else {
            vec![Modality::Text]
        },
        output_modalities: vec![Modality::Text],
        max_input_tokens: entry.get("max_context_length").and_then(json_u64),
        max_output_tokens: None,
        supports_tool_calling: entry
            .get("capabilities")
            .and_then(|caps| caps.get("function_calling"))
            .and_then(|value| value.as_bool()),
        supports_vision: vision_supported,
        pricing: None,
        source: "mistral.models".to_string(),
        fetched_at: Utc::now(),
    })
}

async fn fetch_ollama(client: &Client) -> Result<(String, Vec<ModelCapabilityRecord>), String> {
    let response = client
        .get(OLLAMA_TAGS_URL)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Ollama returned HTTP {}", response.status()));
    }
    let payload: Value = response.json().await.map_err(|err| err.to_string())?;
    let models = payload
        .get("models")
        .and_then(|models| models.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let model_id = entry.get("name")?.as_str()?.trim().to_string();
                    if model_id.is_empty() {
                        return None;
                    }
                    let supports_vision = infer_supports_vision("ollama", &model_id);
                    Some(ModelCapabilityRecord {
                        provider: "ollama".to_string(),
                        model_id: model_id.clone(),
                        display_name: Some(model_id),
                        input_modalities: if supports_vision {
                            vec![Modality::Text, Modality::Image]
                        } else {
                            vec![Modality::Text]
                        },
                        output_modalities: vec![Modality::Text],
                        max_input_tokens: None,
                        max_output_tokens: None,
                        supports_tool_calling: None,
                        supports_vision,
                        pricing: None,
                        source: "ollama.tags".to_string(),
                        fetched_at: Utc::now(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(("ollama.tags".to_string(), models))
}

async fn fetch_openai_compatible(
    client: &Client,
    provider: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
    openrouter_overlay: Option<&[ModelCapabilityRecord]>,
) -> Result<(String, Vec<ModelCapabilityRecord>), String> {
    let base = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| default_openai_compatible_base(provider));
    let Some(base) = base else {
        return Err(format!("No API base URL configured for provider '{provider}'"));
    };
    let url = models_url(&base);
    let mut request = client.get(url);
    if let Some(key) = api_key.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("{provider} returned HTTP {}", response.status()));
    }
    let payload: Value = response.json().await.map_err(|err| err.to_string())?;
    let models = payload
        .get("data")
        .and_then(|data| data.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    parse_openai_compatible_model(provider, entry, openrouter_overlay)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(("openai_compatible.models".to_string(), models))
}

fn parse_openai_compatible_model(
    provider: &str,
    entry: &Value,
    openrouter_overlay: Option<&[ModelCapabilityRecord]>,
) -> Option<ModelCapabilityRecord> {
    let model_id = entry.get("id")?.as_str()?.trim().to_string();
    if model_id.is_empty() {
        return None;
    }
    let overlay = openrouter_overlay.and_then(|records| {
        records.iter().find(|record| {
            record.model_id.eq_ignore_ascii_case(&model_id)
                || record.model_id.ends_with(&format!("/{model_id}"))
        })
    });
    let supports_vision = overlay.map(|record| record.supports_vision).unwrap_or_else(|| {
        infer_supports_vision(provider, &model_id)
    });
    Some(ModelCapabilityRecord {
        provider: provider.to_string(),
        model_id: model_id.clone(),
        display_name: None,
        input_modalities: if supports_vision {
            vec![Modality::Text, Modality::Image]
        } else {
            vec![Modality::Text]
        },
        output_modalities: vec![Modality::Text],
        max_input_tokens: overlay.and_then(|record| record.max_input_tokens),
        max_output_tokens: overlay.and_then(|record| record.max_output_tokens),
        supports_tool_calling: overlay.and_then(|record| record.supports_tool_calling),
        supports_vision,
        pricing: overlay.and_then(|record| record.pricing.clone()),
        source: overlay
            .map(|record| format!("{}+openrouter.overlay", record.source))
            .unwrap_or_else(|| "openai_compatible.models".to_string()),
        fetched_at: Utc::now(),
    })
}

fn default_openai_compatible_base(provider: &str) -> Option<String> {
    match provider {
        "openai" => Some("https://api.openai.com/v1".to_string()),
        "openrouter" => Some("https://openrouter.ai/api/v1".to_string()),
        "deepseek" => Some("https://api.deepseek.com/v1".to_string()),
        "groq" => Some("https://api.groq.com/openai/v1".to_string()),
        "together" => Some("https://api.together.xyz/v1".to_string()),
        "fireworks" => Some("https://api.fireworks.ai/inference/v1".to_string()),
        "xai" => Some("https://api.x.ai/v1".to_string()),
        "perplexity" => Some("https://api.perplexity.ai".to_string()),
        "ollama" | "local" => Some("http://127.0.0.1:11434/v1/".to_string()),
        _ => None,
    }
}

fn models_url(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
}

fn parse_modalities(value: Option<&Value>) -> Vec<Modality> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return vec![Modality::Text];
    };
    let mut out = Vec::new();
    for item in items {
        let Some(raw) = item.as_str() else {
            continue;
        };
        match raw.trim().to_ascii_lowercase().as_str() {
            "text" => out.push(Modality::Text),
            "image" => out.push(Modality::Image),
            "audio" => out.push(Modality::Audio),
            "file" => out.push(Modality::File),
            "video" => out.push(Modality::Video),
            _ => {}
        }
    }
    if out.is_empty() {
        out.push(Modality::Text);
    }
    out
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().filter(|value| *value >= 0).map(|value| value as u64))
        .or_else(|| value.as_f64().map(|value| value as u64))
}

pub fn openrouter_slug_for(provider: &str, model: &str) -> String {
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim();
    if model.contains('/') {
        return model.to_string();
    }
    format!("{provider}/{model}")
}
