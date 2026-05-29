use medousa::{StageRouteCommandRequest, StageRouteCommandResponse, StageRouteCommandSpec};

use super::daemon_commands::daemon_stage_route_command;
use super::*;

pub(crate) async fn handle_stage_route_family_command(
    cmd: &str,
    args: Vec<&str>,
    state: &mut TuiState,
) -> EventOutcome {
    let command = match build_stage_route_command_spec(cmd, &args) {
        Ok(command) => command,
        Err(err) => {
            push_obs(state, err);
            return EventOutcome::Continue;
        }
    };

    let request = StageRouteCommandRequest {
        stage_routing: state.stage_routing.clone(),
        provider: state.settings.provider.clone(),
        model: state.settings.model.clone(),
        command,
    };

    match execute_stage_route_command_with_daemon_fallback(&state.daemon_url, request).await {
        Ok((response, backend_notice)) => {
            let did_change_routing = response.stage_routing != state.stage_routing;
            state.stage_routing = response.stage_routing;
            if let Some(notice) = backend_notice {
                push_obs(state, notice);
            }
            push_obs(state, response.rendered_output);
            if did_change_routing {
                persist_stage_routing_defaults(state);
            }
        }
        Err(err) => {
            push_obs(state, format!("⚠ stage route command failed: {err}"));
        }
    }

    EventOutcome::Continue
}

fn build_stage_route_command_spec(
    cmd: &str,
    args: &[&str],
) -> Result<StageRouteCommandSpec, String> {
    match cmd {
        "/stage-routes" => Ok(StageRouteCommandSpec::Routes {
            role: args
                .first()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        }),
        "/stage-route-set" => {
            if args.len() < 2 {
                return Err(
                    "⚠ usage: /stage-route-set <role> <provider:model|model> [policy_profile] [fallback_csv]"
                        .to_string(),
                );
            }

            Ok(StageRouteCommandSpec::Set {
                role: args[0].trim().to_string(),
                target: args[1].trim().to_string(),
                policy_profile: args
                    .get(2)
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
                fallback_chain: args.get(3).map(|fallback_csv| {
                    fallback_csv
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                }),
            })
        }
        "/stage-route-reset" => Ok(StageRouteCommandSpec::Reset),
        _ => Err("⚠ unknown stage route command".to_string()),
    }
}

async fn execute_stage_route_command_with_daemon_fallback(
    daemon_url: &str,
    request: StageRouteCommandRequest,
) -> Result<(StageRouteCommandResponse, Option<String>), String> {
    match daemon_stage_route_command(daemon_url, &request).await {
        Ok(response) => Ok((response, None)),
        Err(daemon_err) => {
            let daemon_err_text = truncate_error(&daemon_err.to_string(), 140);
            let local = medousa::stage_route_command_runtime::execute_stage_route_command(request)
                .map_err(|local_err| {
                    format!(
                        "daemon_error={} | local_error={}",
                        daemon_err_text,
                        truncate_error(&local_err.to_string(), 180)
                    )
                })?;
            Ok((
                local,
                Some(format!(
                    "◈ stage route runtime backend=local fallback daemon_error={daemon_err_text}"
                )),
            ))
        }
    }
}

fn truncate_error(value: &str, max_chars: usize) -> String {
    let out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        format!("{out}...")
    } else {
        out
    }
}

fn persist_stage_routing_defaults(state: &TuiState) {
    let mut defaults = load_tui_defaults();
    defaults.stage_routing = Some(state.stage_routing.clone());
    save_tui_defaults(&defaults);
}
