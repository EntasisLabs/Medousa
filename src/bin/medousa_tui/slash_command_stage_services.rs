use super::*;

pub(crate) fn handle_stage_routes_command(args: Vec<&str>, state: &mut TuiState) -> EventOutcome {
    let role = args
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    if let Some(role) = role {
        match state.stage_routing.get(role) {
            Some(route) => {
                let rendered =
                    serde_json::to_string_pretty(route).unwrap_or_else(|_| "{}".to_string());
                push_obs(state, format!("◈ stage route {}\n{}", route.role, rendered));
            }
            None => push_obs(
                state,
                format!(
                    "⚠ unknown stage role '{}'. roles={}",
                    role,
                    medousa::stage_routing::StageRoutingMatrix::roles().join(",")
                ),
            ),
        }
    } else {
        let rendered =
            serde_json::to_string_pretty(&state.stage_routing).unwrap_or_else(|_| "{}".to_string());
        push_obs(state, format!("◈ stage routing matrix\n{}", rendered));
    }

    EventOutcome::Continue
}

pub(crate) fn handle_stage_route_set_command(args: Vec<&str>, state: &mut TuiState) -> EventOutcome {
    if args.len() < 2 {
        push_obs(
            state,
            "⚠ usage: /stage-route-set <role> <provider:model|model> [policy_profile] [fallback_csv]"
                .to_string(),
        );
        return EventOutcome::Continue;
    }

    let role = args[0].trim();
    let (route_role, route_provider, route_model, route_policy, route_fallback) = {
        let Some(route) = state.stage_routing.get_mut(role) else {
            push_obs(
                state,
                format!(
                    "⚠ unknown stage role '{}'. roles={}",
                    role,
                    medousa::stage_routing::StageRoutingMatrix::roles().join(",")
                ),
            );
            return EventOutcome::Continue;
        };

        let target = args[1].trim();
        if let Some((provider, model)) = target.split_once(':') {
            route.provider = provider.trim().to_string();
            route.model = model.trim().to_string();
        } else {
            route.model = target.to_string();
        }
        if let Some(policy) = args.get(2) {
            route.policy_profile = policy.trim().to_string();
        }
        if let Some(fallback_csv) = args.get(3) {
            route.fallback_chain = fallback_csv
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>();
        }

        (
            route.role.clone(),
            route.provider.clone(),
            route.model.clone(),
            route.policy_profile.clone(),
            route.fallback_chain.join(","),
        )
    };

    persist_stage_routing_defaults(state);
    push_obs(
        state,
        format!(
            "◈ stage route updated role={} target={}:{} policy={} fallback={}",
            route_role, route_provider, route_model, route_policy, route_fallback,
        ),
    );

    EventOutcome::Continue
}

pub(crate) fn handle_stage_route_reset_command(state: &mut TuiState) -> EventOutcome {
    state.stage_routing = medousa::stage_routing::StageRoutingMatrix::default_for(
        &state.settings.provider,
        &state.settings.model,
    );
    persist_stage_routing_defaults(state);
    push_obs(
        state,
        format!(
            "◈ stage routing reset to provider={} model={} defaults",
            state.settings.provider, state.settings.model
        ),
    );

    EventOutcome::Continue
}

fn persist_stage_routing_defaults(state: &TuiState) {
    let mut defaults = load_tui_defaults();
    defaults.stage_routing = Some(state.stage_routing.clone());
    save_tui_defaults(&defaults);
}
