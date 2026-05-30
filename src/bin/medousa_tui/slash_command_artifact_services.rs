use medousa::{
    
    ArtifactCommandRequest, ArtifactCommandResponse, ArtifactCommandSpec,
    ArtifactVerificationPolicyInput, RuntimeConfigCommandSpec, RuntimeVerifyPolicyState,
};

use super::daemon_commands::daemon_artifact_command;
use super::*;

pub(crate) async fn handle_artifact_family_command(
    cmd: &str,
    args: Vec<&str>,
    state: &mut TuiState,
) -> EventOutcome {
    let Some(command) = build_artifact_command_spec(cmd, &args) else {
        return EventOutcome::Continue;
    };

    let verification_policy = build_verification_policy_input_if_needed(&command, state);
    let verifier_route_label = build_verifier_route_label_if_needed(&command, state);

    let request = ArtifactCommandRequest {
        session_id: state.session_id.clone(),
        selected_context_pack_query: state.selected_context_pack_query.clone(),
        command,
        verification_policy,
        verifier_route_label,
    };

    match execute_artifact_command_via_daemon(&state.daemon_url, request).await {
        Ok((response, backend_notice)) => {
            state.selected_context_pack_query = response.selected_context_pack_query;
            if let Some(notice) = backend_notice {
                push_obs(state, notice);
            }
            push_obs(state, response.rendered_output);
        }
        Err(err) => {
            push_obs(state, format!("⚠ artifact command failed: {err}"));
        }
    }

    EventOutcome::Continue
}

pub(crate) async fn handle_verify_policy_command(
    args: Vec<&str>,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    let request = super::slash_command_services::build_runtime_config_request(
        state,
        RuntimeConfigCommandSpec::VerifyPolicy {
            args: args.into_iter().map(ToString::to_string).collect::<Vec<_>>(),
            current: RuntimeVerifyPolicyState {
                min_citation_coverage: state.settings.verifier_min_citation_coverage.clone(),
                min_avg_support_strength: state.settings.verifier_min_avg_support_strength.clone(),
                min_supported_claim_ratio: state.settings.verifier_min_supported_claim_ratio.clone(),
                min_claim_support_strength: state.settings.verifier_min_claim_support_strength.clone(),
            },
        },
    );

    match super::slash_command_services::execute_runtime_config_command_via_daemon(
        &state.daemon_url,
        request,
    )
    .await
    {
        Ok(response) => {
            super::slash_command_services::apply_runtime_config_response(
                response, state, tui_rt, event_tx,
            )
            .await;
        }
        Err(err) => {
            push_obs(state, format!("⚠ verify policy command failed: {err}"));
        }
    }

    EventOutcome::Continue
}

fn build_artifact_command_spec(cmd: &str, args: &[&str]) -> Option<ArtifactCommandSpec> {
    match cmd {
        "/artifact" => Some(ArtifactCommandSpec::Lookup {
            query: joined_query(args),
        }),
        "/artifact-chunks" => Some(ArtifactCommandSpec::Chunks {
            query: joined_query(args),
        }),
        "/artifact-list" => Some(ArtifactCommandSpec::List {
            limit: parse_usize_with_bounds(args.first().copied(), 20, 1, 200),
        }),
        "/artifact-maintain" => Some(ArtifactCommandSpec::Maintain {
            max_per_session: parse_usize_with_bounds(args.first().copied(), 200, 1, 10_000),
            max_age_days: parse_i64_with_bounds(args.get(1).copied(), 14, 1, 3650),
        }),
        "/artifact-extract" => Some(ArtifactCommandSpec::Extract {
            query: joined_query(args),
        }),
        "/artifact-extractions" => Some(ArtifactCommandSpec::Extractions {
            limit: parse_usize_with_bounds(args.first().copied(), 20, 1, 200),
        }),
        "/artifact-pack" => Some(ArtifactCommandSpec::Pack {
            artifact_query: args
                .first()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "last".to_string()),
            max_tokens: parse_usize_with_bounds(args.get(1).copied(), 3200, 256, 200_000),
            max_claims: parse_usize_with_bounds(args.get(2).copied(), 6, 1, 64),
            max_chunks: parse_usize_with_bounds(args.get(3).copied(), 12, 1, 512),
        }),
        "/artifact-packs" => Some(ArtifactCommandSpec::Packs {
            limit: parse_usize_with_bounds(args.first().copied(), 20, 1, 200),
        }),
        "/artifact-pack-use" => Some(ArtifactCommandSpec::PackUse {
            query: joined_query(args),
        }),
        "/artifact-pack-auto" => Some(ArtifactCommandSpec::PackAuto),
        "/artifact-verify" => Some(ArtifactCommandSpec::Verify {
            query: joined_query(args),
        }),
        "/artifact-verifications" => Some(ArtifactCommandSpec::Verifications {
            limit: parse_usize_with_bounds(args.first().copied(), 20, 1, 200),
        }),
        "/artifact-verification" => Some(ArtifactCommandSpec::Verification {
            query: joined_query(args),
        }),
        _ => None,
    }
}

fn build_verification_policy_input_if_needed(
    command: &ArtifactCommandSpec,
    state: &TuiState,
) -> Option<ArtifactVerificationPolicyInput> {
    if !matches!(command, ArtifactCommandSpec::Verify { .. }) {
        return None;
    }

    Some(ArtifactVerificationPolicyInput {
        min_citation_coverage: super::parse_f32_with_bounds(
            &state.settings.verifier_min_citation_coverage,
            0.60,
            0.0,
            1.0,
        ),
        min_avg_support_strength: super::parse_f32_with_bounds(
            &state.settings.verifier_min_avg_support_strength,
            0.70,
            0.0,
            1.0,
        ),
        min_supported_claim_ratio: super::parse_f32_with_bounds(
            &state.settings.verifier_min_supported_claim_ratio,
            0.60,
            0.0,
            1.0,
        ),
        min_claim_support_strength: super::parse_f32_with_bounds(
            &state.settings.verifier_min_claim_support_strength,
            0.65,
            0.0,
            1.0,
        ),
    })
}

fn build_verifier_route_label_if_needed(
    command: &ArtifactCommandSpec,
    state: &TuiState,
) -> Option<String> {
    if !matches!(command, ArtifactCommandSpec::Verify { .. }) {
        return None;
    }

    state
        .stage_routing
        .get("verifier")
        .map(|route| format!("{}:{} policy={}", route.provider, route.model, route.policy_profile))
        .or_else(|| Some("default".to_string()))
}

async fn execute_artifact_command_via_daemon(
    daemon_url: &str,
    request: ArtifactCommandRequest,
) -> Result<(ArtifactCommandResponse, Option<String>), String> {
    daemon_artifact_command(daemon_url, &request)
        .await
        .map(|response| (response, None))
        .map_err(|err| {
            format!("daemon error: {}", truncate_error(&err.to_string(), 200))
        })
}

fn joined_query(args: &[&str]) -> Option<String> {
    let joined = args.join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_usize_with_bounds(value: Option<&str>, default: usize, min: usize, max: usize) -> usize {
    value
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn parse_i64_with_bounds(value: Option<&str>, default: i64, min: i64, max: i64) -> i64 {
    value
        .and_then(|raw| raw.parse::<i64>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn truncate_error(value: &str, max_chars: usize) -> String {
    let out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        format!("{out}...")
    } else {
        out
    }
}
