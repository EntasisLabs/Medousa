//! Workshop transport: LAN HTTP with auth headers for Tauri workshop routing.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use medousa_sdk::{SdkError, Transport};

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

    async fn request_json(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, SdkError> {
        let url = self.url(path);
        let mut builder = self.client.request(method, url);
        if let Some(token) = &self.config.bearer_token {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        for (key, value) in &self.config.extra_headers {
            builder = builder.header(key, value);
        }
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
}
