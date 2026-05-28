use crate::settings_guard::{invalid_module_ids, parse_allowed_modules};

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
    pub thinking_capture: String,
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
            "invalid allowed module ids: {}",
            invalid_modules.join(", ")
        ));
    }

    let env_errors = env_overrides_validation_errors(&settings.env_overrides);
    errors.extend(env_errors);

    validate_unit_interval(
        "verifier min citation coverage",
        &settings.verifier_min_citation_coverage,
        &mut errors,
    );
    validate_unit_interval(
        "verifier min avg support strength",
        &settings.verifier_min_avg_support_strength,
        &mut errors,
    );
    validate_unit_interval(
        "verifier min supported claim ratio",
        &settings.verifier_min_supported_claim_ratio,
        &mut errors,
    );
    validate_unit_interval(
        "verifier min claim support strength",
        &settings.verifier_min_claim_support_strength,
        &mut errors,
    );

    validate_usize_range(
        "activation direct-answer max prompt chars",
        &settings.activation_direct_answer_max_prompt_chars,
        64,
        4000,
        &mut errors,
    );
    validate_usize_range(
        "activation long-session turn threshold",
        &settings.activation_long_session_turn_threshold,
        8,
        500,
        &mut errors,
    );
    validate_usize_range(
        "activation long-session max prompt chars",
        &settings.activation_long_session_max_prompt_chars,
        64,
        4000,
        &mut errors,
    );
    validate_usize_range(
        "slice hot-window turns",
        &settings.slice_hot_window_turns,
        2,
        32,
        &mut errors,
    );
    validate_usize_range(
        "slice cold-window turns",
        &settings.slice_cold_window_turns,
        4,
        128,
        &mut errors,
    );
    validate_usize_range(
        "retry runtime max retries",
        &settings.retry_runtime_max_retries,
        0,
        5,
        &mut errors,
    );
    validate_usize_range(
        "retry runtime max rounds",
        &settings.retry_runtime_max_rounds,
        1,
        10,
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
            errors.push("slice cold-window turns must be >= hot-window turns".to_string());
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
    match value.unwrap_or("surreal-mem").trim() {
        "in-memory" => "in-memory".to_string(),
        "surreal-mem" => "surreal-mem".to_string(),
        _ => "surreal-mem".to_string(),
    }
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
    let choices = ["surreal-mem", "in-memory"];
    cycle_choice(current, &choices, forward)
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
        assert_eq!(resolve_backend_name(Some("unknown")), "surreal-mem");
    }

    #[test]
    fn cycles_backend_choices() {
        assert_eq!(cycle_backend("surreal-mem", true), "in-memory");
        assert_eq!(cycle_backend("in-memory", true), "surreal-mem");
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
            thinking_capture: "true".to_string(),
            thinking_max_lines: "300".to_string(),
            activation_direct_answer_max_prompt_chars: "320".to_string(),
            activation_long_session_turn_threshold: "28".to_string(),
            activation_long_session_max_prompt_chars: "420".to_string(),
            slice_hot_window_turns: "8".to_string(),
            slice_cold_window_turns: "24".to_string(),
            retry_runtime_max_retries: "1".to_string(),
            retry_runtime_max_rounds: "3".to_string(),
            verifier_min_citation_coverage: "0.6".to_string(),
            verifier_min_avg_support_strength: "0.7".to_string(),
            verifier_min_supported_claim_ratio: "0.6".to_string(),
            verifier_min_claim_support_strength: "0.65".to_string(),
        };

        let errors = settings_validation_errors(&settings);
        assert!(!errors.is_empty());
    }
}
