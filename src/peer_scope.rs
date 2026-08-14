//! Reject Peer escalation and enforce Shared-mode portal settings ACL.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::header::{CONTENT_TYPE, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::daemon::route_policy::DeclaredRouter;
use crate::pairing::PairingService;
use crate::remote_trust::is_trusted_local;
use crate::request_principal::{Capability, RequestPrincipal, TransportClass};
use medousa_local_credential::LocalCredentialVerifier;

#[derive(Clone)]
pub struct DaemonAccessState {
    pairing: Option<Arc<PairingService>>,
    local_credential: Option<Arc<LocalCredentialVerifier>>,
    mcp_policy_token: Option<Arc<str>>,
    surface: AccessSurface,
    legacy_loopback_compatibility: bool,
}

impl DaemonAccessState {
    pub fn new(pairing: Option<Arc<PairingService>>) -> Self {
        Self {
            pairing,
            local_credential: None,
            mcp_policy_token: None,
            surface: AccessSurface::Protected,
            legacy_loopback_compatibility: true,
        }
    }

    pub fn with_local_credential(mut self, verifier: Arc<LocalCredentialVerifier>) -> Self {
        self.local_credential = Some(verifier);
        self
    }

    pub fn with_legacy_loopback_compatibility(mut self, enabled: bool) -> Self {
        self.legacy_loopback_compatibility = enabled;
        self
    }

    pub fn with_mcp_policy_token(mut self, token: Option<String>) -> Self {
        self.mcp_policy_token = token
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(Arc::from);
        self
    }

    fn for_surface(&self, surface: AccessSurface) -> Self {
        Self {
            pairing: self.pairing.clone(),
            local_credential: self.local_credential.clone(),
            mcp_policy_token: self.mcp_policy_token.clone(),
            surface,
            legacy_loopback_compatibility: self.legacy_loopback_compatibility,
        }
    }
}

#[derive(Clone, Copy)]
enum AccessSurface {
    Bootstrap,
    Declared,
    Preview,
    Protected,
}

static LEGACY_COMPATIBILITY_SURFACES: AtomicU8 = AtomicU8::new(0);
pub const LEGACY_LOOPBACK_COMPATIBILITY_REMOVAL_VERSION: &str = "0.11.0";

pub fn legacy_loopback_compatibility_enabled(listener: SocketAddr) -> bool {
    listener.ip().is_loopback()
        && version_tuple(env!("CARGO_PKG_VERSION"))
            < version_tuple(LEGACY_LOOPBACK_COMPATIBILITY_REMOVAL_VERSION)
}

fn version_tuple(version: &str) -> (u64, u64, u64) {
    let mut parts = version
        .split('.')
        .map(|part| part.parse().unwrap_or(u64::MAX));
    (
        parts.next().unwrap_or(u64::MAX),
        parts.next().unwrap_or(u64::MAX),
        parts.next().unwrap_or(u64::MAX),
    )
}

fn record_legacy_compatibility_use(surface: AccessSurface) {
    let (bit, surface) = match surface {
        AccessSurface::Bootstrap => (1, "bootstrap"),
        AccessSurface::Declared => (2, "declared"),
        AccessSurface::Preview => (4, "preview"),
        AccessSurface::Protected => (8, "compatibility"),
    };
    if LEGACY_COMPATIBILITY_SURFACES.fetch_or(bit, Ordering::Relaxed) & bit == 0 {
        tracing::warn!(
            client_surface = surface,
            removal_release = LEGACY_LOOPBACK_COMPATIBILITY_REMOVAL_VERSION,
            "credentialless loopback request used temporary compatibility authority"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccessDenial {
    AuthenticationRequired,
    InvalidCredential,
    Forbidden,
}

impl AccessDenial {
    pub(crate) fn into_response(self) -> Response {
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
    assemble_daemon_access_boundary_with_declared(
        protected,
        DeclaredRouter::default(),
        DeclaredRouter::default(),
        bootstrap,
        state,
    )
}

/// Assemble compatibility, capability-declared, preview-token, and bootstrap
/// authority branches. Capability routes authenticate here and authorize in
/// their per-method policy layer. Preview routes intentionally bypass daemon
/// bearer authentication and authorize the short-lived resource token in their
/// declared method layer.
pub fn assemble_daemon_access_boundary_with_declared(
    protected: Router,
    declared: DeclaredRouter,
    preview: DeclaredRouter,
    bootstrap: Router,
    state: DaemonAccessState,
) -> Router {
    let protected = protected.layer(axum::middleware::from_fn_with_state(
        state.for_surface(AccessSurface::Protected),
        enforce_daemon_access,
    ));
    let declared = declared
        .into_router()
        .layer(axum::middleware::from_fn_with_state(
            state.for_surface(AccessSurface::Declared),
            enforce_daemon_access,
        ));
    let preview = preview
        .into_router()
        .layer(axum::middleware::from_fn_with_state(
            state.for_surface(AccessSurface::Preview),
            enforce_daemon_access,
        ));
    let bootstrap = bootstrap.layer(axum::middleware::from_fn_with_state(
        state.for_surface(AccessSurface::Bootstrap),
        enforce_daemon_access,
    ));
    protected.merge(declared).merge(preview).merge(bootstrap)
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

    // Preview URLs carry their own short-lived resource token. The declared
    // preview middleware validates it before the proxy handler runs, so this
    // branch must not require or forward a daemon bearer credential.
    if matches!(state.surface, AccessSurface::Preview) {
        return next.run(request).await;
    }

    let credential = bearer_credential(request.headers());
    let local_credential_id = match credential {
        BearerCredential::Valid(token) if trusted_local => state
            .local_credential
            .as_ref()
            .filter(|verifier| verifier.verify(token))
            .map(|verifier| verifier.credential_id_arc()),
        _ => None,
    };
    let record = match credential {
        BearerCredential::Missing | BearerCredential::Invalid => None,
        BearerCredential::Valid(_) if local_credential_id.is_some() => None,
        BearerCredential::Valid(token) => state
            .pairing
            .as_ref()
            .and_then(|pairing| pairing.resolve_bearer_record(token).ok().flatten()),
    };
    let mcp_policy_authenticated = trusted_local
        && record.is_none()
        && local_credential_id.is_none()
        && state.mcp_policy_token.is_some()
        && matches!(credential, BearerCredential::Valid(_))
        && medousa_mcp_gateway::verify_policy_bearer(
            request
                .headers()
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            state.mcp_policy_token.as_deref(),
        );

    // A malformed, ambiguous, or unknown credential must not fall back to
    // anonymous bootstrap authority.
    if !matches!(credential, BearerCredential::Missing)
        && record.is_none()
        && local_credential_id.is_none()
        && !mcp_policy_authenticated
    {
        return AccessDenial::InvalidCredential.into_response();
    }

    if matches!(state.surface, AccessSurface::Bootstrap) {
        let shared_mode = crate::shared_mode::is_shared_mode();
        let principal = local_credential_id
            .map(|credential_id| RequestPrincipal::local_app(credential_id, transport))
            .or_else(|| {
                record.map(|record| {
                    RequestPrincipal::from_pairing_record(record, transport, shared_mode)
                })
            })
            .or_else(|| {
                mcp_policy_authenticated.then(|| RequestPrincipal::mcp_policy_service(transport))
            })
            .unwrap_or_else(|| RequestPrincipal::anonymous(transport));
        request.extensions_mut().insert(principal);
        return next.run(request).await;
    }

    if record.is_none()
        && local_credential_id.is_none()
        && !mcp_policy_authenticated
        && (!trusted_local || !state.legacy_loopback_compatibility)
    {
        return AccessDenial::AuthenticationRequired.into_response();
    }

    let shared_mode = crate::shared_mode::is_shared_mode();
    let principal = match (local_credential_id, record) {
        (Some(credential_id), _) => RequestPrincipal::local_app(credential_id, transport),
        (None, Some(record)) => {
            RequestPrincipal::from_pairing_record(record, transport, shared_mode)
        }
        (None, None) if mcp_policy_authenticated => RequestPrincipal::mcp_policy_service(transport),
        (None, None)
            if trusted_local
                && state.legacy_loopback_compatibility
                && state.mcp_policy_token.is_none() =>
        {
            record_legacy_compatibility_use(state.surface);
            RequestPrincipal::legacy_local_with_mcp_policy()
        }
        (None, None) if trusted_local && state.legacy_loopback_compatibility => {
            record_legacy_compatibility_use(state.surface);
            RequestPrincipal::legacy_local()
        }
        (None, None) => RequestPrincipal::anonymous(transport),
    };
    if matches!(state.surface, AccessSurface::Declared) {
        request.extensions_mut().insert(principal);
        return next.run(request).await;
    }
    if principal.capabilities().contains(Capability::WorkshopRead) {
        request.extensions_mut().insert(principal);
        next.run(request).await
    } else if principal.kind() == crate::request_principal::PrincipalKind::Anonymous {
        AccessDenial::AuthenticationRequired.into_response()
    } else {
        AccessDenial::Forbidden.into_response()
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
    use axum::Extension;
    use axum::body::to_bytes;
    use axum::http::header::AUTHORIZATION;
    use axum::routing::{get, post};
    use tower::ServiceExt;

    use crate::daemon::route_policy::{
        BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
    };
    use crate::request_principal::Capability;

    fn protected_test_app() -> Router {
        let protected = Router::new().route("/v1/turns", get(|| async { StatusCode::NO_CONTENT }));
        let bootstrap = Router::new().route("/health", get(|| async { StatusCode::OK }));
        assemble_daemon_access_boundary(protected, bootstrap, DaemonAccessState::new(None))
    }

    fn authenticated_local_test_app(compatibility: bool) -> Router {
        let protected = Router::new().route(
            "/v1/turns",
            get(
                |Extension(principal): Extension<RequestPrincipal>| async move {
                    if principal.kind() == crate::request_principal::PrincipalKind::LocalApp
                        && principal.credential_id().map(|id| id.as_str()) == Some("home-id")
                    {
                        StatusCode::NO_CONTENT
                    } else {
                        StatusCode::IM_A_TEAPOT
                    }
                },
            ),
        );
        assemble_daemon_access_boundary(
            protected,
            Router::new(),
            DaemonAccessState::new(None)
                .with_local_credential(Arc::new(LocalCredentialVerifier::from_token(
                    "home-id",
                    "home-secret",
                )))
                .with_legacy_loopback_compatibility(compatibility),
        )
    }

    async fn authenticated_local_request(source: &str, bearer: Option<&str>) -> StatusCode {
        let mut request = Request::builder()
            .uri("/v1/turns")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            source.parse::<SocketAddr>().expect("valid source"),
        ));
        if let Some(bearer) = bearer {
            request.headers_mut().insert(
                AUTHORIZATION,
                format!("Bearer {bearer}").parse().expect("valid header"),
            );
        }
        authenticated_local_test_app(false)
            .oneshot(request)
            .await
            .expect("middleware response")
            .status()
    }

    fn mcp_policy_test_app(token: Option<&str>) -> Router {
        let declared = DeclaredRouter::default()
            .route(
                RoutePolicy {
                    method: axum::http::Method::POST,
                    path: "/v1/mcp/policy/evaluate",
                    group: RouteGroup::Administration,
                    required_capability: Some(Capability::McpPolicyEvaluate),
                    bootstrap_public: false,
                    browser_policy: BrowserPolicy::NativeOnly,
                    body_limit: 1024,
                    rate_limit_class: RateLimitClass::Administration,
                },
                post(|| async { StatusCode::NO_CONTENT }),
            )
            .route(
                RoutePolicy {
                    method: axum::http::Method::POST,
                    path: "/v1/runtime/admin",
                    group: RouteGroup::Administration,
                    required_capability: Some(Capability::AdminRuntime),
                    bootstrap_public: false,
                    browser_policy: BrowserPolicy::NativeOnly,
                    body_limit: 1024,
                    rate_limit_class: RateLimitClass::Administration,
                },
                post(|| async { StatusCode::NO_CONTENT }),
            );
        assemble_daemon_access_boundary_with_declared(
            Router::new(),
            declared,
            DeclaredRouter::default(),
            Router::new(),
            DaemonAccessState::new(None).with_mcp_policy_token(token.map(str::to_string)),
        )
    }

    async fn mcp_request(path: &str, source: &str, bearer: Option<&str>) -> StatusCode {
        let mut request = Request::builder()
            .method(axum::http::Method::POST)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            source.parse::<SocketAddr>().expect("valid source"),
        ));
        if let Some(bearer) = bearer {
            request.headers_mut().insert(
                AUTHORIZATION,
                format!("Bearer {bearer}").parse().expect("valid header"),
            );
        }
        mcp_policy_test_app(Some("policy-secret"))
            .oneshot(request)
            .await
            .expect("middleware response")
            .status()
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
    fn compatibility_window_has_not_reached_its_removal_release() {
        assert!(
            version_tuple(env!("CARGO_PKG_VERSION"))
                < version_tuple(LEGACY_LOOPBACK_COMPATIBILITY_REMOVAL_VERSION),
            "remove credentialless loopback compatibility before releasing {}",
            LEGACY_LOOPBACK_COMPATIBILITY_REMOVAL_VERSION
        );
        assert!(legacy_loopback_compatibility_enabled(
            "127.0.0.1:7419".parse().unwrap()
        ));
        assert!(!legacy_loopback_compatibility_enabled(
            "0.0.0.0:7419".parse().unwrap()
        ));
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
    async fn mcp_policy_credential_has_single_local_service_capability() {
        assert_eq!(
            mcp_request(
                "/v1/mcp/policy/evaluate",
                "127.0.0.1:43100",
                Some("policy-secret")
            )
            .await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            mcp_request(
                "/v1/runtime/admin",
                "127.0.0.1:43100",
                Some("policy-secret")
            )
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            mcp_request(
                "/v1/mcp/policy/evaluate",
                "192.0.2.10:43100",
                Some("policy-secret")
            )
            .await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn configured_mcp_policy_token_cannot_be_bypassed_on_loopback() {
        assert_eq!(
            mcp_request("/v1/mcp/policy/evaluate", "127.0.0.1:43100", None).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            mcp_request("/v1/mcp/policy/evaluate", "127.0.0.1:43100", Some("wrong")).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn unset_mcp_policy_token_retains_loopback_compatibility_only() {
        let mut request = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/v1/mcp/policy/evaluate")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:43100"
                .parse::<SocketAddr>()
                .expect("valid source"),
        ));
        let response = mcp_policy_test_app(None)
            .oneshot(request)
            .await
            .expect("middleware response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
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
    async fn named_local_credential_replaces_loopback_authority() {
        assert_eq!(
            authenticated_local_request("127.0.0.1:43100", Some("home-secret")).await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            authenticated_local_request("127.0.0.1:43100", None).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn named_local_credential_cannot_be_replayed_remotely() {
        assert_eq!(
            authenticated_local_request("192.0.2.10:43100", Some("home-secret")).await,
            StatusCode::UNAUTHORIZED
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
    async fn declared_branch_uses_method_policy_not_legacy_path_classification() {
        let declared = DeclaredRouter::default().route(
            RoutePolicy {
                method: axum::http::Method::GET,
                path: "/health",
                group: RouteGroup::Portal,
                required_capability: Some(Capability::WorkshopRead),
                bootstrap_public: false,
                browser_policy: BrowserPolicy::NativeOnly,
                body_limit: 1024,
                rate_limit_class: RateLimitClass::Read,
            },
            get(|| async { StatusCode::NO_CONTENT }),
        );
        let app = assemble_daemon_access_boundary_with_declared(
            Router::new(),
            declared,
            DeclaredRouter::default(),
            Router::new(),
            DaemonAccessState::new(None),
        );
        let mut request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:43100".parse::<SocketAddr>().unwrap(),
        ));
        assert_eq!(
            app.oneshot(request).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );
    }

    #[tokio::test]
    async fn preview_branch_accepts_only_a_live_resource_token() {
        let token = crate::daemon::forge_preview::mint_preview_grant(
            "work-preview-test",
            "run-preview-test",
            "http://127.0.0.1:3000/",
        )
        .await
        .expect("preview token");
        let preview = DeclaredRouter::default().route(
            RoutePolicy {
                method: axum::http::Method::GET,
                path: "/v1/forge/preview/{token}",
                group: RouteGroup::Preview,
                required_capability: None,
                bootstrap_public: false,
                browser_policy: BrowserPolicy::ExactOrigin,
                body_limit: 2 * 1024 * 1024,
                rate_limit_class: RateLimitClass::Read,
            },
            get(|| async { StatusCode::NO_CONTENT }),
        );
        let app = assemble_daemon_access_boundary_with_declared(
            Router::new(),
            DeclaredRouter::default(),
            preview,
            Router::new(),
            DaemonAccessState::new(None),
        );

        for (candidate, expected) in [
            (token.as_str(), StatusCode::NO_CONTENT),
            ("missing-preview-token", StatusCode::NOT_FOUND),
        ] {
            let mut request = Request::builder()
                .uri(format!("/v1/forge/preview/{candidate}"))
                .body(Body::empty())
                .unwrap();
            request.extensions_mut().insert(ConnectInfo(
                "192.0.2.10:43100".parse::<SocketAddr>().unwrap(),
            ));
            assert_eq!(
                app.clone().oneshot(request).await.unwrap().status(),
                expected
            );
        }
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
