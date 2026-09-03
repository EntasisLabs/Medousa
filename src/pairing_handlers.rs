use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{delete, get, post, put},
};
use serde::Deserialize;

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::pairing::{
    PairHeartbeatRequest, PairInitRequest, PairSessionChallengeRequest,
    PairSessionRefreshRequest, PairTrustPolicyUpdateRequest, PairVerifyRequest, PairingService,
    RevokePairingAuthority, RevokePairingResult,
};
use crate::request_principal::{Capability, PrincipalKind, RequestPrincipal};

#[derive(Clone)]
pub struct PairingApiState {
    pub service: Arc<PairingService>,
}

/// Anonymous pairing ceremony routes. H01 will add the active-window and
/// admission limits; no administrative/readback route belongs here.
pub fn bootstrap_routes() -> Router<PairingApiState> {
    bootstrap_surface().into_router()
}

pub fn bootstrap_surface() -> DeclaredRouter<PairingApiState> {
    DeclaredRouter::default()
        .route(
            RoutePolicy {
                method: axum::http::Method::POST,
                path: "/pair/init",
                group: RouteGroup::PairingCeremony,
                required_capability: None,
                bootstrap_public: true,
                browser_policy: BrowserPolicy::Public,
                body_limit: 16 * 1024,
                rate_limit_class: RateLimitClass::PairingCeremony,
            },
            post(pair_init),
        )
        .route(
            RoutePolicy {
                method: axum::http::Method::POST,
                path: "/pair/verify",
                group: RouteGroup::PairingCeremony,
                required_capability: None,
                bootstrap_public: true,
                browser_policy: BrowserPolicy::Public,
                body_limit: 8 * 1024,
                rate_limit_class: RateLimitClass::PairingCeremony,
            },
            post(pair_verify),
        )
        .route(
            RoutePolicy {
                method: axum::http::Method::POST,
                path: "/pair/session/challenge",
                group: RouteGroup::PairingCeremony,
                required_capability: None,
                bootstrap_public: true,
                browser_policy: BrowserPolicy::Public,
                body_limit: 8 * 1024,
                rate_limit_class: RateLimitClass::PairingCeremony,
            },
            post(pair_session_challenge),
        )
        .route(
            RoutePolicy {
                method: axum::http::Method::POST,
                path: "/pair/session/refresh",
                group: RouteGroup::PairingCeremony,
                required_capability: None,
                bootstrap_public: true,
                browser_policy: BrowserPolicy::Public,
                body_limit: 16 * 1024,
                rate_limit_class: RateLimitClass::PairingCeremony,
            },
            post(pair_session_refresh),
        )
}

/// Pairing administration and authenticated peer lifecycle routes.
pub fn protected_routes() -> Router<PairingApiState> {
    protected_surface().into_router()
}

pub fn protected_surface() -> DeclaredRouter<PairingApiState> {
    DeclaredRouter::default()
        .route(
            peer_policy(axum::http::Method::GET, "/pair/status", 1024),
            get(pair_status),
        )
        .route(
            peer_policy(axum::http::Method::GET, "/pair/iroh-ticket", 1024),
            get(get_iroh_ticket),
        )
        .route(
            admin_policy(axum::http::Method::GET, "/qr", 1024),
            get(get_qr),
        )
        .route(
            admin_policy(axum::http::Method::POST, "/qr/rotate", 16 * 1024),
            post(rotate_qr),
        )
        .route(
            admin_policy(axum::http::Method::GET, "/qr/image", 1024),
            get(get_qr_image),
        )
        .route(
            admin_policy(axum::http::Method::GET, "/qr.png", 1024),
            get(get_qr_png),
        )
        .route(
            admin_policy(axum::http::Method::GET, "/pair/code", 1024),
            get(get_pair_code),
        )
        .methods([
            (
                peer_policy(axum::http::Method::GET, "/pair/heartbeat", 1024),
                get(pair_heartbeat),
            ),
            (
                peer_policy(axum::http::Method::POST, "/pair/heartbeat", 64 * 1024),
                post(pair_heartbeat_post),
            ),
        ])
        .route(
            peer_policy(axum::http::Method::DELETE, "/pair/{pairing_id}", 1024),
            delete(revoke_pairing),
        )
        .route(
            admin_policy(
                axum::http::Method::PUT,
                "/pair/{pairing_id}/policy",
                8 * 1024,
            ),
            put(update_pairing_policy),
        )
}

fn peer_policy(method: axum::http::Method, path: &'static str, body_limit: usize) -> RoutePolicy {
    protected_policy(
        method,
        path,
        RouteGroup::PeerExchange,
        Capability::PeerExchange,
        body_limit,
        RateLimitClass::PeerExchange,
    )
}

fn admin_policy(method: axum::http::Method, path: &'static str, body_limit: usize) -> RoutePolicy {
    protected_policy(
        method,
        path,
        RouteGroup::Administration,
        Capability::AdminIdentity,
        body_limit,
        RateLimitClass::Administration,
    )
}

fn protected_policy(
    method: axum::http::Method,
    path: &'static str,
    group: RouteGroup,
    required_capability: Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
}

async fn pair_status(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.service.pair_status().await {
        Ok(response) => Ok(Json(serde_json::to_value(response).unwrap_or_default())),
        Err(err) => {
            eprintln!("medousa-daemon: GET /pair/status failed: {err:#}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_iroh_ticket(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.service.iroh_ticket() {
        Some(response) => Ok(Json(serde_json::to_value(response).unwrap_or_default())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QrQuery {
    /// When true, embed Iroh ticket (v2). Default is compact v1 for camera/Messages.
    #[serde(default)]
    full: Option<bool>,
    /// Shared-mode seat invite: bind the pairing to this profile id.
    #[serde(default)]
    profile_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RotateQrBody {
    #[serde(default)]
    profile_id: Option<String>,
}

fn wants_full_qr(query: &QrQuery) -> bool {
    query.full.unwrap_or(false)
}

async fn get_qr(
    State(state): State<PairingApiState>,
    Query(query): Query<QrQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .current_qr_with_options(wants_full_qr(&query))
        .await
        .map(|response| Json(serde_json::to_value(response).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn rotate_qr(
    State(state): State<PairingApiState>,
    Query(query): Query<QrQuery>,
    body: Option<Json<RotateQrBody>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let profile_id = body
        .and_then(|Json(body)| body.profile_id)
        .or(query.profile_id);
    state
        .service
        .rotate_qr_for_profile(profile_id.as_deref())
        .await
        .map(|response| Json(serde_json::to_value(response).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_qr_png(
    State(state): State<PairingApiState>,
    Query(query): Query<QrQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let qr = state
        .service
        .current_qr_with_options(wants_full_qr(&query))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let png = state
        .service
        .render_qr_png(&qr.url)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(axum::http::header::CONTENT_TYPE, "image/png")], png))
}

async fn get_qr_image(
    State(state): State<PairingApiState>,
    Query(query): Query<QrQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .current_qr_image_with_options(wants_full_qr(&query))
        .await
        .map(|response| Json(serde_json::to_value(response).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_pair_code(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .current_short_code()
        .await
        .map(|short_code| Json(serde_json::json!({ "shortCode": short_code })))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn pair_init(
    State(state): State<PairingApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<PairInitRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let _permit = state
        .service
        .try_acquire_ceremony()
        .ok_or(StatusCode::TOO_MANY_REQUESTS)?;
    let response = state
        .service
        .pair_init(body, addr.ip())
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let status = if response.status == "challenge" {
        StatusCode::OK
    } else if response.reason.as_deref() == Some("rate_limited") {
        StatusCode::TOO_MANY_REQUESTS
    } else if response.reason.as_deref() == Some("busy") {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::BAD_REQUEST
    };
    Ok((
        status,
        Json(serde_json::to_value(response).unwrap_or_default()),
    ))
}

async fn pair_verify(
    State(state): State<PairingApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<PairVerifyRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let _permit = state
        .service
        .try_acquire_ceremony()
        .ok_or(StatusCode::TOO_MANY_REQUESTS)?;
    match state.service.pair_verify(body, addr.ip()).await {
        Ok(response) => {
            let status = if response.status == "paired" {
                StatusCode::OK
            } else if response.reason.as_deref() == Some("rate_limited") {
                StatusCode::TOO_MANY_REQUESTS
            } else {
                StatusCode::BAD_REQUEST
            };
            Ok((
                status,
                Json(serde_json::to_value(response).unwrap_or_default()),
            ))
        }
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn pair_session_challenge(
    State(state): State<PairingApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<PairSessionChallengeRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let _permit = state
        .service
        .try_acquire_ceremony()
        .ok_or(StatusCode::TOO_MANY_REQUESTS)?;
    let response = state
        .service
        .pair_session_challenge(body, addr.ip())
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let status = match (response.status.as_str(), response.reason.as_deref()) {
        ("challenge", _) => StatusCode::OK,
        (_, Some("rate_limited")) => StatusCode::TOO_MANY_REQUESTS,
        (_, Some("busy")) => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::UNAUTHORIZED,
    };
    Ok((
        status,
        Json(serde_json::to_value(response).unwrap_or_default()),
    ))
}

async fn pair_session_refresh(
    State(state): State<PairingApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<PairSessionRefreshRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let _permit = state
        .service
        .try_acquire_ceremony()
        .ok_or(StatusCode::TOO_MANY_REQUESTS)?;
    let response = state
        .service
        .pair_session_refresh(body, addr.ip())
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let status = match (response.status.as_str(), response.reason.as_deref()) {
        ("refreshed", _) => StatusCode::OK,
        (_, Some("rate_limited")) => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::UNAUTHORIZED,
    };
    Ok((
        status,
        Json(serde_json::to_value(response).unwrap_or_default()),
    ))
}

async fn pair_heartbeat(
    State(state): State<PairingApiState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    run_pair_heartbeat(&state, &principal, None).await
}

async fn pair_heartbeat_post(
    State(state): State<PairingApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(body): Json<PairHeartbeatRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    run_pair_heartbeat(&state, &principal, Some(body)).await
}

async fn run_pair_heartbeat(
    state: &PairingApiState,
    principal: &RequestPrincipal,
    body: Option<PairHeartbeatRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let response = state
        .service
        .pair_heartbeat(principal.credential_id().map(|id| id.as_str()), body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = if response.status == "ok" {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    };
    Ok((
        status,
        Json(serde_json::to_value(response).unwrap_or_default()),
    ))
}

async fn revoke_pairing(
    State(state): State<PairingApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(pairing_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let authority = match principal.kind() {
        PrincipalKind::LocalApp | PrincipalKind::Root => RevokePairingAuthority::Administrator,
        _ => principal
            .credential_id()
            .map(|id| RevokePairingAuthority::Credential(id.as_str()))
            .unwrap_or(RevokePairingAuthority::Unauthenticated),
    };
    match state.service.revoke_pairing(&pairing_id, authority).await {
        Ok(RevokePairingResult::Removed) => Ok(StatusCode::NO_CONTENT),
        Ok(RevokePairingResult::NotFound) => Err(StatusCode::NOT_FOUND),
        Ok(RevokePairingResult::Unauthorized) => Err(StatusCode::UNAUTHORIZED),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn update_pairing_policy(
    State(state): State<PairingApiState>,
    Path(pairing_id): Path<String>,
    Json(body): Json<PairTrustPolicyUpdateRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let pairing_id = pairing_id.trim();
    if pairing_id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    match state.service.update_trust_policy(pairing_id, body).await {
        Ok(Some(summary)) => Ok(Json(serde_json::to_value(summary).unwrap_or_default())),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_bootstrap_inventory_is_exact() {
        let entries = bootstrap_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].method, "POST");
        assert_eq!(entries[0].path, "/pair/init");
        assert_eq!(entries[0].body_limit, 16 * 1024);
        assert_eq!(entries[1].method, "POST");
        assert_eq!(entries[1].path, "/pair/verify");
        assert_eq!(entries[1].body_limit, 8 * 1024);
        assert_eq!(entries[2].method, "POST");
        assert_eq!(entries[2].path, "/pair/session/challenge");
        assert_eq!(entries[3].method, "POST");
        assert_eq!(entries[3].path, "/pair/session/refresh");
        assert!(entries.iter().all(|entry| entry.bootstrap_public));
    }

    #[test]
    fn protected_pairing_inventory_separates_peer_and_admin_authority() {
        let entries = protected_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 11);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("peer.exchange"))
                .count(),
            5
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("admin.identity"))
                .count(),
            6
        );
        assert!(entries.iter().all(|entry| !entry.bootstrap_public));
    }
}
