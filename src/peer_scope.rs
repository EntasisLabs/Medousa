//! Reject peer-scoped bearer tokens outside inbox/share surfaces.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::body::Body;

use crate::pairing::{path_allowed_for_peer, PairingRole, PairingService};

pub async fn reject_peer_scope_escalation(
    State(pairing): State<Arc<PairingService>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    if let Some(token) = bearer_token(request.headers()) {
        match pairing.resolve_bearer_role(token) {
            Ok(Some(PairingRole::Peer)) if !path_allowed_for_peer(&path) => {
                return (
                    StatusCode::FORBIDDEN,
                    "Peer credentials can only use inbox and share surfaces",
                )
                    .into_response();
            }
            _ => {}
        }
    }
    next.run(request).await
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
