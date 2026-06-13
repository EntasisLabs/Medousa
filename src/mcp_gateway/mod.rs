//! MCP Client gateway — config, registry, HTTP handlers.

mod auth;
mod catalog;
mod config;
mod policy_client;
mod registry;
mod server_config;
mod starter_config;
mod remote_client;
mod stdio_client;

pub use auth::{verify_admin_bearer, verify_gateway_bearer, verify_policy_bearer};
pub use catalog::{discover_from_catalog, mock_catalog_sync_response, mock_tool_catalog};
pub use config::{
    resolve_mcp_gateway_admin_token, resolve_mcp_gateway_token, resolve_mcp_policy_token,
};
pub use registry::ServerRegistry;
pub use server_config::{McpGatewayFullConfig, gateway_config_path};
pub use starter_config::{STARTER_MCP_GATEWAY_TOML, install_starter_gateway_config_if_missing};

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use tokio::sync::RwLock;

use crate::mcp_gateway_api::{
    McpAdminStatusResponse, McpDiscoverRequest, McpDiscoverResponse, McpGatewayHealthResponse,
    McpInvokeRequest, McpInvokeResponse, McpServersResponse,
};

#[derive(Clone)]
pub struct GatewayState {
    pub config: Arc<McpGatewayFullConfig>,
    pub invokes_enabled: Arc<RwLock<bool>>,
    pub registry: Arc<ServerRegistry>,
}

pub fn build_router(state: GatewayState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/mcp/discover", post(discover))
        .route("/v1/mcp/catalog", get(catalog))
        .route("/v1/mcp/servers", get(list_servers))
        .route("/v1/mcp/invoke", post(invoke))
        .route("/v1/admin/invokes/disable", post(admin_disable_invokes))
        .route("/v1/admin/invokes/enable", post(admin_enable_invokes))
        .route("/v1/admin/catalog/refresh", post(admin_refresh_catalog))
        .with_state(state)
}

pub async fn serve(config: McpGatewayFullConfig) -> anyhow::Result<()> {
    let addr: SocketAddr = config
        .bind
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid MCP gateway bind {}: {error}", config.bind))?;

    let config = Arc::new(config);
    let registry = Arc::new(ServerRegistry::new(config.clone()));
    registry.bootstrap().await;
    registry.clone().spawn_refresh_loop();

    let state = GatewayState {
        invokes_enabled: Arc::new(RwLock::new(config.invokes_enabled)),
        registry,
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
    let (registered, connected, entries) = state.registry.health_stats().await;
    Json(McpGatewayHealthResponse {
        status: "ok".to_string(),
        invokes_enabled: *state.invokes_enabled.read().await,
        registered_servers: registered,
        connected_servers: connected,
        catalog_entries: entries,
        now_utc: Utc::now(),
    })
}

async fn discover(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(request): Json<McpDiscoverRequest>,
) -> Result<Json<McpDiscoverResponse>, (StatusCode, String)> {
    authorize_gateway(&headers, &state)?;

    let limit = request.limit.clamp(1, 100);
    let matches = state
        .registry
        .discover(&request.query, request.server_id.as_deref(), limit)
        .await;
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
) -> Result<Json<crate::capability_catalog::McpCatalogSyncResponse>, (StatusCode, String)> {
    authorize_gateway(&headers, &state)?;
    Ok(Json(state.registry.catalog_sync().await))
}

async fn list_servers(
    State(state): State<GatewayState>,
    headers: HeaderMap,
) -> Result<Json<McpServersResponse>, (StatusCode, String)> {
    authorize_gateway(&headers, &state)?;
    Ok(Json(state.registry.list_servers().await))
}

async fn invoke(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(request): Json<McpInvokeRequest>,
) -> Result<Json<McpInvokeResponse>, (StatusCode, String)> {
    authorize_gateway(&headers, &state)?;
    let invokes_enabled = *state.invokes_enabled.read().await;
    Ok(Json(
        state.registry.invoke(request, invokes_enabled).await,
    ))
}

async fn admin_refresh_catalog(
    State(state): State<GatewayState>,
    headers: HeaderMap,
) -> Result<Json<McpGatewayHealthResponse>, (StatusCode, String)> {
    if !verify_admin_bearer(
        headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
        state.config.admin_token.as_deref(),
    ) {
        return Err((StatusCode::UNAUTHORIZED, "invalid admin bearer token".to_string()));
    }

    state
        .registry
        .refresh_catalog()
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;

    health(State(state)).await.pipe(Ok)
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

fn authorize_gateway(headers: &HeaderMap, state: &GatewayState) -> Result<(), (StatusCode, String)> {
    if verify_gateway_bearer(
        headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
        state.config.gateway_token.as_deref(),
    ) {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "invalid gateway bearer token".to_string()))
    }
}

trait Pipe {
    type Output;
    fn pipe<F, R>(self, func: F) -> R
    where
        F: FnOnce(Self) -> R,
        Self: Sized;
}

impl<T> Pipe for T {
    type Output = T;
    fn pipe<F, R>(self, func: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        func(self)
    }
}
