use std::sync::Arc;

use anyhow::{Result, anyhow};
use axum::Router;
use stasis::dashboard::{DashboardState, RuntimeDashboardQueryService, router as dashboard_router};

use crate::daemon::state::AppState;

#[derive(Clone, Debug, Default)]
pub struct DashboardActionAuthConfig {
    pub bearer_token: Option<String>,
    pub required_role: Option<String>,
    pub role_claim_header: Option<String>,
}

pub fn parse_dashboard_action_auth(args: &[String]) -> Result<DashboardActionAuthConfig> {
    let bearer_token = parse_arg_or_env(
        args,
        "--dashboard-action-bearer-token",
        "MEDOUSA_DASHBOARD_ACTION_BEARER_TOKEN",
    );
    let required_role = parse_arg_or_env(
        args,
        "--dashboard-action-required-role",
        "MEDOUSA_DASHBOARD_ACTION_REQUIRED_ROLE",
    );
    let role_claim_header = parse_arg_or_env(
        args,
        "--dashboard-action-role-claim-header",
        "MEDOUSA_DASHBOARD_ACTION_ROLE_CLAIM_HEADER",
    );

    if role_claim_header.is_some() && required_role.is_none() {
        return Err(anyhow!(
            "dashboard action role claim header requires --dashboard-action-required-role"
        ));
    }

    if let Some(header) = role_claim_header.as_ref()
        && header.chars().any(char::is_whitespace)
    {
        return Err(anyhow!(
            "dashboard action role claim header must not contain whitespace"
        ));
    }

    Ok(DashboardActionAuthConfig {
        bearer_token,
        required_role,
        role_claim_header,
    })
}

pub fn apply_dashboard_action_auth(
    mut state: DashboardState,
    config: &DashboardActionAuthConfig,
) -> DashboardState {
    if let Some(token) = config.bearer_token.as_deref() {
        state = state.with_action_auth_bearer_token(token);
    }
    if let Some(role) = config.required_role.as_deref() {
        state = state.with_action_required_role(role);
    }
    if let Some(header_name) = config.role_claim_header.as_deref() {
        state = state.with_action_role_claim_header(header_name);
    }
    state
}

/// Catalog, capability, grapheme, workflow, vault, workspace, budget, and dashboard routers.
pub fn build_feature_routers(
    state: &AppState,
    dashboard_action_auth: &DashboardActionAuthConfig,
) -> Router {
    let catalog_router = crate::manuscript_handlers::manuscript_router();

    let capability_router = Router::new()
        .route(
            "/v1/capabilities",
            axum::routing::get(crate::mcp_daemon_handlers::list_capabilities),
        )
        .route(
            "/v1/capabilities/{capability_id}",
            axum::routing::get(crate::mcp_daemon_handlers::get_capability),
        )
        .route(
            "/v1/capabilities/reindex",
            axum::routing::post(crate::mcp_daemon_handlers::reindex_capabilities),
        )
        .with_state(crate::mcp_daemon_handlers::CapabilityApiState {
            agent_runtime: state.platform.agent_handle(),
        });

    let grapheme_router = crate::grapheme_handlers::grapheme_router(
        crate::grapheme_handlers::GraphemeApiState {
            composition: Arc::new(state.composition().clone()),
        },
    );

    let workflow_state = crate::workflow_handlers::WorkflowApiState {
        composition: Arc::new(state.composition().clone()),
    };
    let workflow_router = crate::workflow_handlers::workflow_router(workflow_state.clone());
    let tool_history_router = crate::tool_history_handlers::tool_history_router(workflow_state);

    let policy_router = Router::new()
        .route(
            "/v1/mcp/policy/evaluate",
            axum::routing::post(crate::mcp_daemon_handlers::mcp_policy_evaluate),
        )
        .with_state(crate::mcp_daemon_handlers::McpPolicyApiState {
            identity_service: state.identity_service.clone(),
        });

    let vault_router = Router::new()
        .route(
            "/v1/vault/roots",
            axum::routing::get(crate::vault_handlers::list_vault_roots)
                .post(crate::vault_handlers::add_vault_root_handler),
        )
        .route(
            "/v1/vault/active",
            axum::routing::put(crate::vault_handlers::set_vault_active_root),
        )
        .route(
            "/v1/vault/notes",
            axum::routing::get(crate::vault_handlers::list_vault_notes)
                .post(crate::vault_handlers::post_vault_note),
        )
        .route(
            "/v1/vault/tags",
            axum::routing::get(crate::vault_handlers::list_vault_tags),
        )
        .route(
            "/v1/vault/search",
            axum::routing::get(crate::vault_handlers::search_vault_notes),
        )
        .route(
            "/v1/vault/backlinks",
            axum::routing::get(crate::vault_handlers::get_vault_backlinks),
        )
        .route(
            "/v1/vault/notes/{*note_path}",
            axum::routing::get(crate::vault_handlers::get_vault_note)
                .put(crate::vault_handlers::put_vault_note)
                .delete(crate::vault_handlers::delete_vault_note),
        );

    let workspace_router = Router::new()
        .route(
            "/v1/workspace/cards",
            axum::routing::get(crate::workspace_handlers::list_workspace_cards),
        )
        .route(
            "/v1/workspace/cards/{card_id}",
            axum::routing::get(crate::workspace_handlers::get_workspace_card),
        )
        .route(
            "/v1/workspace/cards/{card_id}/cancel",
            axum::routing::post(crate::workspace_handlers::cancel_workspace_card),
        )
        .route(
            "/v1/workspace/cards/{card_id}/archive",
            axum::routing::post(crate::workspace_handlers::archive_workspace_card),
        )
        .route(
            "/v1/workspace/cards/{card_id}/link-vault",
            axum::routing::post(crate::workspace_handlers::link_workspace_card_vault),
        )
        .route(
            "/v1/workspace/feed",
            axum::routing::get(crate::workspace_handlers::list_workspace_feed),
        )
        .route(
            "/v1/workspace/snapshot",
            axum::routing::get(crate::workspace_handlers::get_workspace_snapshot),
        )
        .route(
            "/v1/workspace/rebuild",
            axum::routing::post(crate::workspace_handlers::rebuild_workspace),
        )
        .route(
            "/v1/workspace/stream",
            axum::routing::get(crate::workspace_handlers::workspace_stream),
        )
        .with_state(crate::workspace_handlers::WorkspaceHandlerState {
            composition: Arc::new(state.composition().clone()),
            worker_id: state.worker_id.clone(),
        });

    let budget_router = Router::new()
        .route(
            "/v1/turns/budget-requests",
            axum::routing::get(crate::turn_budget_handlers::list_turn_budget_requests),
        )
        .route(
            "/v1/turns/budget-requests/{request_id}",
            axum::routing::get(crate::turn_budget_handlers::get_turn_budget_request),
        )
        .route(
            "/v1/turns/budget-requests/{request_id}/approve",
            axum::routing::post(crate::turn_budget_handlers::approve_turn_budget_request),
        )
        .route(
            "/v1/turns/budget-requests/{request_id}/deny",
            axum::routing::post(crate::turn_budget_handlers::deny_turn_budget_request),
        )
        .with_state(crate::turn_budget_handlers::TurnBudgetHandlerState);

    let dashboard_service = Arc::new(RuntimeDashboardQueryService::from_runtime_composition(
        state.composition().clone(),
    ));
    let dashboard_state =
        apply_dashboard_action_auth(DashboardState::new(dashboard_service), dashboard_action_auth);
    let dashboard = dashboard_router(dashboard_state);

    catalog_router
        .merge(capability_router)
        .merge(grapheme_router)
        .merge(workflow_router)
        .merge(tool_history_router)
        .merge(policy_router)
        .merge(vault_router)
        .merge(crate::locus_handlers::locus_router(
            state.platform.agent_handle().locus_store.clone(),
            state.platform.agent_handle().semantic_index.clone(),
            state.platform.agent_handle().memory_reader.clone(),
        ))
        .merge(workspace_router)
        .merge(budget_router)
        .merge(crate::local_inference_handlers::routes())
        .merge(crate::model_capability_registry::handlers::routes())
        .merge(crate::inference_profiles_handlers::routes())
        .merge(crate::stt_handlers::routes())
        .merge(dashboard)
}

fn parse_arg_or_env(args: &[String], arg_key: &str, env_key: &str) -> Option<String> {
    find_arg_value(args, arg_key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            std::env::var(env_key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn find_arg_value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == key)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}
