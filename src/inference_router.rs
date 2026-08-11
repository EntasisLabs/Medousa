//! Cross-provider inference fallback router (Phase 3).

use std::future::Future;

use crate::inference_profiles::{InferenceProfile, InferenceProfileKind, InferenceTarget};
use crate::session::{load_tui_defaults, provider_api_key_configured};
use crate::turn_failure::{TurnFailure, TurnFailureCategory};

pub const OPENAI_CODEX_PROVIDER_ID: &str = "openai-codex";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityRequirement {
    None,
    Vision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCredentialRequirement {
    None,
    ApiKey,
    ChatGptOAuth,
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
    target_ineligibility_reason(target, required).is_none()
}

pub fn provider_credential_requirement(provider: &str) -> ProviderCredentialRequirement {
    match provider.trim().to_ascii_lowercase().as_str() {
        "ollama" | "local" | "lmstudio" | "lm-studio" | "medousa-local" => {
            ProviderCredentialRequirement::None
        }
        OPENAI_CODEX_PROVIDER_ID => ProviderCredentialRequirement::ChatGptOAuth,
        _ => ProviderCredentialRequirement::ApiKey,
    }
}

pub fn target_ineligibility_reason(
    target: &InferenceTarget,
    required: CapabilityRequirement,
) -> Option<&'static str> {
    match provider_credential_requirement(&target.provider) {
        ProviderCredentialRequirement::None => {}
        ProviderCredentialRequirement::ApiKey => {
            if !provider_api_key_configured(&target.provider) {
                return Some("missing_api_key");
            }
        }
        ProviderCredentialRequirement::ChatGptOAuth => {
            if !crate::session::chatgpt_oauth_configured() {
                return Some("missing_chatgpt_oauth");
            }
        }
    }

    match required {
        CapabilityRequirement::None => None,
        CapabilityRequirement::Vision => (!crate::model_capability_registry::registry()
            .supports_vision(&target.provider, &target.model))
        .then_some("missing_capability"),
    }
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
        TurnFailureCategory::Timeout | TurnFailureCategory::ProviderDown
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
        if let Some(reason) = target_ineligibility_reason(&target, required) {
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
                    if should_retry_same_target(last_failure.category) && same_target_retries < 1 {
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

    fn target(provider: &str) -> InferenceTarget {
        InferenceTarget {
            provider: provider.into(),
            model: "test-model".into(),
            base_url: None,
        }
    }

    #[test]
    fn auth_errors_advance_fallback() {
        assert!(should_advance_fallback(TurnFailureCategory::Auth));
    }

    #[test]
    fn timeout_retries_before_advance() {
        assert!(should_retry_same_target(TurnFailureCategory::Timeout));
    }

    #[test]
    fn unknown_failures_do_not_replay_the_turn() {
        assert!(!should_retry_same_target(TurnFailureCategory::Unknown));
    }

    #[test]
    fn credential_requirements_are_explicit() {
        assert_eq!(
            provider_credential_requirement("ollama"),
            ProviderCredentialRequirement::None
        );
        assert_eq!(
            provider_credential_requirement("openai"),
            ProviderCredentialRequirement::ApiKey
        );
        assert_eq!(
            provider_credential_requirement("OPENAI-CODEX"),
            ProviderCredentialRequirement::ChatGptOAuth
        );
    }

    #[test]
    fn openai_codex_requires_oauth_not_api_key() {
        assert_eq!(
            target_ineligibility_reason(
                &target(OPENAI_CODEX_PROVIDER_ID),
                CapabilityRequirement::None
            ),
            Some("missing_chatgpt_oauth")
        );
    }

    #[test]
    fn main_profile_preserves_api_and_oauth_as_separate_targets() {
        let mut defaults = crate::session::TuiDefaults::default();
        defaults.inference_profiles = Some(crate::inference_profiles::InferenceProfilesConfig {
            main: Some(InferenceProfile {
                provider: OPENAI_CODEX_PROVIDER_ID.into(),
                model: "gpt-5.6-sol".into(),
                base_url: None,
                fallbacks: vec![InferenceTarget {
                    provider: "openai".into(),
                    model: "gpt-5.6-sol".into(),
                    base_url: None,
                }],
            }),
            vision: None,
            stt: None,
        });

        let targets = profile_targets_from_defaults(InferenceProfileKind::Main, &defaults);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].provider, OPENAI_CODEX_PROVIDER_ID);
        assert_eq!(targets[1].provider, "openai");
    }
}
