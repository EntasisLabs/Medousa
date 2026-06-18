use std::time::Duration;

use reqwest::Client;
use serde::de::DeserializeOwned;

use crate::pairing_client::WorkshopTransportConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkshopRoute {
    Lan,
    Iroh,
}

pub async fn workshop_get(config: &WorkshopTransportConfig, path: &str) -> Result<(), String> {
    let route = pick_route(config).await;
    let headers = auth_headers(config);
    match route {
        WorkshopRoute::Lan => {
            let client = lan_client()?;
            let response = client
                .get(format!("{}{}", config.lan_base, normalize_path(path)))
                .headers(headers)
                .send()
                .await
                .map_err(|err| err.to_string())?;
            if response.status().is_success() {
                Ok(())
            } else {
                Err(format!("workshop returned HTTP {}", response.status()))
            }
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        WorkshopRoute::Iroh => iroh_get(config, path, &headers).await.map(|_| ()),
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        WorkshopRoute::Iroh => Err("iroh transport is only available on mobile".to_string()),
    }
}

pub async fn workshop_get_json<T: DeserializeOwned>(
    config: &WorkshopTransportConfig,
    path: &str,
) -> Result<T, String> {
    let route = pick_route(config).await;
    let headers = auth_headers(config);
    match route {
        WorkshopRoute::Lan => {
            let client = lan_client()?;
            let response = client
                .get(format!("{}{}", config.lan_base, normalize_path(path)))
                .headers(headers)
                .send()
                .await
                .map_err(|err| err.to_string())?;
            if !response.status().is_success() {
                return Err(format!("workshop returned HTTP {}", response.status()));
            }
            response.json::<T>().await.map_err(|err| err.to_string())
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        WorkshopRoute::Iroh => {
            let body = iroh_get(config, path, &headers).await?;
            serde_json::from_str(&body).map_err(|err| err.to_string())
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        WorkshopRoute::Iroh => Err("iroh transport is only available on mobile".to_string()),
    }
}

pub async fn workshop_post_json<T: DeserializeOwned, B: serde::Serialize>(
    config: &WorkshopTransportConfig,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let route = pick_route(config).await;
    let headers = auth_headers(config);
    let payload = serde_json::to_vec(body).map_err(|err| err.to_string())?;
    match route {
        WorkshopRoute::Lan => {
            let client = lan_client()?;
            let response = client
                .post(format!("{}{}", config.lan_base, normalize_path(path)))
                .headers(headers)
                .header("Content-Type", "application/json")
                .body(payload)
                .send()
                .await
                .map_err(|err| err.to_string())?;
            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                return Err(format!("workshop returned HTTP {status}: {text}"));
            }
            response.json::<T>().await.map_err(|err| err.to_string())
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        WorkshopRoute::Iroh => {
            let response_body =
                iroh_request(config, "POST", path, &headers, Some(&payload)).await?;
            serde_json::from_str(&response_body).map_err(|err| err.to_string())
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        WorkshopRoute::Iroh => Err("iroh transport is only available on mobile".to_string()),
    }
}

pub async fn workshop_get_bytes_stream(
    config: &WorkshopTransportConfig,
    path: &str,
) -> Result<WorkshopByteStream, String> {
    let route = pick_route(config).await;
    let headers = auth_headers(config);
    match route {
        WorkshopRoute::Lan => {
            let client = Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(600))
                .build()
                .map_err(|err| err.to_string())?;
            let response = client
                .get(format!("{}{}", config.lan_base, normalize_path(path)))
                .headers(headers)
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
            WorkshopByteStream::Lan(response) => {
                use futures_util::StreamExt;
                let chunk = response
                    .chunk()
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(chunk.map(|bytes| bytes.to_vec()))
            }
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

async fn pick_route(config: &WorkshopTransportConfig) -> WorkshopRoute {
    if config.iroh_ticket.is_none() {
        return WorkshopRoute::Lan;
    }
    if lan_reachable(config).await {
        WorkshopRoute::Lan
    } else {
        WorkshopRoute::Iroh
    }
}

async fn lan_reachable(config: &WorkshopTransportConfig) -> bool {
    let client = match lan_client() {
        Ok(client) => client,
        Err(_) => return false,
    };
    client
        .get(format!("{}/health", config.lan_base))
        .timeout(Duration::from_millis(1500))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

fn lan_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())
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

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn iroh_get(
    config: &WorkshopTransportConfig,
    path: &str,
    headers: &reqwest::header::HeaderMap,
) -> Result<String, String> {
    iroh_request(config, "GET", path, headers, None).await
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
    body: Option<&[u8]>,
) -> Result<String, String> {
    let ticket = config
        .iroh_ticket
        .as_deref()
        .ok_or_else(|| "missing iroh ticket".to_string())?;
    let header_pairs = iroh_header_refs(headers);
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
