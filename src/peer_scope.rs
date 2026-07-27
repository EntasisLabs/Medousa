//! Reject Peer escalation and enforce Shared-mode portal settings ACL.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::pairing::PairingService;
use crate::portal_acl::{PortalAclDecision, authorize_request};
use crate::remote_trust::is_trusted_local;

pub async fn reject_peer_scope_escalation(
    State(pairing): State<Arc<PairingService>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();
    let trusted_local = is_trusted_local(addr.ip(), request.headers());

    let record = bearer_token(request.headers()).and_then(|token| {
        pairing
            .resolve_bearer_record(token)
            .ok()
            .flatten()
    });

    let shared_mode = crate::shared_mode::is_shared_mode();
    match authorize_request(trusted_local, shared_mode, record.as_ref(), &method, &path) {
        PortalAclDecision::Allow => next.run(request).await,
        PortalAclDecision::Deny(reason) => (StatusCode::FORBIDDEN, reason).into_response(),
    }
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
