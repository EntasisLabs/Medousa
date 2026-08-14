//! HTTP handlers for LAN discovery (`/v1/lan/*`).

use std::time::Duration;

use axum::routing::get;
use axum::Json;
use serde::Serialize;

use crate::pairing::mdns::{browse_workshops, DiscoveredWorkshop};
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanWorkshopsResponse {
    pub workshops: Vec<DiscoveredWorkshop>,
    pub browse_ms: u64,
}

pub fn lan_surface() -> DeclaredRouter {
    DeclaredRouter::default().route(
        RoutePolicy {
            method: axum::http::Method::GET,
            path: "/v1/lan/workshops",
            group: RouteGroup::Administration,
            required_capability: Some(crate::request_principal::Capability::AdminIdentity),
            bootstrap_public: false,
            browser_policy: BrowserPolicy::NativeOnly,
            body_limit: 1024,
            rate_limit_class: RateLimitClass::Administration,
        },
        get(list_lan_workshops),
    )
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
