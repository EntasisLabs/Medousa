use crate::session::{load_tui_defaults, load_provider_api_key};
use crate::tui::settings::parse_env_overrides;
use crate::resolve_llm_provider;

/// Apply workshop LLM credentials to the current process (daemon / runtime).
///
/// Loads `env_overrides` from `tui_defaults.json` and maps the stored workshop API key
/// to provider env vars (`DEEPSEEK_API_KEY`, `STASIS_DEEPSEEK_API_KEY`, etc.).
pub fn apply_workshop_llm_env() {
    let defaults = load_tui_defaults();
    apply_env_overrides(defaults.env_overrides.as_deref());

    let provider = resolve_llm_provider(defaults.provider.as_deref());
    apply_provider_llm_env(&provider);
}

/// Inject only the API key for the active inference attempt's provider.
pub fn apply_provider_llm_env(provider: &str) {
    if let Some(key) = load_provider_api_key(provider) {
        inject_provider_api_key_env(provider, &key);
    }
}

fn apply_env_overrides(raw: Option<&str>) {
    if let Some(raw) = raw {
        for (key, value) in parse_env_overrides(raw) {
            if value.is_empty() {
                unsafe { std::env::remove_var(&key) };
            } else {
                unsafe { std::env::set_var(&key, &value) };
            }
        }
    }
}

pub(crate) fn inject_provider_api_key_env(provider: &str, key: &str) {
    let normalized = provider.trim().to_ascii_uppercase().replace('-', "_");
    unsafe {
        std::env::set_var(format!("STASIS_{normalized}_API_KEY"), key);
        std::env::set_var(format!("MEDOUSA_{normalized}_API_KEY"), key);
        std::env::set_var("STASIS_LLM_API_KEY", key);
    }

    match provider.trim().to_ascii_lowercase().as_str() {
        "deepseek" => unsafe { std::env::set_var("DEEPSEEK_API_KEY", key) },
        "openai" => unsafe { std::env::set_var("OPENAI_API_KEY", key) },
        "anthropic" => unsafe { std::env::set_var("ANTHROPIC_API_KEY", key) },
        "gemini" | "google" => unsafe { std::env::set_var("GEMINI_API_KEY", key) },
        "groq" => unsafe { std::env::set_var("GROQ_API_KEY", key) },
        "xai" => unsafe { std::env::set_var("XAI_API_KEY", key) },
        "mistral" => unsafe { std::env::set_var("MISTRAL_API_KEY", key) },
        "cohere" => unsafe { std::env::set_var("COHERE_API_KEY", key) },
        "perplexity" => unsafe { std::env::set_var("PERPLEXITY_API_KEY", key) },
        "together" => unsafe { std::env::set_var("TOGETHER_API_KEY", key) },
        "fireworks" => unsafe { std::env::set_var("FIREWORKS_API_KEY", key) },
        "openrouter" => unsafe { std::env::set_var("OPENROUTER_API_KEY", key) },
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::inject_provider_api_key_env;

    #[test]
    fn inject_deepseek_sets_provider_env_names() {
        inject_provider_api_key_env("deepseek", "sk-test");
        assert_eq!(std::env::var("DEEPSEEK_API_KEY").ok(), Some("sk-test".to_string()));
        assert_eq!(
            std::env::var("STASIS_DEEPSEEK_API_KEY").ok(),
            Some("sk-test".to_string())
        );
    }
}
