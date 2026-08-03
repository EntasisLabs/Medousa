//! Full workshop `tui_defaults.json` read/write for per-engine settings parity.

use axum::{Json, Router, http::StatusCode, routing::get};
use serde_json::Value;

use crate::session::{load_tui_defaults_value, save_tui_defaults_merged};

pub fn routes() -> Router {
    Router::new()
        .route(
            "/v1/runtime/tui-defaults",
            get(get_tui_defaults).put(put_tui_defaults),
        )
        .route(
            "/v1/runtime/workers",
            get(get_runtime_workers).put(put_runtime_workers),
        )
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
