//! HTTP handlers for `/v1/components/{component_id}/store/*` (MedousaStore).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::Json;
use medousa_types::component_store::{
    ComponentStoreDeleteResponse, ComponentStoreGetResponse, ComponentStoreListResponse,
    ComponentStoreQuery, ComponentStoreSetRequest, ComponentStoreSetResponse,
};

use crate::component_store::{component_exists_in_profile, component_store_service};
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::environment_store::resolve_profile_id;

#[derive(Clone)]
pub struct ComponentStoreApiState;

pub fn component_store_surface() -> DeclaredRouter<ComponentStoreApiState> {
    use axum::routing::{delete, put};

    DeclaredRouter::default()
        .methods([
            (
                component_store_read_policy("/v1/components/{component_id}/store"),
                get(get_store),
            ),
            (
                component_store_write_policy(
                    axum::http::Method::PUT,
                    "/v1/components/{component_id}/store",
                    256 * 1024,
                ),
                put(put_store_entry),
            ),
        ])
        .route(
            component_store_read_policy("/v1/components/{component_id}/store/keys"),
            get(list_store_keys),
        )
        .methods([
            (
                component_store_read_policy("/v1/components/{component_id}/store/{key}"),
                get(get_store_key),
            ),
            (
                component_store_write_policy(
                    axum::http::Method::PUT,
                    "/v1/components/{component_id}/store/{key}",
                    256 * 1024,
                ),
                put(put_store_key),
            ),
            (
                component_store_write_policy(
                    axum::http::Method::DELETE,
                    "/v1/components/{component_id}/store/{key}",
                    1024,
                ),
                delete(delete_store_key),
            ),
        ])
}

fn component_store_read_policy(path: &'static str) -> RoutePolicy {
    component_store_policy(
        axum::http::Method::GET,
        path,
        crate::request_principal::Capability::ContentRead,
        1024,
        RateLimitClass::Read,
    )
}

fn component_store_write_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    component_store_policy(
        method,
        path,
        crate::request_principal::Capability::ContentWrite,
        body_limit,
        RateLimitClass::Mutation,
    )
}

fn component_store_policy(
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

async fn ensure_component_allowed(
    profile_id: &str,
    component_id: &str,
) -> Result<(), (StatusCode, String)> {
    if !component_exists_in_profile(profile_id, component_id).await {
        return Err((
            StatusCode::NOT_FOUND,
            format!("component '{component_id}' is not registered on profile '{profile_id}'"),
        ));
    }
    Ok(())
}

async fn get_store(
    State(_state): State<ComponentStoreApiState>,
    Path(component_id): Path<String>,
    Query(query): Query<ComponentStoreQuery>,
) -> Result<Json<ComponentStoreGetResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let component_id = component_id.trim();
    ensure_component_allowed(&profile_id, component_id).await?;
    let key = query
        .key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    component_store_service()
        .get(&profile_id, component_id, key)
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn get_store_key(
    State(_state): State<ComponentStoreApiState>,
    Path((component_id, key)): Path<(String, String)>,
    Query(query): Query<ComponentStoreQuery>,
) -> Result<Json<ComponentStoreGetResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let component_id = component_id.trim();
    let key = key.trim();
    ensure_component_allowed(&profile_id, component_id).await?;
    component_store_service()
        .get(&profile_id, component_id, Some(key))
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn put_store_entry(
    State(_state): State<ComponentStoreApiState>,
    Path(component_id): Path<String>,
    Query(query): Query<ComponentStoreQuery>,
    Json(body): Json<ComponentStoreSetRequest>,
) -> Result<Json<ComponentStoreSetResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(
        body.profile_id
            .as_deref()
            .or(query.profile_id.as_deref()),
    );
    let component_id = component_id.trim();
    let key = query
        .key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "query parameter key is required".to_string(),
            )
        })?;
    ensure_component_allowed(&profile_id, component_id).await?;
    component_store_service()
        .set(&profile_id, component_id, key, body.value)
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn put_store_key(
    State(_state): State<ComponentStoreApiState>,
    Path((component_id, key)): Path<(String, String)>,
    Json(body): Json<ComponentStoreSetRequest>,
) -> Result<Json<ComponentStoreSetResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(body.profile_id.as_deref());
    let component_id = component_id.trim();
    let key = key.trim();
    ensure_component_allowed(&profile_id, component_id).await?;
    component_store_service()
        .set(&profile_id, component_id, key, body.value)
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn delete_store_key(
    State(_state): State<ComponentStoreApiState>,
    Path((component_id, key)): Path<(String, String)>,
    Query(query): Query<ComponentStoreQuery>,
) -> Result<Json<ComponentStoreDeleteResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let component_id = component_id.trim();
    let key = key.trim();
    ensure_component_allowed(&profile_id, component_id).await?;
    component_store_service()
        .delete(&profile_id, component_id, key)
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn list_store_keys(
    State(_state): State<ComponentStoreApiState>,
    Path(component_id): Path<String>,
    Query(query): Query<ComponentStoreQuery>,
) -> Result<Json<ComponentStoreListResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let component_id = component_id.trim();
    ensure_component_allowed(&profile_id, component_id).await?;
    component_store_service()
        .list_keys(&profile_id, component_id)
        .await
        .map(Json)
        .map_err(internal_error)
}

fn internal_error(message: String) -> (StatusCode, String) {
    if message.contains("invalid component_id") || message.contains("invalid store key") {
        return (StatusCode::BAD_REQUEST, message);
    }
    if message.contains("key limit reached") || message.contains("exceeds max bytes") {
        return (StatusCode::PAYLOAD_TOO_LARGE, message);
    }
    (StatusCode::INTERNAL_SERVER_ERROR, message)
}
