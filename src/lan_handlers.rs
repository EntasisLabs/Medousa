//! HTTP handlers for LAN discovery (`/v1/lan/*`).

use std::time::Duration;

use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::pairing::mdns::{browse_workshops, DiscoveredWorkshop};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanWorkshopsResponse {
    pub workshops: Vec<DiscoveredWorkshop>,
    pub browse_ms: u64,
}

pub fn lan_router() -> Router {
    Router::new().route("/v1/lan/workshops", get(list_lan_workshops))
}

async fn list_lan_workshops() -> Result<Json<LanWorkshopsResponse>, (axum::http::StatusCode, String)> {
    let browse_ms = 2500u64;
    let workshops = browse_workshops(Duration::from_millis(browse_ms)).map_err(|err| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            err.to_string(),
        )
    })?;
    Ok(Json(LanWorkshopsResponse {
        workshops,
        browse_ms,
    }))
}
