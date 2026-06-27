//! HTTP client for desktop Home BrowserHost (`127.0.0.1:7422`).

use medousa_browser_lite::{FetchResult, SearchResponse};

const DEFAULT_BROWSER_HOST_URL: &str = "http://127.0.0.1:7422";
const REQUEST_TIMEOUT_SECS: u64 = 8;

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

pub async fn browser_host_search(query: &str, max_results: usize) -> Result<SearchResponse, String> {
    post_json(
        "/v1/search",
        serde_json::json!({
            "query": query,
            "max_results": max_results,
        }),
    )
    .await
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

async fn post_json<T: serde::de::DeserializeOwned>(path: &str, body: serde_json::Value) -> Result<T, String> {
    let url = format!("{}{}", browser_host_base_url(), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
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
