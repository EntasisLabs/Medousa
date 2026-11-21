const DEFAULT_LLM_MODEL: &str = "gpt-4o-mini";
const DEFAULT_LLM_PROVIDER: &str = "openai";
pub const DEFAULT_MEDOUSA_LOCAL_BASE_URL: &str = "http://127.0.0.1:7421/v1";

fn provider_base_url_env_keys(provider: &str) -> (String, String) {
    let normalized = provider.trim().to_ascii_uppercase().replace('-', "_");
    (
        format!("MEDOUSA_{normalized}_BASE_URL"),
        format!("STASIS_{normalized}_BASE_URL"),
    )
}

pub fn resolve_llm_model(explicit_model: Option<&str>) -> String {
    explicit_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var("MEDOUSA_LLM_MODEL").ok())
        .or_else(|| std::env::var("STASIS_LLM_MODEL").ok())
        .unwrap_or_else(|| DEFAULT_LLM_MODEL.to_string())
}

pub fn resolve_llm_provider(explicit_provider: Option<&str>) -> String {
    explicit_provider
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var("MEDOUSA_LLM_PROVIDER").ok())
        .or_else(|| std::env::var("STASIS_LLM_PROVIDER").ok())
        .unwrap_or_else(|| DEFAULT_LLM_PROVIDER.to_string())
}

pub fn resolve_llm_base_url(
    explicit_provider: Option<&str>,
    explicit_base_url: Option<&str>,
) -> Option<String> {
    if let Some(explicit) = explicit_base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(explicit.to_string());
    }

    let provider = resolve_llm_provider(explicit_provider);
    let (medousa_provider_key, stasis_provider_key) = provider_base_url_env_keys(&provider);

    if provider.eq_ignore_ascii_case("medousa-local") {
        return std::env::var(&medousa_provider_key)
            .ok()
            .or_else(|| std::env::var(&stasis_provider_key).ok())
            .or_else(|| std::env::var("MEDOUSA_LOCAL_ENGINE_BASE_URL").ok())
            .or_else(|| Some(DEFAULT_MEDOUSA_LOCAL_BASE_URL.to_string()));
    }

    std::env::var(&medousa_provider_key)
        .ok()
        .or_else(|| std::env::var(&stasis_provider_key).ok())
        .or_else(|| {
            // Honour the standard Ollama env var so users don't need a separate flag.
            if provider.eq_ignore_ascii_case("ollama") {
                std::env::var("OLLAMA_HOST").ok()
            } else {
                None
            }
        })
        .or_else(|| std::env::var("MEDOUSA_LLM_BASE_URL").ok())
        .or_else(|| std::env::var("STASIS_LLM_BASE_URL").ok())
}
