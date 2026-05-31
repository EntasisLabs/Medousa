//! MCP Client gateway — config, mock catalog, HTTP handlers.
//!
//! Design: docs/internal/mcp-client-gateway-design.md

mod auth;
mod catalog;
mod config;

pub use auth::{verify_admin_bearer, verify_gateway_bearer, verify_policy_bearer};
pub use catalog::{discover_from_catalog, mock_catalog_sync_response, mock_tool_catalog};
pub use config::{
    McpGatewayConfig, resolve_mcp_gateway_admin_token, resolve_mcp_gateway_token,
    resolve_mcp_policy_token,
};

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use tokio::sync::RwLock;

use crate::capability_catalog::McpCatalogSyncResponse;
use crate::mcp_gateway_api::{
    McpAdminStatusResponse, McpDiscoverRequest, McpDiscoverResponse,
    McpGatewayHealthResponse,
};

#[derive(Clone)]
pub struct GatewayState {
    pub config: McpGatewayConfig,
    pub invokes_enabled: Arc<RwLock<bool>>,
}

pub fn build_router(state: GatewayState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/mcp/discover", post(discover))
        .route("/v1/mcp/catalog", get(catalog))
        .route("/v1/admin/invokes/disable", post(admin_disable_invokes))
        .route("/v1/admin/invokes/enable", post(admin_enable_invokes))
        .with_state(state)
}

pub async fn serve(config: McpGatewayConfig) -> anyhow::Result<()> {
    let addr: SocketAddr = config
        .bind
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid MCP gateway bind {}: {error}", config.bind))?;

    let state = GatewayState {
        invokes_enabled: Arc::new(RwLock::new(config.invokes_enabled)),
        config,
    };

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|error| anyhow::anyhow!("failed to bind medousa-mcp-gateway on {addr}: {error}"))?;

    println!("medousa-mcp-gateway listening on http://{addr}");
    axum::serve(listener, app)
        .await
        .map_err(|error| anyhow::anyhow!("medousa-mcp-gateway server error: {error}"))
}

async fn health(State(state): State<GatewayState>) -> Json<McpGatewayHealthResponse> {
    let catalog = mock_tool_catalog();
    Json(McpGatewayHealthResponse {
        status: "ok".to_string(),
        invokes_enabled: *state.invokes_enabled.read().await,
        registered_servers: 3,
        connected_servers: 3,
        catalog_entries: catalog.len(),
        now_utc: Utc::now(),
    })
}

async fn discover(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(request): Json<McpDiscoverRequest>,
) -> Result<Json<McpDiscoverResponse>, (StatusCode, String)> {
    if !verify_gateway_bearer(
        headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
        state.config.gateway_token.as_deref(),
    ) {
        return Err((StatusCode::UNAUTHORIZED, "invalid gateway bearer token".to_string()));
    }

    let limit = request.limit.clamp(1, 100);
    let matches = discover_from_catalog(&request.query, request.server_id.as_deref(), limit);
    let truncated = matches.len() >= limit;

    Ok(Json(McpDiscoverResponse {
        query: request.query,
        matches,
        truncated,
        gateway_unreachable: None,
    }))
}

async fn catalog(
    State(state): State<GatewayState>,
    headers: HeaderMap,
) -> Result<Json<McpCatalogSyncResponse>, (StatusCode, String)> {
    if !verify_gateway_bearer(
        headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
        state.config.gateway_token.as_deref(),
    ) {
        return Err((StatusCode::UNAUTHORIZED, "invalid gateway bearer token".to_string()));
    }

    Ok(Json(mock_catalog_sync_response()))
}

async fn admin_disable_invokes(
    State(state): State<GatewayState>,
    headers: HeaderMap,
) -> Result<Json<McpAdminStatusResponse>, (StatusCode, String)> {
    if !verify_admin_bearer(
        headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
        state.config.admin_token.as_deref(),
    ) {
        return Err((StatusCode::UNAUTHORIZED, "invalid admin bearer token".to_string()));
    }

    *state.invokes_enabled.write().await = false;
    Ok(Json(McpAdminStatusResponse {
        invokes_enabled: false,
        changed_at_utc: Utc::now(),
        reason: Some("admin disable".to_string()),
    }))
}

async fn admin_enable_invokes(
    State(state): State<GatewayState>,
    headers: HeaderMap,
) -> Result<Json<McpAdminStatusResponse>, (StatusCode, String)> {
    if !verify_admin_bearer(
        headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
        state.config.admin_token.as_deref(),
    ) {
        return Err((StatusCode::UNAUTHORIZED, "invalid admin bearer token".to_string()));
    }

    *state.invokes_enabled.write().await = true;
    Ok(Json(McpAdminStatusResponse {
        invokes_enabled: true,
        changed_at_utc: Utc::now(),
        reason: Some("admin enable".to_string()),
    }))
}
