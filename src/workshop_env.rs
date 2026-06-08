use crate::session::{load_tui_api_key, load_tui_defaults};
use crate::tui::settings::parse_env_overrides;
use crate::resolve_llm_provider;

/// Apply workshop LLM credentials to the current process (daemon / runtime).
///
/// Loads `env_overrides` from `tui_defaults.json` and maps the stored workshop API key
/// to provider env vars (`DEEPSEEK_API_KEY`, `STASIS_DEEPSEEK_API_KEY`, etc.).
pub fn apply_workshop_llm_env() {
    let defaults = load_tui_defaults();
    if let Some(raw) = defaults.env_overrides.as_deref() {
        for (key, value) in parse_env_overrides(raw) {
            if value.is_empty() {
                unsafe { std::env::remove_var(&key) };
            } else {
                unsafe { std::env::set_var(&key, &value) };
            }
        }
    }

    let provider = resolve_llm_provider(defaults.provider.as_deref());
    if let Some(key) = load_tui_api_key() {
        inject_provider_api_key_env(&provider, &key);
    }
}

fn inject_provider_api_key_env(provider: &str, key: &str) {
    let normalized = provider.trim().to_ascii_uppercase().replace('-', "_");
    unsafe {
        std::env::set_var(format!("STASIS_{normalized}_API_KEY"), key);
        std::env::set_var("STASIS_LLM_API_KEY", key);
    }

    match provider.trim().to_ascii_lowercase().as_str() {
        "deepseek" => unsafe { std::env::set_var("DEEPSEEK_API_KEY", key) },
        "openai" => unsafe { std::env::set_var("OPENAI_API_KEY", key) },
        "anthropic" => unsafe { std::env::set_var("ANTHROPIC_API_KEY", key) },
        "gemini" | "google" => unsafe { std::env::set_var("GEMINI_API_KEY", key) },
        "groq" => unsafe { std::env::set_var("GROQ_API_KEY", key) },
        "xai" => unsafe { std::env::set_var("XAI_API_KEY", key) },
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
