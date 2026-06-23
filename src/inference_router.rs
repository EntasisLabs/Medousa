//! Cross-provider inference fallback router (Phase 3).

use std::future::Future;

use crate::inference_profiles::{InferenceProfile, InferenceProfileKind, InferenceTarget};
use crate::session::{load_tui_defaults, provider_api_key_configured};
use crate::turn_failure::{TurnFailure, TurnFailureCategory};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityRequirement {
    None,
    Vision,
}

#[derive(Debug, Clone)]
pub struct InferenceExecution<T> {
    pub result: T,
    pub target: InferenceTarget,
    pub attempt_index: usize,
    pub total_attempts: usize,
}

pub fn profile_targets(kind: InferenceProfileKind) -> Vec<InferenceTarget> {
    let defaults = load_tui_defaults();
    profile_targets_from_defaults(kind, &defaults)
}

pub fn profile_targets_from_defaults(
    kind: InferenceProfileKind,
    defaults: &crate::session::TuiDefaults,
) -> Vec<InferenceTarget> {
    let profile = match kind {
        InferenceProfileKind::Main => defaults
            .inference_profiles
            .as_ref()
            .and_then(|profiles| profiles.main.clone())
            .or_else(|| {
                Some(InferenceProfile {
                    provider: crate::resolve_llm_provider(defaults.provider.as_deref()),
                    model: crate::resolve_llm_model(defaults.model.as_deref()),
                    base_url: defaults
                        .base_url
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    fallbacks: Vec::new(),
                })
            }),
        InferenceProfileKind::Vision => defaults
            .inference_profiles
            .as_ref()
            .and_then(|profiles| profiles.vision.clone()),
        InferenceProfileKind::Stt => defaults
            .inference_profiles
            .as_ref()
            .and_then(|profiles| profiles.stt.clone()),
    };

    let Some(profile) = profile.and_then(|profile| profile.trimmed()) else {
        return Vec::new();
    };

    let mut targets = vec![profile.as_target()];
    for fallback in profile.fallbacks {
        if let Some(target) = fallback.trimmed() {
            targets.push(target);
        }
    }
    targets
}

pub fn target_is_eligible(target: &InferenceTarget, required: CapabilityRequirement) -> bool {
    if !provider_needs_api_key(&target.provider) {
        return true;
    }
    if !provider_api_key_configured(&target.provider) {
        return false;
    }
    match required {
        CapabilityRequirement::None => true,
        CapabilityRequirement::Vision => crate::model_capability_registry::registry()
            .supports_vision(&target.provider, &target.model),
    }
}

pub fn provider_needs_api_key(provider: &str) -> bool {
    !matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "ollama" | "local" | "lmstudio" | "lm-studio" | "medousa-local"
    )
}

pub fn should_advance_fallback(category: TurnFailureCategory) -> bool {
    matches!(
        category,
        TurnFailureCategory::Auth
            | TurnFailureCategory::RateLimit
            | TurnFailureCategory::ModelNotFound
            | TurnFailureCategory::ProviderDown
    )
}

pub fn should_retry_same_target(category: TurnFailureCategory) -> bool {
    matches!(
        category,
        TurnFailureCategory::Timeout | TurnFailureCategory::ProviderDown | TurnFailureCategory::Unknown
    )
}

pub fn telemetry_line(
    profile: InferenceProfileKind,
    attempt_index: usize,
    total: usize,
    target: &InferenceTarget,
    reason: &str,
) -> String {
    format!(
        "◈ inference profile={} attempt={}/{} target={}:{} reason={}",
        profile.label(),
        attempt_index + 1,
        total,
        target.provider,
        target.model,
        reason
    )
}

impl InferenceProfileKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Vision => "vision",
            Self::Stt => "stt",
        }
    }
}

pub async fn execute_with_fallbacks<T, F, Fut>(
    profile: InferenceProfileKind,
    required: CapabilityRequirement,
    mut on_notice: impl FnMut(String),
    operation: F,
) -> Result<InferenceExecution<T>, TurnFailure>
where
    F: Fn(InferenceTarget) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let targets = profile_targets(profile);
    if targets.is_empty() {
        return Err(TurnFailure::validation(
            "No inference profile configured for this capability.",
            format!("empty target list for profile={}", profile.label()),
        ));
    }

    let total = targets.len();
    let mut last_failure = unknown_failure("all inference targets failed");

    for (attempt_index, target) in targets.into_iter().enumerate() {
        if !target_is_eligible(&target, required) {
            let reason = if provider_needs_api_key(&target.provider)
                && !provider_api_key_configured(&target.provider)
            {
                "missing_api_key"
            } else {
                "missing_capability"
            };
            on_notice(telemetry_line(
                profile,
                attempt_index,
                total,
                &target,
                reason,
            ));
            continue;
        }

        crate::workshop_env::apply_provider_llm_env(&target.provider);
        on_notice(telemetry_line(
            profile,
            attempt_index,
            total,
            &target,
            "attempt",
        ));

        let mut same_target_retries = 0u8;
        loop {
            match operation(target.clone()).await {
                Ok(result) => {
                    return Ok(InferenceExecution {
                        result,
                        target,
                        attempt_index,
                        total_attempts: total,
                    });
                }
                Err(raw) => {
                    last_failure = TurnFailure::from_debug(&raw);
                    if should_retry_same_target(last_failure.category)
                        && same_target_retries < 1
                    {
                        same_target_retries += 1;
                        on_notice(telemetry_line(
                            profile,
                            attempt_index,
                            total,
                            &target,
                            &format!("retry_{}", last_failure.category_label()),
                        ));
                        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                        continue;
                    }
                    if should_advance_fallback(last_failure.category) {
                        on_notice(telemetry_line(
                            profile,
                            attempt_index,
                            total,
                            &target,
                            last_failure.category_label(),
                        ));
                    }
                    break;
                }
            }
        }
    }

    Err(last_failure)
}

fn unknown_failure(message: &str) -> TurnFailure {
    TurnFailure::from_debug(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_errors_advance_fallback() {
        assert!(should_advance_fallback(TurnFailureCategory::Auth));
    }

    #[test]
    fn timeout_retries_before_advance() {
        assert!(should_retry_same_target(TurnFailureCategory::Timeout));
    }
}
