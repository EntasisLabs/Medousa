//! Workshop transport: LAN HTTP with auth headers for Tauri workshop routing.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use medousa_sdk::{SdkError, Transport};

#[cfg(feature = "sse")]
use futures_util::{Stream, StreamExt, TryStreamExt};

#[derive(Debug, Clone, Default)]
pub struct WorkshopTransportConfig {
    pub lan_base_url: String,
    pub bearer_token: Option<String>,
    pub extra_headers: HashMap<String, String>,
}

#[derive(Clone)]
pub struct WorkshopTransport {
    client: reqwest::Client,
    config: WorkshopTransportConfig,
}

impl WorkshopTransport {
    pub fn new(config: WorkshopTransportConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }

    pub fn from_lan_base(lan_base: impl Into<String>) -> Self {
        Self::new(WorkshopTransportConfig {
            lan_base_url: lan_base.into().trim_end_matches('/').to_string(),
            bearer_token: None,
            extra_headers: HashMap::new(),
        })
    }

    fn url(&self, path: &str) -> String {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{}{}", self.config.lan_base_url, path)
    }

    fn apply_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut builder = builder;
        if let Some(token) = &self.config.bearer_token {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        for (key, value) in &self.config.extra_headers {
            builder = builder.header(key, value);
        }
        builder
    }

    async fn request_json(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, SdkError> {
        let url = self.url(path);
        let mut builder = self.apply_headers(self.client.request(method, url));
        if let Some(body) = body {
            builder = builder.json(&body);
        }
        let response = builder
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(SdkError::Http(format!("{status}: {text}")));
        }
        if text.trim().is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_str(&text).map_err(Into::into)
    }

    pub fn into_arc(self) -> Arc<dyn Transport> {
        Arc::new(self)
    }
}

impl Transport for WorkshopTransport {
    fn get_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move {
            self.request_json(reqwest::Method::GET, &path, None).await
        })
    }

    fn post_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move {
            self.request_json(reqwest::Method::POST, &path, Some(body))
                .await
        })
    }

    fn delete_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move {
            self.request_json(reqwest::Method::DELETE, &path, None)
                .await
        })
    }

    fn put_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move {
            self.request_json(reqwest::Method::PUT, &path, Some(body))
                .await
        })
    }

    fn patch_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move {
            self.request_json(reqwest::Method::PATCH, &path, Some(body))
                .await
        })
    }

    fn post_empty_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move {
            self.request_json(reqwest::Method::POST, &path, None)
                .await
        })
    }

    fn put_text<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: String,
        extra_headers: Vec<(&'static str, String)>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move {
            let url = self.url(&path);
            let mut builder = self
                .apply_headers(self.client.request(reqwest::Method::PUT, url))
                .header("Content-Type", "text/plain; charset=utf-8")
                .body(body);
            for (key, value) in extra_headers {
                builder = builder.header(key, value);
            }
            let response = builder
                .send()
                .await
                .map_err(|e| SdkError::Http(e.to_string()))?;
            let status = response.status();
            let text = response
                .text()
                .await
                .map_err(|e| SdkError::Http(e.to_string()))?;
            if !status.is_success() {
                return Err(SdkError::Http(format!("{status}: {text}")));
            }
            if text.trim().is_empty() {
                return Ok(serde_json::Value::Null);
            }
            serde_json::from_str(&text).map_err(Into::into)
        })
    }

    #[cfg(feature = "sse")]
    fn stream_sse<'a>(
        &'a self,
        _base_url: &'a str,
        path: String,
    ) -> Pin<Box<dyn Stream<Item = Result<bytes::Bytes, SdkError>> + Send + 'a>> {
        Box::pin(
            futures_util::stream::once(async move {
                let url = self.url(&path);
                let response = self
                    .apply_headers(
                        self.client
                            .get(&url)
                            .header("Accept", "text/event-stream"),
                    )
                    .send()
                    .await
                    .map_err(|e| SdkError::Http(e.to_string()))?;
                let status = response.status();
                if !status.is_success() {
                    let text = response.text().await.unwrap_or_default();
                    return Err(SdkError::Http(format!("{status}: {text}")));
                }
                Ok(response.bytes_stream().map(|r| {
                    r.map_err(|e| SdkError::Http(e.to_string()))
                }))
            })
            .try_flatten(),
        )
    }
}
