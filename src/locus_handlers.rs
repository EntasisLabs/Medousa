//! HTTP adapter for transport-free Locus operations (`/v1/locus/*`).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;

use crate::daemon_api::{
    LocusNodeDetailResponse, LocusNodesListResponse, LocusNodesQuery, LocusTagsListResponse,
    LocusTagsQuery,
};
pub use crate::locus_service::LocusService as LocusApiState;
use crate::locus_service::LocusServiceError;

fn map_locus_error(error: LocusServiceError) -> (StatusCode, String) {
    let status = match &error {
        LocusServiceError::Invalid(_) => StatusCode::BAD_REQUEST,
        LocusServiceError::NotFound(_) => StatusCode::NOT_FOUND,
        LocusServiceError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string())
}

pub fn locus_surface() -> crate::daemon::route_policy::DeclaredRouter<LocusApiState> {
    use crate::daemon::route_policy::{
        BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
    };
    use crate::request_principal::Capability;

    let policy = |path| RoutePolicy {
        method: axum::http::Method::GET,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(Capability::ContentRead),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: 1024,
        rate_limit_class: RateLimitClass::Read,
    };

    DeclaredRouter::default()
        .route(policy("/v1/locus/nodes"), get(list_locus_nodes))
        .route(policy("/v1/locus/nodes/{sync_key}"), get(get_locus_node))
        .route(policy("/v1/locus/tags"), get(list_locus_tags))
}

pub async fn list_locus_nodes(
    State(service): State<LocusApiState>,
    Query(query): Query<LocusNodesQuery>,
) -> Result<Json<LocusNodesListResponse>, (StatusCode, String)> {
    service
        .list_nodes(query)
        .await
        .map(Json)
        .map_err(map_locus_error)
}

pub async fn list_locus_tags(
    State(service): State<LocusApiState>,
    Query(query): Query<LocusTagsQuery>,
) -> Result<Json<LocusTagsListResponse>, (StatusCode, String)> {
    service
        .list_tags(query)
        .await
        .map(Json)
        .map_err(map_locus_error)
}

pub async fn get_locus_node(
    State(service): State<LocusApiState>,
    Path(sync_key): Path<String>,
) -> Result<Json<LocusNodeDetailResponse>, (StatusCode, String)> {
    service
        .get_node(&sync_key)
        .await
        .map(Json)
        .map_err(map_locus_error)
}
