use std::sync::OnceLock;
use std::time::Duration;

use medousa_sdk_iroh::{WorkshopRoute, is_connect_error};
use reqwest::Client;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::pairing_client::WorkshopTransportConfig;

static LAN_CLIENT: OnceLock<Client> = OnceLock::new();
static LAN_STREAM_CLIENT: OnceLock<Client> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct MultipartField {
    pub name: String,
    pub filename: Option<String>,
    pub mime: Option<String>,
    pub data: Vec<u8>,
}

/// Flush the shared LAN/Iroh route cache.
///
/// SSE + multipart byte paths (this module) and JSON + `/health`
/// (`medousa-sdk-iroh`) now consult a single cache in `medousa-sdk-iroh`, so a
/// single invalidation covers every transport. Kept as a named function because
/// several app-level triggers call it directly.
pub fn invalidate_workshop_route_cache() {
    medousa_sdk_iroh::invalidate_route_cache();
}

/// Backwards-compatible alias for [`invalidate_workshop_route_cache`]. Both the
/// legacy byte transport and the SDK transport share one cache now, so this is a
/// single flush; app-level triggers call this on daemon URL change, workshop
/// switch, pairing complete, unpair, and foreground resume.
pub fn invalidate_all_route_caches() {
    invalidate_workshop_route_cache();
}

pub fn path_with_query(path: &str, query: &[(&str, String)]) -> String {
    if query.is_empty() {
        return normalize_path(path);
    }
    let mut out = normalize_path(path);
    out.push('?');
    out.push_str(
        &query
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    urlencoding::encode(key),
                    urlencoding::encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&"),
    );
    out
}

pub async fn workshop_get(config: &WorkshopTransportConfig, path: &str) -> Result<(), String> {
    workshop_request(config, "GET", path, RequestPayload::Empty, false)
        .await
        .map(|_| ())
}

pub async fn workshop_get_json<T: DeserializeOwned>(
    config: &WorkshopTransportConfig,
    path: &str,
) -> Result<T, String> {
    let body = workshop_request(config, "GET", path, RequestPayload::Empty, false).await?;
    serde_json::from_str(&body).map_err(|err| err.to_string())
}

pub async fn workshop_post_json<T: DeserializeOwned, B: serde::Serialize>(
    config: &WorkshopTransportConfig,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let payload = serde_json::to_vec(body).map_err(|err| err.to_string())?;
    let response =
        workshop_request(config, "POST", path, RequestPayload::Json(payload), false).await?;
    serde_json::from_str(&response).map_err(|err| err.to_string())
}

pub async fn workshop_post_empty_json<T: DeserializeOwned>(
    config: &WorkshopTransportConfig,
    path: &str,
) -> Result<T, String> {
    let response = workshop_request(config, "POST", path, RequestPayload::Empty, false).await?;
    serde_json::from_str(&response).map_err(|err| err.to_string())
}

pub async fn workshop_put_json<T: DeserializeOwned, B: serde::Serialize>(
    config: &WorkshopTransportConfig,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let payload = serde_json::to_vec(body).map_err(|err| err.to_string())?;
    let response =
        workshop_request(config, "PUT", path, RequestPayload::Json(payload), false).await?;
    serde_json::from_str(&response).map_err(|err| err.to_string())
}

pub async fn workshop_put_raw<T: DeserializeOwned>(
    config: &WorkshopTransportConfig,
    path: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> Result<T, String> {
    let response = workshop_request(
        config,
        "PUT",
        path,
        RequestPayload::Raw {
            content_type: content_type.to_string(),
            bytes: body.to_vec(),
            extra_headers: extra_headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
        },
        false,
    )
    .await?;
    serde_json::from_str(&response).map_err(|err| err.to_string())
}

pub async fn workshop_patch_json<T: DeserializeOwned, B: serde::Serialize>(
    config: &WorkshopTransportConfig,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let payload = serde_json::to_vec(body).map_err(|err| err.to_string())?;
    let response =
        workshop_request(config, "PATCH", path, RequestPayload::Json(payload), false).await?;
    serde_json::from_str(&response).map_err(|err| err.to_string())
}

pub async fn workshop_delete_json<T: DeserializeOwned>(
    config: &WorkshopTransportConfig,
    path: &str,
) -> Result<T, String> {
    let response = workshop_request(config, "DELETE", path, RequestPayload::Empty, false).await?;
    serde_json::from_str(&response).map_err(|err| err.to_string())
}

/// SDK adapters supply an already-resolved, immutable workshop config. This
/// path preserves conditional/idempotency headers and never resolves another
/// workshop or replays a mutation after an uncertain connection failure.
pub async fn workshop_body_request(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    body: Vec<u8>,
    content_type: &str,
    headers: Vec<(String, String)>,
) -> Result<serde_json::Value, String> {
    let payload = RequestPayload::Raw {
        bytes: body,
        content_type: content_type.into(),
        extra_headers: headers,
    };
    let response = workshop_request(config, method, path, payload, false).await?;
    if response.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&response).map_err(|e| e.to_string())
}

pub async fn workshop_json_request(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let payload = match body {
        Some(body) => {
            RequestPayload::Json(serde_json::to_vec(body).map_err(|err| err.to_string())?)
        }
        None => RequestPayload::Empty,
    };
    let response = workshop_request(config, method, path, payload, false).await?;
    if response.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&response).map_err(|err| err.to_string())
}

/// JSON request with a streaming response limit on LAN and Iroh, including errors.
/// Uses the same pinned config, credential refresh and no-mutation-replay policy.
pub async fn workshop_json_request_bounded(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
    max_bytes: usize,
) -> Result<serde_json::Value, String> {
    let payload = match body {
        Some(body) => RequestPayload::Json(serde_json::to_vec(body).map_err(|e| e.to_string())?),
        None => RequestPayload::Empty,
    };
    let response =
        workshop_request_limited(config, method, path, payload, false, Some(max_bytes)).await?;
    if response.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&response).map_err(|e| e.to_string())
}

pub async fn workshop_post_multipart<T: DeserializeOwned>(
    config: &WorkshopTransportConfig,
    path: &str,
    fields: &[MultipartField],
) -> Result<T, String> {
    let (body, content_type) = build_multipart_body(fields);
    let response = workshop_request(
        config,
        "POST",
        path,
        RequestPayload::Raw {
            content_type,
            bytes: body,
            extra_headers: Vec::new(),
        },
        false,
    )
    .await?;
    serde_json::from_str(&response).map_err(|err| err.to_string())
}

pub async fn workshop_get_bytes_stream(
    config: &WorkshopTransportConfig,
    path: &str,
) -> Result<WorkshopByteStream, String> {
    workshop_get_bytes_stream_with_accept(config, path, None).await
}

pub async fn workshop_get_bytes_stream_with_accept(
    config: &WorkshopTransportConfig,
    path: &str,
    accept: Option<&str>,
) -> Result<WorkshopByteStream, String> {
    tokio::time::timeout(
        Duration::from_secs(20),
        open_authenticated_stream(config, path, accept),
    )
    .await
    .map_err(|_| "Workshop stream opening timed out after 20 seconds.".to_string())?
}

async fn open_authenticated_stream(
    config: &WorkshopTransportConfig,
    path: &str,
    accept: Option<&str>,
) -> Result<WorkshopByteStream, String> {
    let effective = crate::pairing_client::ensure_fresh_session(config)
        .await
        .unwrap_or_else(|_| config.clone());
    let result = workshop_get_bytes_stream_once(&effective, path, accept).await;
    if result
        .as_ref()
        .is_err_and(|error| is_unauthorized_error(error))
        && !effective.pairing_id.trim().is_empty()
    {
        let refreshed =
            crate::pairing_client::refresh_session_after_unauthorized(&effective).await?;
        return workshop_get_bytes_stream_once(&refreshed, path, accept).await;
    }
    result
}

async fn workshop_get_bytes_stream_once(
    config: &WorkshopTransportConfig,
    path: &str,
    accept: Option<&str>,
) -> Result<WorkshopByteStream, String> {
    let route = pick_route(config).await;
    let mut headers = auth_headers(config);
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_str(accept.unwrap_or("text/event-stream"))
            .map_err(|err| err.to_string())?,
    );
    let result = match route {
        WorkshopRoute::Lan => lan_get_stream(config, path, &headers).await,
        WorkshopRoute::Iroh => iroh_open_stream(config, path, &headers)
            .await
            .map(WorkshopByteStream::Iroh),
    };
    match result {
        Err(error) if is_connect_error(&error) => {
            invalidate_workshop_route_cache();
            // GET stream opens may safely switch transport after a network handoff.
            if route == WorkshopRoute::Lan && config.iroh_ticket.is_some() {
                return iroh_open_stream(config, path, &headers)
                    .await
                    .map(WorkshopByteStream::Iroh);
            }
            if route == WorkshopRoute::Iroh && pick_route(config).await == WorkshopRoute::Lan {
                return lan_get_stream(config, path, &headers).await;
            }
            Err(error)
        }
        result => result,
    }
}

pub enum WorkshopByteStream {
    Lan(reqwest::Response),
    Iroh(medousa_iroh_http::IrohHttpBody),
}

impl WorkshopByteStream {
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, String> {
        match self {
            WorkshopByteStream::Lan(response) => response
                .chunk()
                .await
                .map_err(|err| err.to_string())
                .map(|chunk| chunk.map(|bytes| bytes.to_vec())),
            WorkshopByteStream::Iroh(body) => {
                body.read_chunk().await.map_err(|err| err.to_string())
            }
        }
    }
}

pub fn config_from_lan_base(lan_base: &str) -> WorkshopTransportConfig {
    let local_credential = local_credential_for_lan_base(lan_base);
    let mut config = crate::pairing_client::load_workshop_transport_config(lan_base)
        .unwrap_or_else(|| WorkshopTransportConfig {
            lan_base: lan_base.trim().trim_end_matches('/').to_string(),
            iroh_ticket: None,
            session_token: None,
            pairing_id: String::new(),
            session_expires_at: None,
            phone_id: String::new(),
            workshop_device_id: String::new(),
            daemon_public_key: None,
        });
    if local_credential.is_some() {
        config.session_token = local_credential;
    }
    config
}

fn local_credential_for_lan_base(lan_base: &str) -> Option<String> {
    let url = url::Url::parse(lan_base).ok()?;
    let loopback = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if !loopback || !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let data_dir = crate::workshop_registry::active_local_data_dir_for_url(lan_base)?;
    medousa_local_credential::load_home_local_secret(&data_dir)
        .ok()
        .map(|secret| secret.token().to_string())
}

#[derive(Clone)]
enum RequestPayload {
    Empty,
    Json(Vec<u8>),
    Raw {
        content_type: String,
        bytes: Vec<u8>,
        extra_headers: Vec<(String, String)>,
    },
}

async fn workshop_request(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    payload: RequestPayload,
    is_stream: bool,
) -> Result<String, String> {
    workshop_request_limited(config, method, path, payload, is_stream, None).await
}

async fn workshop_request_limited(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    payload: RequestPayload,
    is_stream: bool,
    max_bytes: Option<usize>,
) -> Result<String, String> {
    let effective = crate::pairing_client::ensure_fresh_session(config)
        .await
        .unwrap_or_else(|_| config.clone());
    let result =
        workshop_request_once_limited(&effective, method, path, &payload, is_stream, max_bytes)
            .await;
    if result
        .as_ref()
        .is_err_and(|error| is_unauthorized_error(error))
        && !effective.pairing_id.trim().is_empty()
    {
        let refreshed =
            crate::pairing_client::refresh_session_after_unauthorized(&effective).await?;
        return workshop_request_once_limited(
            &refreshed, method, path, &payload, is_stream, max_bytes,
        )
        .await;
    }
    result
}

async fn workshop_request_once(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    payload: &RequestPayload,
    is_stream: bool,
) -> Result<String, String> {
    workshop_request_once_limited(config, method, path, payload, is_stream, None).await
}

async fn workshop_request_once_limited(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    payload: &RequestPayload,
    is_stream: bool,
    max_bytes: Option<usize>,
) -> Result<String, String> {
    let route = pick_route(config).await;
    let headers = auth_headers(config);
    let result = match route {
        WorkshopRoute::Lan => {
            lan_request_limited(
                config, method, path, &headers, payload, is_stream, max_bytes,
            )
            .await
        }
        WorkshopRoute::Iroh => {
            iroh_request_limited(config, method, path, &headers, payload, max_bytes).await
        }
    };

    match result {
        Ok(body) => Ok(body),
        Err(err)
            if route == WorkshopRoute::Lan
                && config.iroh_ticket.is_some()
                && is_connect_error(&err) =>
        {
            // Never replay a potentially accepted mutation after a timeout.
            // The next request still re-probes the route.
            invalidate_workshop_route_cache();
            if method != "GET" {
                return Err(err);
            }
            iroh_request_limited(config, method, path, &headers, payload, max_bytes).await
        }
        Err(err) if route == WorkshopRoute::Iroh && is_connect_error(&err) => {
            invalidate_workshop_route_cache();
            if method == "GET" && pick_route(config).await == WorkshopRoute::Lan {
                return lan_request_limited(
                    config, method, path, &headers, payload, is_stream, max_bytes,
                )
                .await;
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
}

fn is_unauthorized_error(error: &str) -> bool {
    error.contains("HTTP 401") || error.contains("401 Unauthorized")
}

/// Select LAN vs Iroh via the shared `medousa-sdk-iroh` route cache so this
/// legacy byte transport and the SDK JSON transport can never diverge.
async fn pick_route(config: &WorkshopTransportConfig) -> WorkshopRoute {
    medousa_sdk_iroh::pick_route_with_bearer(
        &config.lan_base,
        config.iroh_ticket.is_some(),
        config.session_token.as_deref(),
    )
    .await
}

fn lan_client() -> Result<&'static Client, String> {
    if let Some(client) = LAN_CLIENT.get() {
        return Ok(client);
    }
    let client = build_lan_client()?;
    let _ = LAN_CLIENT.set(client);
    Ok(LAN_CLIENT.get().expect("lan client initialized"))
}

fn build_lan_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(8)
        .build()
        .map_err(|err| err.to_string())
}

fn lan_stream_client() -> Result<&'static Client, String> {
    if let Some(client) = LAN_STREAM_CLIENT.get() {
        return Ok(client);
    }
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        // Keep long-lived SSE streams alive indefinitely, but recover a stalled
        // connection when no body bytes arrive for this interval.
        .read_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(4)
        .build()
        .map_err(|err| err.to_string())?;
    let _ = LAN_STREAM_CLIENT.set(client);
    Ok(LAN_STREAM_CLIENT
        .get()
        .expect("lan stream client initialized"))
}

async fn lan_get_stream(
    config: &WorkshopTransportConfig,
    path: &str,
    headers: &reqwest::header::HeaderMap,
) -> Result<WorkshopByteStream, String> {
    let client = lan_stream_client()?;
    let response = client
        .get(format!("{}{}", config.lan_base, normalize_path(path)))
        .headers(headers.clone())
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("workshop returned HTTP {status}: {body}"));
    }
    Ok(WorkshopByteStream::Lan(response))
}

#[cfg(test)]
async fn lan_request(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    headers: &reqwest::header::HeaderMap,
    payload: &RequestPayload,
    is_stream: bool,
) -> Result<String, String> {
    lan_request_limited(config, method, path, headers, payload, is_stream, None).await
}

async fn lan_request_limited(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    headers: &reqwest::header::HeaderMap,
    payload: &RequestPayload,
    is_stream: bool,
    max_bytes: Option<usize>,
) -> Result<String, String> {
    if is_stream {
        return Err("lan_request does not support streaming bodies".to_string());
    }
    let client = lan_client()?;
    let url = format!("{}{}", config.lan_base, normalize_path(path));
    let attempts = lan_body_attempts(method);
    let mut last_body_error = None;

    for attempt in 0..attempts {
        let retry_client = (attempt > 0).then(build_lan_client).transpose()?;
        let request_client = retry_client.as_ref().unwrap_or(client);
        let mut request = match method {
            "GET" => request_client.get(&url),
            "POST" => request_client.post(&url),
            "PUT" => request_client.put(&url),
            "PATCH" => request_client.patch(&url),
            "DELETE" => request_client.delete(&url),
            other => return Err(format!("unsupported HTTP method {other}")),
        };
        request = request.headers(headers.clone());
        if attempt > 0 {
            // A successful response with an unreadable body generally means a
            // stale keep-alive connection was closed mid-frame. Do not reuse
            // that connection for the one safe, idempotent retry.
            request = request.header(reqwest::header::CONNECTION, "close");
        }
        request = match payload {
            RequestPayload::Empty => request,
            RequestPayload::Json(body) => request
                .header("Content-Type", "application/json")
                .body(body.clone()),
            RequestPayload::Raw {
                content_type,
                bytes,
                extra_headers,
            } => {
                let mut req = request
                    .header("Content-Type", content_type)
                    .body(bytes.clone());
                for (name, value) in extra_headers {
                    req = req.header(name, value);
                }
                req
            }
        };

        let response = request.send().await.map_err(|err| err.to_string())?;
        let status = response.status();
        let expected_length = response.content_length();
        let response_bytes = match read_lan_body(response, max_bytes).await {
            Ok(bytes) => bytes,
            Err(err) if attempt + 1 < attempts && !err.contains("exceeds size limit") => {
                last_body_error = Some(format_response_body_error(
                    method,
                    path,
                    status,
                    expected_length,
                    &err,
                ));
                continue;
            }
            Err(err) => {
                return Err(format_response_body_error(
                    method,
                    path,
                    status,
                    expected_length,
                    &err,
                ));
            }
        };
        let response_body = String::from_utf8(response_bytes.to_vec()).map_err(|err| {
            format!(
                "workshop returned a non-UTF-8 body for {method} {} (HTTP {status}): {err}",
                normalize_path(path),
            )
        })?;
        if !status.is_success() {
            return Err(format!("workshop returned HTTP {status}: {response_body}"));
        }
        return Ok(response_body);
    }

    Err(last_body_error.unwrap_or_else(|| {
        format!(
            "workshop response body was unavailable for {method} {}",
            normalize_path(path)
        )
    }))
}

fn lan_body_attempts(method: &str) -> usize {
    if method == "GET" { 2 } else { 1 }
}

fn format_response_body_error(
    method: &str,
    path: &str,
    status: reqwest::StatusCode,
    expected_length: Option<u64>,
    error: &impl std::fmt::Display,
) -> String {
    let length = expected_length
        .map(|value| format!(", expected {value} bytes"))
        .unwrap_or_default();
    format!(
        "could not read workshop response body for {method} {} (HTTP {status}{length}): {error}",
        normalize_path(path),
    )
}

pub fn auth_headers(config: &WorkshopTransportConfig) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(token) = config.session_token.as_deref() {
        if let Ok(mut value) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
            value.set_sensitive(true);
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
    }
    headers
}

pub fn protected_http_client(
    base_url: &str,
    connect_timeout: Duration,
    timeout: Duration,
) -> Result<Client, String> {
    let config = config_from_lan_base(base_url);
    if config.session_token.is_none() {
        return Err(format!(
            "no credential is available for protected Medousa Engine requests at {base_url}; start the local Engine once or pair this workshop"
        ));
    }
    Client::builder()
        .default_headers(auth_headers(&config))
        .connect_timeout(connect_timeout)
        .timeout(timeout)
        .build()
        .map_err(|error| error.to_string())
}

fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn diagnostic_path(path: &str) -> String {
    normalize_path(path)
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string()
}

fn build_multipart_body(fields: &[MultipartField]) -> (Vec<u8>, String) {
    let boundary = format!("medousa-{}", Uuid::new_v4());
    let mut body = Vec::new();
    for field in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        if let Some(filename) = &field.filename {
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                    field.name, filename
                )
                .as_bytes(),
            );
            if let Some(mime) = &field.mime {
                body.extend_from_slice(format!("Content-Type: {mime}\r\n").as_bytes());
            }
        } else {
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"\r\n",
                    field.name
                )
                .as_bytes(),
            );
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&field.data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (body, format!("multipart/form-data; boundary={boundary}"))
}

async fn iroh_open_stream(
    config: &WorkshopTransportConfig,
    path: &str,
    headers: &reqwest::header::HeaderMap,
) -> Result<medousa_iroh_http::IrohHttpBody, String> {
    let ticket = config
        .iroh_ticket
        .as_deref()
        .ok_or_else(|| "missing iroh ticket".to_string())?;
    let header_pairs = iroh_header_refs(headers);
    let header_slice = iroh_header_slice(&header_pairs);
    let response = medousa_iroh_http::iroh_http_request(ticket, "GET", path, &header_slice, None)
        .await
        .map_err(|err| err.to_string())?;
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "workshop returned HTTP {} over iroh for GET {}",
            response.status,
            diagnostic_path(path),
        ));
    }
    Ok(response.body)
}

async fn iroh_request_limited(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    headers: &reqwest::header::HeaderMap,
    payload: &RequestPayload,
    max_bytes: Option<usize>,
) -> Result<String, String> {
    let ticket = config
        .iroh_ticket
        .as_deref()
        .ok_or_else(|| "missing iroh ticket".to_string())?;
    let mut header_pairs = iroh_header_refs(headers);
    let (body, extra_content_type) = match payload {
        RequestPayload::Empty => (None, None),
        RequestPayload::Json(bytes) => (Some(bytes.as_slice()), Some("application/json")),
        RequestPayload::Raw {
            content_type,
            bytes,
            extra_headers,
        } => {
            for (name, value) in extra_headers {
                header_pairs.push((name.clone(), value.clone()));
            }
            (Some(bytes.as_slice()), Some(content_type.as_str()))
        }
    };
    if let Some(content_type) = extra_content_type {
        header_pairs.push(("Content-Type".to_string(), content_type.to_string()));
    }
    let header_slice = iroh_header_slice(&header_pairs);
    let mut response =
        medousa_iroh_http::iroh_http_request(ticket, method, path, &header_slice, body)
            .await
            .map_err(|err| err.to_string())?;
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "workshop returned HTTP {} over iroh for {method} {}",
            response.status,
            diagnostic_path(path),
        ));
    }
    let mut out = Vec::new();
    while let Some(chunk) = response
        .body
        .read_chunk()
        .await
        .map_err(|err| err.to_string())?
    {
        append_bounded(&mut out, &chunk, max_bytes)?;
    }
    Ok(String::from_utf8_lossy(&out).to_string())
}

fn append_bounded(out: &mut Vec<u8>, chunk: &[u8], max_bytes: Option<usize>) -> Result<(), String> {
    if max_bytes.is_some_and(|max| chunk.len() > max.saturating_sub(out.len())) {
        return Err("Workshop response exceeds size limit".into());
    }
    out.extend_from_slice(chunk);
    Ok(())
}

async fn read_lan_body(
    mut response: reqwest::Response,
    max_bytes: Option<usize>,
) -> Result<Vec<u8>, String> {
    if max_bytes.is_some_and(|max| {
        response
            .content_length()
            .is_some_and(|len| len > max as u64)
    }) {
        return Err("Workshop response exceeds size limit".into());
    }
    let mut out = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        append_bounded(&mut out, &chunk, max_bytes)?;
    }
    Ok(out)
}

fn iroh_header_refs(headers: &reqwest::header::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect()
}

fn iroh_header_slice(pairs: &[(String, String)]) -> Vec<(&str, &str)> {
    pairs
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::{RequestPayload, auth_headers, diagnostic_path, lan_body_attempts, lan_request};
    use crate::pairing_client::WorkshopTransportConfig;

    #[test]
    fn retries_only_idempotent_get_response_bodies() {
        assert_eq!(lan_body_attempts("GET"), 2);
        assert_eq!(lan_body_attempts("POST"), 1);
        assert_eq!(lan_body_attempts("PUT"), 1);
        assert_eq!(lan_body_attempts("PATCH"), 1);
        assert_eq!(lan_body_attempts("DELETE"), 1);
    }

    #[test]
    fn transport_diagnostics_strip_query_values() {
        assert_eq!(
            diagnostic_path("v1/sessions/search?q=private"),
            "/v1/sessions/search"
        );
    }

    #[test]
    fn native_transport_attaches_configured_bearer() {
        let config = WorkshopTransportConfig {
            lan_base: "http://127.0.0.1:7419".to_string(),
            iroh_ticket: None,
            session_token: Some("home-secret".to_string()),
            pairing_id: String::new(),
            session_expires_at: None,
            phone_id: String::new(),
            workshop_device_id: String::new(),
            daemon_public_key: None,
        };
        let headers = auth_headers(&config);
        assert_eq!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer home-secret")
        );
        assert!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .is_some_and(reqwest::header::HeaderValue::is_sensitive)
        );
    }

    #[tokio::test]
    async fn retries_a_truncated_get_body_on_a_fresh_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let server = std::thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept test request");
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request).expect("read test request");
                if attempt == 0 {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\nshort",
                        )
                        .expect("write truncated response");
                } else {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                        )
                        .expect("write complete response");
                }
            }
        });
        let config = WorkshopTransportConfig {
            lan_base: format!("http://{address}"),
            iroh_ticket: None,
            session_token: None,
            pairing_id: String::new(),
            session_expires_at: None,
            phone_id: String::new(),
            workshop_device_id: String::new(),
            daemon_public_key: None,
        };

        let body = lan_request(
            &config,
            "GET",
            "/source",
            &reqwest::header::HeaderMap::new(),
            &RequestPayload::Empty,
            false,
        )
        .await
        .expect("retry should recover the complete response");

        server.join().expect("test server completes");
        assert_eq!(body, "{\"ok\":true}");
    }
}

/// Explicit peer mutation: refresh credentials before dispatch, then make exactly
/// one attempt. Never replay a possibly accepted message after any failure.
pub async fn workshop_post_json_once<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    config: &WorkshopTransportConfig,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let effective = crate::pairing_client::ensure_fresh_session(config).await?;
    let payload = RequestPayload::Json(serde_json::to_vec(body).map_err(|e| e.to_string())?);
    let response = workshop_request_once(&effective, "POST", path, &payload, false).await?;
    serde_json::from_str(&response).map_err(|e| e.to_string())
}

#[cfg(test)]
mod mutation_tests {
    use super::*;
    use std::io::{Read, Write};
    #[tokio::test]
    async fn explicit_mutation_never_replays_an_accepted_truncated_response() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            let n = stream.read(&mut buf).unwrap();
            assert!(String::from_utf8_lossy(&buf[..n]).starts_with("POST "));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\nConnection: close\r\n\r\nshort",
                )
                .unwrap();
            drop(stream);
            listener.set_nonblocking(true).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
            assert!(listener.accept().is_err(), "POST was silently replayed");
        });
        let config = WorkshopTransportConfig {
            lan_base: format!("http://{address}"),
            iroh_ticket: None,
            session_token: Some("fixture".into()),
            pairing_id: String::new(),
            session_expires_at: None,
            phone_id: String::new(),
            workshop_device_id: String::new(),
            daemon_public_key: None,
        };
        assert!(
            workshop_post_json_once::<serde_json::Value, _>(
                &config,
                "/message",
                &serde_json::json!({"body":"hello"})
            )
            .await
            .is_err()
        );
        server.join().unwrap();
    }
}

#[cfg(test)]
mod bounded_tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn streamed_chunk_accumulator_accepts_exact_limit_and_rejects_before_extending() {
        // Shared by LAN and the bounded 64-KiB Iroh stream reader.
        let mut out = Vec::new();
        append_bounded(&mut out, b"{", Some(2)).unwrap();
        append_bounded(&mut out, b"}", Some(2)).unwrap();
        assert_eq!(out, b"{}");
        assert!(append_bounded(&mut out, b" ", Some(2)).is_err());
        assert_eq!(out, b"{}");
        assert!(append_bounded(&mut Vec::new(), b"x", Some(0)).is_err());
        assert!(append_bounded(&mut Vec::new(), b"", Some(0)).is_ok());
    }

    async fn bounded_lan_case(wire: &'static [u8], max: usize) -> Result<String, String> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            let n = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..n]).to_ascii_lowercase();
            assert!(request.starts_with("post /bounded "));
            assert!(request.contains("authorization: bearer test-token"));
            // Fragment the HTTP body so neither reader can rely on one buffer.
            for fragment in wire.chunks(3) {
                if stream.write_all(fragment).is_err() {
                    break;
                }
            }
        });
        let config = WorkshopTransportConfig {
            lan_base: base,
            iroh_ticket: None,
            session_token: Some("test-token".into()),
            pairing_id: String::new(),
            session_expires_at: None,
            phone_id: "phone".into(),
            workshop_device_id: "device".into(),
            daemon_public_key: Some("key".into()),
        };
        let result = lan_request_limited(
            &config,
            "POST",
            "/bounded",
            &auth_headers(&config),
            &RequestPayload::Json(b"{}".to_vec()),
            false,
            Some(max),
        )
        .await;
        server.join().unwrap();
        result
    }

    #[tokio::test]
    async fn lan_json_bound_covers_content_length_chunked_and_error_bodies() {
        assert_eq!(
            bounded_lan_case(
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                2
            )
            .await
            .unwrap(),
            "{}"
        );
        assert!(
            bounded_lan_case(
                b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\n{} ",
                2
            )
            .await
            .unwrap_err()
            .contains("exceeds size limit")
        );
        assert_eq!(bounded_lan_case(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n1\r\n{\r\n1\r\n}\r\n0\r\n\r\n", 2).await.unwrap(), "{}");
        assert!(bounded_lan_case(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n2\r\n{}\r\n1\r\n \r\n0\r\n\r\n", 2).await.unwrap_err().contains("exceeds size limit"));
        assert!(bounded_lan_case(b"HTTP/1.1 500 Error\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n0\r\n\r\n", 2).await.unwrap_err().contains("exceeds size limit"));
    }
}
