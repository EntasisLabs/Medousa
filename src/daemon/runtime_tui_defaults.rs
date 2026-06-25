//! Full workshop `tui_defaults.json` read/write for per-engine settings parity.

use axum::{http::StatusCode, routing::get, Json, Router};
use serde_json::Value;

use crate::session::{load_tui_defaults_value, save_tui_defaults_merged};

pub fn routes() -> Router {
    Router::new().route(
        "/v1/runtime/tui-defaults",
        get(get_tui_defaults).put(put_tui_defaults),
    )
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
