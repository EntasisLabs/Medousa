//! Reject Peer escalation and enforce Shared-mode portal settings ACL.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::header::{CONTENT_TYPE, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::pairing::PairingService;
use crate::portal_acl::{PortalAclDecision, authorize_request};
use crate::remote_trust::is_trusted_local;
use crate::request_principal::{RequestPrincipal, TransportClass};

#[derive(Clone)]
pub struct DaemonAccessState {
    pairing: Option<Arc<PairingService>>,
    surface: AccessSurface,
}

impl DaemonAccessState {
    pub fn new(pairing: Option<Arc<PairingService>>) -> Self {
        Self {
            pairing,
            surface: AccessSurface::Protected,
        }
    }

    fn for_surface(&self, surface: AccessSurface) -> Self {
        Self {
            pairing: self.pairing.clone(),
            surface,
        }
    }
}

#[derive(Clone, Copy)]
enum AccessSurface {
    Bootstrap,
    Protected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessDenial {
    AuthenticationRequired,
    InvalidCredential,
    Forbidden,
}

impl AccessDenial {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            Self::AuthenticationRequired => (
                StatusCode::UNAUTHORIZED,
                r#"{"code":"authentication_required","message":"authentication is required"}"#,
            ),
            Self::InvalidCredential => (
                StatusCode::UNAUTHORIZED,
                r#"{"code":"invalid_credential","message":"the credential is invalid or expired"}"#,
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                r#"{"code":"forbidden","message":"the credential cannot access this resource"}"#,
            ),
        };
        let mut response = Response::builder()
            .status(status)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .expect("static access denial response");
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"medousa\""),
            );
        }
        response
    }
}

/// Assemble the final HTTP authority boundary. Both route groups traverse the
/// credential parser so malformed credentials cannot fall back to bootstrap;
/// only routes assembled into the bootstrap branch may proceed anonymously.
pub fn assemble_daemon_access_boundary(
    protected: Router,
    bootstrap: Router,
    state: DaemonAccessState,
) -> Router {
    let protected = protected.layer(axum::middleware::from_fn_with_state(
        state.for_surface(AccessSurface::Protected),
        enforce_daemon_access,
    ));
    let bootstrap = bootstrap.layer(axum::middleware::from_fn_with_state(
        state.for_surface(AccessSurface::Bootstrap),
        enforce_daemon_access,
    ));
    protected.merge(bootstrap)
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
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let trusted_local = is_trusted_local(addr.ip(), request.headers());
    let transport = TransportClass::from_request(addr.ip(), request.headers());

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
        return AccessDenial::InvalidCredential.into_response();
    }

    if matches!(state.surface, AccessSurface::Bootstrap) {
        let shared_mode = crate::shared_mode::is_shared_mode();
        let principal = record
            .map(|record| RequestPrincipal::from_pairing_record(record, transport, shared_mode))
            .unwrap_or_else(|| RequestPrincipal::anonymous(transport));
        request.extensions_mut().insert(principal);
        return next.run(request).await;
    }

    if !trusted_local && record.is_none() {
        return AccessDenial::AuthenticationRequired.into_response();
    }

    let shared_mode = crate::shared_mode::is_shared_mode();
    match authorize_request(
        trusted_local,
        shared_mode,
        record.as_ref(),
        request.method(),
        request.uri().path(),
    ) {
        PortalAclDecision::Allow => {
            let principal = match record {
                Some(record) => {
                    RequestPrincipal::from_pairing_record(record, transport, shared_mode)
                }
                None => RequestPrincipal::legacy_local(),
            };
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        PortalAclDecision::Deny(_reason) => {
            if record.is_some() {
                AccessDenial::Forbidden.into_response()
            } else {
                AccessDenial::AuthenticationRequired.into_response()
            }
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
    use axum::body::to_bytes;
    use axum::http::header::AUTHORIZATION;
    use axum::routing::get;
    use tower::ServiceExt;

    fn protected_test_app() -> Router {
        let protected = Router::new().route("/v1/turns", get(|| async { StatusCode::NO_CONTENT }));
        let bootstrap = Router::new().route("/health", get(|| async { StatusCode::OK }));
        assemble_daemon_access_boundary(protected, bootstrap, DaemonAccessState::new(None))
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
        response_with_authorization_values(path, source, values)
            .await
            .status()
    }

    async fn response_with_authorization_values(
        path: &str,
        source: &str,
        values: &[&str],
    ) -> Response {
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
    async fn access_denials_are_stable_json_and_challenge_on_401() {
        let cases = [
            (
                AccessDenial::AuthenticationRequired,
                StatusCode::UNAUTHORIZED,
                r#"{"code":"authentication_required","message":"authentication is required"}"#,
            ),
            (
                AccessDenial::InvalidCredential,
                StatusCode::UNAUTHORIZED,
                r#"{"code":"invalid_credential","message":"the credential is invalid or expired"}"#,
            ),
            (
                AccessDenial::Forbidden,
                StatusCode::FORBIDDEN,
                r#"{"code":"forbidden","message":"the credential cannot access this resource"}"#,
            ),
        ];

        for (denial, status, expected_body) in cases {
            let response = denial.into_response();
            assert_eq!(response.status(), status);
            assert_eq!(
                response.headers().get(CONTENT_TYPE).unwrap(),
                "application/json"
            );
            assert_eq!(
                response
                    .headers()
                    .get(WWW_AUTHENTICATE)
                    .map(|value| value.to_str().unwrap()),
                (status == StatusCode::UNAUTHORIZED).then_some("Bearer realm=\"medousa\"")
            );
            let body = to_bytes(response.into_body(), 256).await.unwrap();
            assert_eq!(body.as_ref(), expected_body.as_bytes());
        }
    }

    #[tokio::test]
    async fn invalid_credential_response_does_not_echo_the_secret() {
        let response = response_with_authorization_values(
            "/health",
            "192.0.2.10:43100",
            &["Bearer super-secret-token"],
        )
        .await;
        let body = to_bytes(response.into_body(), 256).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("super-secret-token"));
    }

    #[tokio::test]
    async fn loopback_retains_temporary_local_compatibility() {
        assert_eq!(
            request_from("/v1/turns", "127.0.0.1:43100", None).await,
            StatusCode::NO_CONTENT
        );
    }

    #[tokio::test]
    async fn route_path_cannot_grant_bootstrap_authority() {
        let protected = Router::new().route("/health", get(|| async { StatusCode::NO_CONTENT }));
        let app =
            assemble_daemon_access_boundary(protected, Router::new(), DaemonAccessState::new(None));
        let mut request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "192.0.2.10:43100"
                .parse::<SocketAddr>()
                .expect("valid source"),
        ));
        assert_eq!(
            app.oneshot(request).await.expect("response").status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn final_socket_boundary_denies_proxied_remote_application_access() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                protected_test_app().into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .expect("serve access boundary");
        });
        let client = reqwest::Client::new();

        let liveness = client
            .get(format!("http://{addr}/health"))
            .send()
            .await
            .expect("liveness request");
        assert_eq!(liveness.status(), StatusCode::OK);

        let protected = client
            .get(format!("http://{addr}/v1/turns"))
            .header(
                crate::remote_trust::TRANSPORT_HEADER,
                crate::remote_trust::TRANSPORT_IROH,
            )
            .send()
            .await
            .expect("proxied request");
        assert_eq!(protected.status(), StatusCode::UNAUTHORIZED);

        let invalid_bootstrap = client
            .get(format!("http://{addr}/health"))
            .header(
                crate::remote_trust::TRANSPORT_HEADER,
                crate::remote_trust::TRANSPORT_IROH,
            )
            .bearer_auth("unknown")
            .send()
            .await
            .expect("invalid bootstrap request");
        assert_eq!(invalid_bootstrap.status(), StatusCode::UNAUTHORIZED);

        server.abort();
    }
}
