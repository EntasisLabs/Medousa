//! Stable settings row identities — tab ranges index into [`ALL_SETTINGS_ROWS`].

use super::TuiState;
use crate::settings_ui::{route_target_presets, routing_editor_role};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsRowId {
    Backend,
    Provider,
    Model,
    BaseUrl,
    ApiKey,
    AllowedModules,
    ToolCallMode,
    MaxToolRounds,
    HostBusMaxToolRounds,
    HostTurnBusMode,
    ActivationToolIntentMaxRounds,
    ActivationShortTurnMaxToolRounds,
    ContinuationMaxToolRounds,
    MaxTextOnlyStuckContinues,
    ClassifierRestrictedMaxToolRounds,
    ActivationDirectAnswerMaxPromptChars,
    ActivationLongSessionTurnThreshold,
    ActivationLongSessionMaxPromptChars,
    SliceHotWindowTurns,
    SliceColdWindowTurns,
    ThinkingCapture,
    StasisOtelEnabled,
    ThinkingMaxLines,
    RetryRuntimeMaxRetries,
    RetryRuntimeMaxRounds,
    VerifierMinCitationCoverage,
    VerifierMinAvgSupportStrength,
    VerifierMinSupportedClaimRatio,
    VerifierMinClaimSupportStrength,
    EnvOverrides,
    ReviewConfiguration,
    ClearApiKey,
    UpdateApiKey,
    RouteRole,
    RouteProvider,
    RouteModel,
    SetAllRouteTargets,
    RouteTargetPreset,
    RoutePolicyProfile,
    RouteFallbackChain,
    ResetSelectedRoute,
    RevertChanges,
    ApplyChanges,
    Cancel,
    ThemeMenu,
}

pub(crate) const SETTINGS_TABS: [(&str, usize, usize); 8] = [
    ("Setup", 0, 5),
    ("Tools", 6, 14),
    ("Memory", 15, 19),
    ("Diagnostics", 20, 22),
    ("Quality", 23, 28),
    ("Secrets", 29, 32),
    ("Specialists", 33, 40),
    ("Save", 41, 44),
];

pub(crate) const ALL_SETTINGS_ROWS: [SettingsRowId; 45] = [
    SettingsRowId::Backend,
    SettingsRowId::Provider,
    SettingsRowId::Model,
    SettingsRowId::BaseUrl,
    SettingsRowId::ApiKey,
    SettingsRowId::AllowedModules,
    SettingsRowId::ToolCallMode,
    SettingsRowId::MaxToolRounds,
    SettingsRowId::HostBusMaxToolRounds,
    SettingsRowId::HostTurnBusMode,
    SettingsRowId::ActivationToolIntentMaxRounds,
    SettingsRowId::ActivationShortTurnMaxToolRounds,
    SettingsRowId::ContinuationMaxToolRounds,
    SettingsRowId::MaxTextOnlyStuckContinues,
    SettingsRowId::ClassifierRestrictedMaxToolRounds,
    SettingsRowId::ActivationDirectAnswerMaxPromptChars,
    SettingsRowId::ActivationLongSessionTurnThreshold,
    SettingsRowId::ActivationLongSessionMaxPromptChars,
    SettingsRowId::SliceHotWindowTurns,
    SettingsRowId::SliceColdWindowTurns,
    SettingsRowId::ThinkingCapture,
    SettingsRowId::StasisOtelEnabled,
    SettingsRowId::ThinkingMaxLines,
    SettingsRowId::RetryRuntimeMaxRetries,
    SettingsRowId::RetryRuntimeMaxRounds,
    SettingsRowId::VerifierMinCitationCoverage,
    SettingsRowId::VerifierMinAvgSupportStrength,
    SettingsRowId::VerifierMinSupportedClaimRatio,
    SettingsRowId::VerifierMinClaimSupportStrength,
    SettingsRowId::EnvOverrides,
    SettingsRowId::ReviewConfiguration,
    SettingsRowId::ClearApiKey,
    SettingsRowId::UpdateApiKey,
    SettingsRowId::RouteRole,
    SettingsRowId::RouteProvider,
    SettingsRowId::RouteModel,
    SettingsRowId::SetAllRouteTargets,
    SettingsRowId::RouteTargetPreset,
    SettingsRowId::RoutePolicyProfile,
    SettingsRowId::RouteFallbackChain,
    SettingsRowId::ResetSelectedRoute,
    SettingsRowId::RevertChanges,
    SettingsRowId::ApplyChanges,
    SettingsRowId::Cancel,
    SettingsRowId::ThemeMenu,
];

pub(crate) fn settings_row_id(index: usize) -> SettingsRowId {
    ALL_SETTINGS_ROWS
        .get(index)
        .copied()
        .unwrap_or(SettingsRowId::Backend)
}

pub(crate) fn is_numeric_row(id: SettingsRowId) -> bool {
    matches!(
        id,
        SettingsRowId::MaxToolRounds
            | SettingsRowId::HostBusMaxToolRounds
            | SettingsRowId::ActivationToolIntentMaxRounds
            | SettingsRowId::ActivationShortTurnMaxToolRounds
            | SettingsRowId::ContinuationMaxToolRounds
            | SettingsRowId::MaxTextOnlyStuckContinues
            | SettingsRowId::ClassifierRestrictedMaxToolRounds
            | SettingsRowId::ActivationDirectAnswerMaxPromptChars
            | SettingsRowId::ActivationLongSessionTurnThreshold
            | SettingsRowId::ActivationLongSessionMaxPromptChars
            | SettingsRowId::SliceHotWindowTurns
            | SettingsRowId::SliceColdWindowTurns
            | SettingsRowId::ThinkingMaxLines
            | SettingsRowId::RetryRuntimeMaxRetries
            | SettingsRowId::RetryRuntimeMaxRounds
    )
}

pub(crate) fn is_float_row(id: SettingsRowId) -> bool {
    matches!(
        id,
        SettingsRowId::VerifierMinCitationCoverage
            | SettingsRowId::VerifierMinAvgSupportStrength
            | SettingsRowId::VerifierMinSupportedClaimRatio
            | SettingsRowId::VerifierMinClaimSupportStrength
    )
}

pub(crate) fn is_toggle_row(id: SettingsRowId) -> bool {
    matches!(
        id,
        SettingsRowId::Backend
            | SettingsRowId::ToolCallMode
            | SettingsRowId::HostTurnBusMode
            | SettingsRowId::ThinkingCapture
            | SettingsRowId::StasisOtelEnabled
    )
}

pub(crate) fn is_routing_edit_row(id: SettingsRowId) -> bool {
    matches!(
        id,
        SettingsRowId::RouteProvider | SettingsRowId::RouteModel
    )
}

pub(crate) fn selected_settings_field_mut<'a>(
    state: &'a mut TuiState,
    id: SettingsRowId,
) -> Option<&'a mut String> {
    let draft = &mut state.settings_draft;
    Some(match id {
        SettingsRowId::Backend => &mut draft.backend,
        SettingsRowId::Provider => &mut draft.provider,
        SettingsRowId::Model => &mut draft.model,
        SettingsRowId::BaseUrl => &mut draft.base_url,
        SettingsRowId::ApiKey => &mut draft.api_key,
        SettingsRowId::AllowedModules => &mut draft.allowed_modules,
        SettingsRowId::ToolCallMode => &mut draft.tool_call_mode,
        SettingsRowId::MaxToolRounds => &mut draft.max_tool_rounds,
        SettingsRowId::HostBusMaxToolRounds => &mut draft.host_bus_max_tool_rounds,
        SettingsRowId::HostTurnBusMode => &mut draft.host_turn_bus_mode,
        SettingsRowId::ActivationToolIntentMaxRounds => {
            &mut draft.activation_tool_intent_max_rounds
        }
        SettingsRowId::ActivationShortTurnMaxToolRounds => {
            &mut draft.activation_short_turn_max_tool_rounds
        }
        SettingsRowId::ContinuationMaxToolRounds => &mut draft.continuation_max_tool_rounds,
        SettingsRowId::MaxTextOnlyStuckContinues => &mut draft.max_text_only_stuck_continues,
        SettingsRowId::ClassifierRestrictedMaxToolRounds => {
            &mut draft.classifier_restricted_max_tool_rounds
        }
        SettingsRowId::ActivationDirectAnswerMaxPromptChars => {
            &mut draft.activation_direct_answer_max_prompt_chars
        }
        SettingsRowId::ActivationLongSessionTurnThreshold => {
            &mut draft.activation_long_session_turn_threshold
        }
        SettingsRowId::ActivationLongSessionMaxPromptChars => {
            &mut draft.activation_long_session_max_prompt_chars
        }
        SettingsRowId::SliceHotWindowTurns => &mut draft.slice_hot_window_turns,
        SettingsRowId::SliceColdWindowTurns => &mut draft.slice_cold_window_turns,
        SettingsRowId::ThinkingCapture => &mut draft.thinking_capture,
        SettingsRowId::StasisOtelEnabled => &mut draft.stasis_otel_enabled,
        SettingsRowId::ThinkingMaxLines => &mut draft.thinking_max_lines,
        SettingsRowId::RetryRuntimeMaxRetries => &mut draft.retry_runtime_max_retries,
        SettingsRowId::RetryRuntimeMaxRounds => &mut draft.retry_runtime_max_rounds,
        SettingsRowId::VerifierMinCitationCoverage => &mut draft.verifier_min_citation_coverage,
        SettingsRowId::VerifierMinAvgSupportStrength => {
            &mut draft.verifier_min_avg_support_strength
        }
        SettingsRowId::VerifierMinSupportedClaimRatio => {
            &mut draft.verifier_min_supported_claim_ratio
        }
        SettingsRowId::VerifierMinClaimSupportStrength => {
            &mut draft.verifier_min_claim_support_strength
        }
        _ => return None,
    })
}

pub(crate) fn quick_adjust_setting(state: &mut TuiState, id: SettingsRowId, forward: bool) {
    match id {
        SettingsRowId::Backend => {
            state.settings_draft.backend =
                super::cycle_backend(&state.settings_draft.backend, forward);
        }
        SettingsRowId::ToolCallMode => {
            state.settings_draft.tool_call_mode =
                super::cycle_tool_call_mode(&state.settings_draft.tool_call_mode, forward);
        }
        SettingsRowId::HostTurnBusMode => {
            state.settings_draft.host_turn_bus_mode = super::cycle_host_turn_bus_mode(
                &state.settings_draft.host_turn_bus_mode,
                forward,
            );
        }
        SettingsRowId::ThinkingCapture => {
            let value = super::parse_bool_with_default(&state.settings_draft.thinking_capture, true);
            state.settings_draft.thinking_capture = (!value).to_string();
        }
        SettingsRowId::StasisOtelEnabled => {
            let value =
                super::parse_bool_with_default(&state.settings_draft.stasis_otel_enabled, false);
            state.settings_draft.stasis_otel_enabled = (!value).to_string();
        }
        SettingsRowId::MaxToolRounds => adjust_max_tool_rounds(state, forward),
        SettingsRowId::HostBusMaxToolRounds => adjust_round_field(
            &mut state.settings_draft.host_bus_max_tool_rounds,
            medousa::agent_runtime::DEFAULT_HOST_BUS_MAX_TOOL_ROUNDS,
            forward,
        ),
        SettingsRowId::ActivationToolIntentMaxRounds => adjust_round_field(
            &mut state.settings_draft.activation_tool_intent_max_rounds,
            medousa::agent_runtime::DEFAULT_ACTIVATION_TOOL_INTENT_MAX_ROUNDS,
            forward,
        ),
        SettingsRowId::ActivationShortTurnMaxToolRounds => adjust_round_field(
            &mut state.settings_draft.activation_short_turn_max_tool_rounds,
            medousa::agent_runtime::DEFAULT_ACTIVATION_SHORT_TURN_MAX_TOOL_ROUNDS,
            forward,
        ),
        SettingsRowId::ContinuationMaxToolRounds => adjust_round_field(
            &mut state.settings_draft.continuation_max_tool_rounds,
            medousa::agent_runtime::DEFAULT_CONTINUATION_MAX_TOOL_ROUNDS,
            forward,
        ),
        SettingsRowId::MaxTextOnlyStuckContinues => adjust_round_field(
            &mut state.settings_draft.max_text_only_stuck_continues,
            medousa::agent_runtime::DEFAULT_MAX_TEXT_ONLY_STUCK_CONTINUES,
            forward,
        ),
        SettingsRowId::ClassifierRestrictedMaxToolRounds => adjust_round_field(
            &mut state.settings_draft.classifier_restricted_max_tool_rounds,
            medousa::agent_runtime::DEFAULT_CLASSIFIER_RESTRICTED_MAX_TOOL_ROUNDS,
            forward,
        ),
        SettingsRowId::ThinkingMaxLines => {
            let current =
                super::parse_usize_with_bounds(&state.settings_draft.thinking_max_lines, 300, 50, 5000);
            let step = if current < 500 { 50 } else { 100 };
            let next = step_usize(current, step, forward).clamp(50, 5000);
            state.settings_draft.thinking_max_lines = next.to_string();
        }
        SettingsRowId::ActivationDirectAnswerMaxPromptChars => {
            adjust_prompt_chars(
                &mut state
                    .settings_draft
                    .activation_direct_answer_max_prompt_chars,
                320,
                forward,
            );
        }
        SettingsRowId::ActivationLongSessionTurnThreshold => {
            let current = super::parse_usize_with_bounds(
                &state.settings_draft.activation_long_session_turn_threshold,
                28,
                8,
                500,
            );
            let step = if current < 100 { 1 } else { 10 };
            state.settings_draft.activation_long_session_turn_threshold =
                step_usize(current, step, forward).clamp(8, 500).to_string();
        }
        SettingsRowId::ActivationLongSessionMaxPromptChars => {
            adjust_prompt_chars(
                &mut state
                    .settings_draft
                    .activation_long_session_max_prompt_chars,
                420,
                forward,
            );
        }
        SettingsRowId::SliceHotWindowTurns => {
            let current =
                super::parse_usize_with_bounds(&state.settings_draft.slice_hot_window_turns, 8, 2, 32);
            let next = step_usize(current, 1, forward).clamp(2, 32);
            state.settings_draft.slice_hot_window_turns = next.to_string();
            let cold = super::parse_usize_with_bounds(
                &state.settings_draft.slice_cold_window_turns,
                24,
                4,
                128,
            );
            if cold < next {
                state.settings_draft.slice_cold_window_turns = next.to_string();
            }
        }
        SettingsRowId::SliceColdWindowTurns => {
            let hot =
                super::parse_usize_with_bounds(&state.settings_draft.slice_hot_window_turns, 8, 2, 32);
            let current = super::parse_usize_with_bounds(
                &state.settings_draft.slice_cold_window_turns,
                24,
                4,
                128,
            );
            let next = step_usize(current, 1, forward).clamp(4, 128).max(hot);
            state.settings_draft.slice_cold_window_turns = next.to_string();
        }
        SettingsRowId::RetryRuntimeMaxRetries => {
            let current = super::parse_usize_with_bounds(
                &state.settings_draft.retry_runtime_max_retries,
                1,
                medousa::agent_runtime::RETRY_LIMIT_MIN,
                medousa::agent_runtime::RETRY_LIMIT_MAX,
            );
            let next = step_usize(current, 1, forward).clamp(
                medousa::agent_runtime::RETRY_LIMIT_MIN,
                medousa::agent_runtime::RETRY_LIMIT_MAX,
            );
            state.settings_draft.retry_runtime_max_retries = next.to_string();
        }
        SettingsRowId::RetryRuntimeMaxRounds => {
            let field = &mut state.settings_draft.retry_runtime_max_rounds;
            let current = super::parse_usize_with_bounds(
                field,
                medousa::agent_runtime::turn_orchestrator::DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
                medousa::agent_runtime::ROUND_LIMIT_MIN,
                medousa::agent_runtime::ROUND_LIMIT_MAX,
            );
            let step = if current < 20 {
                1
            } else if current < 200 {
                5
            } else {
                25
            };
            *field = step_usize(current, step, forward)
                .clamp(
                    medousa::agent_runtime::ROUND_LIMIT_MIN,
                    medousa::agent_runtime::ROUND_LIMIT_MAX,
                )
                .to_string();
        }
        SettingsRowId::VerifierMinCitationCoverage => {
            adjust_verifier_float(&mut state.settings_draft.verifier_min_citation_coverage, 0.60, forward);
        }
        SettingsRowId::VerifierMinAvgSupportStrength => {
            adjust_verifier_float(
                &mut state.settings_draft.verifier_min_avg_support_strength,
                0.70,
                forward,
            );
        }
        SettingsRowId::VerifierMinSupportedClaimRatio => {
            adjust_verifier_float(
                &mut state.settings_draft.verifier_min_supported_claim_ratio,
                0.60,
                forward,
            );
        }
        SettingsRowId::VerifierMinClaimSupportStrength => {
            adjust_verifier_float(
                &mut state.settings_draft.verifier_min_claim_support_strength,
                0.65,
                forward,
            );
        }
        SettingsRowId::RouteRole => {
            let roles = medousa::stage_routing::StageRoutingMatrix::roles();
            if roles.is_empty() {
                return;
            }
            state.routing_editor_role_idx = if forward {
                (state.routing_editor_role_idx + 1) % roles.len()
            } else if state.routing_editor_role_idx == 0 {
                roles.len() - 1
            } else {
                state.routing_editor_role_idx - 1
            };
        }
        SettingsRowId::RouteTargetPreset => {
            let role = routing_editor_role(state).to_string();
            if let Some(route) = state.stage_routing_draft.get_mut(&role) {
                let presets = route_target_presets();
                let current = format!("{}:{}", route.provider, route.model);
                let idx = presets.iter().position(|v| *v == current).unwrap_or(0);
                let next = if forward {
                    (idx + 1) % presets.len()
                } else if idx == 0 {
                    presets.len() - 1
                } else {
                    idx - 1
                };
                if let Some((provider, model)) = presets[next].split_once(':') {
                    route.provider = provider.to_string();
                    route.model = model.to_string();
                }
            }
        }
        SettingsRowId::RoutePolicyProfile => {
            let role = routing_editor_role(state).to_string();
            if let Some(route) = state.stage_routing_draft.get_mut(&role) {
                let options = ["balanced", "strict", "analytical", "fast"];
                let idx = options
                    .iter()
                    .position(|v| v.eq_ignore_ascii_case(route.policy_profile.as_str()))
                    .unwrap_or(0);
                let next = if forward {
                    (idx + 1) % options.len()
                } else if idx == 0 {
                    options.len() - 1
                } else {
                    idx - 1
                };
                route.policy_profile = options[next].to_string();
            }
        }
        SettingsRowId::RouteFallbackChain => {
            let role = routing_editor_role(state).to_string();
            if let Some(route) = state.stage_routing_draft.get_mut(&role) {
                let options = vec![
                    vec![role.clone(), "safe-default".to_string()],
                    vec!["safe-default".to_string()],
                    vec![role, "balanced".to_string(), "safe-default".to_string()],
                ];
                let idx = options
                    .iter()
                    .position(|v| *v == route.fallback_chain)
                    .unwrap_or(0);
                let next = if forward {
                    (idx + 1) % options.len()
                } else if idx == 0 {
                    options.len() - 1
                } else {
                    idx - 1
                };
                route.fallback_chain = options[next].clone();
            }
        }
        SettingsRowId::ResetSelectedRoute => {
            let role = routing_editor_role(state).to_string();
            let defaults = medousa::stage_routing::StageRoutingMatrix::default_for(
                &state.settings_draft.provider,
                &state.settings_draft.model,
            );
            if let (Some(current), Some(default_route)) = (
                state.stage_routing_draft.get_mut(&role),
                defaults.get(&role),
            ) {
                *current = default_route.clone();
            }
        }
        _ => {}
    }
}

fn step_usize(current: usize, step: usize, forward: bool) -> usize {
    if forward {
        current.saturating_add(step)
    } else {
        current.saturating_sub(step)
    }
}

fn adjust_max_tool_rounds(state: &mut TuiState, forward: bool) {
    let field = &mut state.settings_draft.max_tool_rounds;
    let current = super::parse_usize_with_bounds(
        field,
        10,
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let step = if current < 20 {
        1
    } else if current < 200 {
        5
    } else {
        25
    };
    *field = step_usize(current, step, forward)
        .clamp(
            medousa::agent_runtime::ROUND_LIMIT_MIN,
            medousa::agent_runtime::ROUND_LIMIT_MAX,
        )
        .to_string();
}

fn adjust_round_field(field: &mut String, default_value: usize, forward: bool) {
    let current = super::parse_usize_with_bounds(
        field,
        default_value,
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let step = if current < 20 {
        1
    } else if current < 200 {
        5
    } else {
        25
    };
    let next = step_usize(current, step, forward).clamp(
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    *field = next.to_string();
}

fn adjust_prompt_chars(field: &mut String, default: usize, forward: bool) {
    let current = super::parse_usize_with_bounds(field, default, 64, 4000);
    let step = if current < 1000 { 32 } else { 128 };
    *field = step_usize(current, step, forward).clamp(64, 4000).to_string();
}

fn adjust_verifier_float(field: &mut String, default: f32, forward: bool) {
    let current = super::parse_f32_with_bounds(field, default, 0.0, 1.0);
    let step = 0.05;
    let next = if forward {
        current + step
    } else {
        current - step
    }
    .clamp(0.0, 1.0);
    *field = format!("{next:.2}");
}
