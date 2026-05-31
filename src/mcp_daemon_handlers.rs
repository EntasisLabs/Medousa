use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::Json;

use crate::capability_catalog::{CapabilityListResponse, CapabilityResolveResponse};
use crate::mcp_gateway::verify_policy_bearer;
use crate::mcp_gateway_api::{McpPolicyEvaluateRequest, McpPolicyEvaluateResponse};
use crate::mcp_policy::evaluate_mcp_policy;
use crate::tools::TuiRuntime;

#[derive(Clone)]
pub struct CapabilityApiState {
    pub agent_runtime: std::sync::Arc<TuiRuntime>,
}

pub async fn list_capabilities(
    State(state): State<CapabilityApiState>,
) -> Json<CapabilityListResponse> {
    let registry = state.agent_runtime.capability_registry.read().await;
    Json(registry.list())
}

pub async fn get_capability(
    State(state): State<CapabilityApiState>,
    Path(capability_id): Path<String>,
) -> Result<Json<CapabilityResolveResponse>, (StatusCode, String)> {
    let capability_id = capability_id.trim();
    if capability_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "capability_id is required".to_string()));
    }

    let registry = state.agent_runtime.capability_registry.read().await;
    registry
        .resolve(capability_id)
        .map(Json)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("unknown capability '{capability_id}'"),
            )
        })
}

pub async fn mcp_policy_evaluate(
    headers: HeaderMap,
    Json(request): Json<McpPolicyEvaluateRequest>,
) -> Result<Json<McpPolicyEvaluateResponse>, (StatusCode, String)> {
    if !verify_policy_bearer(
        headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
        crate::mcp_gateway::resolve_mcp_policy_token().as_deref(),
    ) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "invalid MCP policy bearer token".to_string(),
        ));
    }

    Ok(Json(evaluate_mcp_policy(&request)))
}
