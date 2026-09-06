//! HTTP client for desktop Home BrowserHost (`127.0.0.1:7422`).

use medousa_browser_lite::{FetchResult, SearchResponse};
use medousa_browser_bridge::{BrowserObservation, BrowserScreenshotCapture};
use serde::Deserialize;

const DEFAULT_BROWSER_HOST_URL: &str = "http://127.0.0.1:7422";
const REQUEST_TIMEOUT_SECS: u64 = 8;
const BATCH_REQUEST_TIMEOUT_SECS: u64 = 16;
const MAX_SCREENSHOT_RESPONSE_BYTES: usize = 12 * 1024 * 1024;

pub fn browser_host_base_url() -> String {
    std::env::var("MEDOUSA_BROWSER_HOST_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_BROWSER_HOST_URL.to_string())
}

pub async fn browser_host_healthy() -> bool {
    let url = format!("{}/health", browser_host_base_url());
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get(url)
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub async fn browser_host_search(
    query: &str,
    max_results: usize,
) -> Result<SearchResponse, String> {
    post_json(
        "/v1/search",
        serde_json::json!({
            "query": query,
            "max_results": max_results,
        }),
    )
    .await
}

#[derive(Debug, Clone)]
pub struct BrowserHostWorldContext {
    pub driver_id: String,
    pub tab_group_id: String,
    pub tab_id: String,
    pub url: String,
    pub control: String,
}

#[derive(Debug, Deserialize)]
struct TabGroupResponse {
    ok: bool,
    #[serde(default)]
    tab_group: Option<TabGroupWire>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TabGroupWire {
    id: String,
    driver_id: String,
    control: String,
    tabs: Vec<TabWire>,
}

#[derive(Debug, Deserialize)]
struct TabWire {
    id: String,
    url: String,
    active: bool,
}

pub async fn browser_host_current_context() -> Result<BrowserHostWorldContext, String> {
    let response: TabGroupResponse = get_json("/v1/tab-groups/current").await?;
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| "BrowserHost has no current tab group".to_string()));
    }
    let group = response
        .tab_group
        .ok_or_else(|| "BrowserHost current response is missing its tab group".to_string())?;
    let tab = group
        .tabs
        .into_iter()
        .find(|tab| tab.active)
        .ok_or_else(|| "BrowserHost current tab group has no active tab".to_string())?;
    Ok(BrowserHostWorldContext {
        driver_id: group.driver_id,
        tab_group_id: group.id,
        tab_id: tab.id,
        url: tab.url,
        control: group.control,
    })
}

pub async fn browser_host_act(
    tab_group_id: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let encoded_group = urlencoding::encode(tab_group_id);
    let timeout_secs = if body.get("actions").is_some() {
        BATCH_REQUEST_TIMEOUT_SECS
    } else {
        REQUEST_TIMEOUT_SECS
    };
    post_json_with_timeout(
        &format!("/v1/tab-groups/{encoded_group}/act"),
        body,
        timeout_secs,
    )
    .await
}

pub async fn browser_host_observe(
    tab_group_id: &str,
    since_revision: Option<u64>,
    max_nodes: usize,
) -> Result<BrowserObservation, String> {
    let encoded_group = urlencoding::encode(tab_group_id);
    post_json(
        &format!("/v1/tab-groups/{encoded_group}/observe"),
        serde_json::json!({
            "since_revision": since_revision,
            "max_nodes": max_nodes,
        }),
    )
    .await
}

pub async fn browser_host_screenshot(
    tab_group_id: &str,
    body: serde_json::Value,
) -> Result<BrowserScreenshotCapture, String> {
    let encoded_group = urlencoding::encode(tab_group_id);
    let url = format!(
        "{}/v1/tab-groups/{encoded_group}/screenshot",
        browser_host_base_url()
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|err| err.to_string())?;
    let mut response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("browser_host unreachable: {err}"))?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_SCREENSHOT_RESPONSE_BYTES as u64)
    {
        return Err("browser_host screenshot response exceeds 12 MB".to_string());
    }
    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(0)
            .min(MAX_SCREENSHOT_RESPONSE_BYTES),
    );
    while let Some(chunk) = response.chunk().await.map_err(|err| err.to_string())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_SCREENSHOT_RESPONSE_BYTES {
            return Err("browser_host screenshot response exceeds 12 MB".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        let detail = String::from_utf8_lossy(&bytes)
            .trim()
            .chars()
            .take(512)
            .collect::<String>();
        return Err(if detail.is_empty() {
            format!("browser_host error: status {}", status.as_u16())
        } else {
            format!("browser_host error: status {}: {detail}", status.as_u16())
        });
    }
    serde_json::from_slice(&bytes).map_err(|err| err.to_string())
}

pub async fn browser_host_fetch(url: &str, max_chars: usize) -> Result<FetchResult, String> {
    post_json(
        "/v1/fetch",
        serde_json::json!({
            "url": url,
            "max_chars": max_chars,
        }),
    )
    .await
}

async fn post_json<T: serde::de::DeserializeOwned>(
    path: &str,
    body: serde_json::Value,
) -> Result<T, String> {
    post_json_with_timeout(path, body, REQUEST_TIMEOUT_SECS).await
}

async fn post_json_with_timeout<T: serde::de::DeserializeOwned>(
    path: &str,
    body: serde_json::Value,
    timeout_secs: u64,
) -> Result<T, String> {
    let url = format!("{}{}", browser_host_base_url(), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("browser_host unreachable: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "browser_host error: status {}",
            response.status().as_u16()
        ));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}

async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    let url = format!("{}{}", browser_host_base_url(), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("browser_host unreachable: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "browser_host error: status {}",
            response.status().as_u16()
        ));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}
