//! Full workshop `tui_defaults.json` read/write for per-engine settings parity.

use axum::{Json, http::StatusCode, routing::get};
use serde_json::Value;

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;
use crate::session::{load_tui_defaults_value, save_tui_defaults_merged};

pub fn surface() -> DeclaredRouter<AppState> {
    DeclaredRouter::default()
        .methods([
            (
                runtime_policy(axum::http::Method::GET, "/v1/runtime/tui-defaults", 1024),
                get(get_tui_defaults),
            ),
            (
                runtime_policy(
                    axum::http::Method::PUT,
                    "/v1/runtime/tui-defaults",
                    1024 * 1024,
                ),
                axum::routing::put(put_tui_defaults),
            ),
        ])
        .methods([
            (
                runtime_policy(axum::http::Method::GET, "/v1/runtime/workers", 1024),
                get(get_runtime_workers),
            ),
            (
                runtime_policy(axum::http::Method::PUT, "/v1/runtime/workers", 64 * 1024),
                axum::routing::put(put_runtime_workers),
            ),
        ])
}

fn runtime_policy(
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

async fn get_runtime_workers() -> Json<crate::product_config::RuntimeWorkerConfig> {
    Json(crate::load_product_config().runtime.workers)
}

async fn put_runtime_workers(
    Json(workers): Json<crate::product_config::RuntimeWorkerConfig>,
) -> Result<Json<crate::product_config::RuntimeWorkerConfig>, (StatusCode, String)> {
    workers
        .validate()
        .map_err(|err| (StatusCode::BAD_REQUEST, err))?;
    let mut config = crate::load_product_config();
    config.runtime.workers = workers;
    crate::save_product_config(&config)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(config.runtime.workers))
}

async fn get_tui_defaults() -> Json<Value> {
    Json(load_tui_defaults_value())
}

async fn put_tui_defaults(
    Json(incoming): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let saved = save_tui_defaults_merged(incoming).map_err(|err| (StatusCode::BAD_REQUEST, err))?;
    let _ = saved;
    Ok(Json(load_tui_defaults_value()))
}
