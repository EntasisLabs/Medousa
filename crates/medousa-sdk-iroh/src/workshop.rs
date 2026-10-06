//! Workshop transport: pooled LAN HTTP with optional Iroh fallback.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use medousa_sdk::{SdkError, Transport};

#[cfg(feature = "sse")]
use futures_util::{Stream, StreamExt, TryStreamExt};

use crate::iroh_hook::IrohHttpHook;
use crate::route::{
    WorkshopRoute, invalidate_route_cache, is_connect_error, pick_route_with_bearer,
};

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
        Self { config, iroh: None }
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
        if path.starts_with("http://") || path.starts_with("https://") {
            return path.to_string();
        }
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{}{}", self.config.lan_base_url, path)
    }

    async fn pick_workshop_route(&self) -> WorkshopRoute {
        pick_route_with_bearer(
            &self.config.lan_base_url,
            self.iroh_available(),
            self.config.bearer_token.as_deref(),
        )
        .await
    }

    async fn request_json(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, SdkError> {
        let route = self.pick_workshop_route().await;

        let payload = body
            .map(|value| serde_json::to_vec(&value).map_err(|e| SdkError::Serde(e.to_string())));
        let payload = match payload {
            Some(Ok(bytes)) => Some(bytes),
            Some(Err(e)) => return Err(e),
            None => None,
        };
        let mut headers: Vec<(String, String)> = self
            .auth_header_pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        if payload.is_some() {
            headers.push(("Content-Type".to_string(), "application/json".to_string()));
        }
        let header_refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let result = match route {
            WorkshopRoute::Lan => {
                self.lan_request_json(method, path, payload.as_deref())
                    .await
            }
            WorkshopRoute::Iroh => {
                let hook = self
                    .iroh
                    .as_ref()
                    .ok_or_else(|| SdkError::Transport("iroh hook missing".to_string()))?;
                hook.request_json(method, path, &header_refs, payload.as_deref())
                    .await
                    .and_then(|bytes| parse_iroh_json_bytes(&bytes))
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
                // A timeout may arrive after the server accepted a mutation.
                // Re-pick for the next request, but only replay reads now.
                if method != "GET" {
                    return Err(err);
                }
                let hook = self
                    .iroh
                    .as_ref()
                    .ok_or_else(|| SdkError::Transport("iroh hook missing".to_string()))?;
                let bytes = hook
                    .request_json(method, path, &header_refs, payload.as_deref())
                    .await?;
                parse_iroh_json_bytes(&bytes)
            }
            Err(err) if route == WorkshopRoute::Iroh && is_connect_error(&err.to_string()) => {
                // A failed relay must not pin healthy LAN traffic to Iroh for
                // the remainder of its TTL. Only reads may cross transports
                // after a potentially transmitted request.
                invalidate_route_cache();
                if method == "GET" && self.pick_workshop_route().await == WorkshopRoute::Lan {
                    return self
                        .lan_request_json(method, path, payload.as_deref())
                        .await;
                }
                Err(err)
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

    fn post_json_with_headers<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
        headers: Vec<(&'static str, String)>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let mut transport = self.clone();
        for (key, value) in headers {
            transport
                .config
                .extra_headers
                .insert(key.to_string(), value);
        }
        let path = path.to_string();
        Box::pin(async move { transport.request_json("POST", &path, Some(body)).await })
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
        base_url: &'a str,
        path: String,
    ) -> Pin<Box<dyn Stream<Item = Result<bytes::Bytes, SdkError>> + Send + 'a>> {
        self.stream_sse_with_accept(base_url, path, "text/event-stream")
    }

    #[cfg(feature = "sse")]
    fn stream_sse_with_accept<'a>(
        &'a self,
        _base_url: &'a str,
        path: String,
        accept: &'static str,
    ) -> Pin<Box<dyn Stream<Item = Result<bytes::Bytes, SdkError>> + Send + 'a>> {
        let transport = self.clone();
        Box::pin(
            futures_util::stream::once(async move {
                let route = transport.pick_workshop_route().await;
                let mut headers: Vec<(String, String)> = transport
                    .auth_header_pairs()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect();
                headers.push(("Accept".to_string(), accept.to_string()));
                let header_refs: Vec<(&str, &str)> = headers
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();

                match route {
                    WorkshopRoute::Lan => open_lan_sse(&transport, &path, accept).await,
                    WorkshopRoute::Iroh => {
                        let hook = transport
                            .iroh
                            .as_ref()
                            .ok_or_else(|| SdkError::Transport("iroh hook missing".to_string()))?;
                        Ok(hook.stream_sse(stream_route_path(&path), &header_refs))
                    }
                }
            })
            .try_flatten(),
        )
    }
}

fn parse_iroh_json_bytes(bytes: &[u8]) -> Result<serde_json::Value, SdkError> {
    if bytes.is_empty() || bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_slice(bytes).map_err(Into::into)
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
    accept: &str,
) -> Result<Pin<Box<dyn Stream<Item = Result<bytes::Bytes, SdkError>> + Send>>, SdkError> {
    let client = crate::pool::streaming_client();
    let url = transport.url(path);
    let response = transport
        .apply_headers(client.get(url).header("Accept", accept))
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

#[cfg(feature = "sse")]
fn stream_route_path(path: &str) -> String {
    if let Ok(url) = reqwest::Url::parse(path) {
        let mut route = url.path().to_string();
        if let Some(query) = url.query() {
            route.push('?');
            route.push_str(query);
        }
        return route;
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::{parse_iroh_json_bytes, stream_route_path};

    struct UnavailableIroh(std::sync::atomic::AtomicUsize);

    impl crate::IrohHttpHook for UnavailableIroh {
        fn request_json<'a>(
            &'a self,
            _method: &'a str,
            _path: &'a str,
            _headers: &'a [(&'a str, &'a str)],
            _body: Option<&'a [u8]>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Vec<u8>, medousa_sdk::SdkError>>
                    + Send
                    + 'a,
            >,
        > {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async {
                Err(medousa_sdk::SdkError::Http(
                    "connect to workshop over iroh: timed out".into(),
                ))
            })
        }

        #[cfg(feature = "sse")]
        fn stream_sse(
            &self,
            _path: String,
            _headers: &[(&str, &str)],
        ) -> std::pin::Pin<
            Box<
                dyn futures_util::Stream<Item = Result<bytes::Bytes, medousa_sdk::SdkError>> + Send,
            >,
        > {
            Box::pin(futures_util::stream::empty())
        }
    }

    #[tokio::test]
    async fn stale_iroh_route_recovers_reads_over_lan_without_replaying_mutations() {
        use std::io::{Read, Write};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while requests.len() < 4 && std::time::Instant::now() < deadline {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                    .unwrap();
                let mut request = [0u8; 4096];
                let read = stream.read(&mut request).unwrap();
                requests.push(
                    String::from_utf8_lossy(&request[..read])
                        .lines()
                        .next()
                        .unwrap()
                        .to_string(),
                );
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").unwrap();
            }
            requests
        });
        let hook = Arc::new(UnavailableIroh(AtomicUsize::new(0)));
        let transport =
            super::WorkshopTransport::new(super::WorkshopTransportConfig::from_workshop_parts(
                base.clone(),
                None,
                Some("test-ticket".into()),
            ))
            .with_iroh_hook(hook.clone());
        crate::route::write_cache(&base, crate::WorkshopRoute::Iroh);
        assert_eq!(
            transport
                .request_json("GET", "/history", None)
                .await
                .unwrap()["ok"],
            true
        );
        crate::route::write_cache(&base, crate::WorkshopRoute::Iroh);
        assert!(
            transport
                .request_json("POST", "/mutate", None)
                .await
                .is_err()
        );
        assert_eq!(
            transport
                .request_json("GET", "/history", None)
                .await
                .unwrap()["ok"],
            true
        );
        assert_eq!(hook.0.load(Ordering::SeqCst), 2);
        assert_eq!(
            server.join().unwrap(),
            [
                "GET /health HTTP/1.1",
                "GET /history HTTP/1.1",
                "GET /health HTTP/1.1",
                "GET /history HTTP/1.1"
            ]
        );
        crate::invalidate_route_cache();
    }

    #[tokio::test]
    async fn lan_disconnect_after_accepting_a_mutation_does_not_replay_it_over_iroh() {
        use std::io::{Read, Write};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < deadline {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                    .unwrap();
                let mut bytes = [0; 4096];
                let read = stream.read(&mut bytes).unwrap();
                let line = String::from_utf8_lossy(&bytes[..read])
                    .lines()
                    .next()
                    .unwrap()
                    .to_string();
                if line.starts_with("POST") {
                    return line;
                } // Accepted, but no response.
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                    )
                    .unwrap();
            }
            panic!("mutation was never received");
        });
        let hook = Arc::new(UnavailableIroh(AtomicUsize::new(0)));
        let transport =
            super::WorkshopTransport::new(super::WorkshopTransportConfig::from_workshop_parts(
                base,
                None,
                Some("test-ticket".into()),
            ))
            .with_iroh_hook(hook.clone());
        assert!(
            transport
                .request_json("POST", "/connections", None)
                .await
                .is_err()
        );
        assert_eq!(hook.0.load(Ordering::SeqCst), 0);
        assert_eq!(server.join().unwrap(), "POST /connections HTTP/1.1");
    }

    #[test]
    fn parse_iroh_json_bytes_treats_whitespace_as_null() {
        assert!(parse_iroh_json_bytes(b"   ").unwrap().is_null());
    }

    #[test]
    fn parse_iroh_json_bytes_parses_object() {
        let value = parse_iroh_json_bytes(br#"{"turn_id":"t1"}"#).unwrap();
        assert_eq!(value["turn_id"], "t1");
    }

    #[cfg(feature = "sse")]
    #[test]
    fn absolute_stream_url_becomes_iroh_route_path() {
        assert_eq!(
            stream_route_path("https://workshop.example/v1/turn/t1/stream?since=7"),
            "/v1/turn/t1/stream?since=7"
        );
    }
}
