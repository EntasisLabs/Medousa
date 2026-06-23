use axum::{
    extract::Query,
    routing::{get, post},
    Json, Router,
};

use super::registry;
use super::types::{
    ModelCapabilitiesLookupQuery, ModelCapabilitiesLookupResponse, ModelCatalogListQuery,
    ModelCatalogListResponse, ModelCatalogRefreshRequest, ModelCatalogRefreshResponse,
};

pub fn routes() -> Router {
    Router::new()
        .route("/v1/models/catalog", get(list_catalog))
        .route("/v1/models/capabilities", get(lookup_capabilities))
        .route("/v1/models/catalog/refresh", post(refresh_catalog))
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
