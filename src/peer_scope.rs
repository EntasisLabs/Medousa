//! Reject Peer escalation and enforce Shared-mode portal settings ACL.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::header::WWW_AUTHENTICATE;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::credential_lifecycle::CredentialLifecycle;
use crate::daemon::route_policy::DeclaredRouter;
use crate::pairing::PairingService;
use crate::remote_trust::is_trusted_local;
use crate::request_principal::{Capability, RequestPrincipal, TransportClass};
use medousa_local_credential::LocalCredentialSet;

#[derive(Clone)]
pub struct DaemonAccessState {
    pairing: Option<Arc<PairingService>>,
    local_credentials: Option<Arc<LocalCredentialSet>>,
    mcp_policy_token: Option<Arc<str>>,
    surface: AccessSurface,
    credential_lifecycle: CredentialLifecycle,
}

impl DaemonAccessState {
    pub fn new(pairing: Option<Arc<PairingService>>) -> Self {
        let credential_lifecycle = pairing
            .as_ref()
            .map(|pairing| pairing.credential_lifecycle())
            .unwrap_or_default();
        Self {
            pairing,
            local_credentials: None,
            mcp_policy_token: None,
            surface: AccessSurface::Protected,
            credential_lifecycle,
        }
    }

    pub fn with_local_credentials(mut self, credentials: Arc<LocalCredentialSet>) -> Self {
        self.local_credentials = Some(credentials);
        self
    }

    pub fn with_credential_lifecycle(mut self, lifecycle: CredentialLifecycle) -> Self {
        self.credential_lifecycle = lifecycle;
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
            local_credentials: self.local_credentials.clone(),
            mcp_policy_token: self.mcp_policy_token.clone(),
            surface,
            credential_lifecycle: self.credential_lifecycle.clone(),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccessDenial {
    AuthenticationRequired,
    InvalidCredential,
    Forbidden,
}

impl AccessDenial {
    const fn code(self) -> &'static str {
        match self {
            Self::AuthenticationRequired => "authentication_required",
            Self::InvalidCredential => "invalid_credential",
            Self::Forbidden => "forbidden",
        }
    }

    pub(crate) fn into_response_with_request_id(self, request_id: impl Into<String>) -> Response {
        let envelope = medousa_types::ApiErrorEnvelope::new(
            self.code(),
            match self {
                Self::AuthenticationRequired => "authentication is required",
                Self::InvalidCredential => "the credential is invalid or expired",
                Self::Forbidden => "the credential cannot access this resource",
            },
            request_id,
        );
        let mut response = crate::daemon::http::envelope_response(
            match self {
                Self::AuthenticationRequired | Self::InvalidCredential => StatusCode::UNAUTHORIZED,
                Self::Forbidden => StatusCode::FORBIDDEN,
            },
            envelope,
        );
        if response.status() == StatusCode::UNAUTHORIZED {
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

/// Assemble protected, capability-declared, preview-token, and bootstrap
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
    let trusted_local = is_trusted_local(addr.ip(), request.headers());
    let transport = TransportClass::from_request(addr.ip(), request.headers());

    // Preview URLs carry their own short-lived resource token. The declared
    // preview middleware validates it before the proxy handler runs, so this
    // branch must not require or forward a daemon bearer credential.
    if matches!(state.surface, AccessSurface::Preview) {
        return next.run(request).await;
    }

    let credential = bearer_credential(request.headers());
    let local_credential = match credential {
        BearerCredential::Valid(token) if trusted_local => state
            .local_credentials
            .as_ref()
            .and_then(|credentials| credentials.resolve(token)),
        _ => None,
    };
    let record = match credential {
        BearerCredential::Missing | BearerCredential::Invalid => None,
        BearerCredential::Valid(_) if local_credential.is_some() => None,
        BearerCredential::Valid(token) => state
            .pairing
            .as_ref()
            .and_then(|pairing| pairing.resolve_bearer_record(token).ok().flatten()),
    };
    let signed_mesh_principal = if matches!(credential, BearerCredential::Missing) {
        match signed_mesh_principal(&state, request.headers(), transport) {
            Ok(principal) => principal,
            Err(()) => {
                return deny(
                    &state.credential_lifecycle,
                    AccessDenial::InvalidCredential,
                    request.headers(),
                );
            }
        }
    } else {
        None
    };
    let mcp_policy_authenticated = trusted_local
        && record.is_none()
        && signed_mesh_principal.is_none()
        && local_credential.is_none()
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
        && local_credential.is_none()
        && !mcp_policy_authenticated
    {
        return deny(
            &state.credential_lifecycle,
            AccessDenial::InvalidCredential,
            request.headers(),
        );
    }

    if matches!(state.surface, AccessSurface::Bootstrap) {
        let shared_mode = crate::shared_mode::is_shared_mode();
        let principal = local_credential
            .map(|(credential_id, generation)| {
                RequestPrincipal::local_app_with_generation(credential_id, transport, generation)
            })
            .or(signed_mesh_principal.clone())
            .or_else(|| {
                record.map(|record| {
                    RequestPrincipal::from_pairing_record(record, transport, shared_mode)
                })
            })
            .or_else(|| {
                mcp_policy_authenticated.then(|| RequestPrincipal::mcp_policy_service(transport))
            })
            .unwrap_or_else(|| RequestPrincipal::anonymous(transport));
        return run_with_principal(&state.credential_lifecycle, principal, request, next).await;
    }

    if record.is_none()
        && signed_mesh_principal.is_none()
        && local_credential.is_none()
        && !mcp_policy_authenticated
    {
        return deny(
            &state.credential_lifecycle,
            AccessDenial::AuthenticationRequired,
            request.headers(),
        );
    }

    let shared_mode = crate::shared_mode::is_shared_mode();
    let principal = match (local_credential, record, signed_mesh_principal) {
        (Some((credential_id, generation)), _, _) => {
            RequestPrincipal::local_app_with_generation(credential_id, transport, generation)
        }
        (None, Some(record), _) => {
            RequestPrincipal::from_pairing_record(record, transport, shared_mode)
        }
        (None, None, Some(principal)) => principal,
        (None, None, None) if mcp_policy_authenticated => {
            RequestPrincipal::mcp_policy_service(transport)
        }
        (None, None, None) => RequestPrincipal::anonymous(transport),
    };
    if matches!(state.surface, AccessSurface::Declared) {
        return run_with_principal(&state.credential_lifecycle, principal, request, next).await;
    }
    if principal.capabilities().contains(Capability::WorkshopRead) {
        run_with_principal(&state.credential_lifecycle, principal, request, next).await
    } else if principal.kind() == crate::request_principal::PrincipalKind::Anonymous {
        deny(
            &state.credential_lifecycle,
            AccessDenial::AuthenticationRequired,
            request.headers(),
        )
    } else {
        deny(
            &state.credential_lifecycle,
            AccessDenial::Forbidden,
            request.headers(),
        )
    }
}

fn signed_mesh_principal(
    state: &DaemonAccessState,
    headers: &axum::http::HeaderMap,
    transport: TransportClass,
) -> Result<Option<RequestPrincipal>, ()> {
    let Some(raw) = headers.get(crate::mesh::MESH_ENVELOPE_HEADER) else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| ())?;
    let envelope = crate::mesh::decode_envelope_header(raw).map_err(|_| ())?;
    let capability = crate::mesh::MeshCapability::parse(&envelope.capability).ok_or(())?;
    let pairing = state.pairing.as_ref().ok_or(())?;
    let record = pairing
        .find_by_phone_id(&envelope.sender_device_id)
        .map_err(|_| ())?
        .ok_or(())?;
    crate::mesh::envelope::verify_envelope(crate::mesh::envelope::VerifyEnvelopeParams {
        envelope: &envelope,
        payload_hash: &envelope.payload_hash,
        sender_public_key_b64: &record.phone_public_key,
        expected_sender_device_id: &record.phone_id,
        expected_recipient_device_id: pairing.device_id(),
        required_capability: capability,
        capability_granted: crate::mesh::record_has_capability(&record, capability.as_str()),
        now: chrono::Utc::now(),
    })
    .map_err(|_| ())?;
    Ok(Some(RequestPrincipal::from_signed_mesh_record(
        record, transport,
    )))
}

fn deny(
    lifecycle: &CredentialLifecycle,
    denial: AccessDenial,
    headers: &axum::http::HeaderMap,
) -> Response {
    lifecycle.record_denial(denial.code());
    denial.into_response_with_request_id(crate::daemon::http::request_id_from(headers))
}

async fn run_with_principal(
    lifecycle: &CredentialLifecycle,
    principal: RequestPrincipal,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let lease = lifecycle.lease(&principal);
    request.extensions_mut().insert(principal);
    if let Some(lease) = lease {
        request.extensions_mut().insert(lease.clone());
        lease.wrap_response(next.run(request).await)
    } else {
        next.run(request).await
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
    use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
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

    fn authenticated_local_test_app() -> Router {
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
            DaemonAccessState::new(None).with_local_credentials(Arc::new(LocalCredentialSet::new(
                [
                    medousa_local_credential::LocalCredentialVerifier::from_token(
                        "home-id",
                        "home-secret",
                    ),
                ],
            ))),
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
        authenticated_local_test_app()
            .oneshot(request)
            .await
            .expect("middleware response")
            .status()
    }

    #[tokio::test]
    async fn verifier_swap_invalidates_reused_service_without_restart() {
        let credentials = Arc::new(LocalCredentialSet::new([
            medousa_local_credential::LocalCredentialVerifier::from_token("local-id", "old"),
        ]));
        let lifecycle = CredentialLifecycle::default();
        let app = assemble_daemon_access_boundary(
            Router::new().route("/v1/turns", get(|| async { StatusCode::NO_CONTENT })),
            Router::new(),
            DaemonAccessState::new(None)
                .with_local_credentials(credentials.clone())
                .with_credential_lifecycle(lifecycle.clone()),
        );
        let request = |token: &'static str| {
            let mut request = Request::builder()
                .uri("/v1/turns")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            request.extensions_mut().insert(ConnectInfo(
                "127.0.0.1:40000".parse::<SocketAddr>().unwrap(),
            ));
            request
        };
        assert_eq!(
            app.clone().oneshot(request("old")).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );

        credentials.replace(
            medousa_local_credential::LocalCredentialVerifier::from_token_with_generation(
                "local-id", "new", 2,
            ),
        );
        lifecycle.revoke(
            "local-id",
            1,
            crate::credential_lifecycle::CredentialKind::LocalApp,
            "rotation",
        );
        assert_eq!(
            app.clone().oneshot(request("old")).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.oneshot(request("new")).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(lifecycle.snapshot().denials.invalid_credential, 1);
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
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            mcp_request("/v1/mcp/policy/evaluate", "127.0.0.1:43100", Some("wrong")).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn unset_mcp_policy_token_denies_credentialless_loopback() {
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
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn access_denials_are_stable_json_and_challenge_on_401() {
        let cases = [
            (
                AccessDenial::AuthenticationRequired,
                StatusCode::UNAUTHORIZED,
                "authentication_required",
                "authentication is required",
            ),
            (
                AccessDenial::InvalidCredential,
                StatusCode::UNAUTHORIZED,
                "invalid_credential",
                "the credential is invalid or expired",
            ),
            (
                AccessDenial::Forbidden,
                StatusCode::FORBIDDEN,
                "forbidden",
                "the credential cannot access this resource",
            ),
        ];

        for (denial, status, code, message) in cases {
            let response =
                denial.into_response_with_request_id(crate::daemon::http::UNASSIGNED_REQUEST_ID);
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
            let body = to_bytes(response.into_body(), 1024).await.unwrap();
            let envelope: medousa_types::ApiErrorEnvelope = serde_json::from_slice(&body).unwrap();
            assert_eq!(envelope.schema_version, 1);
            assert_eq!(envelope.code, code);
            assert_eq!(envelope.message, message);
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
    async fn loopback_requires_a_named_local_credential() {
        assert_eq!(
            request_from("/v1/turns", "127.0.0.1:43100", None).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn named_local_credential_grants_loopback_authority() {
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
    async fn declared_branch_uses_method_policy_not_path_classification() {
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
            DaemonAccessState::new(None).with_local_credentials(Arc::new(LocalCredentialSet::new(
                [
                    medousa_local_credential::LocalCredentialVerifier::from_token(
                        "home-id",
                        "home-secret",
                    ),
                ],
            ))),
        );
        let mut request = Request::builder()
            .uri("/health")
            .header(AUTHORIZATION, "Bearer home-secret")
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
