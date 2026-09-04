//! Health, stats, runtime defaults, and runtime command handlers.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;

use crate::daemon::heartbeat::{build_heartbeat_status_response, safe_stats_snapshot};
use crate::daemon_api::{
    ArtifactCommandRequest, ArtifactCommandResponse, DaemonStatsResponse, HealthResponse,
    HeartbeatStatusResponse, RuntimeConfigCommandRequest, RuntimeConfigCommandResponse,
    RuntimeDefaultsResponse, StageRouteCommandRequest, StageRouteCommandResponse,
};
use stasis::prelude::RuntimeSdk;

use crate::daemon::http::internal_error;
use crate::daemon::state::AppState;

fn active_profile_snapshot(
    registry: &crate::user_profiles::UserProfileRegistry,
) -> (String, String) {
    let active_profile_id = registry.active_profile_id().to_string();
    let active_profile_display_name = registry
        .list_profiles()
        .into_iter()
        .find(|profile| profile.profile_id == active_profile_id)
        .map(|profile| profile.display_name)
        .unwrap_or_else(|| "Personal".to_string());
    (active_profile_id, active_profile_display_name)
}
pub async fn health(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, (StatusCode, String)> {
    let (active_profile_id, active_profile_display_name) = state
        .profile_registry
        .read()
        .map(|registry| active_profile_snapshot(&registry))
        .unwrap_or_default();
    let authority_id = crate::workshop_authority::current()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .clone();
    let advertised_capabilities = [
        "auth.chatgpt-account",
        "deployment.native-workloads",
        "transport.http",
    ]
    .into_iter()
    .chain(cfg!(feature = "iroh-transport").then_some("transport.iroh"))
    .chain(
        state
            .work_environment
            .is_some()
            .then_some(medousa_runtime::OCI_WORK_ENVIRONMENT_CAPABILITY),
    );

    Ok(Json(crate::daemon_runtime::health_response(
        authority_id,
        "full",
        advertised_capabilities,
        crate::daemon_runtime::DaemonHealthSnapshot {
            backend: state.backend,
            worker_id: state.worker_id,
            agent_runtime_version: crate::daemon_runtime::AGENT_RUNTIME_VERSION.to_string(),
            tool_registry_count: state.agent_tool_registry_count,
            last_agent_turn_latency_ms: *state.last_agent_turn_latency_ms.read().await,
            last_agent_turn_at_utc: *state.last_agent_turn_at.read().await,
            active_profile_id,
            active_profile_display_name,
        },
    )))
}

pub async fn stats(
    State(state): State<AppState>,
) -> Result<Json<DaemonStatsResponse>, (StatusCode, String)> {
    let sdk = RuntimeSdk::new(state.composition().clone());
    let snapshot = safe_stats_snapshot(&sdk, 5000)
        .await
        .map_err(internal_error)?;

    let last_tick_at_utc = *state.last_tick_at.read().await;
    let execution_registry = &state.platform.agent_handle().execution_registry;

    Ok(Json(crate::daemon_runtime::stats_response(
        snapshot,
        crate::daemon_runtime::DaemonStatsObservation {
            last_tick_at_utc,
            active_turn_executions: execution_registry.live_count(),
            active_turn_executions_high_water: execution_registry.high_water(),
            missing_turn_context_invocations:
                crate::agent_runtime::execution_context::missing_turn_context_invocations(),
        },
    )))
}

pub async fn execution_targets(
    State(state): State<AppState>,
) -> Json<crate::workshop_contract::ExecutionTargetInventory> {
    let runtime_id = state
        .platform
        .agent()
        .worker_scheduler
        .execution_runtime_id();
    let candidate = crate::workshop_contract::ExecutionTargetCandidate::local(
        runtime_id.clone(),
        stasis::domain::runtime::placement::WorkerCapabilities::any()
            .node_id(&runtime_id)
            .platform(std::env::consts::OS)
            .architecture(std::env::consts::ARCH)
            .with_capability("assistant.work"),
    );
    Json(crate::workshop_contract::ExecutionTargetInventory {
        schema_version:
            crate::workshop_contract::EXECUTION_TARGET_INVENTORY_SCHEMA_VERSION,
        parent_runtime_id: runtime_id.clone(),
        default_runtime_id: Some(runtime_id),
        targets: vec![candidate.inventory_entry()],
    })
}

pub async fn runtime_defaults(state: State<AppState>) -> Json<RuntimeDefaultsResponse> {
    let saved = crate::session::load_tui_defaults();
    let product = crate::load_product_config();
    let main = crate::inference_profiles::main_target(&saved);
    let provider = saved
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or(main.provider);
    let model = saved
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or(main.model);
    let response_depth_mode = saved
        .response_depth_mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(product.tui.response_depth_mode.as_str())
        .to_string();
    let reasoning_effort = saved
        .reasoning_effort
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::reasoning_effort::REASONING_EFFORT_DEFAULT.to_string());
    let base_url = saved
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let stage_routing = saved.stage_routing.clone().unwrap_or_else(|| {
        crate::stage_routing::StageRoutingMatrix::default_for(&provider, &model)
    });
    let retention =
        crate::workspace::retention::WorkspaceRetentionConfig::from_tui_defaults(&saved);
    let (active_profile_id, active_profile_display_name) = state
        .profile_registry
        .read()
        .map(|registry| active_profile_snapshot(&registry))
        .unwrap_or_default();
    Json(RuntimeDefaultsResponse {
        backend: state.backend.clone(),
        provider,
        model,
        response_depth_mode,
        reasoning_effort,
        base_url,
        stage_routing,
        work_card_hide_after_hours: retention.hide_after_hours,
        work_card_wipe_after_days: retention.wipe_after_days,
        active_profile_id,
        active_profile_display_name,
        catalog_freshness: Some(crate::model_capability_registry::registry().catalog_freshness()),
        inference_profiles: saved.inference_profiles.clone(),
    })
}

pub async fn heartbeat_status(
    State(state): State<AppState>,
) -> Result<Json<HeartbeatStatusResponse>, (StatusCode, String)> {
    let now_utc = Utc::now();
    let last_tick_at_utc = *state.last_tick_at.read().await;
    let maybe_report = state.last_heartbeat_report.read().await.clone();
    let metrics = state.heartbeat_metrics.read().await.clone();
    Ok(Json(
        build_heartbeat_status_response(
            state.composition(),
            state.heartbeat_policy,
            state.heartbeat_delivery_policy,
            last_tick_at_utc,
            maybe_report,
            metrics,
            now_utc,
        )
        .await?,
    ))
}
pub async fn artifact_command(
    Json(request): Json<ArtifactCommandRequest>,
) -> Result<Json<ArtifactCommandResponse>, (StatusCode, String)> {
    crate::session_storage::validate_session_id(&request.session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;

    let response = crate::artifact_command_runtime::execute_artifact_command(request)
        .map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn artifact_fetch(
    Json(request): Json<crate::daemon_api::ArtifactFetchRequest>,
) -> Result<Json<crate::daemon_api::ArtifactFetchResponse>, (StatusCode, String)> {
    crate::session_storage::validate_session_id(&request.session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    if request.artifact_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "artifact_id is required".to_string(),
        ));
    }

    let response =
        crate::artifact_command_runtime::execute_artifact_fetch(request).map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn artifact_list_ui(
    Json(request): Json<crate::daemon_api::ArtifactListUiRequest>,
) -> Result<Json<crate::daemon_api::ArtifactListUiResponse>, (StatusCode, String)> {
    if let Some(session_id) = request.session_id.as_deref() {
        crate::session_storage::validate_session_id(session_id)
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    }
    let response = crate::artifact_command_runtime::execute_artifact_list_ui(request)
        .map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn artifact_write(
    Json(request): Json<crate::daemon_api::ArtifactWriteRequest>,
) -> Result<Json<crate::daemon_api::ArtifactWriteResponse>, (StatusCode, String)> {
    crate::session_storage::validate_session_id(&request.session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    if request.artifact_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "artifact_id is required".to_string(),
        ));
    }
    let response =
        crate::artifact_command_runtime::execute_artifact_write(request).map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn artifact_delete(
    Json(request): Json<crate::daemon_api::ArtifactDeleteRequest>,
) -> Result<Json<crate::daemon_api::ArtifactDeleteResponse>, (StatusCode, String)> {
    crate::session_storage::validate_session_id(&request.session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    if request.artifact_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "artifact_id is required".to_string(),
        ));
    }
    let response = crate::artifact_command_runtime::execute_artifact_delete(request)
        .map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn runtime_config_command(
    Json(request): Json<RuntimeConfigCommandRequest>,
) -> Result<Json<RuntimeConfigCommandResponse>, (StatusCode, String)> {
    let response = crate::runtime_config_command_runtime::execute_runtime_config_command(request)
        .map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn stage_route_command(
    Json(request): Json<StageRouteCommandRequest>,
) -> Result<Json<StageRouteCommandResponse>, (StatusCode, String)> {
    let response = crate::stage_route_command_runtime::execute_stage_route_command(request)
        .map_err(internal_error)?;
    Ok(Json(response))
}
