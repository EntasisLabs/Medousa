//! Workshop transport: pooled LAN HTTP with optional Iroh fallback.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use medousa_sdk::{SdkError, Transport};

#[cfg(feature = "sse")]
use futures_util::{Stream, StreamExt, TryStreamExt};

use crate::iroh_hook::IrohHttpHook;
use crate::route::{is_connect_error, pick_route, invalidate_route_cache, WorkshopRoute};

#[derive(Debug, Clone, Default)]
pub struct WorkshopTransportConfig {
    pub lan_base_url: String,
    pub bearer_token: Option<String>,
    pub iroh_ticket: Option<String>,
    pub extra_headers: HashMap<String, String>,
}

impl WorkshopTransportConfig {
    pub fn from_workshop_parts(
        lan_base: impl Into<String>,
        session_token: Option<String>,
        iroh_ticket: Option<String>,
    ) -> Self {
        Self {
            lan_base_url: lan_base.into().trim_end_matches('/').to_string(),
            bearer_token: session_token,
            iroh_ticket,
            extra_headers: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct WorkshopTransport {
    config: WorkshopTransportConfig,
    iroh: Option<Arc<dyn IrohHttpHook>>,
}

impl WorkshopTransport {
    pub fn new(config: WorkshopTransportConfig) -> Self {
        Self {
            config,
            iroh: None,
        }
    }

    pub fn from_lan_base(lan_base: impl Into<String>) -> Self {
        Self::new(WorkshopTransportConfig {
            lan_base_url: lan_base.into().trim_end_matches('/').to_string(),
            bearer_token: None,
            iroh_ticket: None,
            extra_headers: HashMap::new(),
        })
    }

    pub fn with_iroh_hook(mut self, hook: Arc<dyn IrohHttpHook>) -> Self {
        self.iroh = Some(hook);
        self
    }

    pub fn config(&self) -> &WorkshopTransportConfig {
        &self.config
    }

    fn iroh_available(&self) -> bool {
        self.config.iroh_ticket.is_some() && self.iroh.is_some()
    }

    fn auth_header_pairs(&self) -> Vec<(&str, String)> {
        let mut out = Vec::new();
        if let Some(token) = &self.config.bearer_token {
            out.push(("Authorization", format!("Bearer {token}")));
        }
        for (key, value) in &self.config.extra_headers {
            out.push((key.as_str(), value.clone()));
        }
        out
    }

    fn url(&self, path: &str) -> String {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{}{}", self.config.lan_base_url, path)
    }

    async fn pick_workshop_route(&self) -> WorkshopRoute {
        pick_route(&self.config.lan_base_url, self.iroh_available()).await
    }

    async fn request_json(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, SdkError> {
        let route = self.pick_workshop_route().await;
        let headers: Vec<(String, String)> = self
            .auth_header_pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let header_refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let payload = body.map(|value| {
            serde_json::to_vec(&value).map_err(|e| SdkError::Serde(e.to_string()))
        });
        let payload = match payload {
            Some(Ok(bytes)) => Some(bytes),
            Some(Err(e)) => return Err(e),
            None => None,
        };

        let result = match route {
            WorkshopRoute::Lan => {
                self.lan_request_json(method, path, payload.as_deref()).await
            }
            WorkshopRoute::Iroh => {
                let hook = self
                    .iroh
                    .as_ref()
                    .ok_or_else(|| SdkError::Transport("iroh hook missing".to_string()))?;
                let bytes = hook
                    .request_json(method, path, &header_refs, payload.as_deref())
                    .await?;
                if bytes.is_empty() {
                    Ok(serde_json::Value::Null)
                } else {
                    serde_json::from_slice(&bytes).map_err(Into::into)
                }
            }
        };

        match result {
            Ok(value) => Ok(value),
            Err(err)
                if route == WorkshopRoute::Lan
                    && self.iroh_available()
                    && is_connect_error(&err.to_string()) =>
            {
                invalidate_route_cache();
                let hook = self
                    .iroh
                    .as_ref()
                    .ok_or_else(|| SdkError::Transport("iroh hook missing".to_string()))?;
                let bytes = hook
                    .request_json(method, path, &header_refs, payload.as_deref())
                    .await?;
                if bytes.is_empty() {
                    Ok(serde_json::Value::Null)
                } else {
                    serde_json::from_slice(&bytes).map_err(Into::into)
                }
            }
            Err(err) => Err(err),
        }
    }

    async fn lan_request_json(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<serde_json::Value, SdkError> {
        let client = crate::pool::standard_client();
        let url = self.url(path);
        let mut builder = match method {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "PATCH" => client.patch(url),
            "DELETE" => client.delete(url),
            other => {
                return Err(SdkError::Transport(format!(
                    "unsupported HTTP method {other}"
                )));
            }
        };
        for (key, value) in self.auth_header_pairs() {
            builder = builder.header(key, value);
        }
        if let Some(body) = body {
            builder = builder
                .header("Content-Type", "application/json")
                .body(body.to_vec());
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
        Box::pin(async move { self.request_json("GET", &path, None).await })
    }

    fn post_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.request_json("POST", &path, Some(body)).await })
    }

    fn delete_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.request_json("DELETE", &path, None).await })
    }

    fn put_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.request_json("PUT", &path, Some(body)).await })
    }

    fn patch_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.request_json("PATCH", &path, Some(body)).await })
    }

    fn post_empty_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let path = path.to_string();
        Box::pin(async move { self.request_json("POST", &path, None).await })
    }

    fn put_text<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: String,
        extra_headers: Vec<(&'static str, String)>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let transport = self.clone();
        let path = path.to_string();
        Box::pin(async move {
            let client = crate::pool::standard_client();
            let url = transport.url(&path);
            let mut builder = transport
                .apply_headers(client.request(reqwest::Method::PUT, url))
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
        let transport = self.clone();
        Box::pin(
            futures_util::stream::once(async move {
                let route = transport.pick_workshop_route().await;
                let headers: Vec<(String, String)> = transport
                    .auth_header_pairs()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect();
                let header_refs: Vec<(&str, &str)> = headers
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();

                match route {
                    WorkshopRoute::Lan => open_lan_sse(&transport, &path).await,
                    WorkshopRoute::Iroh => {
                        let hook = transport.iroh.as_ref().ok_or_else(|| {
                            SdkError::Transport("iroh hook missing".to_string())
                        })?;
                        Ok(hook.stream_sse(path.clone(), &header_refs))
                    }
                }
            })
            .try_flatten(),
        )
    }
}

impl WorkshopTransport {
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
}

#[cfg(feature = "sse")]
async fn open_lan_sse(
    transport: &WorkshopTransport,
    path: &str,
) -> Result<Pin<Box<dyn Stream<Item = Result<bytes::Bytes, SdkError>> + Send>>, SdkError> {
    let client = crate::pool::streaming_client();
    let url = transport.url(path);
    let response = transport
        .apply_headers(client.get(url).header("Accept", "text/event-stream"))
        .send()
        .await
        .map_err(|e| SdkError::Http(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(SdkError::Http(format!("{status}: {text}")));
    }
    Ok(Box::pin(
        response
            .bytes_stream()
            .map(|r| r.map_err(|e| SdkError::Http(e.to_string()))),
    ))
}
