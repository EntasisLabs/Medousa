//! Generated HTTP surface for integration connections. Secret values never leave
//! the daemon.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post, put};
use medousa_types::{
    CreateIntegrationRequest, DeleteIntegrationResponse, IntegrationListResponse,
    IntegrationSecretMutationResponse, IntegrationSecretSlot, PatchIntegrationRequest,
    UpsertIntegrationSecretRequest,
};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::integration_store;
use crate::request_principal::Capability;

const SECRET_BODY_LIMIT: usize = 16 * 1024;

pub fn surface() -> DeclaredRouter {
    DeclaredRouter::default()
        .route(list_policy(), get(list_connections))
        .route(create_policy(), post(create_connection))
        .route(get_policy(), get(get_connection))
        .route(patch_policy(), patch(patch_connection))
        .route(delete_policy(), delete(delete_connection))
        .route(put_secret_policy(), put(put_secret))
        .route(delete_secret_policy(), delete(delete_secret))
}

fn admin_policy(method: axum::http::Method, path: &'static str, body_limit: usize) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(Capability::AdminRuntime),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class: RateLimitClass::Administration,
    }
}

fn list_policy() -> RoutePolicy {
    admin_policy(axum::http::Method::GET, "/v1/integrations", 1024)
}

fn create_policy() -> RoutePolicy {
    admin_policy(axum::http::Method::POST, "/v1/integrations", SECRET_BODY_LIMIT)
}

fn get_policy() -> RoutePolicy {
    admin_policy(
        axum::http::Method::GET,
        "/v1/integrations/{connection_id}",
        1024,
    )
}

fn patch_policy() -> RoutePolicy {
    admin_policy(
        axum::http::Method::PATCH,
        "/v1/integrations/{connection_id}",
        SECRET_BODY_LIMIT,
    )
}

fn delete_policy() -> RoutePolicy {
    admin_policy(
        axum::http::Method::DELETE,
        "/v1/integrations/{connection_id}",
        1024,
    )
}

fn put_secret_policy() -> RoutePolicy {
    admin_policy(
        axum::http::Method::PUT,
        "/v1/integrations/{connection_id}/secrets/{slot}",
        SECRET_BODY_LIMIT,
    )
}

fn delete_secret_policy() -> RoutePolicy {
    admin_policy(
        axum::http::Method::DELETE,
        "/v1/integrations/{connection_id}/secrets/{slot}",
        1024,
    )
}

async fn list_connections() -> Result<Json<IntegrationListResponse>, (StatusCode, String)> {
    let connections = integration_store::list_connections().map_err(internal)?;
    Ok(Json(IntegrationListResponse { connections }))
}

async fn create_connection(
    Json(request): Json<CreateIntegrationRequest>,
) -> Result<Json<medousa_types::IntegrationConnection>, (StatusCode, String)> {
    integration_store::create_connection(request)
        .map(Json)
        .map_err(bad_request)
}

async fn get_connection(
    Path(connection_id): Path<String>,
) -> Result<Json<medousa_types::IntegrationConnection>, (StatusCode, String)> {
    integration_store::get_connection(&connection_id)
        .map(Json)
        .map_err(not_found)
}

async fn patch_connection(
    Path(connection_id): Path<String>,
    Json(request): Json<PatchIntegrationRequest>,
) -> Result<Json<medousa_types::IntegrationConnection>, (StatusCode, String)> {
    integration_store::patch_connection(&connection_id, request)
        .map(Json)
        .map_err(bad_request)
}

async fn delete_connection(
    Path(connection_id): Path<String>,
) -> Result<Json<DeleteIntegrationResponse>, (StatusCode, String)> {
    let connection_id = integration_store::delete_connection(&connection_id).map_err(not_found)?;
    Ok(Json(DeleteIntegrationResponse {
        deleted: true,
        connection_id,
    }))
}

async fn put_secret(
    Path((connection_id, slot)): Path<(String, String)>,
    Json(request): Json<UpsertIntegrationSecretRequest>,
) -> Result<Json<IntegrationSecretMutationResponse>, (StatusCode, String)> {
    mutate_secret(connection_id, slot, Some(request.value.trim()))
}

async fn delete_secret(
    Path((connection_id, slot)): Path<(String, String)>,
) -> Result<Json<IntegrationSecretMutationResponse>, (StatusCode, String)> {
    mutate_secret(connection_id, slot, None)
}

fn mutate_secret(
    connection_id: String,
    slot: String,
    value: Option<&str>,
) -> Result<Json<IntegrationSecretMutationResponse>, (StatusCode, String)> {
    let connection = medousa_types::ConnectionId::parse(&connection_id)
        .map_err(|err| bad_request(err.to_string()))?;
    let slot = IntegrationSecretSlot::parse(&slot).map_err(|err| bad_request(err.to_string()))?;
    let _ = integration_store::get_connection(connection.as_str()).map_err(not_found)?;
    let configured = integration_store::save_connection_secret(&connection, slot, value)
        .map_err(internal)?;
    Ok(Json(IntegrationSecretMutationResponse {
        connection_id: connection.as_str().to_string(),
        slot,
        configured,
    }))
}

fn bad_request(message: String) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message)
}

fn not_found(message: String) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, message)
}

fn internal(message: String) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, message)
}
