//! Private preview proxy for workshop-local development servers.
//!
//! Tokens gate reverse-proxy access to `127.0.0.1:{port}` on the workshop so
//! Home can open a Browser tab without binding the app on a public interface.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, head, options, patch, post, put};
use futures_util::StreamExt;
use tokio::sync::RwLock;

use super::route_policy::{BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy};
use super::state::AppState;

const PREVIEW_TTL: Duration = Duration::from_secs(2 * 60 * 60);

#[derive(Debug, Clone)]
struct PreviewGrant {
    work_id: String,
    run_id: String,
    port: u16,
    expires: Instant,
}

static PREVIEW_GRANTS: LazyLock<Arc<RwLock<HashMap<String, PreviewGrant>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Capture a http(s) loopback URL (or localhost:port) from readiness output.
pub fn extract_ready_url(text: &str) -> Option<String> {
    let url_re =
        regex::Regex::new(r"(?i)\b((?:https?://)?(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\])(?::\d+)?(?:/[^\s<>]*)?)")
            .ok()?;
    let raw = url_re
        .find(text)?
        .as_str()
        .trim_end_matches(['.', ',', ';', ')', ']', '"', '\'']);
    normalize_loopback_url(raw)
}

fn normalize_loopback_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("http://{trimmed}")
    };
    // http://host:port/path
    let without_scheme = with_scheme
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(with_scheme.as_str());
    let (authority, path) = match without_scheme.split_once('/') {
        Some((auth, path)) => (auth, format!("/{path}")),
        None => (without_scheme, "/".to_owned()),
    };
    let authority = authority.trim_start_matches('[').trim_end_matches(']');
    let (host, port) = if let Some((host, port)) = authority.rsplit_once(':') {
        let port = port.parse::<u16>().ok()?;
        (host.to_ascii_lowercase(), port)
    } else {
        (authority.to_ascii_lowercase(), 80)
    };
    let loopback = matches!(host.as_str(), "localhost" | "127.0.0.1" | "0.0.0.0" | "::1");
    if !loopback {
        return None;
    }
    Some(format!("http://127.0.0.1:{port}{path}"))
}

pub fn port_from_ready_url(url: &str) -> Option<u16> {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let authority = without_scheme.split('/').next().unwrap_or(without_scheme);
    authority
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse().ok())
}

pub async fn mint_preview_grant(work_id: &str, run_id: &str, ready_url: &str) -> Option<String> {
    let port = port_from_ready_url(ready_url)?;
    if port == 0 {
        return None;
    }
    let token = uuid::Uuid::new_v4().to_string();
    let mut grants = PREVIEW_GRANTS.write().await;
    grants.retain(|_, grant| grant.expires > Instant::now());
    // Replace any prior grant for this run.
    grants.retain(|_, grant| !(grant.work_id == work_id && grant.run_id == run_id));
    grants.insert(
        token.clone(),
        PreviewGrant {
            work_id: work_id.to_owned(),
            run_id: run_id.to_owned(),
            port,
            expires: Instant::now() + PREVIEW_TTL,
        },
    );
    Some(token)
}

pub async fn preview_token_for_run(work_id: &str, run_id: &str) -> Option<String> {
    let grants = PREVIEW_GRANTS.read().await;
    grants.iter().find_map(|(token, grant)| {
        (grant.work_id == work_id && grant.run_id == run_id && grant.expires > Instant::now())
            .then(|| token.clone())
    })
}

pub fn preview_path_for_token(token: &str) -> String {
    format!("/v1/forge/preview/{token}/")
}

pub fn forge_preview_surface() -> DeclaredRouter<AppState> {
    preview_methods("/v1/forge/preview/{token}", true)
        .merge(preview_methods("/v1/forge/preview/{token}/{*rest}", false))
}

fn preview_methods(path: &'static str, root: bool) -> DeclaredRouter<AppState> {
    if root {
        DeclaredRouter::default().methods([
            (
                preview_policy(axum::http::Method::GET, path),
                get(preview_proxy_root),
            ),
            (
                preview_policy(axum::http::Method::HEAD, path),
                head(preview_proxy_root),
            ),
            (
                preview_policy(axum::http::Method::OPTIONS, path),
                options(preview_proxy_root),
            ),
            (
                preview_policy(axum::http::Method::POST, path),
                post(preview_proxy_root),
            ),
            (
                preview_policy(axum::http::Method::PUT, path),
                put(preview_proxy_root),
            ),
            (
                preview_policy(axum::http::Method::PATCH, path),
                patch(preview_proxy_root),
            ),
            (
                preview_policy(axum::http::Method::DELETE, path),
                delete(preview_proxy_root),
            ),
        ])
    } else {
        DeclaredRouter::default().methods([
            (
                preview_policy(axum::http::Method::GET, path),
                get(preview_proxy),
            ),
            (
                preview_policy(axum::http::Method::HEAD, path),
                head(preview_proxy),
            ),
            (
                preview_policy(axum::http::Method::OPTIONS, path),
                options(preview_proxy),
            ),
            (
                preview_policy(axum::http::Method::POST, path),
                post(preview_proxy),
            ),
            (
                preview_policy(axum::http::Method::PUT, path),
                put(preview_proxy),
            ),
            (
                preview_policy(axum::http::Method::PATCH, path),
                patch(preview_proxy),
            ),
            (
                preview_policy(axum::http::Method::DELETE, path),
                delete(preview_proxy),
            ),
        ])
    }
}

fn preview_policy(method: axum::http::Method, path: &'static str) -> RoutePolicy {
    let rate_limit_class = if matches!(method, axum::http::Method::GET | axum::http::Method::HEAD) {
        RateLimitClass::Read
    } else {
        RateLimitClass::Mutation
    };
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Preview,
        required_capability: None,
        bootstrap_public: false,
        browser_policy: BrowserPolicy::ExactOrigin,
        body_limit: 2 * 1024 * 1024,
        rate_limit_class,
    }
}

pub(crate) async fn enforce_preview_grant(
    request: axum::http::Request<Body>,
    next: Next,
) -> Response {
    let Some(token) = preview_token_from_path(request.uri().path()) else {
        return preview_not_found();
    };
    match resolve_preview_grant(token).await {
        Ok(_) => next.run(request).await,
        Err(PreviewGrantError::Missing) => preview_not_found(),
        Err(PreviewGrantError::Expired) => preview_expired(),
    }
}

async fn preview_proxy_root(
    State(state): State<AppState>,
    Path(token): Path<String>,
    req: Request,
) -> Response {
    proxy_preview(state, token, String::new(), req).await
}

async fn preview_proxy(
    State(state): State<AppState>,
    Path((token, rest)): Path<(String, String)>,
    req: Request,
) -> Response {
    proxy_preview(state, token, rest, req).await
}

async fn proxy_preview(_state: AppState, token: String, rest: String, req: Request) -> Response {
    let grant = match resolve_preview_grant(&token).await {
        Ok(grant) => grant,
        Err(PreviewGrantError::Missing) => return preview_not_found(),
        Err(PreviewGrantError::Expired) => return preview_expired(),
    };

    let path = if rest.is_empty() {
        "/".to_owned()
    } else if rest.starts_with('/') {
        rest
    } else {
        format!("/{rest}")
    };
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let upstream = format!("http://127.0.0.1:{}{path}{query}", grant.port);

    let method = req.method().clone();
    let headers = filter_request_headers(req.headers(), grant.port);
    let body = match axum::body::to_bytes(req.into_body(), 2 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                "Preview request body is too large",
            )
                .into_response();
        }
    };

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let mut builder = client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
        &upstream,
    );
    for (name, value) in headers.iter() {
        if let Ok(value) = value.to_str() {
            builder = builder.header(name.as_str(), value);
        }
    }
    if !body.is_empty() {
        builder = builder.body(body);
    }

    let upstream_response = match builder.send().await {
        Ok(response) => response,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!(
                    "Could not reach workshop service on port {}: {err}",
                    grant.port
                ),
            )
                .into_response();
        }
    };

    let status = StatusCode::from_u16(upstream_response.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    let mut response = Response::builder().status(status);
    if let Some(headers) = response.headers_mut() {
        for (name, value) in upstream_response.headers().iter() {
            if skip_response_header(name) {
                continue;
            }
            if let (Ok(name), Ok(value)) = (
                HeaderName::from_bytes(name.as_str().as_bytes()),
                HeaderValue::from_bytes(value.as_bytes()),
            ) {
                headers.append(name, value);
            }
        }
    }
    let stream = upstream_response
        .bytes_stream()
        .map(|chunk| chunk.map_err(|err| std::io::Error::other(err.to_string())));
    response
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

#[derive(Clone, Copy)]
enum PreviewGrantError {
    Missing,
    Expired,
}

async fn resolve_preview_grant(token: &str) -> Result<PreviewGrant, PreviewGrantError> {
    let grant = PREVIEW_GRANTS.read().await.get(token).cloned();
    let Some(grant) = grant else {
        return Err(PreviewGrantError::Missing);
    };
    if grant.expires <= Instant::now() {
        PREVIEW_GRANTS.write().await.remove(token);
        return Err(PreviewGrantError::Expired);
    }
    Ok(grant)
}

fn preview_token_from_path(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/forge/preview/")?
        .split('/')
        .next()
        .filter(|token| !token.is_empty())
}

fn preview_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        "Preview link expired or was not found",
    )
        .into_response()
}

fn preview_expired() -> Response {
    (StatusCode::GONE, "Preview link expired").into_response()
}

fn filter_request_headers(headers: &HeaderMap, port: u16) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in headers.iter() {
        let key = name.as_str();
        if matches!(
            key,
            "host"
                | "authorization"
                | "connection"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "te"
                | "trailers"
                | "transfer-encoding"
                | "upgrade"
                | "content-length"
        ) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    if let Ok(host) = HeaderValue::from_str(&format!("127.0.0.1:{port}")) {
        out.insert(axum::http::header::HOST, host);
    }
    out
}

fn skip_response_header(name: &reqwest::header::HeaderName) -> bool {
    matches!(
        name.as_str(),
        "transfer-encoding" | "connection" | "keep-alive"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_vite_local_url() {
        let url = extract_ready_url("  ➜  Local:   http://localhost:5173/").unwrap();
        assert_eq!(url, "http://127.0.0.1:5173/");
    }

    #[test]
    fn extracts_bare_localhost_port() {
        let url = extract_ready_url("listening on localhost:3000").unwrap();
        assert_eq!(url, "http://127.0.0.1:3000/");
    }

    #[test]
    fn rejects_public_hosts() {
        assert!(extract_ready_url("https://example.com:443/app").is_none());
    }

    #[test]
    fn port_from_ready_url_reads_authority() {
        assert_eq!(port_from_ready_url("http://127.0.0.1:5173/app"), Some(5173));
    }

    #[test]
    fn proxy_does_not_forward_daemon_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer daemon-secret"),
        );
        headers.insert(
            axum::http::header::ACCEPT,
            HeaderValue::from_static("text/html"),
        );
        let filtered = filter_request_headers(&headers, 5173);
        assert!(!filtered.contains_key(axum::http::header::AUTHORIZATION));
        assert_eq!(
            filtered.get(axum::http::header::ACCEPT),
            Some(&HeaderValue::from_static("text/html"))
        );
    }
}
