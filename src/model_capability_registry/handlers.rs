use axum::{
    Json,
    extract::Query,
    routing::{get, post},
};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};

use super::registry;
use super::types::{
    ModelCapabilitiesLookupQuery, ModelCapabilitiesLookupResponse, ModelCatalogListQuery,
    ModelCatalogListResponse, ModelCatalogRefreshRequest, ModelCatalogRefreshResponse,
};

pub fn surface() -> DeclaredRouter {
    DeclaredRouter::default()
        .route(
            model_catalog_policy(axum::http::Method::GET, "/v1/models/catalog", 1024),
            get(list_catalog),
        )
        .route(
            model_catalog_policy(axum::http::Method::GET, "/v1/models/capabilities", 1024),
            get(lookup_capabilities),
        )
        .route(
            model_catalog_policy(
                axum::http::Method::POST,
                "/v1/models/catalog/refresh",
                256 * 1024,
            ),
            post(refresh_catalog),
        )
}

fn model_catalog_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(crate::request_principal::Capability::AdminRuntime),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class: RateLimitClass::Administration,
    }
}

async fn list_catalog(
    Query(query): Query<ModelCatalogListQuery>,
) -> Json<ModelCatalogListResponse> {
    Json(registry().list_catalog(query))
}

async fn lookup_capabilities(
    Query(query): Query<ModelCapabilitiesLookupQuery>,
) -> Json<ModelCapabilitiesLookupResponse> {
    Json(registry().resolve(&query.provider, &query.model))
}

async fn refresh_catalog(
    Json(request): Json<ModelCatalogRefreshRequest>,
) -> Json<ModelCatalogRefreshResponse> {
    Json(registry().refresh(request.providers).await)
}
