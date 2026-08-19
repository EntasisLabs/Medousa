use std::sync::Arc;

use anyhow::{Result, anyhow};
use axum::Router;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use stasis::dashboard::{DashboardState, RuntimeDashboardQueryService, router as dashboard_router};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RouteInventory, RoutePolicy,
};
use crate::daemon::state::AppState;

const LIVENESS_BODY: &str = r#"{"status":"ok","apiVersion":"v1"}"#;

/// Constant-size anonymous liveness surface. Detailed runtime health is
/// protected at `/v1/health`.
pub fn build_liveness_router() -> Router {
    build_liveness_surface().into_router()
}

pub fn build_liveness_surface() -> DeclaredRouter {
    DeclaredRouter::default().route(
        RoutePolicy {
            method: axum::http::Method::GET,
            path: "/health",
            group: RouteGroup::Liveness,
            required_capability: None,
            bootstrap_public: true,
            browser_policy: BrowserPolicy::Public,
            body_limit: 1024,
            rate_limit_class: RateLimitClass::Liveness,
        },
        axum::routing::get(|| async {
            ([(CONTENT_TYPE, "application/json")], LIVENESS_BODY).into_response()
        }),
    )
}

/// Inventory for every route currently migrated to declared policy assembly.
/// The same surface constructors build the production routers, so policy and
/// handler registration cannot drift into separate handwritten lists.
pub fn build_declared_route_inventory(pairing_enabled: bool) -> RouteInventory {
    let mut inventory = RouteInventory::default();
    inventory
        .extend(build_liveness_surface().inventory())
        .expect("duplicate liveness route policy");
    if pairing_enabled {
        inventory
            .extend(crate::pairing_handlers::bootstrap_surface().inventory())
            .expect("duplicate pairing bootstrap route policy");
        inventory
            .extend(crate::pairing_handlers::protected_surface().inventory())
            .expect("duplicate protected pairing route policy");
    }
    inventory
        .extend(crate::share_handlers::share_surface().inventory())
        .expect("duplicate share route policy");
    inventory
        .extend(crate::peer_message_handlers::peer_message_surface().inventory())
        .expect("duplicate peer message route policy");
    inventory
        .extend(crate::mesh::handlers::mesh_surface().inventory())
        .expect("duplicate mesh route policy");
    inventory
        .extend(build_identity_surface().inventory())
        .expect("duplicate identity route policy");
    inventory
        .extend(build_runtime_admin_surface().inventory())
        .expect("duplicate runtime administration route policy");
    inventory
        .extend(crate::daemon::runtime_tui_defaults::surface().inventory())
        .expect("duplicate runtime defaults route policy");
    inventory
        .extend(crate::local_credential_handlers::surface().inventory())
        .expect("duplicate local credential route policy");
    inventory
        .extend(crate::inference_profiles_handlers::surface().inventory())
        .expect("duplicate inference profiles route policy");
    inventory
        .extend(crate::integration_handlers::surface().inventory())
        .expect("duplicate integration route policy");
    inventory
        .extend(crate::environment_handlers::environment_surface().inventory())
        .expect("duplicate environment route policy");
    inventory
        .extend(crate::mcp_daemon_handlers::gateway_status_surface().inventory())
        .expect("duplicate MCP gateway status route policy");
    inventory
        .extend(crate::mcp_daemon_handlers::policy_surface().inventory())
        .expect("duplicate MCP policy route policy");
    inventory
        .extend(crate::vault_handlers::vault_surface().inventory())
        .expect("duplicate vault route policy");
    inventory
        .extend(crate::daemon::agents::permission_surface().inventory())
        .expect("duplicate agent permission route policy");
    inventory
        .extend(crate::daemon::jobs::recurring_surface().inventory())
        .expect("duplicate recurring route policy");
    inventory
        .extend(crate::workspace_handlers::workspace_surface().inventory())
        .expect("duplicate workspace route policy");
    inventory
        .extend(crate::daemon::jobs::workspace_retry_surface().inventory())
        .expect("duplicate workspace retry route policy");
    inventory
        .extend(build_workshop_surface().inventory())
        .expect("duplicate workshop route policy");
    inventory
        .extend(build_core_service_surface().inventory())
        .expect("duplicate core service route policy");
    inventory
        .extend(crate::mcp_daemon_handlers::capability_surface().inventory())
        .expect("duplicate capability route policy");
    inventory
        .extend(crate::turn_budget_handlers::budget_surface().inventory())
        .expect("duplicate turn budget route policy");
    inventory
        .extend(crate::calendar_handlers::calendar_surface().inventory())
        .expect("duplicate calendar route policy");
    inventory
        .extend(crate::manuscript_handlers::manuscript_surface().inventory())
        .expect("duplicate manuscript route policy");
    inventory
        .extend(crate::locus_handlers::locus_surface().inventory())
        .expect("duplicate locus route policy");
    inventory
        .extend(crate::feed_handlers::feed_surface().inventory())
        .expect("duplicate feed route policy");
    inventory
        .extend(crate::component_store_handlers::component_store_surface().inventory())
        .expect("duplicate component store route policy");
    inventory
        .extend(crate::workflow_handlers::workflow_surface().inventory())
        .expect("duplicate workflow route policy");
    inventory
        .extend(crate::tool_history_handlers::tool_history_surface().inventory())
        .expect("duplicate tool history route policy");
    inventory
        .extend(crate::grapheme_handlers::grapheme_surface().inventory())
        .expect("duplicate Grapheme route policy");
    inventory
        .extend(crate::local_inference_handlers::surface().inventory())
        .expect("duplicate local inference route policy");
    inventory
        .extend(crate::model_capability_registry::handlers::surface().inventory())
        .expect("duplicate model catalog route policy");
    inventory
        .extend(crate::stt_handlers::surface().inventory())
        .expect("duplicate STT route policy");
    inventory
        .extend(crate::lan_handlers::lan_surface().inventory())
        .expect("duplicate LAN discovery route policy");
    inventory
        .extend(crate::component_runtime_handlers::component_runtime_surface().inventory())
        .expect("duplicate component runtime route policy");
    inventory
        .extend(crate::daemon::coding_engine_host::coding_engine_surface().inventory())
        .expect("duplicate coding engine route policy");
    inventory
        .extend(crate::daemon::shell_session_host::shell_session_surface().inventory())
        .expect("duplicate shell session route policy");
    inventory
        .extend(crate::daemon::detamu_host::world_surface().inventory())
        .expect("duplicate world model route policy");
    inventory
        .extend(crate::daemon::forge_api::forge_surface().inventory())
        .expect("duplicate Forge route policy");
    inventory
        .extend(crate::daemon::forge_preview::forge_preview_surface().inventory())
        .expect("duplicate Forge preview route policy");
    inventory
        .extend(crate::browser_handlers::browser_surface().inventory())
        .expect("duplicate browser bridge route policy");
    inventory
}

pub fn build_identity_surface() -> DeclaredRouter<AppState> {
    use axum::routing::{get, post, put};

    use crate::daemon::identity::{
        create_user_profile, export_user_profile, identity_commit_update, identity_digest_preview,
        identity_export_markdown, identity_get_context, identity_list_history,
        identity_propose_update, identity_remember, identity_rollback_version, import_user_profile,
        list_user_profiles, set_active_user_profile,
    };

    DeclaredRouter::default()
        .route(
            profile_policy(axum::http::Method::POST, "/v1/identity/context", 64 * 1024),
            post(identity_get_context),
        )
        .route(
            profile_policy(
                axum::http::Method::POST,
                "/v1/identity/remember",
                256 * 1024,
            ),
            post(identity_remember),
        )
        .route(
            profile_policy(
                axum::http::Method::POST,
                "/v1/identity/digest-preview",
                64 * 1024,
            ),
            post(identity_digest_preview),
        )
        .route(
            profile_policy(
                axum::http::Method::POST,
                "/v1/identity/export-markdown",
                128 * 1024,
            ),
            post(identity_export_markdown),
        )
        .methods([
            (
                profile_policy(axum::http::Method::GET, "/v1/identity/profiles", 1024),
                get(list_user_profiles),
            ),
            (
                identity_admin_policy(axum::http::Method::POST, "/v1/identity/profiles", 64 * 1024),
                post(create_user_profile),
            ),
        ])
        .route(
            identity_admin_policy(
                axum::http::Method::PUT,
                "/v1/identity/profiles/active",
                16 * 1024,
            ),
            put(set_active_user_profile),
        )
        .route(
            identity_admin_policy(
                axum::http::Method::POST,
                "/v1/identity/profiles/export",
                64 * 1024,
            ),
            post(export_user_profile),
        )
        .route(
            identity_admin_policy(
                axum::http::Method::POST,
                "/v1/identity/profiles/import",
                8 * 1024 * 1024,
            ),
            post(import_user_profile),
        )
        .methods([
            (
                profile_policy(axum::http::Method::GET, "/v1/shared-mode", 1024),
                get(crate::daemon::shared_mode::shared_mode_status),
            ),
            (
                identity_admin_policy(axum::http::Method::PUT, "/v1/shared-mode", 16 * 1024),
                put(crate::daemon::shared_mode::set_shared_mode),
            ),
        ])
        .route(
            profile_policy(
                axum::http::Method::POST,
                "/v1/identity/update/propose",
                128 * 1024,
            ),
            post(identity_propose_update),
        )
        .route(
            identity_admin_policy(
                axum::http::Method::POST,
                "/v1/identity/update/commit",
                128 * 1024,
            ),
            post(identity_commit_update),
        )
        .route(
            profile_policy(axum::http::Method::POST, "/v1/identity/history", 64 * 1024),
            post(identity_list_history),
        )
        .route(
            identity_admin_policy(axum::http::Method::POST, "/v1/identity/rollback", 64 * 1024),
            post(identity_rollback_version),
        )
}

pub fn build_runtime_admin_surface() -> DeclaredRouter<AppState> {
    use axum::routing::{get, post, put};

    use crate::daemon::core::{
        artifact_command, artifact_delete, artifact_fetch, artifact_list_ui, artifact_write,
        runtime_config_command, runtime_defaults, stage_route_command,
    };
    use crate::maintenance_handlers::{get_artifact_retention_status, update_artifact_retention};

    DeclaredRouter::default()
        .route(
            runtime_admin_policy(axum::http::Method::GET, "/v1/runtime/defaults", 1024),
            get(runtime_defaults),
        )
        .route(
            runtime_admin_policy(
                axum::http::Method::POST,
                "/v1/runtime/artifact/command",
                8 * 1024 * 1024,
            ),
            post(artifact_command),
        )
        .route(
            runtime_admin_policy(
                axum::http::Method::POST,
                "/v1/runtime/artifact/fetch",
                64 * 1024,
            ),
            post(artifact_fetch),
        )
        .route(
            runtime_admin_policy(
                axum::http::Method::POST,
                "/v1/runtime/artifact/write",
                8 * 1024 * 1024,
            ),
            post(artifact_write),
        )
        .route(
            runtime_admin_policy(
                axum::http::Method::POST,
                "/v1/runtime/artifact/delete",
                64 * 1024,
            ),
            post(artifact_delete),
        )
        .route(
            runtime_admin_policy(
                axum::http::Method::POST,
                "/v1/runtime/artifact/list-ui",
                64 * 1024,
            ),
            post(artifact_list_ui),
        )
        .methods([
            (
                runtime_admin_policy(axum::http::Method::GET, "/v1/maintenance/artifacts", 1024),
                get(get_artifact_retention_status),
            ),
            (
                runtime_admin_policy(
                    axum::http::Method::PUT,
                    "/v1/maintenance/artifacts",
                    64 * 1024,
                ),
                put(update_artifact_retention),
            ),
        ])
        .methods([
            (
                runtime_admin_policy(axum::http::Method::GET, "/v1/maintenance/storage", 1024),
                get(crate::daemon::storage_governor::get_storage_status),
            ),
            (
                runtime_admin_policy(
                    axum::http::Method::PUT,
                    "/v1/maintenance/storage",
                    64 * 1024,
                ),
                put(crate::daemon::storage_governor::put_storage_settings),
            ),
            (
                runtime_admin_policy(
                    axum::http::Method::POST,
                    "/v1/maintenance/storage",
                    16 * 1024,
                ),
                post(crate::daemon::storage_governor::post_storage_maintenance),
            ),
        ])
        .route(
            runtime_admin_policy(
                axum::http::Method::POST,
                "/v1/runtime/config/command",
                256 * 1024,
            ),
            post(runtime_config_command),
        )
        .route(
            runtime_admin_policy(
                axum::http::Method::POST,
                "/v1/runtime/stage-route/command",
                256 * 1024,
            ),
            post(stage_route_command),
        )
}

fn profile_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    let rate_limit_class = if method == axum::http::Method::GET {
        RateLimitClass::Read
    } else {
        RateLimitClass::Mutation
    };
    protected_policy(
        method,
        path,
        RouteGroup::Portal,
        crate::request_principal::Capability::ProfileSelf,
        body_limit,
        rate_limit_class,
    )
}

fn identity_admin_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    protected_policy(
        method,
        path,
        RouteGroup::Administration,
        crate::request_principal::Capability::AdminIdentity,
        body_limit,
        RateLimitClass::Administration,
    )
}

fn runtime_admin_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    protected_policy(
        method,
        path,
        RouteGroup::Administration,
        crate::request_principal::Capability::AdminRuntime,
        body_limit,
        RateLimitClass::Administration,
    )
}

fn protected_policy(
    method: axum::http::Method,
    path: &'static str,
    group: RouteGroup,
    required_capability: crate::request_principal::Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
}

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

/// Transitional compatibility boundary for the dependency-owned Stasis
/// dashboard. New Medousa routes must use `DeclaredRouter`; this remains raw
/// only until Stasis exports method/path descriptors with its router. Reviewed
/// paths live in [`crate::daemon::contract::DASHBOARD_COMPATIBILITY_MOUNTS`]
/// and are not part of the declared generated contract.
pub fn build_dashboard_compatibility_router(
    state: &AppState,
    dashboard_action_auth: &DashboardActionAuthConfig,
) -> Router {
    let dashboard_service = Arc::new(RuntimeDashboardQueryService::from_runtime_composition(
        state.composition().clone(),
    ));
    let dashboard_state = apply_dashboard_action_auth(
        DashboardState::new(dashboard_service),
        dashboard_action_auth,
    );
    dashboard_router(dashboard_state)
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

pub fn build_workshop_surface() -> DeclaredRouter<AppState> {
    use axum::routing::{delete, get, post, put};

    use crate::daemon::core::{health, heartbeat_status, stats};
    use crate::daemon::interactive::{
        cancel_active_session_turn, create_turn_ticket, delete_session_handler,
        get_active_session_turn, get_turn_ticket, interactive_turn_stream, list_session_turns,
        start_interactive_turn,
    };
    use crate::daemon::jobs::{
        archive_ask_job, complete_ask_job_actions, enqueue_ask, enqueue_prompt, enqueue_report,
        get_job_report, get_job_result,
    };
    use crate::request_principal::Capability;

    DeclaredRouter::default()
        .route(workshop_read_policy("/v1/health"), get(health))
        .route(workshop_read_policy("/v1/stats"), get(stats))
        .route(
            workshop_read_policy("/v1/agent-modes"),
            get(crate::daemon_handlers::list_agent_modes),
        )
        .methods([
            (
                workshop_read_policy("/v1/agent-modes/policy"),
                get(crate::daemon_handlers::get_agent_mode_transition_policy),
            ),
            (
                workshop_admin_policy(axum::http::Method::PUT, "/v1/agent-modes/policy", 64 * 1024),
                put(crate::daemon_handlers::set_agent_mode_transition_policy),
            ),
        ])
        .methods([
            (
                workshop_read_policy("/v1/sessions"),
                get(crate::daemon_handlers::list_session_history),
            ),
            (
                workshop_mutation_policy(
                    axum::http::Method::POST,
                    "/v1/sessions",
                    Capability::WorkshopInteract,
                    256 * 1024,
                ),
                post(crate::daemon_handlers::create_session),
            ),
        ])
        .route(
            workshop_read_policy("/v1/sessions/{session_id}/history"),
            get(crate::daemon_handlers::get_session_history),
        )
        .methods([
            (
                workshop_read_policy("/v1/sessions/{session_id}/turns"),
                get(list_session_turns),
            ),
            (
                workshop_mutation_policy(
                    axum::http::Method::POST,
                    "/v1/sessions/{session_id}/turns",
                    Capability::WorkshopInteract,
                    1024 * 1024,
                ),
                post(crate::daemon_handlers::append_session_turn),
            ),
        ])
        .route(
            workshop_mutation_policy(
                axum::http::Method::PUT,
                "/v1/sessions/{session_id}/name",
                Capability::WorkshopInteract,
                64 * 1024,
            ),
            put(crate::daemon_handlers::set_session_display_name),
        )
        .methods([
            (
                workshop_read_policy("/v1/sessions/{session_id}/agent-mode"),
                get(crate::daemon_handlers::get_session_agent_mode),
            ),
            (
                workshop_mutation_policy(
                    axum::http::Method::PUT,
                    "/v1/sessions/{session_id}/agent-mode",
                    Capability::WorkshopInteract,
                    64 * 1024,
                ),
                put(crate::daemon_handlers::set_session_agent_mode),
            ),
            (
                workshop_mutation_policy(
                    axum::http::Method::DELETE,
                    "/v1/sessions/{session_id}/agent-mode",
                    Capability::WorkshopInteract,
                    1024,
                ),
                delete(crate::daemon_handlers::clear_session_agent_mode),
            ),
        ])
        .route(
            workshop_read_policy("/v1/sessions/{session_id}/agent-mode/proposals"),
            get(crate::daemon_handlers::list_session_agent_mode_proposals),
        )
        .route(
            workshop_mutation_policy(
                axum::http::Method::PUT,
                "/v1/sessions/{session_id}/agent-mode/proposals/{proposal_id}",
                Capability::WorkshopInteract,
                64 * 1024,
            ),
            put(crate::daemon_handlers::decide_session_agent_mode_proposal),
        )
        .methods([
            (
                workshop_read_policy("/v1/sessions/{session_id}/code-binding"),
                get(crate::daemon_handlers::get_session_code_binding),
            ),
            (
                workshop_mutation_policy(
                    axum::http::Method::PUT,
                    "/v1/sessions/{session_id}/code-binding",
                    Capability::WorkspaceWrite,
                    64 * 1024,
                ),
                put(crate::daemon_handlers::set_session_code_binding),
            ),
            (
                workshop_mutation_policy(
                    axum::http::Method::DELETE,
                    "/v1/sessions/{session_id}/code-binding",
                    Capability::WorkspaceWrite,
                    1024,
                ),
                delete(crate::daemon_handlers::clear_session_code_binding),
            ),
        ])
        .route(
            workshop_mutation_policy(
                axum::http::Method::DELETE,
                "/v1/sessions/{session_id}",
                Capability::WorkshopInteract,
                1024,
            ),
            delete(delete_session_handler),
        )
        .route(
            workshop_read_policy("/v1/session-deletions/{deletion_id}"),
            get(crate::daemon_handlers::get_session_deletion),
        )
        .methods([
            (
                workshop_read_policy("/v1/sessions/{session_id}/active-turn"),
                get(get_active_session_turn),
            ),
            (
                workshop_mutation_policy(
                    axum::http::Method::POST,
                    "/v1/sessions/{session_id}/active-turn",
                    Capability::WorkshopInteract,
                    16 * 1024,
                ),
                post(cancel_active_session_turn),
            ),
        ])
        .route(
            workshop_mutation_policy(
                axum::http::Method::POST,
                "/v1/sessions/{session_id}/workshop/steer",
                Capability::WorkshopInteract,
                256 * 1024,
            ),
            post(crate::daemon::workshop_steer::steer_bound_workshop_handler),
        )
        .route(
            workshop_mutation_policy(
                axum::http::Method::POST,
                "/v1/turns",
                Capability::WorkshopInteract,
                1024 * 1024,
            ),
            post(create_turn_ticket),
        )
        .route(
            workshop_read_policy("/v1/turns/{turn_id}"),
            get(get_turn_ticket),
        )
        .route(
            workshop_read_policy("/v1/heartbeat/status"),
            get(heartbeat_status),
        )
        .route(
            workshop_read_policy("/v1/jobs/{job_id}/result"),
            get(get_job_result),
        )
        .route(
            workshop_read_policy("/v1/jobs/{job_id}/report"),
            get(get_job_report),
        )
        .route(
            workshop_mutation_policy(
                axum::http::Method::POST,
                "/v1/jobs/{job_id}/complete-actions",
                Capability::WorkshopInteract,
                256 * 1024,
            ),
            post(complete_ask_job_actions),
        )
        .route(
            workshop_mutation_policy(
                axum::http::Method::POST,
                "/v1/jobs/{job_id}/archive",
                Capability::WorkshopInteract,
                16 * 1024,
            ),
            post(archive_ask_job),
        )
        .route(
            workshop_mutation_policy(
                axum::http::Method::POST,
                "/v1/jobs/ask",
                Capability::WorkshopInteract,
                1024 * 1024,
            ),
            post(enqueue_ask),
        )
        .route(
            workshop_mutation_policy(
                axum::http::Method::POST,
                "/v1/jobs/report",
                Capability::WorkshopInteract,
                1024 * 1024,
            ),
            post(enqueue_report),
        )
        .route(
            workshop_mutation_policy(
                axum::http::Method::POST,
                "/v1/jobs/prompt",
                Capability::WorkshopInteract,
                1024 * 1024,
            ),
            post(enqueue_prompt),
        )
        .route(
            workshop_mutation_policy(
                axum::http::Method::POST,
                "/v1/interactive/turn",
                Capability::WorkshopInteract,
                1024 * 1024,
            ),
            post(start_interactive_turn),
        )
        .route(
            workshop_stream_policy("/v1/interactive/turn/{turn_id}/stream"),
            get(interactive_turn_stream),
        )
}

fn workshop_read_policy(path: &'static str) -> RoutePolicy {
    workshop_policy(
        axum::http::Method::GET,
        path,
        crate::request_principal::Capability::WorkshopRead,
        1024,
        RateLimitClass::Read,
    )
}

fn workshop_stream_policy(path: &'static str) -> RoutePolicy {
    RoutePolicy {
        rate_limit_class: RateLimitClass::Stream,
        ..workshop_read_policy(path)
    }
}

fn workshop_mutation_policy(
    method: axum::http::Method,
    path: &'static str,
    capability: crate::request_principal::Capability,
    body_limit: usize,
) -> RoutePolicy {
    workshop_policy(
        method,
        path,
        capability,
        body_limit,
        RateLimitClass::Mutation,
    )
}

fn workshop_admin_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    RoutePolicy {
        group: RouteGroup::Administration,
        rate_limit_class: RateLimitClass::Administration,
        ..workshop_policy(
            method,
            path,
            crate::request_principal::Capability::AdminRuntime,
            body_limit,
            RateLimitClass::Administration,
        )
    }
}

fn workshop_policy(
    method: axum::http::Method,
    path: &'static str,
    required_capability: crate::request_principal::Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
}

pub fn build_core_service_surface() -> DeclaredRouter<AppState> {
    use axum::routing::{delete, get, post};

    use crate::daemon::continuations::{
        continuation_lineage, continuation_status, replay_and_resume_job,
    };
    use crate::daemon::ingest::{
        deliver_outbox_webhook, deliver_poll, delivery_status, ingest_handler, ingest_stream,
    };
    use crate::request_principal::Capability;

    DeclaredRouter::default()
        .route(
            service_read_policy("/v1/agents/runtimes"),
            get(crate::daemon::agents::list_agent_runtimes),
        )
        .route(
            service_policy(
                axum::http::Method::POST,
                "/v1/agents/sessions",
                Capability::AdminExecute,
                256 * 1024,
                RateLimitClass::Administration,
            ),
            post(crate::daemon::agents::create_agent_session),
        )
        .route(
            service_policy(
                axum::http::Method::POST,
                "/v1/agents/sessions/{agent_session_id}/prompt",
                Capability::AdminExecute,
                1024 * 1024,
                RateLimitClass::Administration,
            ),
            post(crate::daemon::agents::prompt_agent_session),
        )
        .route(
            service_policy(
                axum::http::Method::POST,
                "/v1/agents/sessions/{agent_session_id}/config",
                Capability::AdminExecute,
                64 * 1024,
                RateLimitClass::Administration,
            ),
            post(crate::daemon::agents::set_agent_session_config_option),
        )
        .route(
            service_policy(
                axum::http::Method::GET,
                "/v1/agents/sessions/{agent_session_id}/stream",
                Capability::AdminExecute,
                1024,
                RateLimitClass::Stream,
            ),
            get(crate::daemon::agents::agent_session_stream),
        )
        .route(
            service_policy(
                axum::http::Method::POST,
                "/v1/agents/sessions/{agent_session_id}/cancel",
                Capability::AdminExecute,
                16 * 1024,
                RateLimitClass::Administration,
            ),
            post(crate::daemon::agents::cancel_agent_session),
        )
        .methods([
            (
                service_admin_policy(axum::http::Method::GET, "/v1/auth/chatgpt", 1024),
                get(crate::daemon::chatgpt_oauth::status),
            ),
            (
                service_admin_policy(axum::http::Method::DELETE, "/v1/auth/chatgpt", 1024),
                delete(crate::daemon::chatgpt_oauth::disconnect),
            ),
        ])
        .route(
            service_admin_policy(
                axum::http::Method::POST,
                "/v1/auth/chatgpt/begin",
                64 * 1024,
            ),
            post(crate::daemon::chatgpt_oauth::begin),
        )
        .route(
            service_admin_policy(
                axum::http::Method::POST,
                "/v1/auth/chatgpt/complete",
                64 * 1024,
            ),
            post(crate::daemon::chatgpt_oauth::complete),
        )
        .route(
            service_admin_policy(
                axum::http::Method::POST,
                "/v1/auth/chatgpt/refresh",
                16 * 1024,
            ),
            post(crate::daemon::chatgpt_oauth::refresh),
        )
        .route(
            service_admin_policy(axum::http::Method::GET, "/v1/auth/chatgpt/models", 1024),
            get(crate::daemon::chatgpt_oauth::models),
        )
        .route(
            service_policy(
                axum::http::Method::POST,
                "/v1/ingest",
                Capability::WorkshopInteract,
                8 * 1024 * 1024,
                RateLimitClass::Mutation,
            ),
            post(ingest_handler),
        )
        .route(
            service_policy(
                axum::http::Method::GET,
                "/v1/ingest/{stream_id}/stream",
                Capability::WorkshopRead,
                1024,
                RateLimitClass::Stream,
            ),
            get(ingest_stream),
        )
        .route(
            service_policy(
                axum::http::Method::POST,
                "/v1/media/upload",
                Capability::ContentWrite,
                32 * 1024 * 1024,
                RateLimitClass::Mutation,
            ),
            post(crate::media_handlers::upload_media),
        )
        .route(
            service_policy(
                axum::http::Method::GET,
                "/v1/media/{media_id}",
                Capability::ContentRead,
                1024,
                RateLimitClass::Read,
            ),
            get(crate::media_handlers::get_media),
        )
        .route(
            service_policy(
                axum::http::Method::POST,
                "/v1/deliver/outbox",
                Capability::WorkshopInteract,
                1024 * 1024,
                RateLimitClass::Mutation,
            ),
            post(deliver_outbox_webhook),
        )
        .route(
            service_read_policy("/v1/deliver/poll/{job_id}"),
            get(deliver_poll),
        )
        .route(
            service_read_policy("/v1/delivery/status"),
            get(delivery_status),
        )
        .route(
            service_read_policy("/v1/continuations/status"),
            get(continuation_status),
        )
        .route(
            service_read_policy("/v1/continuations/lineage/{turn_correlation_id}"),
            get(continuation_lineage),
        )
        .route(
            service_policy(
                axum::http::Method::POST,
                "/v1/jobs/{job_id}/replay-and-resume",
                Capability::WorkshopInteract,
                64 * 1024,
                RateLimitClass::Mutation,
            ),
            post(replay_and_resume_job),
        )
}

fn service_read_policy(path: &'static str) -> RoutePolicy {
    service_policy(
        axum::http::Method::GET,
        path,
        crate::request_principal::Capability::WorkshopRead,
        1024,
        RateLimitClass::Read,
    )
}

fn service_admin_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    RoutePolicy {
        group: RouteGroup::Administration,
        ..service_policy(
            method,
            path,
            crate::request_principal::Capability::AdminRuntime,
            body_limit,
            RateLimitClass::Administration,
        )
    }
}

fn service_policy(
    method: axum::http::Method,
    path: &'static str,
    required_capability: crate::request_principal::Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
}

/// Transitional H08 boundary for unprefixed browser-bridge aliases.
/// Canonical `/v1` copies live on [`crate::browser_handlers::browser_surface`].
/// New daemon routes must not be added here. Unprefixed mounts are inventoried
/// in [`crate::daemon::contract::BROWSER_COMPATIBILITY_MOUNTS`].
pub fn build_browser_compatibility_router(state: AppState) -> Router {
    crate::browser_handlers::browser_router().with_state(state)
}

/// Remaining compatibility surface. Policy-owned application and preview
/// routes are assembled separately at the final socket boundary.
pub fn build_daemon_router(
    state: AppState,
    dashboard_action_auth: &DashboardActionAuthConfig,
    request_boundary: Arc<crate::daemon::request_boundary::RequestBoundary>,
) -> Router {
    let allowed_origins = request_boundary.allowed_origin_values();
    build_browser_compatibility_router(state.clone())
        .merge(build_dashboard_compatibility_router(
            &state,
            dashboard_action_auth,
        ))
        .layer(axum::middleware::from_fn_with_state(
            request_boundary,
            crate::daemon::request_boundary::enforce_compatibility_origin,
        ))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(allowed_origins))
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::CONTENT_TYPE,
                ]),
        )
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header::CONTENT_TYPE};
    use tower::ServiceExt;

    use super::{
        LIVENESS_BODY, RateLimitClass, RouteGroup, build_core_service_surface,
        build_declared_route_inventory, build_identity_surface, build_liveness_router,
        build_liveness_surface, build_runtime_admin_surface, build_workshop_surface,
    };
    use crate::daemon::route_policy::AuthorizationClass;

    #[tokio::test]
    async fn public_liveness_is_constant_and_contains_no_runtime_detail() {
        let response = build_liveness_router()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("liveness response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        let body = to_bytes(response.into_body(), 1024)
            .await
            .expect("bounded body");
        assert_eq!(body.as_ref(), LIVENESS_BODY.as_bytes());
        for private_field in ["backend", "worker", "profile", "tool", "latency"] {
            assert!(!LIVENESS_BODY.contains(private_field));
        }
    }

    #[tokio::test]
    async fn liveness_router_exposes_no_application_alias() {
        let response = build_liveness_router()
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("missing response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn liveness_policy_is_exported_from_router_construction() {
        let entries = build_liveness_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].method, "GET");
        assert_eq!(entries[0].path, "/health");
        assert!(entries[0].bootstrap_public);
        assert_eq!(entries[0].required_capability, None);
    }

    #[test]
    fn combined_declared_inventory_matches_optional_pairing_composition() {
        let without_pairing = build_declared_route_inventory(false);
        let with_pairing = build_declared_route_inventory(true);
        assert_eq!(without_pairing.entries().len(), 377);
        assert_eq!(with_pairing.entries().len(), 389);

        let json = with_pairing.to_pretty_json().expect("serialize inventory");
        let rows: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(rows.len(), 389);
        assert_eq!(rows[0]["path"], "/health");
        assert!(rows.iter().any(|row| {
            row["method"] == "POST"
                && row["path"] == "/v1/mesh/outbox"
                && row["required_capability"] == "peer.exchange"
        }));
        assert!(rows.iter().any(|row| {
            row["method"] == "POST"
                && row["path"] == "/v1/admin/local-credentials/{name}/rotate"
                && row["required_capability"] == "admin.identity"
        }));
        assert!(rows.iter().any(|row| {
            row["method"] == "GET"
                && row["path"] == "/qr"
                && row["required_capability"] == "admin.identity"
        }));
        assert!(rows.iter().any(|row| {
            row["method"] == "PUT"
                && row["path"] == "/v1/runtime/inference-profiles"
                && row["required_capability"] == "admin.runtime"
        }));
    }

    #[test]
    fn identity_inventory_separates_self_service_from_administration() {
        let entries = build_identity_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 15);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("profile.self"))
                .count(),
            8
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("admin.identity"))
                .count(),
            7
        );
    }

    #[test]
    fn runtime_inventory_requires_runtime_administration() {
        let entries = build_runtime_admin_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 13);
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::Administration
                && entry.required_capability == Some("admin.runtime")
                && entry.rate_limit_class == RateLimitClass::Administration
        }));
    }

    #[test]
    fn workshop_inventory_separates_read_interaction_workspace_and_admin() {
        let entries = build_workshop_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 36);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("workshop.read"))
                .count(),
            17
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("workshop.interact"))
                .count(),
            16
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("workspace.write"))
                .count(),
            2
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("admin.runtime"))
                .count(),
            1
        );
    }

    #[test]
    fn core_service_inventory_separates_execution_credentials_and_content() {
        let entries = build_core_service_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 22);
        for (capability, count) in [
            ("workshop.read", 6),
            ("workshop.interact", 3),
            ("admin.execute", 5),
            ("admin.runtime", 6),
            ("content.read", 1),
            ("content.write", 1),
        ] {
            assert_eq!(
                entries
                    .iter()
                    .filter(|entry| entry.required_capability == Some(capability))
                    .count(),
                count,
                "unexpected route count for {capability}",
            );
        }
    }

    #[test]
    fn feature_control_inventories_separate_reads_from_admin_mutations() {
        let capabilities = crate::mcp_daemon_handlers::capability_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(capabilities.len(), 4);
        assert_eq!(
            capabilities
                .iter()
                .filter(|entry| entry.required_capability == Some("workshop.read"))
                .count(),
            3
        );
        assert_eq!(
            capabilities
                .iter()
                .filter(|entry| entry.required_capability == Some("admin.runtime"))
                .count(),
            1
        );

        let budgets = crate::turn_budget_handlers::budget_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(budgets.len(), 4);
        assert_eq!(
            budgets
                .iter()
                .filter(|entry| entry.required_capability == Some("workshop.read"))
                .count(),
            2
        );
        assert_eq!(
            budgets
                .iter()
                .filter(|entry| entry.required_capability == Some("admin.execute"))
                .count(),
            2
        );
    }

    #[test]
    fn calendar_inventory_separates_content_reads_and_writes() {
        let entries = crate::calendar_handlers::calendar_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 6);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("content.read"))
                .count(),
            2
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("content.write"))
                .count(),
            4
        );
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::Portal
                && entry.browser_policy == super::BrowserPolicy::NativeOnly
        }));
    }

    #[test]
    fn content_feature_inventory_is_complete_and_native_only() {
        let manuscripts = crate::manuscript_handlers::manuscript_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let locus = crate::locus_handlers::locus_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let feeds = crate::feed_handlers::feed_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let component_store = crate::component_store_handlers::component_store_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let entries = manuscripts
            .iter()
            .chain(&locus)
            .chain(&feeds)
            .chain(&component_store)
            .collect::<Vec<_>>();

        assert_eq!(entries.len(), 19);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("content.read"))
                .count(),
            12
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("content.write"))
                .count(),
            7
        );
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::Portal
                && entry.browser_policy == super::BrowserPolicy::NativeOnly
        }));
        assert!(entries.iter().any(|entry| {
            entry.path == "/v1/feeds/stream" && entry.rate_limit_class == RateLimitClass::Stream
        }));
    }

    #[test]
    fn workflow_and_grapheme_inventory_separates_execution_from_authoring() {
        let workflows = crate::workflow_handlers::workflow_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let history = crate::tool_history_handlers::tool_history_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let grapheme = crate::grapheme_handlers::grapheme_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let entries = workflows
            .iter()
            .chain(&history)
            .chain(&grapheme)
            .collect::<Vec<_>>();

        assert_eq!(entries.len(), 24);
        for (capability, count) in [
            ("workshop.read", 9),
            ("workshop.interact", 1),
            ("content.read", 2),
            ("content.write", 3),
            ("admin.runtime", 3),
            ("admin.execute", 6),
        ] {
            assert_eq!(
                entries
                    .iter()
                    .filter(|entry| entry.required_capability == Some(capability))
                    .count(),
                count,
                "unexpected route count for {capability}",
            );
        }
        assert!(entries.iter().any(|entry| {
            entry.path == "/v1/grapheme/lsp" && entry.rate_limit_class == RateLimitClass::Stream
        }));
    }

    #[test]
    fn runtime_support_inventory_uses_narrow_authority_classes() {
        let local = crate::local_inference_handlers::surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let models = crate::model_capability_registry::handlers::surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let stt = crate::stt_handlers::surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let lan = crate::lan_handlers::lan_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let components = crate::component_runtime_handlers::component_runtime_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let entries = local
            .iter()
            .chain(&models)
            .chain(&stt)
            .chain(&lan)
            .chain(&components)
            .collect::<Vec<_>>();

        assert_eq!(entries.len(), 17);
        for (capability, count) in [
            ("admin.runtime", 11),
            ("admin.identity", 1),
            ("workshop.read", 1),
            ("workshop.interact", 1),
            ("content.read", 1),
            ("content.write", 2),
        ] {
            assert_eq!(
                entries
                    .iter()
                    .filter(|entry| entry.required_capability == Some(capability))
                    .count(),
                count,
                "unexpected route count for {capability}",
            );
        }
    }

    #[test]
    fn native_execution_hosts_require_execute_authority() {
        let coding = crate::daemon::coding_engine_host::coding_engine_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let shell = crate::daemon::shell_session_host::shell_session_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let world = crate::daemon::detamu_host::world_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        let entries = coding
            .iter()
            .chain(&shell)
            .chain(&world)
            .collect::<Vec<_>>();

        assert_eq!(entries.len(), 29);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("admin.execute"))
                .count(),
            21
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("workshop.read"))
                .count(),
            8
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.rate_limit_class == RateLimitClass::Stream)
                .count(),
            2
        );
    }

    #[test]
    fn forge_inventory_is_execute_only_and_preserves_large_edit_bound() {
        let entries = crate::daemon::forge_api::forge_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 74);
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::Administration
                && entry.required_capability == Some("admin.execute")
                && entry.browser_policy == super::BrowserPolicy::NativeOnly
        }));
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.rate_limit_class == RateLimitClass::Stream)
                .count(),
            3
        );
        let workspace_edit = entries
            .iter()
            .find(|entry| entry.path.ends_with("/source/workspace-edit"))
            .expect("workspace edit policy");
        assert_eq!(workspace_edit.body_limit, 64 * 1024 * 1024);
    }

    #[test]
    fn preview_inventory_is_token_owned_and_excludes_tunnel_methods() {
        let entries = crate::daemon::forge_preview::forge_preview_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 14);
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::Preview
                && entry.required_capability.is_none()
                && entry.authorization == AuthorizationClass::PreviewToken
                && entry.browser_policy == super::BrowserPolicy::ExactOrigin
                && entry.body_limit == 2 * 1024 * 1024
        }));
        assert!(
            !entries
                .iter()
                .any(|entry| matches!(entry.method.as_str(), "CONNECT" | "TRACE"))
        );
    }
}
