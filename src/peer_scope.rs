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

#[derive(Clone, Default)]
pub struct DaemonAccessState {
    pairing: Option<Arc<PairingService>>,
}

impl DaemonAccessState {
    pub fn new(pairing: Option<Arc<PairingService>>) -> Self {
        Self { pairing }
    }
}

/// Refuse a remotely reachable listener when no credential verifier can exist.
///
/// Loopback retains the bounded legacy-local compatibility path during H01.
/// This check is deliberately performed before binding or runtime startup.
pub fn validate_listener_security(
    addr: SocketAddr,
    pairing_enabled: bool,
) -> Result<(), &'static str> {
    if !addr.ip().is_loopback() && !pairing_enabled {
        return Err(
            "non-loopback daemon binding requires pairing authentication; remove MEDOUSA_PAIRING_DISABLE or bind to loopback",
        );
    }
    Ok(())
}

pub async fn enforce_daemon_access(
    State(state): State<DaemonAccessState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();
    let trusted_local = is_trusted_local(addr.ip(), request.headers());

    let credential = bearer_credential(request.headers());
    let record = match credential {
        BearerCredential::Missing | BearerCredential::Invalid => None,
        BearerCredential::Valid(token) => state
            .pairing
            .as_ref()
            .and_then(|pairing| pairing.resolve_bearer_record(token).ok().flatten()),
    };

    // A malformed, ambiguous, or unknown credential must not fall back to
    // anonymous bootstrap authority.
    if !matches!(credential, BearerCredential::Missing) && record.is_none() {
        return (StatusCode::UNAUTHORIZED, "invalid bearer credential").into_response();
    }

    let shared_mode = crate::shared_mode::is_shared_mode();
    match authorize_request(trusted_local, shared_mode, record.as_ref(), &method, &path) {
        PortalAclDecision::Allow => next.run(request).await,
        PortalAclDecision::Deny(reason) => {
            let status = if record.is_some() {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNAUTHORIZED
            };
            (status, reason).into_response()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BearerCredential<'a> {
    Missing,
    Valid(&'a str),
    Invalid,
}

fn bearer_credential(headers: &axum::http::HeaderMap) -> BearerCredential<'_> {
    let mut values = headers.get_all(axum::http::header::AUTHORIZATION).iter();
    let Some(value) = values.next() else {
        return BearerCredential::Missing;
    };
    if values.next().is_some() {
        return BearerCredential::Invalid;
    }
    let Ok(value) = value.to_str() else {
        return BearerCredential::Invalid;
    };
    let Some(token) = value.strip_prefix("Bearer ").map(str::trim) else {
        return BearerCredential::Invalid;
    };
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return BearerCredential::Invalid;
    }
    BearerCredential::Valid(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::header::AUTHORIZATION;
    use axum::routing::get;
    use tower::ServiceExt;

    fn protected_test_app() -> Router {
        Router::new()
            .route("/health", get(|| async { StatusCode::OK }))
            .route("/v1/turns", get(|| async { StatusCode::NO_CONTENT }))
            .layer(axum::middleware::from_fn_with_state(
                DaemonAccessState::default(),
                enforce_daemon_access,
            ))
    }

    async fn request_from(path: &str, source: &str, bearer: Option<&str>) -> StatusCode {
        let mut request = Request::builder().uri(path).body(Body::empty()).unwrap();
        request.extensions_mut().insert(ConnectInfo(
            source.parse::<SocketAddr>().expect("valid source"),
        ));
        if let Some(bearer) = bearer {
            request.headers_mut().insert(
                AUTHORIZATION,
                format!("Bearer {bearer}").parse().expect("valid header"),
            );
        }
        protected_test_app()
            .oneshot(request)
            .await
            .expect("middleware response")
            .status()
    }

    async fn request_with_authorization_values(
        path: &str,
        source: &str,
        values: &[&str],
    ) -> StatusCode {
        let mut request = Request::builder().uri(path).body(Body::empty()).unwrap();
        request.extensions_mut().insert(ConnectInfo(
            source.parse::<SocketAddr>().expect("valid source"),
        ));
        for value in values {
            request
                .headers_mut()
                .append(AUTHORIZATION, value.parse().expect("valid header value"));
        }
        protected_test_app()
            .oneshot(request)
            .await
            .expect("middleware response")
            .status()
    }

    #[test]
    fn listener_security_allows_loopback_without_pairing() {
        let addr = "127.0.0.1:7419".parse().expect("valid address");
        assert!(validate_listener_security(addr, false).is_ok());
    }

    #[test]
    fn listener_security_rejects_remote_without_pairing() {
        for addr in ["0.0.0.0:7419", "[::]:7419", "192.0.2.10:7419"] {
            let addr = addr.parse().expect("valid address");
            assert_eq!(
                validate_listener_security(addr, false),
                Err(
                    "non-loopback daemon binding requires pairing authentication; remove MEDOUSA_PAIRING_DISABLE or bind to loopback"
                )
            );
        }
    }

    #[test]
    fn listener_security_allows_remote_with_pairing() {
        let addr = "0.0.0.0:7419".parse().expect("valid address");
        assert!(validate_listener_security(addr, true).is_ok());
    }

    #[tokio::test]
    async fn remote_anonymous_request_is_limited_to_bootstrap() {
        assert_eq!(
            request_from("/health", "192.0.2.10:43100", None).await,
            StatusCode::OK
        );
        assert_eq!(
            request_from("/v1/turns", "192.0.2.10:43100", None).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn missing_pairing_state_keeps_remote_application_closed() {
        assert_eq!(
            request_from("/v1/turns", "192.0.2.10:43100", Some("unknown")).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn invalid_credential_cannot_fall_back_to_public_access() {
        assert_eq!(
            request_from("/health", "192.0.2.10:43100", Some("unknown")).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn malformed_or_ambiguous_authorization_is_not_anonymous() {
        for values in [
            &["Basic abc"] as &[&str],
            &["Bearer "],
            &["Bearer token with spaces"],
            &["Bearer one", "Bearer two"],
        ] {
            assert_eq!(
                request_with_authorization_values("/health", "192.0.2.10:43100", values).await,
                StatusCode::UNAUTHORIZED
            );
        }
    }

    #[tokio::test]
    async fn loopback_retains_temporary_local_compatibility() {
        assert_eq!(
            request_from("/v1/turns", "127.0.0.1:43100", None).await,
            StatusCode::NO_CONTENT
        );
    }
}
