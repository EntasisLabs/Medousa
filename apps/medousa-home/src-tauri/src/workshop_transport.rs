use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::pairing_client::WorkshopTransportConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkshopRoute {
    Lan,
    Iroh,
}

struct RouteCacheEntry {
    lan_base: String,
    route: WorkshopRoute,
    expires_at: Instant,
}

static ROUTE_CACHE: Mutex<Option<RouteCacheEntry>> = Mutex::new(None);
static ROUTE_PROBE: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));
static LAN_CLIENT: OnceLock<Client> = OnceLock::new();
static LAN_STREAM_CLIENT: OnceLock<Client> = OnceLock::new();

const LAN_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
const ROUTE_CACHE_LAN_TTL: Duration = Duration::from_secs(15);
const ROUTE_CACHE_IROH_TTL: Duration = Duration::from_secs(45);

#[derive(Debug, Clone)]
pub struct MultipartField {
    pub name: String,
    pub filename: Option<String>,
    pub mime: Option<String>,
    pub data: Vec<u8>,
}

pub fn invalidate_workshop_route_cache() {
    if let Ok(mut guard) = ROUTE_CACHE.lock() {
        *guard = None;
    }
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
    let route = pick_route(config).await;
    let headers = auth_headers(config);
    match route {
        WorkshopRoute::Lan => lan_get_stream(config, path, &headers).await,
        #[cfg(any(target_os = "ios", target_os = "android"))]
        WorkshopRoute::Iroh => {
            let body = iroh_open_stream(config, path, &headers).await?;
            Ok(WorkshopByteStream::Iroh(body))
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        WorkshopRoute::Iroh => Err("iroh transport is only available on mobile".to_string()),
    }
}

pub enum WorkshopByteStream {
    Lan(reqwest::Response),
    #[cfg(any(target_os = "ios", target_os = "android"))]
    Iroh(medousa::iroh_transport::IrohHttpBody),
}

impl WorkshopByteStream {
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, String> {
        match self {
            WorkshopByteStream::Lan(response) => response
                .chunk()
                .await
                .map_err(|err| err.to_string())
                .map(|chunk| chunk.map(|bytes| bytes.to_vec())),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            WorkshopByteStream::Iroh(body) => body
                .read_chunk()
                .await
                .map_err(|err| err.to_string()),
        }
    }
}

pub fn config_from_lan_base(lan_base: &str) -> WorkshopTransportConfig {
    crate::pairing_client::load_workshop_transport_config(lan_base).unwrap_or_else(|| {
        WorkshopTransportConfig {
            lan_base: lan_base.trim().trim_end_matches('/').to_string(),
            iroh_ticket: None,
            session_token: None,
        }
    })
}

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
    let route = pick_route(config).await;
    let headers = auth_headers(config);
    let result = match route {
        WorkshopRoute::Lan => {
            lan_request(config, method, path, &headers, &payload, is_stream).await
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        WorkshopRoute::Iroh => iroh_request(config, method, path, &headers, &payload).await,
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        WorkshopRoute::Iroh => Err("iroh transport is only available on mobile".to_string()),
    };

    match result {
        Ok(body) => Ok(body),
        Err(err) if route == WorkshopRoute::Lan && config.iroh_ticket.is_some() && is_connect_error(&err) => {
            invalidate_workshop_route_cache();
            write_route_cache(&config.lan_base, WorkshopRoute::Iroh);
            #[cfg(any(target_os = "ios", target_os = "android"))]
            {
                iroh_request(config, method, path, &headers, &payload).await
            }
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            {
                Err(err)
            }
        }
        Err(err) => Err(err),
    }
}

async fn pick_route(config: &WorkshopTransportConfig) -> WorkshopRoute {
    if config.iroh_ticket.is_none() {
        return WorkshopRoute::Lan;
    }
    if let Some(route) = read_route_cache(&config.lan_base) {
        return route;
    }

    let _guard = ROUTE_PROBE.lock().await;
    if let Some(route) = read_route_cache(&config.lan_base) {
        return route;
    }

    let route = if lan_reachable(config).await {
        WorkshopRoute::Lan
    } else {
        WorkshopRoute::Iroh
    };
    write_route_cache(&config.lan_base, route);
    route
}

fn read_route_cache(lan_base: &str) -> Option<WorkshopRoute> {
    let guard = ROUTE_CACHE.lock().ok()?;
    let entry = guard.as_ref()?;
    if entry.lan_base == lan_base && entry.expires_at > Instant::now() {
        Some(entry.route)
    } else {
        None
    }
}

fn write_route_cache(lan_base: &str, route: WorkshopRoute) {
    let ttl = match route {
        WorkshopRoute::Lan => ROUTE_CACHE_LAN_TTL,
        WorkshopRoute::Iroh => ROUTE_CACHE_IROH_TTL,
    };
    if let Ok(mut guard) = ROUTE_CACHE.lock() {
        *guard = Some(RouteCacheEntry {
            lan_base: lan_base.to_string(),
            route,
            expires_at: Instant::now() + ttl,
        });
    }
}

async fn lan_reachable(config: &WorkshopTransportConfig) -> bool {
    let client = match lan_client() {
        Ok(client) => client,
        Err(_) => return false,
    };
    client
        .get(format!("{}/health", config.lan_base))
        .timeout(LAN_PROBE_TIMEOUT)
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

fn lan_client() -> Result<&'static Client, String> {
    if let Some(client) = LAN_CLIENT.get() {
        return Ok(client);
    }
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(8)
        .build()
        .map_err(|err| err.to_string())?;
    let _ = LAN_CLIENT.set(client);
    Ok(LAN_CLIENT.get().expect("lan client initialized"))
}

fn lan_stream_client() -> Result<&'static Client, String> {
    if let Some(client) = LAN_STREAM_CLIENT.get() {
        return Ok(client);
    }
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(600))
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

async fn lan_request(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    headers: &reqwest::header::HeaderMap,
    payload: &RequestPayload,
    is_stream: bool,
) -> Result<String, String> {
    if is_stream {
        return Err("lan_request does not support streaming bodies".to_string());
    }
    let client = lan_client()?;
    let url = format!("{}{}", config.lan_base, normalize_path(path));
    let mut request = match method {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        other => return Err(format!("unsupported HTTP method {other}")),
    };
    request = request.headers(headers.clone());
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
            let mut req = request.header("Content-Type", content_type).body(bytes.clone());
            for (name, value) in extra_headers {
                req = req.header(name, value);
            }
            req
        }
    };

    let response = request.send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("workshop returned HTTP {status}: {body}"));
    }
    response.text().await.map_err(|err| err.to_string())
}

fn auth_headers(config: &WorkshopTransportConfig) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(token) = config.session_token.as_deref() {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
    }
    headers
}

fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn is_connect_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("connect")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("unreachable")
        || lower.contains("network")
        || lower.contains("dns")
        || lower.contains("connection refused")
        || lower.contains("failed to lookup")
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
                format!("Content-Disposition: form-data; name=\"{}\"\r\n", field.name).as_bytes(),
            );
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&field.data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (
        body,
        format!("multipart/form-data; boundary={boundary}"),
    )
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn iroh_open_stream(
    config: &WorkshopTransportConfig,
    path: &str,
    headers: &reqwest::header::HeaderMap,
) -> Result<medousa::iroh_transport::IrohHttpBody, String> {
    let ticket = config
        .iroh_ticket
        .as_deref()
        .ok_or_else(|| "missing iroh ticket".to_string())?;
    let header_pairs = iroh_header_refs(headers);
    let header_slice = iroh_header_slice(&header_pairs);
    let response = medousa::iroh_transport::iroh_http_request(
        ticket,
        "GET",
        path,
        &header_slice,
        None,
    )
    .await
    .map_err(|err| err.to_string())?;
    if !(200..300).contains(&response.status) {
        return Err(format!("workshop returned HTTP {} over iroh", response.status));
    }
    Ok(response.body)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn iroh_request(
    config: &WorkshopTransportConfig,
    method: &str,
    path: &str,
    headers: &reqwest::header::HeaderMap,
    payload: &RequestPayload,
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
    let mut response = medousa::iroh_transport::iroh_http_request(
        ticket,
        method,
        path,
        &header_slice,
        body,
    )
    .await
    .map_err(|err| err.to_string())?;
    if !(200..300).contains(&response.status) {
        return Err(format!("workshop returned HTTP {} over iroh", response.status));
    }
    let mut out = Vec::new();
    while let Some(chunk) = response.body.read_chunk().await.map_err(|err| err.to_string())? {
        out.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&out).to_string())
}

#[cfg(any(target_os = "ios", target_os = "android"))]
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

#[cfg(any(target_os = "ios", target_os = "android"))]
fn iroh_header_slice<'a>(pairs: &'a [(String, String)]) -> Vec<(&'a str, &'a str)> {
    pairs
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect()
}
