use crate::settings_guard::{invalid_module_ids, parse_allowed_modules};

/// Minimum tool-loop / retry round counts (operator settings).
pub const OPERATOR_ROUND_LIMIT_MIN: usize = 1;
/// Hard in-process safety ceiling for tool-loop rounds (memory/API), not usage policing.
pub const OPERATOR_ROUND_LIMIT_MAX: usize = 4096;
pub const OPERATOR_RETRY_LIMIT_MIN: usize = 0;
pub const OPERATOR_RETRY_LIMIT_MAX: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSettings {
    pub backend: String,
    pub theme_id: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub env_overrides: String,
    pub api_key: String,
    pub allowed_modules: String,
    pub tool_call_mode: String,
    pub max_tool_rounds: String,
    pub host_bus_max_tool_rounds: String,
    pub host_turn_bus_mode: String,
    pub activation_tool_intent_max_rounds: String,
    pub activation_short_turn_max_tool_rounds: String,
    pub continuation_max_tool_rounds: String,
    pub max_text_only_stuck_continues: String,
    pub classifier_restricted_max_tool_rounds: String,
    pub thinking_capture: String,
    pub stasis_otel_enabled: String,
    pub thinking_max_lines: String,
    pub activation_direct_answer_max_prompt_chars: String,
    pub activation_long_session_turn_threshold: String,
    pub activation_long_session_max_prompt_chars: String,
    pub slice_hot_window_turns: String,
    pub slice_cold_window_turns: String,
    pub retry_runtime_max_retries: String,
    pub retry_runtime_max_rounds: String,
    pub verifier_min_citation_coverage: String,
    pub verifier_min_avg_support_strength: String,
    pub verifier_min_supported_claim_ratio: String,
    pub verifier_min_claim_support_strength: String,
}

pub fn settings_validation_errors(settings: &RuntimeSettings) -> Vec<String> {
    let mut errors = Vec::new();
    let allowed_modules = parse_allowed_modules(&settings.allowed_modules);
    let invalid_modules = invalid_module_ids(&allowed_modules);
    if !invalid_modules.is_empty() {
        errors.push(format!(
            "unrecognized tool names: {}",
            invalid_modules.join(", ")
        ));
    }

    let env_errors = env_overrides_validation_errors(&settings.env_overrides);
    errors.extend(env_errors);

    validate_unit_interval(
        "source coverage",
        &settings.verifier_min_citation_coverage,
        &mut errors,
    );
    validate_unit_interval(
        "average source strength",
        &settings.verifier_min_avg_support_strength,
        &mut errors,
    );
    validate_unit_interval(
        "share of claims backed up",
        &settings.verifier_min_supported_claim_ratio,
        &mut errors,
    );
    validate_unit_interval(
        "strength per claim",
        &settings.verifier_min_claim_support_strength,
        &mut errors,
    );

    validate_usize_range(
        "short question size limit",
        &settings.activation_direct_answer_max_prompt_chars,
        64,
        4000,
        &mut errors,
    );
    validate_usize_range(
        "long chat starts after (turns)",
        &settings.activation_long_session_turn_threshold,
        8,
        500,
        &mut errors,
    );
    validate_usize_range(
        "long chat size limit",
        &settings.activation_long_session_max_prompt_chars,
        64,
        4000,
        &mut errors,
    );
    validate_usize_range(
        "recent messages kept",
        &settings.slice_hot_window_turns,
        2,
        32,
        &mut errors,
    );
    validate_usize_range(
        "older messages summarized",
        &settings.slice_cold_window_turns,
        4,
        128,
        &mut errors,
    );
    validate_usize_range(
        "retry whole turn on error",
        &settings.retry_runtime_max_retries,
        OPERATOR_RETRY_LIMIT_MIN,
        OPERATOR_RETRY_LIMIT_MAX,
        &mut errors,
    );
    validate_usize_range(
        "retry tool steps on error",
        &settings.retry_runtime_max_rounds,
        OPERATOR_ROUND_LIMIT_MIN,
        OPERATOR_ROUND_LIMIT_MAX,
        &mut errors,
    );
    validate_usize_range(
        "tool steps per reply",
        &settings.max_tool_rounds,
        OPERATOR_ROUND_LIMIT_MIN,
        OPERATOR_ROUND_LIMIT_MAX,
        &mut errors,
    );
    validate_usize_range(
        "tool steps (full mode)",
        &settings.host_bus_max_tool_rounds,
        OPERATOR_ROUND_LIMIT_MIN,
        OPERATOR_ROUND_LIMIT_MAX,
        &mut errors,
    );
    validate_host_turn_bus_mode(&settings.host_turn_bus_mode, &mut errors);
    validate_usize_range(
        "extra tool steps (big tasks)",
        &settings.activation_tool_intent_max_rounds,
        OPERATOR_ROUND_LIMIT_MIN,
        OPERATOR_ROUND_LIMIT_MAX,
        &mut errors,
    );
    validate_usize_range(
        "tool steps (quick questions)",
        &settings.activation_short_turn_max_tool_rounds,
        OPERATOR_ROUND_LIMIT_MIN,
        OPERATOR_ROUND_LIMIT_MAX,
        &mut errors,
    );
    validate_usize_range(
        "extra steps when wrapping up",
        &settings.continuation_max_tool_rounds,
        OPERATOR_ROUND_LIMIT_MIN,
        OPERATOR_ROUND_LIMIT_MAX,
        &mut errors,
    );
    validate_usize_range(
        "retries when stuck (no tools)",
        &settings.max_text_only_stuck_continues,
        OPERATOR_ROUND_LIMIT_MIN,
        OPERATOR_ROUND_LIMIT_MAX,
        &mut errors,
    );
    validate_usize_range(
        "tool steps (simple requests)",
        &settings.classifier_restricted_max_tool_rounds,
        OPERATOR_ROUND_LIMIT_MIN,
        OPERATOR_ROUND_LIMIT_MAX,
        &mut errors,
    );

    let hot = settings.slice_hot_window_turns.trim().parse::<usize>().ok();
    let cold = settings
        .slice_cold_window_turns
        .trim()
        .parse::<usize>()
        .ok();
    if let (Some(hot), Some(cold)) = (hot, cold) {
        if cold < hot {
            errors.push(
                "summarized history must be at least as long as recent history".to_string(),
            );
        }
    }

    errors
}

fn validate_unit_interval(name: &str, value: &str, errors: &mut Vec<String>) {
    let trimmed = value.trim();
    let Ok(parsed) = trimmed.parse::<f32>() else {
        errors.push(format!("{name} must be a number in [0.0, 1.0]"));
        return;
    };
    if !(0.0..=1.0).contains(&parsed) {
        errors.push(format!("{name} must be in [0.0, 1.0]"));
    }
}

fn validate_host_turn_bus_mode(value: &str, errors: &mut Vec<String>) {
    let mode = value.trim().to_ascii_lowercase();
    if !matches!(mode.as_str(), "auto" | "force" | "off") {
        errors.push("tool routing must be auto, force, or off".to_string());
    }
}

fn validate_usize_range(name: &str, value: &str, min: usize, max: usize, errors: &mut Vec<String>) {
    let trimmed = value.trim();
    let Ok(parsed) = trimmed.parse::<usize>() else {
        errors.push(format!("{name} must be a number in [{min}, {max}]"));
        return;
    };
    if !(min..=max).contains(&parsed) {
        errors.push(format!("{name} must be in [{min}, {max}]"));
    }
}

pub fn parse_env_overrides(raw: &str) -> Vec<(String, String)> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

pub fn env_overrides_validation_errors(raw: &str) -> Vec<String> {
    let mut errors = Vec::new();

    for (idx, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((key, _value)) = trimmed.split_once('=') else {
            errors.push(format!(
                "env override line {} must use KEY=VALUE format",
                idx + 1
            ));
            continue;
        };

        let key = key.trim();
        if key.is_empty() {
            errors.push(format!("env override line {} has empty key", idx + 1));
            continue;
        }

        let valid = key
            .chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphanumeric() && !(i == 0 && c.is_ascii_digit()));
        if !valid {
            errors.push(format!(
                "env override line {} has invalid key '{}'; use [A-Z0-9_] and do not start with a digit",
                idx + 1,
                key
            ));
        }
    }

    errors
}

pub fn resolve_backend_name(value: Option<&str>) -> String {
    let raw = value.unwrap_or("surreal-mem").trim();
    if raw.eq_ignore_ascii_case("in-memory") {
        return "in-memory".to_string();
    }
    if raw.eq_ignore_ascii_case("surreal-mem") {
        return "surreal-mem".to_string();
    }
    if raw.eq_ignore_ascii_case("surreal-kv") || raw.starts_with("surreal-kv:") {
        return raw.to_string();
    }
    if raw.eq_ignore_ascii_case("surreal-ws") || raw.starts_with("surreal-ws:") {
        return raw.to_string();
    }

    "surreal-mem".to_string()
}

pub fn resolve_theme_id_name(value: Option<&str>) -> String {
    let trimmed = value.unwrap_or("medousa-default").trim();
    if trimmed.is_empty() {
        "medousa-default".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn cycle_backend(current: &str, forward: bool) -> String {
    let canonical = if current.trim().starts_with("surreal-kv") {
        "surreal-kv"
    } else {
        current
    };
    let choices = ["surreal-mem", "in-memory", "surreal-kv"];
    cycle_choice(canonical, &choices, forward)
}

pub fn resolve_tool_call_mode_name(value: Option<&str>) -> String {
    match value.unwrap_or("auto").trim().to_ascii_lowercase().as_str() {
        "strict" => "strict".to_string(),
        _ => "auto".to_string(),
    }
}

pub fn cycle_tool_call_mode(current: &str, forward: bool) -> String {
    let choices = ["auto", "strict"];
    cycle_choice(current, &choices, forward)
}

pub fn cycle_host_turn_bus_mode(current: &str, forward: bool) -> String {
    let choices = ["auto", "force", "off"];
    cycle_choice(current, &choices, forward)
}

fn cycle_choice(current: &str, choices: &[&str], forward: bool) -> String {
    if choices.is_empty() {
        return current.to_string();
    }

    let idx = choices
        .iter()
        .position(|choice| choice.eq_ignore_ascii_case(current))
        .unwrap_or(0);

    let next = if forward {
        (idx + 1) % choices.len()
    } else if idx == 0 {
        choices.len() - 1
    } else {
        idx - 1
    };

    choices[next].to_string()
}

pub fn resolve_bool_arg(value: Option<&str>, default_value: bool) -> bool {
    value
        .and_then(|raw| match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default_value)
}

pub fn parse_bool_with_default(value: &str, default_value: bool) -> bool {
    resolve_bool_arg(Some(value), default_value)
}

pub fn resolve_usize_arg(
    value: Option<&str>,
    default_value: usize,
    min_value: usize,
    max_value: usize,
) -> usize {
    value
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(default_value)
        .clamp(min_value, max_value)
}

pub fn parse_usize_with_bounds(
    value: &str,
    default_value: usize,
    min_value: usize,
    max_value: usize,
) -> usize {
    resolve_usize_arg(Some(value), default_value, min_value, max_value)
}

pub fn resolve_f32_arg(
    value: Option<&str>,
    default_value: f32,
    min_value: f32,
    max_value: f32,
) -> f32 {
    value
        .and_then(|raw| raw.trim().parse::<f32>().ok())
        .unwrap_or(default_value)
        .clamp(min_value, max_value)
}

pub fn parse_f32_with_bounds(
    value: &str,
    default_value: f32,
    min_value: f32,
    max_value: f32,
) -> f32 {
    resolve_f32_arg(Some(value), default_value, min_value, max_value)
}

#[cfg(test)]
mod tests {
    use super::{RuntimeSettings, cycle_backend, resolve_backend_name, settings_validation_errors};

    #[test]
    fn resolves_backend_with_safe_default() {
        assert_eq!(resolve_backend_name(Some("surreal-mem")), "surreal-mem");
        assert_eq!(resolve_backend_name(Some("surreal-kv")), "surreal-kv");
        assert_eq!(
            resolve_backend_name(Some("surreal-kv:/tmp/medousa/runtime.surrealkv")),
            "surreal-kv:/tmp/medousa/runtime.surrealkv"
        );
        assert_eq!(resolve_backend_name(Some("unknown")), "surreal-mem");
    }

    #[test]
    fn cycles_backend_choices() {
        assert_eq!(cycle_backend("surreal-mem", true), "in-memory");
        assert_eq!(cycle_backend("in-memory", true), "surreal-kv");
        assert_eq!(cycle_backend("surreal-kv", true), "surreal-mem");
    }

    #[test]
    fn validates_allowed_module_format() {
        let settings = RuntimeSettings {
            backend: "surreal-mem".to_string(),
            theme_id: "medousa-default".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: String::new(),
            env_overrides: String::new(),
            api_key: String::new(),
            allowed_modules: "bad id".to_string(),
            tool_call_mode: "auto".to_string(),
            max_tool_rounds: "10".to_string(),
            host_bus_max_tool_rounds: "8".to_string(),
            host_turn_bus_mode: "auto".to_string(),
            activation_tool_intent_max_rounds: "12".to_string(),
            activation_short_turn_max_tool_rounds: "1".to_string(),
            continuation_max_tool_rounds: "4".to_string(),
            max_text_only_stuck_continues: "10".to_string(),
            classifier_restricted_max_tool_rounds: "1".to_string(),
            thinking_capture: "true".to_string(),
            stasis_otel_enabled: "false".to_string(),
            thinking_max_lines: "300".to_string(),
            activation_direct_answer_max_prompt_chars: "320".to_string(),
            activation_long_session_turn_threshold: "28".to_string(),
            activation_long_session_max_prompt_chars: "420".to_string(),
            slice_hot_window_turns: "8".to_string(),
            slice_cold_window_turns: "24".to_string(),
            retry_runtime_max_retries: "1".to_string(),
            retry_runtime_max_rounds: "10".to_string(),
            verifier_min_citation_coverage: "0.6".to_string(),
            verifier_min_avg_support_strength: "0.7".to_string(),
            verifier_min_supported_claim_ratio: "0.6".to_string(),
            verifier_min_claim_support_strength: "0.65".to_string(),
        };

        let errors = settings_validation_errors(&settings);
        assert!(!errors.is_empty());
    }
}
