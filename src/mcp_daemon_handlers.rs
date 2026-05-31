use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::Json;
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;

use crate::capability_catalog::{
    CapabilityListResponse, CapabilityReindexResponse, CapabilityResolveResponse,
    capabilities_manifest_path, load_capability_manifest, CapabilityRegistry,
};
use crate::mcp_gateway::verify_policy_bearer;
use crate::mcp_gateway_api::{McpPolicyEvaluateRequest, McpPolicyEvaluateResponse};
use crate::mcp_policy::evaluate_mcp_policy_with_identity;
use crate::tools::TuiRuntime;

#[derive(Clone)]
pub struct CapabilityApiState {
    pub agent_runtime: std::sync::Arc<TuiRuntime>,
}

#[derive(Clone)]
pub struct McpPolicyApiState {
    pub identity_service: std::sync::Arc<IdentityMemoryService>,
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

pub async fn reindex_capabilities(
    State(state): State<CapabilityApiState>,
) -> Json<CapabilityReindexResponse> {
    let (manifest, manifest_loaded_from_file) = load_capability_manifest();
    let mut registry = CapabilityRegistry::from_manifest(&manifest);
    let gateway_synced = if let Ok(catalog) = state
        .agent_runtime
        .mcp_gateway_client
        .fetch_catalog()
        .await
    {
        registry.apply_mcp_catalog_sync(&catalog);
        true
    } else {
        false
    };

    let capability_count = registry.list().capabilities.len();
    let binding_count = registry.binding_count();
    {
        let mut guard = state.agent_runtime.capability_registry.write().await;
        *guard = registry;
    }

    Json(CapabilityReindexResponse {
        capability_count,
        binding_count,
        manifest_path: Some(capabilities_manifest_path().display().to_string()),
        manifest_loaded_from_file,
        gateway_synced,
        now_utc: chrono::Utc::now(),
    })
}

pub async fn mcp_policy_evaluate(
    State(state): State<McpPolicyApiState>,
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

    Ok(Json(
        evaluate_mcp_policy_with_identity(&request, state.identity_service.as_ref()).await,
    ))
}
