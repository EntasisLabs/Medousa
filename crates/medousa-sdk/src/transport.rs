use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::error::SdkError;

/// Object-safe HTTP transport for the Medousa daemon API.
pub trait Transport: Send + Sync {
    fn get_json<'a>(
        &'a self,
        base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>>;

    fn post_json<'a>(
        &'a self,
        base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>>;

    fn delete_json<'a>(
        &'a self,
        base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>>;

    fn put_json<'a>(
        &'a self,
        base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>>;
}

pub async fn decode<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, SdkError> {
    serde_json::from_value(value).map_err(Into::into)
}

#[derive(Clone, Default)]
pub struct HttpTransport {
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    fn url(base_url: &str, path: &str) -> String {
        let base = base_url.trim_end_matches('/');
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{base}{path}")
    }

    async fn request(
        client: reqwest::Client,
        method: reqwest::Method,
        url: String,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut builder = client.request(method, url);
        if let Some(body) = body {
            builder = builder.json(&body);
        }
        let response = builder.send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(SdkError::Http(format!("{status}: {text}")));
        }
        if text.trim().is_empty() {
            return Ok(serde_json::Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }
}

impl Transport for HttpTransport {
    fn get_json<'a>(
        &'a self,
        base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let url = Self::url(base_url, path);
        let client = self.client.clone();
        Box::pin(async move {
            Self::request(client, reqwest::Method::GET, url, None).await
        })
    }

    fn post_json<'a>(
        &'a self,
        base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let url = Self::url(base_url, path);
        let client = self.client.clone();
        Box::pin(async move {
            Self::request(client, reqwest::Method::POST, url, Some(body)).await
        })
    }

    fn delete_json<'a>(
        &'a self,
        base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let url = Self::url(base_url, path);
        let client = self.client.clone();
        Box::pin(async move {
            Self::request(client, reqwest::Method::DELETE, url, None).await
        })
    }

    fn put_json<'a>(
        &'a self,
        base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let url = Self::url(base_url, path);
        let client = self.client.clone();
        Box::pin(async move {
            Self::request(client, reqwest::Method::PUT, url, Some(body)).await
        })
    }
}

pub fn arc_transport<T: Transport + 'static>(transport: T) -> Arc<dyn Transport> {
    Arc::new(transport)
}
