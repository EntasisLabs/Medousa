use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::Json;
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;

use crate::capability_catalog::{
    capabilities_manifest_path, load_capability_manifest, CapabilityListResponse,
    CapabilityReindexResponse, CapabilityRegistry, CapabilityResolveResponse,
};
use medousa_mcp_gateway::verify_policy_bearer;
use crate::mcp_gateway_api::{
    resolve_mcp_gateway_url, McpGatewayHealthResponse, McpPolicyEvaluateRequest,
    McpPolicyEvaluateResponse, McpServerSummary, McpServersResponse,
};
use crate::mcp_policy::evaluate_mcp_policy_with_identity;
use crate::tools::TuiRuntime;
use medousa_types::{
    McpGatewayHealthSnapshot, McpGatewayServerRuntime, McpGatewayStatusResponse,
};

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

pub async fn list_capability_intents(
    State(state): State<CapabilityApiState>,
) -> Json<medousa_types::feed::CapabilityIntentsResponse> {
    let registry = state.agent_runtime.capability_registry.read().await;
    Json(registry.list_intents())
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

pub async fn mcp_gateway_status(
    State(state): State<CapabilityApiState>,
) -> Json<McpGatewayStatusResponse> {
    let gateway_url = resolve_mcp_gateway_url(None);
    let client = &state.agent_runtime.mcp_gateway_client;
    let health_result = client.health().await;
    let servers_result = if health_result.is_ok() {
        client.list_servers().await
    } else {
        Err(anyhow::anyhow!("gateway health check failed"))
    };

    Json(build_mcp_gateway_status_response(
        gateway_url,
        health_result,
        servers_result,
    ))
}

fn build_mcp_gateway_status_response(
    gateway_url: String,
    health_result: Result<McpGatewayHealthResponse, anyhow::Error>,
    servers_result: Result<McpServersResponse, anyhow::Error>,
) -> McpGatewayStatusResponse {
    match health_result {
        Ok(health) => {
            let servers = servers_result
                .map(|response| response.servers.into_iter().map(map_server_runtime).collect())
                .unwrap_or_default();
            McpGatewayStatusResponse {
                gateway_url,
                reachable: true,
                message: "MCP gateway is running".to_string(),
                health: Some(map_health_snapshot(&health)),
                servers,
            }
        }
        Err(err) => McpGatewayStatusResponse {
            gateway_url,
            reachable: false,
            message: format!(
                "MCP gateway is not running on the workshop host ({err:#})"
            ),
            health: None,
            servers: Vec::new(),
        },
    }
}

fn map_health_snapshot(health: &McpGatewayHealthResponse) -> McpGatewayHealthSnapshot {
    McpGatewayHealthSnapshot {
        status: health.status.clone(),
        invokes_enabled: health.invokes_enabled,
        registered_servers: u32::try_from(health.registered_servers).unwrap_or(u32::MAX),
        connected_servers: u32::try_from(health.connected_servers).unwrap_or(u32::MAX),
        catalog_entries: u32::try_from(health.catalog_entries).unwrap_or(u32::MAX),
    }
}

fn map_server_runtime(server: McpServerSummary) -> McpGatewayServerRuntime {
    McpGatewayServerRuntime {
        server_id: server.server_id,
        title: server.title,
        enabled: server.enabled,
        connected: server.connected,
        tool_count: u32::try_from(server.tool_count).unwrap_or(u32::MAX),
        allowed_lanes: server.allowed_lanes,
    }
}

pub async fn mcp_policy_evaluate(
    State(state): State<McpPolicyApiState>,
    headers: HeaderMap,
    Json(request): Json<McpPolicyEvaluateRequest>,
) -> Result<Json<McpPolicyEvaluateResponse>, (StatusCode, String)> {
    if !verify_policy_bearer(
        headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
        medousa_mcp_gateway::resolve_mcp_policy_token().as_deref(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn build_status_when_gateway_healthy() {
        let response = build_mcp_gateway_status_response(
            "http://127.0.0.1:7420".to_string(),
            Ok(McpGatewayHealthResponse {
                status: "ok".to_string(),
                invokes_enabled: true,
                registered_servers: 2,
                connected_servers: 1,
                catalog_entries: 4,
                now_utc: Utc::now(),
            }),
            Ok(McpServersResponse {
                servers: vec![McpServerSummary {
                    server_id: "notion".to_string(),
                    title: "Notion".to_string(),
                    enabled: true,
                    connected: true,
                    tool_count: 3,
                    allowed_lanes: vec!["interactive".to_string()],
                }],
            }),
        );
        assert!(response.reachable);
        assert_eq!(response.servers.len(), 1);
        assert_eq!(
            response.health.as_ref().map(|health| health.catalog_entries),
            Some(4)
        );
    }

    #[test]
    fn build_status_when_gateway_unreachable() {
        let response = build_mcp_gateway_status_response(
            "http://127.0.0.1:7420".to_string(),
            Err(anyhow::anyhow!("connection refused")),
            Err(anyhow::anyhow!("skipped")),
        );
        assert!(!response.reachable);
        assert!(response.health.is_none());
        assert!(response.servers.is_empty());
    }
}
