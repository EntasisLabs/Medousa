//! Curated LLM provider catalog for Home — the shared list lives in
//! `medousa_desktop_host::provider_catalog`; the embedded filter stays here.

pub use medousa_desktop_host::provider_catalog::*;

pub fn providers_catalog_for_embedded() -> ProvidersListResult {
    let mut catalog = providers_catalog();
    catalog.providers.retain(|entry| {
        entry.id == "custom"
            || entry.id == "openai-codex"
            || (cfg!(target_os = "ios") && entry.id == "medousa-local")
            || medousa_runtime::CredentialedAiChatConfig::new(
                entry.id.clone(),
                entry.default_model.clone(),
                entry.default_base_url.clone(),
            )
            .is_ok()
    });
    #[cfg(target_os = "ios")]
    if let Some(local) = catalog
        .providers
        .iter_mut()
        .find(|entry| entry.id == "medousa-local")
    {
        local.default_model = "gemma-4-e2b-it-4bit".to_string();
        local.supports_custom_base_url = false;
        local.default_base_url = None;
        local.blurb = "Private MLX models running entirely on this device".to_string();
    }
    catalog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_uses_portable_runtime_route_admission() {
        let catalog = providers_catalog_for_embedded();
        for provider in [
            "openai",
            "openai-codex",
            "anthropic",
            "google",
            "deepseek",
            "groq",
            "xai",
            "ollama",
            "openrouter",
            "custom",
        ] {
            assert!(
                catalog.providers.iter().any(|entry| entry.id == provider),
                "missing embedded provider {provider}"
            );
        }
        for provider in ["medousa-local", "replicate", "bedrock"] {
            assert!(
                !catalog.providers.iter().any(|entry| entry.id == provider),
                "host-only provider leaked into embedded catalog: {provider}"
            );
        }
    }
}
