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
            "/v1/capabilities/intents",
            axum::routing::get(crate::mcp_daemon_handlers::list_capability_intents),
        )
        .route(
            "/v1/capabilities/reindex",
            axum::routing::post(crate::mcp_daemon_handlers::reindex_capabilities),
        )
        .route(
            "/v1/mcp/gateway/status",
            axum::routing::get(crate::mcp_daemon_handlers::mcp_gateway_status),
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

    let calendar_router = Router::new()
        .route(
            "/v1/calendar/events",
            axum::routing::get(crate::calendar_handlers::list_calendar_events)
                .post(crate::calendar_handlers::create_calendar_event),
        )
        .route(
            "/v1/calendar/events/{uid}",
            axum::routing::put(crate::calendar_handlers::update_calendar_event)
                .delete(crate::calendar_handlers::delete_calendar_event),
        )
        .route(
            "/v1/calendar/import",
            axum::routing::post(crate::calendar_handlers::import_calendar),
        )
        .route(
            "/v1/calendar/export",
            axum::routing::get(crate::calendar_handlers::export_calendar),
        );

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
            "/v1/vault/files/{*file_path}",
            axum::routing::get(crate::vault_handlers::get_vault_file),
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

    let environment_router = crate::environment_handlers::environment_router(
        crate::environment_handlers::EnvironmentApiState {
            hub: crate::environment_store::environment_hub(),
            runtime: Some(Arc::new(state.composition().clone())),
        },
    );

    catalog_router
        .merge(capability_router)
        .merge(grapheme_router)
        .merge(workflow_router)
        .merge(tool_history_router)
        .merge(policy_router)
        .merge(calendar_router)
        .merge(vault_router)
        .merge(crate::locus_handlers::locus_router(
            state.platform.agent_handle().locus_store.clone(),
            state.platform.agent_handle().semantic_index.clone(),
            state.platform.agent_handle().memory_reader.clone(),
        ))
        .merge(workspace_router)
        .merge(environment_router)
        .merge(crate::feed_handlers::feed_router())
        .merge(crate::component_store_handlers::component_store_router())
        .merge(crate::component_runtime_handlers::component_runtime_router())
        .merge(budget_router)
        .merge(crate::local_inference_handlers::routes())
        .merge(crate::model_capability_registry::handlers::routes())
        .merge(crate::inference_profiles_handlers::routes())
        .merge(crate::daemon::runtime_tui_defaults::routes())
        .merge(crate::stt_handlers::routes())
        .merge(crate::lan_handlers::lan_router())
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

/// Core daemon API routes (health, jobs, sessions, interactive, identity, ingest, continuations).
pub fn build_core_router(state: AppState) -> Router {
    use axum::routing::{delete, get, patch, post, put};

    use crate::maintenance_handlers::{
        get_artifact_retention_status, update_artifact_retention,
    };
    use crate::daemon::continuations::{
        continuation_lineage, continuation_status, replay_and_resume_job,
    };
    use crate::daemon::core::{
        artifact_command, artifact_delete, artifact_fetch, artifact_list_ui, artifact_write, health,
        heartbeat_status, runtime_config_command, runtime_defaults, stats, stage_route_command,
    };
    use crate::daemon::identity::{
        create_user_profile, export_user_profile, identity_commit_update, identity_digest_preview,
        identity_export_markdown, identity_get_context, identity_list_history, identity_propose_update,
        identity_remember, identity_rollback_version, import_user_profile, list_user_profiles,
        set_active_user_profile,
    };
    use crate::daemon::ingest::{
        deliver_outbox_webhook, deliver_poll, delivery_status, ingest_handler, ingest_stream,
    };
    use crate::daemon::interactive::{
        cancel_active_session_turn, create_turn_ticket, delete_session_handler,
        get_active_session_turn, get_turn_ticket, interactive_turn_stream, list_session_turns,
        start_interactive_turn,
    };
    use crate::daemon::jobs::{
        archive_ask_job, complete_ask_job_actions, delete_recurring_definition, enqueue_ask,
        enqueue_prompt, enqueue_report, get_job_report, get_job_result, get_recurring_delivery_handler,
        list_recurring_definitions, list_recurring_runs_handler, register_recurring_prompt,
        retry_workspace_card, update_recurring_definition,
    };

    Router::new()
        .route("/health", get(health))
        .route("/v1/stats", get(stats))
        .route("/v1/runtime/defaults", get(runtime_defaults))
        .route("/v1/sessions", get(crate::daemon_handlers::list_session_history))
        .route(
            "/v1/sessions/{session_id}/history",
            get(crate::daemon_handlers::get_session_history),
        )
        .route(
            "/v1/sessions/{session_id}/turns",
            post(crate::daemon_handlers::append_session_turn),
        )
        .route(
            "/v1/sessions/{session_id}/name",
            put(crate::daemon_handlers::set_session_display_name),
        )
        .route("/v1/sessions/{session_id}", delete(delete_session_handler))
        .route(
            "/v1/sessions/{session_id}/active-turn",
            get(get_active_session_turn).post(cancel_active_session_turn),
        )
        .route(
            "/v1/sessions/{session_id}/workshop/steer",
            post(crate::daemon::workshop_steer::steer_bound_workshop_handler),
        )
        .route("/v1/sessions/{session_id}/turns", get(list_session_turns))
        .route("/v1/turns", post(create_turn_ticket))
        .route("/v1/turns/{turn_id}", get(get_turn_ticket))
        .route("/v1/heartbeat/status", get(heartbeat_status))
        .route("/v1/jobs/{job_id}/result", get(get_job_result))
        .route("/v1/jobs/{job_id}/report", get(get_job_report))
        .route(
            "/v1/jobs/{job_id}/complete-actions",
            post(complete_ask_job_actions),
        )
        .route("/v1/jobs/{job_id}/archive", post(archive_ask_job))
        .route("/v1/jobs/ask", post(enqueue_ask))
        .route("/v1/jobs/report", post(enqueue_report))
        .route("/v1/jobs/prompt", post(enqueue_prompt))
        .route("/v1/recurring", get(list_recurring_definitions))
        .route("/v1/recurring/prompt", post(register_recurring_prompt))
        .route(
            "/v1/recurring/{recurring_id}",
            patch(update_recurring_definition).delete(delete_recurring_definition),
        )
        .route(
            "/v1/recurring/{recurring_id}/runs",
            get(list_recurring_runs_handler),
        )
        .route(
            "/v1/recurring/{recurring_id}/delivery",
            get(get_recurring_delivery_handler),
        )
        .route("/v1/interactive/turn", post(start_interactive_turn))
        .route(
            "/v1/interactive/turn/{turn_id}/stream",
            get(interactive_turn_stream),
        )
        .route(
            "/v1/agents/runtimes",
            get(crate::daemon::agents::list_agent_runtimes),
        )
        .route(
            "/v1/agents/sessions",
            post(crate::daemon::agents::create_agent_session),
        )
        .route(
            "/v1/agents/sessions/{agent_session_id}/prompt",
            post(crate::daemon::agents::prompt_agent_session),
        )
        .route(
            "/v1/agents/sessions/{agent_session_id}/stream",
            get(crate::daemon::agents::agent_session_stream),
        )
        .route(
            "/v1/agents/sessions/{agent_session_id}/cancel",
            post(crate::daemon::agents::cancel_agent_session),
        )
        .route(
            "/v1/agents/permission-requests",
            get(crate::daemon::agents::list_agent_permission_requests),
        )
        .route(
            "/v1/agents/permission-requests/{request_id}/approve",
            post(crate::daemon::agents::approve_agent_permission_request),
        )
        .route(
            "/v1/agents/permission-requests/{request_id}/deny",
            post(crate::daemon::agents::deny_agent_permission_request),
        )
        .route("/v1/runtime/artifact/command", post(artifact_command))
        .route("/v1/runtime/artifact/fetch", post(artifact_fetch))
        .route("/v1/runtime/artifact/write", post(artifact_write))
        .route("/v1/runtime/artifact/delete", post(artifact_delete))
        .route("/v1/runtime/artifact/list-ui", post(artifact_list_ui))
        .route(
            "/v1/maintenance/artifacts",
            get(get_artifact_retention_status).put(update_artifact_retention),
        )
        .route("/v1/runtime/config/command", post(runtime_config_command))
        .route("/v1/runtime/stage-route/command", post(stage_route_command))
        .route("/v1/identity/context", post(identity_get_context))
        .route("/v1/identity/remember", post(identity_remember))
        .route("/v1/identity/digest-preview", post(identity_digest_preview))
        .route("/v1/identity/export-markdown", post(identity_export_markdown))
        .route(
            "/v1/identity/profiles",
            get(list_user_profiles).post(create_user_profile),
        )
        .route("/v1/identity/profiles/active", put(set_active_user_profile))
        .route("/v1/identity/profiles/export", post(export_user_profile))
        .route("/v1/identity/profiles/import", post(import_user_profile))
        .route("/v1/identity/update/propose", post(identity_propose_update))
        .route("/v1/identity/update/commit", post(identity_commit_update))
        .route("/v1/identity/history", post(identity_list_history))
        .route("/v1/identity/rollback", post(identity_rollback_version))
        .route("/v1/ingest", post(ingest_handler))
        .route("/v1/ingest/{stream_id}/stream", get(ingest_stream))
        .route("/v1/media/upload", post(crate::media_handlers::upload_media))
        .route("/v1/media/{media_id}", get(crate::media_handlers::get_media))
        .route("/v1/deliver/outbox", post(deliver_outbox_webhook))
        .route("/v1/deliver/poll/{job_id}", get(deliver_poll))
        .route("/v1/delivery/status", get(delivery_status))
        .route("/v1/continuations/status", get(continuation_status))
        .route(
            "/v1/continuations/lineage/{turn_correlation_id}",
            get(continuation_lineage),
        )
        .route(
            "/v1/jobs/{job_id}/replay-and-resume",
            post(replay_and_resume_job),
        )
        .route(
            "/v1/workspace/cards/{card_id}/retry",
            post(retry_workspace_card),
        )
        .merge(crate::browser_handlers::browser_router())
        .with_state(state)
}

/// Full daemon HTTP surface: core routes plus feature routers (catalog, vault, dashboard, …).
pub fn build_daemon_router(
    state: AppState,
    dashboard_action_auth: &DashboardActionAuthConfig,
) -> Router {
    build_core_router(state.clone()).merge(build_feature_routers(&state, dashboard_action_auth))
}
