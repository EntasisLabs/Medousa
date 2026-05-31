use crate::daemon_api::InteractiveTurnRequest;
use crate::tui::settings::RuntimeSettings;

/// Default runtime settings for daemon-hosted agent turns.
pub fn default_daemon_runtime_settings(
    backend: &str,
    provider: &str,
    model: &str,
    base_url: &str,
) -> RuntimeSettings {
    RuntimeSettings {
        backend: backend.to_string(),
        theme_id: "medousa-default".to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        base_url: base_url.to_string(),
        env_overrides: String::new(),
        api_key: String::new(),
        allowed_modules: String::new(),
        tool_call_mode: "auto".to_string(),
        max_tool_rounds: "10".to_string(),
        thinking_capture: "true".to_string(),
        thinking_max_lines: "300".to_string(),
        activation_direct_answer_max_prompt_chars: "320".to_string(),
        activation_long_session_turn_threshold: "28".to_string(),
        activation_long_session_max_prompt_chars: "420".to_string(),
        slice_hot_window_turns: "8".to_string(),
        slice_cold_window_turns: "24".to_string(),
        retry_runtime_max_retries: "1".to_string(),
        retry_runtime_max_rounds: "3".to_string(),
        verifier_min_citation_coverage: "0.60".to_string(),
        verifier_min_avg_support_strength: "0.70".to_string(),
        verifier_min_supported_claim_ratio: "0.60".to_string(),
        verifier_min_claim_support_strength: "0.65".to_string(),
    }
}

/// Merge an interactive turn request into daemon runtime settings.
pub fn runtime_settings_for_interactive_turn(
    backend: &str,
    request: &InteractiveTurnRequest,
) -> RuntimeSettings {
    let provider = crate::resolve_llm_provider(Some(request.provider.trim()));
    let model = crate::resolve_llm_model(Some(request.model.trim()));
    let base_url = crate::resolve_llm_base_url(Some(&provider), None)
        .unwrap_or_default();
    let mut settings = default_daemon_runtime_settings(backend, &provider, &model, &base_url);
    settings.provider = provider;
    settings.model = model;
    settings.base_url = base_url;
    settings
}
