use anyhow::Result;

use crate::daemon_api::{
    RuntimeConfigCommandRequest, RuntimeConfigCommandResponse, RuntimeConfigCommandSpec,
    RuntimeVerifyPolicyState,
};
use crate::reasoning_effort::{normalize_reasoning_effort_value, reasoning_effort_hint, REASONING_EFFORT_DEFAULT};

pub fn execute_runtime_config_command(
    request: RuntimeConfigCommandRequest,
) -> Result<RuntimeConfigCommandResponse> {
    let mut next_draft_provider = request.draft_provider;
    let mut next_draft_model = request.draft_model;
    let mut next_response_depth_mode = request.current_response_depth_mode;
    let mut next_reasoning_effort = if request.current_reasoning_effort.trim().is_empty() {
        REASONING_EFFORT_DEFAULT.to_string()
    } else {
        normalize_reasoning_effort_value(&request.current_reasoning_effort)
    };
    let mut next_verify_policy_draft = None;
    let mut should_apply_settings = false;
    let mut should_persist_depth_defaults = false;
    let mut should_persist_reasoning_defaults = false;

    let rendered_output = match request.command {
        RuntimeConfigCommandSpec::Model { args } => {
            if args.is_empty() {
                Some(format!(
                    "model {}:{}",
                    request.current_provider, request.current_model
                ))
            } else {
                if args.len() == 1 {
                    if let Some((provider, model)) = args[0].split_once(':') {
                        next_draft_provider = provider.trim().to_string();
                        next_draft_model = model.trim().to_string();
                    } else {
                        next_draft_model = args[0].trim().to_string();
                    }
                } else {
                    next_draft_provider = args[0].trim().to_string();
                    next_draft_model = args[1].trim().to_string();
                }
                should_apply_settings = true;
                None
            }
        }
        RuntimeConfigCommandSpec::Depth { mode } => {
            if mode.is_none() {
                let hint = depth_mode_hint(&next_response_depth_mode);
                Some(format!(
                    "◈ response depth mode={} ({hint}) options: concise | standard | deep",
                    next_response_depth_mode,
                ))
            } else {
                let normalized = normalize_response_depth_mode(mode.as_deref().unwrap_or("standard"));
                next_response_depth_mode = normalized.clone();
                should_persist_depth_defaults = true;
                let hint = depth_mode_hint(&normalized);
                Some(format!("✓ response depth mode set to {} ({hint})", normalized))
            }
        }
        RuntimeConfigCommandSpec::Reasoning { mode } => {
            if mode.is_none() {
                let hint = reasoning_effort_hint(&next_reasoning_effort);
                Some(format!(
                    "◈ reasoning effort={} ({hint}) options: default | minimal | low | medium | high | xhigh | max | budget:N",
                    next_reasoning_effort,
                ))
            } else {
                let normalized =
                    normalize_reasoning_effort_value(mode.as_deref().unwrap_or(REASONING_EFFORT_DEFAULT));
                next_reasoning_effort = normalized.clone();
                should_persist_reasoning_defaults = true;
                let hint = reasoning_effort_hint(&normalized);
                Some(format!("✓ reasoning effort set to {} ({hint})", normalized))
            }
        }
        RuntimeConfigCommandSpec::VerifyPolicy { args, current } => {
            if args.is_empty() {
                Some(format!(
                    "◈ verify policy citation={} avg_support={} supported_ratio={} claim_support={}",
                    current.min_citation_coverage,
                    current.min_avg_support_strength,
                    current.min_supported_claim_ratio,
                    current.min_claim_support_strength,
                ))
            } else if args.len() != 4 {
                Some(
                    "⚠ usage: /verify-policy <min_citation_coverage> <min_avg_support_strength> <min_supported_claim_ratio> <min_claim_support_strength>"
                        .to_string(),
                )
            } else {
                next_verify_policy_draft = Some(RuntimeVerifyPolicyState {
                    min_citation_coverage: normalize_verify_policy_value(&args[0], 0.60),
                    min_avg_support_strength: normalize_verify_policy_value(&args[1], 0.70),
                    min_supported_claim_ratio: normalize_verify_policy_value(&args[2], 0.60),
                    min_claim_support_strength: normalize_verify_policy_value(&args[3], 0.65),
                });
                should_apply_settings = true;
                None
            }
        }
    };

    Ok(RuntimeConfigCommandResponse {
        rendered_output,
        next_draft_provider,
        next_draft_model,
        next_response_depth_mode,
        next_reasoning_effort,
        next_verify_policy_draft,
        should_apply_settings,
        should_persist_depth_defaults,
        should_persist_reasoning_defaults,
    })
}

fn normalize_response_depth_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "concise" => "concise".to_string(),
        "deep" => "deep".to_string(),
        _ => "standard".to_string(),
    }
}

fn depth_mode_hint(mode: &str) -> &'static str {
    match mode {
        "concise" => "short direct answers",
        "deep" => "detailed evidence-forward answers",
        _ => "balanced answer depth",
    }
}

fn normalize_verify_policy_value(raw: &str, default: f32) -> String {
    let parsed = parse_f32_with_bounds(raw, default, 0.0, 1.0);
    format!("{parsed:.2}")
}

fn parse_f32_with_bounds(value: &str, default: f32, min: f32, max: f32) -> f32 {
    value
        .trim()
        .parse::<f32>()
        .ok()
        .unwrap_or(default)
        .clamp(min, max)
}
