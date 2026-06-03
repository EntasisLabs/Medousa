use crate::daemon_api::InteractiveTurnRequest;
use crate::session::TuiDefaults;
use crate::tui::settings::RuntimeSettings;

use super::turn_loop_settings::apply_turn_loop_field_defaults;
use super::turn_orchestrator::DEFAULT_RETRY_RUNTIME_MAX_ROUNDS;

const DEFAULT_MAX_TOOL_ROUNDS: usize = 10;

fn apply_tui_defaults_to_runtime_settings(settings: &mut RuntimeSettings, defaults: &TuiDefaults) {
    if let Some(value) = defaults
        .theme_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.theme_id = value.to_string();
    }
    if let Some(value) = defaults
        .env_overrides
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.env_overrides = value.to_string();
    }
    if let Some(modules) = defaults.allowed_modules.as_ref() {
        settings.allowed_modules = modules.join(",");
    }
    if let Some(value) = defaults
        .tool_call_mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.tool_call_mode = value.to_string();
    }
    settings.max_tool_rounds = defaults
        .max_tool_rounds
        .unwrap_or(DEFAULT_MAX_TOOL_ROUNDS)
        .to_string();
    if let Some(value) = defaults.thinking_capture {
        settings.thinking_capture = value.to_string();
    }
    if let Some(value) = defaults.thinking_max_lines {
        settings.thinking_max_lines = value.to_string();
    }
    if let Some(value) = defaults.activation_direct_answer_max_prompt_chars {
        settings.activation_direct_answer_max_prompt_chars = value.to_string();
    }
    if let Some(value) = defaults.activation_long_session_turn_threshold {
        settings.activation_long_session_turn_threshold = value.to_string();
    }
    if let Some(value) = defaults.activation_long_session_max_prompt_chars {
        settings.activation_long_session_max_prompt_chars = value.to_string();
    }
    if let Some(value) = defaults.slice_hot_window_turns {
        settings.slice_hot_window_turns = value.to_string();
    }
    if let Some(value) = defaults.slice_cold_window_turns {
        settings.slice_cold_window_turns = value.to_string();
    }
    if let Some(value) = defaults.retry_runtime_max_retries {
        settings.retry_runtime_max_retries = value.to_string();
    }
    settings.retry_runtime_max_rounds = defaults
        .retry_runtime_max_rounds
        .unwrap_or(DEFAULT_RETRY_RUNTIME_MAX_ROUNDS)
        .to_string();
    if let Some(value) = defaults.host_bus_max_tool_rounds {
        settings.host_bus_max_tool_rounds = value.to_string();
    }
    if let Some(value) = defaults
        .host_turn_bus_mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.host_turn_bus_mode = value.to_string();
    }
    if let Some(value) = defaults.activation_tool_intent_max_rounds {
        settings.activation_tool_intent_max_rounds = value.to_string();
    }
    if let Some(value) = defaults.activation_short_turn_max_tool_rounds {
        settings.activation_short_turn_max_tool_rounds = value.to_string();
    }
    if let Some(value) = defaults.continuation_max_tool_rounds {
        settings.continuation_max_tool_rounds = value.to_string();
    }
    if let Some(value) = defaults.max_text_only_stuck_continues {
        settings.max_text_only_stuck_continues = value.to_string();
    }
    if let Some(value) = defaults.classifier_restricted_max_tool_rounds {
        settings.classifier_restricted_max_tool_rounds = value.to_string();
    }
    apply_turn_loop_field_defaults(settings);
    if let Some(value) = defaults.verifier_min_citation_coverage {
        settings.verifier_min_citation_coverage = format!("{value:.2}");
    }
    if let Some(value) = defaults.verifier_min_avg_support_strength {
        settings.verifier_min_avg_support_strength = format!("{value:.2}");
    }
    if let Some(value) = defaults.verifier_min_supported_claim_ratio {
        settings.verifier_min_supported_claim_ratio = format!("{value:.2}");
    }
    if let Some(value) = defaults.verifier_min_claim_support_strength {
        settings.verifier_min_claim_support_strength = format!("{value:.2}");
    }
}

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
        host_bus_max_tool_rounds: super::turn_loop_settings::DEFAULT_HOST_BUS_MAX_TOOL_ROUNDS
            .to_string(),
        host_turn_bus_mode: super::turn_loop_settings::default_host_turn_bus_mode_label()
            .to_string(),
        activation_tool_intent_max_rounds:
            super::turn_loop_settings::DEFAULT_ACTIVATION_TOOL_INTENT_MAX_ROUNDS.to_string(),
        activation_short_turn_max_tool_rounds:
            super::turn_loop_settings::DEFAULT_ACTIVATION_SHORT_TURN_MAX_TOOL_ROUNDS.to_string(),
        continuation_max_tool_rounds:
            super::turn_loop_settings::DEFAULT_CONTINUATION_MAX_TOOL_ROUNDS.to_string(),
        max_text_only_stuck_continues:
            super::turn_loop_settings::DEFAULT_MAX_TEXT_ONLY_STUCK_CONTINUES.to_string(),
        classifier_restricted_max_tool_rounds:
            super::turn_loop_settings::DEFAULT_CLASSIFIER_RESTRICTED_MAX_TOOL_ROUNDS.to_string(),
        thinking_capture: "true".to_string(),
        thinking_max_lines: "300".to_string(),
        activation_direct_answer_max_prompt_chars: "320".to_string(),
        activation_long_session_turn_threshold: "28".to_string(),
        activation_long_session_max_prompt_chars: "420".to_string(),
        slice_hot_window_turns: "8".to_string(),
        slice_cold_window_turns: "24".to_string(),
        retry_runtime_max_retries: "1".to_string(),
        retry_runtime_max_rounds: "10".to_string(),
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
    let base_url = crate::resolve_llm_base_url(Some(&provider), None).unwrap_or_default();
    let mut settings = default_daemon_runtime_settings(backend, &provider, &model, &base_url);
    apply_tui_defaults_to_runtime_settings(
        &mut settings,
        &crate::session::load_tui_defaults(),
    );
    settings.provider = provider;
    settings.model = model;
    settings.base_url = base_url;
    if let Some(value) = request.max_tool_rounds {
        settings.max_tool_rounds = value.to_string();
    }
    if let Some(value) = request.retry_runtime_max_rounds {
        settings.retry_runtime_max_rounds = value.to_string();
    }
    apply_turn_loop_field_defaults(&mut settings);
    settings
}
