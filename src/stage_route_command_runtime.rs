use anyhow::Result;

use crate::daemon_api::{
    StageRouteCommandRequest, StageRouteCommandResponse, StageRouteCommandSpec,
};
use crate::stage_routing::StageRoutingMatrix;

pub fn execute_stage_route_command(
    request: StageRouteCommandRequest,
) -> Result<StageRouteCommandResponse> {
    let mut stage_routing = request.stage_routing;

    let rendered_output = match request.command {
        StageRouteCommandSpec::Routes { role } => {
            let role = normalize_optional_text(role);
            if let Some(role) = role {
                match stage_routing.get(&role) {
                    Some(route) => {
                        let rendered =
                            serde_json::to_string_pretty(route).unwrap_or_else(|_| "{}".to_string());
                        format!("◈ stage route {}\n{}", route.role, rendered)
                    }
                    None => format!(
                        "⚠ unknown stage role '{}'. roles={}",
                        role,
                        StageRoutingMatrix::roles().join(",")
                    ),
                }
            } else {
                let rendered = serde_json::to_string_pretty(&stage_routing)
                    .unwrap_or_else(|_| "{}".to_string());
                format!("◈ stage routing matrix\n{}", rendered)
            }
        }
        StageRouteCommandSpec::Set {
            role,
            target,
            policy_profile,
            fallback_chain,
        } => {
            let role = role.trim().to_string();
            let Some(route) = stage_routing.get_mut(&role) else {
                return Ok(StageRouteCommandResponse {
                    stage_routing,
                    rendered_output: format!(
                        "⚠ unknown stage role '{}'. roles={}",
                        role,
                        StageRoutingMatrix::roles().join(",")
                    ),
                });
            };

            let target = target.trim();
            if let Some((provider, model)) = target.split_once(':') {
                route.provider = provider.trim().to_string();
                route.model = model.trim().to_string();
            } else {
                route.model = target.to_string();
            }

            if let Some(policy) = normalize_optional_text(policy_profile) {
                route.policy_profile = policy;
            }
            if let Some(chain) = fallback_chain {
                route.fallback_chain = sanitize_fallback_chain(chain);
            }

            format!(
                "◈ stage route updated role={} target={}:{} policy={} fallback={}",
                route.role,
                route.provider,
                route.model,
                route.policy_profile,
                route.fallback_chain.join(","),
            )
        }
        StageRouteCommandSpec::Reset => {
            let provider = request.provider.trim();
            let model = request.model.trim();
            stage_routing = StageRoutingMatrix::default_for(provider, model);
            format!(
                "◈ stage routing reset to provider={} model={} defaults",
                provider, model
            )
        }
    };

    Ok(StageRouteCommandResponse {
        stage_routing,
        rendered_output,
    })
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

fn sanitize_fallback_chain(chain: Vec<String>) -> Vec<String> {
    chain
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
}
