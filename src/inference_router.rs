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

/// Resolve an image turn from the user's selected model first, then the
/// explicitly configured vision profile. Eligibility filtering happens at
/// execution time so a text-only primary target naturally falls through.
pub fn vision_targets_for_turn(
    primary: InferenceTarget,
    defaults: &crate::session::TuiDefaults,
) -> Vec<InferenceTarget> {
    let mut targets = vec![primary];
    if let Some(profile) = defaults
        .inference_profiles
        .as_ref()
        .and_then(|profiles| profiles.vision.clone())
        .and_then(|profile| profile.trimmed())
    {
        targets.push(profile.as_target());
        targets.extend(
            profile
                .fallbacks
                .into_iter()
                .filter_map(|target| target.trimmed()),
        );
    }
    deduplicate_targets(&mut targets);
    targets
}

/// Preserve a per-turn model selection while retaining its configured main
/// fallbacks when the selection is the saved main profile.
pub fn main_targets_for_turn(
    primary: InferenceTarget,
    defaults: &crate::session::TuiDefaults,
) -> Vec<InferenceTarget> {
    let mut targets = vec![primary.clone()];
    if let Some(profile) = defaults
        .inference_profiles
        .as_ref()
        .and_then(|profiles| profiles.main.clone())
        .and_then(|profile| profile.trimmed())
        && same_target(&profile.as_target(), &primary)
    {
        targets.extend(
            profile
                .fallbacks
                .into_iter()
                .filter_map(|target| target.trimmed()),
        );
    }
    deduplicate_targets(&mut targets);
    targets
}

fn deduplicate_targets(targets: &mut Vec<InferenceTarget>) {
    let mut unique = Vec::<InferenceTarget>::new();
    targets.retain(|target| {
        if unique.iter().any(|existing| same_target(existing, target)) {
            false
        } else {
            unique.push(target.clone());
            true
        }
    });
}

fn same_target(left: &InferenceTarget, right: &InferenceTarget) -> bool {
    left.provider.eq_ignore_ascii_case(&right.provider)
        && left.model.eq_ignore_ascii_case(&right.model)
        && left.base_url == right.base_url
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

fn missing_credential_reason(
    requirement: ProviderCredentialRequirement,
    configured: bool,
) -> Option<&'static str> {
    if configured {
        return None;
    }
    match requirement {
        ProviderCredentialRequirement::None => None,
        ProviderCredentialRequirement::ApiKey => Some("missing_api_key"),
        ProviderCredentialRequirement::ChatGptOAuth => Some("missing_chatgpt_oauth"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialAvailability {
    pub api_key_configured: bool,
    pub chatgpt_oauth_configured: bool,
}

pub fn live_credential_availability(provider: &str) -> CredentialAvailability {
    CredentialAvailability {
        api_key_configured: provider_api_key_configured(provider),
        chatgpt_oauth_configured: crate::session::chatgpt_oauth_configured(),
    }
}

pub fn target_ineligibility_reason(
    target: &InferenceTarget,
    required: CapabilityRequirement,
) -> Option<&'static str> {
    target_ineligibility_with_credentials(
        target,
        required,
        live_credential_availability(&target.provider),
    )
}

pub fn target_ineligibility_with_credentials(
    target: &InferenceTarget,
    required: CapabilityRequirement,
    credentials: CredentialAvailability,
) -> Option<&'static str> {
    match provider_credential_requirement(&target.provider) {
        ProviderCredentialRequirement::None => {}
        ProviderCredentialRequirement::ApiKey => {
            if let Some(reason) = missing_credential_reason(
                ProviderCredentialRequirement::ApiKey,
                credentials.api_key_configured,
            ) {
                return Some(reason);
            }
        }
        ProviderCredentialRequirement::ChatGptOAuth => {
            if let Some(reason) = missing_credential_reason(
                ProviderCredentialRequirement::ChatGptOAuth,
                credentials.chatgpt_oauth_configured,
            ) {
                return Some(reason);
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
    fn openai_codex_missing_oauth_reason_does_not_consult_host_credentials() {
        assert_eq!(
            missing_credential_reason(ProviderCredentialRequirement::ChatGptOAuth, false),
            Some("missing_chatgpt_oauth")
        );
        assert_eq!(
            missing_credential_reason(ProviderCredentialRequirement::ChatGptOAuth, true),
            None
        );
        let target = InferenceTarget {
            provider: OPENAI_CODEX_PROVIDER_ID.to_string(),
            model: "gpt-5.4".to_string(),
            base_url: None,
        };
        assert_eq!(
            target_ineligibility_with_credentials(
                &target,
                CapabilityRequirement::None,
                CredentialAvailability {
                    api_key_configured: false,
                    chatgpt_oauth_configured: false,
                },
            ),
            Some("missing_chatgpt_oauth")
        );
        assert_eq!(
            target_ineligibility_with_credentials(
                &target,
                CapabilityRequirement::None,
                CredentialAvailability {
                    api_key_configured: false,
                    chatgpt_oauth_configured: true,
                },
            ),
            None
        );
    }

    #[test]
    fn main_profile_preserves_api_and_oauth_as_separate_targets() {
        let defaults = crate::session::TuiDefaults {
            inference_profiles: Some(crate::inference_profiles::InferenceProfilesConfig {
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
            }),
            ..Default::default()
        };

        let targets = profile_targets_from_defaults(InferenceProfileKind::Main, &defaults);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].provider, OPENAI_CODEX_PROVIDER_ID);
        assert_eq!(targets[1].provider, "openai");
    }

    #[test]
    fn vision_targets_prefer_selected_model_then_configured_fallback() {
        let defaults = crate::session::TuiDefaults {
            inference_profiles: Some(crate::inference_profiles::InferenceProfilesConfig {
                main: None,
                vision: Some(InferenceProfile {
                    provider: "openai".into(),
                    model: "gpt-4.1-mini".into(),
                    base_url: None,
                    fallbacks: Vec::new(),
                }),
                stt: None,
            }),
            ..Default::default()
        };

        let targets = vision_targets_for_turn(
            InferenceTarget {
                provider: OPENAI_CODEX_PROVIDER_ID.into(),
                model: "gpt-5.6-sol".into(),
                base_url: None,
            },
            &defaults,
        );

        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].provider, OPENAI_CODEX_PROVIDER_ID);
        assert_eq!(targets[0].model, "gpt-5.6-sol");
        assert_eq!(targets[1].provider, "openai");
        assert_eq!(targets[1].model, "gpt-4.1-mini");
    }

    #[test]
    fn duplicate_vision_profile_does_not_repeat_selected_target() {
        let primary = InferenceTarget {
            provider: OPENAI_CODEX_PROVIDER_ID.into(),
            model: "gpt-5.6-sol".into(),
            base_url: None,
        };
        let defaults = crate::session::TuiDefaults {
            inference_profiles: Some(crate::inference_profiles::InferenceProfilesConfig {
                main: None,
                vision: Some(InferenceProfile {
                    provider: primary.provider.clone(),
                    model: primary.model.clone(),
                    base_url: None,
                    fallbacks: Vec::new(),
                }),
                stt: None,
            }),
            ..Default::default()
        };

        assert_eq!(vision_targets_for_turn(primary, &defaults).len(), 1);
    }

    #[test]
    fn main_turn_override_is_not_replaced_by_saved_profile() {
        let defaults = crate::session::TuiDefaults {
            inference_profiles: Some(crate::inference_profiles::InferenceProfilesConfig {
                main: Some(InferenceProfile {
                    provider: "anthropic".into(),
                    model: "claude-sonnet-4".into(),
                    base_url: None,
                    fallbacks: vec![InferenceTarget {
                        provider: "openai".into(),
                        model: "gpt-5.6".into(),
                        base_url: None,
                    }],
                }),
                vision: None,
                stt: None,
            }),
            ..Default::default()
        };
        let selected = InferenceTarget {
            provider: OPENAI_CODEX_PROVIDER_ID.into(),
            model: "gpt-5.6-sol".into(),
            base_url: None,
        };

        assert_eq!(
            main_targets_for_turn(selected.clone(), &defaults),
            vec![selected]
        );
    }
}
