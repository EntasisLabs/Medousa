//! Socket-edge Host and browser Origin enforcement for the daemon.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, bail};
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::header::{ACCESS_CONTROL_ALLOW_ORIGIN, HOST, ORIGIN, VARY};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::daemon::route_policy::BrowserPolicy;

const DEFAULT_HOME_ORIGINS: [&str; 4] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "http://localhost:1420",
    "http://127.0.0.1:1420",
];

static INSTALLED_BOUNDARY: OnceLock<Arc<RequestBoundary>> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct RequestBoundary {
    listener_port: u16,
    allowed_hosts: Arc<HashSet<String>>,
    allowed_origins: Arc<HashSet<HeaderValue>>,
}

impl RequestBoundary {
    pub fn for_listener(listener: SocketAddr) -> Result<Self> {
        let mut allowed_hosts = HashSet::new();
        if !listener.ip().is_unspecified() {
            allowed_hosts.insert(listener.ip().to_string().to_ascii_lowercase());
        }
        allowed_hosts.extend([
            "localhost".to_string(),
            IpAddr::from([127, 0, 0, 1]).to_string(),
            IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]).to_string(),
        ]);
        if listener.ip().is_unspecified()
            && let Some(lan_ip) = crate::daemon_api::detect_lan_ipv4()
        {
            allowed_hosts.insert(lan_ip.to_string());
        }
        if let Ok(configured) = std::env::var("MEDOUSA_ALLOWED_HOSTS") {
            for host in configured
                .split(',')
                .map(str::trim)
                .filter(|host| !host.is_empty())
            {
                allowed_hosts.insert(validate_configured_host(host)?);
            }
        }

        let mut allowed_origins = HashSet::new();
        for origin in DEFAULT_HOME_ORIGINS {
            allowed_origins.insert(validate_origin(origin)?);
        }
        if let Ok(configured) = std::env::var("MEDOUSA_BROWSER_ORIGINS") {
            for origin in configured
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
            {
                allowed_origins.insert(validate_origin(origin)?);
            }
        }

        Ok(Self {
            listener_port: listener.port(),
            allowed_hosts: Arc::new(allowed_hosts),
            allowed_origins: Arc::new(allowed_origins),
        })
    }

    pub fn install(self) -> Arc<Self> {
        let boundary = Arc::new(self);
        let _ = INSTALLED_BOUNDARY.set(boundary);
        INSTALLED_BOUNDARY
            .get()
            .expect("request boundary was just installed")
            .clone()
    }

    pub fn allowed_origin_values(&self) -> Vec<HeaderValue> {
        self.allowed_origins.iter().cloned().collect()
    }

    fn permits_origin(&self, headers: &HeaderMap) -> Result<Option<HeaderValue>, StatusCode> {
        let mut origins = headers.get_all(ORIGIN).iter();
        let Some(origin) = origins.next() else {
            return Ok(None);
        };
        if origins.next().is_some() || !self.allowed_origins.contains(origin) {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(Some(origin.clone()))
    }

    fn permits_host(&self, source: SocketAddr, headers: &HeaderMap) -> bool {
        let mut hosts = headers.get_all(HOST).iter();
        let Some(host) = hosts.next().and_then(|host| host.to_str().ok()) else {
            return false;
        };
        if hosts.next().is_some() {
            return false;
        }
        let Ok(authority) = host.parse::<axum::http::uri::Authority>() else {
            return false;
        };
        let hostname = normalize_host(authority.host());
        if hostname == "medousa-workshop" {
            return source.ip().is_loopback() && crate::remote_trust::transport_is_iroh(headers);
        }
        authority.port_u16() == Some(self.listener_port) && self.allowed_hosts.contains(&hostname)
    }
}

pub async fn enforce_host(
    State(boundary): State<Arc<RequestBoundary>>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if boundary.permits_host(source, request.headers()) {
        next.run(request).await
    } else {
        (StatusCode::MISDIRECTED_REQUEST, "invalid request host").into_response()
    }
}

pub async fn enforce_compatibility_origin(
    State(boundary): State<Arc<RequestBoundary>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match boundary.permits_origin(request.headers()) {
        Ok(_) => next.run(request).await,
        Err(status) => status.into_response(),
    }
}

pub async fn enforce_declared_browser_policy(
    State(policy): State<BrowserPolicy>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.headers().get(ORIGIN).is_none() {
        return next.run(request).await;
    }
    if matches!(policy, BrowserPolicy::NativeOnly) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(boundary) = INSTALLED_BOUNDARY.get() else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let origin = match boundary.permits_origin(request.headers()) {
        Ok(Some(origin)) => origin,
        Ok(None) => return next.run(request).await,
        Err(status) => return status.into_response(),
    };
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    response
        .headers_mut()
        .append(VARY, HeaderValue::from_static("Origin"));
    response
}

fn validate_configured_host(raw: &str) -> Result<String> {
    if raw.contains('/') || raw.contains('@') {
        bail!("MEDOUSA_ALLOWED_HOSTS entries must be host names or IP literals");
    }
    let authority = raw
        .parse::<axum::http::uri::Authority>()
        .context("invalid MEDOUSA_ALLOWED_HOSTS entry")?;
    if authority.port().is_some() {
        bail!("MEDOUSA_ALLOWED_HOSTS entries must not include a port");
    }
    Ok(normalize_host(authority.host()))
}

fn normalize_host(host: &str) -> String {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn validate_origin(raw: &str) -> Result<HeaderValue> {
    if raw.eq_ignore_ascii_case("null") {
        bail!("null is not an allowed browser origin");
    }
    let url = reqwest::Url::parse(raw).context("invalid browser origin")?;
    if !matches!(url.scheme(), "http" | "https" | "tauri")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("browser origins must be exact scheme/host/port origins");
    }
    HeaderValue::from_str(raw).context("invalid browser origin header")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use axum::Extension;
    use axum::Router;
    use axum::routing::get;
    use tower::ServiceExt;
    use tower_http::cors::{AllowOrigin, CorsLayer};

    #[test]
    fn origin_configuration_rejects_wildcards_null_and_paths() {
        assert!(validate_origin("null").is_err());
        assert!(validate_origin("*").is_err());
        assert!(validate_origin("https://example.com/path").is_err());
        assert!(validate_origin("https://example.com").is_ok());
    }

    #[test]
    fn configured_hosts_are_names_only() {
        assert!(validate_configured_host("daemon.example.com").is_ok());
        assert!(validate_configured_host("daemon.example.com:7419").is_err());
        assert!(validate_configured_host("https://daemon.example.com").is_err());
    }

    #[test]
    fn host_boundary_rejects_rebinding_wrong_ports_and_remote_iroh_spoofing() {
        let boundary = RequestBoundary::for_listener("127.0.0.1:7419".parse().unwrap()).unwrap();
        let local = "127.0.0.1:40000".parse().unwrap();
        let remote = "192.168.1.8:40000".parse().unwrap();
        let mut headers = HeaderMap::new();

        headers.insert(HOST, HeaderValue::from_static("127.0.0.1:7419"));
        assert!(boundary.permits_host(local, &headers));
        headers.insert(HOST, HeaderValue::from_static("attacker.example:7419"));
        assert!(!boundary.permits_host(local, &headers));
        headers.insert(HOST, HeaderValue::from_static("127.0.0.1:7420"));
        assert!(!boundary.permits_host(local, &headers));

        headers.insert(HOST, HeaderValue::from_static("medousa-workshop"));
        headers.insert(
            crate::remote_trust::TRANSPORT_HEADER,
            HeaderValue::from_static(crate::remote_trust::TRANSPORT_IROH),
        );
        assert!(boundary.permits_host(local, &headers));
        assert!(!boundary.permits_host(remote, &headers));
    }

    #[test]
    fn exact_origin_boundary_rejects_attacker_and_duplicate_origins() {
        let boundary = RequestBoundary::for_listener("127.0.0.1:7419".parse().unwrap()).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, HeaderValue::from_static("http://localhost:1420"));
        assert!(boundary.permits_origin(&headers).is_ok());
        headers.insert(ORIGIN, HeaderValue::from_static("https://attacker.example"));
        assert_eq!(
            boundary.permits_origin(&headers),
            Err(StatusCode::FORBIDDEN)
        );
        headers.insert(ORIGIN, HeaderValue::from_static("http://localhost:1420"));
        headers.append(ORIGIN, HeaderValue::from_static("tauri://localhost"));
        assert_eq!(
            boundary.permits_origin(&headers),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[tokio::test]
    async fn host_middleware_rejects_rebinding_before_handler() {
        let boundary =
            Arc::new(RequestBoundary::for_listener("127.0.0.1:7419".parse().unwrap()).unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let handler_hits = hits.clone();
        let app = Router::new()
            .route(
                "/",
                get(move || {
                    let handler_hits = handler_hits.clone();
                    async move {
                        handler_hits.fetch_add(1, Ordering::Relaxed);
                        StatusCode::NO_CONTENT
                    }
                }),
            )
            .layer(axum::middleware::from_fn_with_state(boundary, enforce_host))
            .layer(Extension(ConnectInfo(
                "127.0.0.1:40000".parse::<SocketAddr>().unwrap(),
            )));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(HOST, "attacker.example:7419")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);
        assert_eq!(hits.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn attacker_preflight_gets_no_cors_authority_or_handler_work() {
        let boundary =
            Arc::new(RequestBoundary::for_listener("127.0.0.1:7419".parse().unwrap()).unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let handler_hits = hits.clone();
        let app = Router::new()
            .route(
                "/",
                get(move || {
                    let handler_hits = handler_hits.clone();
                    async move {
                        handler_hits.fetch_add(1, Ordering::Relaxed);
                        StatusCode::NO_CONTENT
                    }
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                boundary.clone(),
                enforce_compatibility_origin,
            ))
            .layer(
                CorsLayer::new()
                    .allow_origin(AllowOrigin::list(boundary.allowed_origin_values()))
                    .allow_methods([axum::http::Method::GET, axum::http::Method::OPTIONS])
                    .allow_headers([axum::http::header::AUTHORIZATION]),
            );
        let response = app
            .oneshot(
                Request::builder()
                    .method(axum::http::Method::OPTIONS)
                    .uri("/")
                    .header(ORIGIN, "https://attacker.example")
                    .header(axum::http::header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // tower-http answers preflight itself, but withholds every permission
        // header for an unapproved origin. The browser therefore denies it.
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
        assert_eq!(hits.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn declared_public_route_still_requires_an_exact_browser_origin() {
        RequestBoundary::for_listener("127.0.0.1:7419".parse().unwrap())
            .unwrap()
            .install();
        let app = Router::new()
            .route("/", get(|| async { StatusCode::NO_CONTENT }))
            .layer(axum::middleware::from_fn_with_state(
                BrowserPolicy::Public,
                enforce_declared_browser_policy,
            ));

        let denied = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(ORIGIN, "https://attacker.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);

        let allowed = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(ORIGIN, "http://localhost:1420")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            allowed.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("http://localhost:1420"))
        );
    }
}
