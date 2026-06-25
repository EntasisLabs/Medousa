//! Health, stats, runtime defaults, and runtime command handlers.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::daemon::heartbeat::{build_heartbeat_status_response, safe_stats_snapshot};
use stasis::prelude::RuntimeSdk;
use crate::daemon_api::{
    ArtifactCommandRequest, ArtifactCommandResponse, DaemonStatsResponse, HealthResponse,
    HeartbeatStatusResponse, RuntimeConfigCommandRequest, RuntimeConfigCommandResponse,
    RuntimeDefaultsResponse, StageRouteCommandRequest, StageRouteCommandResponse,
};

use crate::daemon::http::internal_error;
use crate::daemon::state::{AgentTurnJobRecord, AppState};

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
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let (active_profile_id, active_profile_display_name) = state
        .profile_registry
        .read()
        .map(|registry| active_profile_snapshot(&registry))
        .unwrap_or_default();
    Json(HealthResponse {
        status: "ok".to_string(),
        backend: state.backend,
        worker_id: state.worker_id,
        now_utc: Utc::now(),
        agent_runtime_version: crate::agent_runtime::AGENT_RUNTIME_VERSION.to_string(),
        tool_registry_count: state.agent_tool_registry_count,
        last_agent_turn_latency_ms: *state.last_agent_turn_latency_ms.read().await,
        last_agent_turn_at_utc: *state.last_agent_turn_at.read().await,
        active_profile_id,
        active_profile_display_name,
    })
}

pub async fn stats(
    State(state): State<AppState>,
) -> Result<Json<DaemonStatsResponse>, (StatusCode, String)> {
    let sdk = RuntimeSdk::new(state.composition().clone());
    let snapshot = safe_stats_snapshot(&sdk, 5000)
        .await
        .map_err(internal_error)?;

    let last_tick_at_utc = *state.last_tick_at.read().await;

    Ok(Json(DaemonStatsResponse {
        enqueued_jobs: snapshot.enqueued_jobs,
        running_jobs: snapshot.running_jobs,
        succeeded_jobs: snapshot.succeeded_jobs,
        failed_jobs: snapshot.failed_jobs,
        dead_letter_jobs: snapshot.dead_letter_jobs,
        pending_outbox_events: snapshot.pending_outbox_events,
        recurring_definitions: snapshot.recurring_definitions,
        last_tick_at_utc,
    }))
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
    let retention = crate::workspace::retention::WorkspaceRetentionConfig::from_tui_defaults(&saved);
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
    if request.session_id.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".to_string()));
    }

    let response = crate::artifact_command_runtime::execute_artifact_command(request)
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
