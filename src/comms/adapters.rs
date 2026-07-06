//! Transport adapters: LAN, HTTP-fallback (both reqwest over a pooled client)
//! and Iroh (feature-gated, HTTP/1.1 over QUIC).
//!
//! Adapters are intentionally dumb: they translate a [`TransportRequest`] onto
//! a wire and back. Route selection, pooling, backoff, and the circuit breaker
//! all live one level up in the service so this code stays small and uniform.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::mpsc;

use super::pool::ConnectionPool;
use super::transport::{
    HttpMethod, ResponseBody, Transport, TransportError, TransportErrorKind, TransportKind,
    TransportRequest, TransportResponse,
};

/// Bounded capacity for the per-stream chunk channel. Keeps SSE backpressure
/// finite instead of buffering an unbounded backlog of chunks in memory.
const STREAM_CHUNK_CAPACITY: usize = 64;

fn map_reqwest_error(err: &reqwest::Error) -> TransportError {
    if err.is_timeout() {
        TransportError::timeout(err.to_string())
    } else if err.is_connect() {
        TransportError::connect(err.to_string())
    } else if let Some(status) = err.status() {
        TransportError::new(TransportErrorKind::Http(status.as_u16()), err.to_string())
    } else {
        TransportError::other(err.to_string())
    }
}

fn reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
    }
}

fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// reqwest-backed adapter used for both [`TransportKind::Lan`] and
/// [`TransportKind::HttpFallback`] (they differ only by base URL + preference).
pub struct HttpAdapter {
    kind: TransportKind,
    base: String,
    pool: Arc<ConnectionPool>,
}

impl HttpAdapter {
    pub fn new(kind: TransportKind, base: impl Into<String>, pool: Arc<ConnectionPool>) -> Self {
        let base = base.into().trim_end_matches('/').to_string();
        Self { kind, base, pool }
    }

    pub fn lan(base: impl Into<String>, pool: Arc<ConnectionPool>) -> Self {
        Self::new(TransportKind::Lan, base, pool)
    }

    pub fn fallback(base: impl Into<String>, pool: Arc<ConnectionPool>) -> Self {
        Self::new(TransportKind::HttpFallback, base, pool)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, normalize_path(path))
    }
}

#[async_trait]
impl Transport for HttpAdapter {
    fn kind(&self) -> TransportKind {
        self.kind
    }

    async fn health(&self) -> bool {
        let url = self.url("/health");
        self.pool
            .standard()
            .get(url)
            .timeout(self.pool.config().connect_timeout)
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    async fn execute(&self, req: &TransportRequest) -> Result<TransportResponse, TransportError> {
        let client = if req.stream {
            self.pool.streaming()
        } else {
            self.pool.standard()
        };
        let mut builder = client.request(reqwest_method(req.method), self.url(&req.path));
        for (name, value) in &req.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        if let Some(body) = &req.body {
            builder = builder.body(body.clone());
        }

        let response = builder.send().await.map_err(|err| map_reqwest_error(&err))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect();

        if req.stream {
            let idle = self.pool.config().stream_idle_timeout;
            let (tx, rx) = mpsc::channel::<Result<Vec<u8>, TransportError>>(STREAM_CHUNK_CAPACITY);
            let mut stream = response.bytes_stream();
            tokio::spawn(async move {
                loop {
                    match tokio::time::timeout(idle, stream.next()).await {
                        Ok(Some(Ok(chunk))) => {
                            if tx.send(Ok(chunk.to_vec())).await.is_err() {
                                break; // consumer dropped
                            }
                        }
                        Ok(Some(Err(err))) => {
                            let _ = tx.send(Err(map_reqwest_error(&err))).await;
                            break;
                        }
                        Ok(None) => break, // stream ended cleanly
                        Err(_) => {
                            let _ = tx
                                .send(Err(TransportError::timeout("SSE idle timeout")))
                                .await;
                            break;
                        }
                    }
                }
            });
            Ok(TransportResponse {
                kind: self.kind,
                status,
                headers,
                body: ResponseBody::Stream(rx),
            })
        } else {
            let bytes = response.bytes().await.map_err(|err| map_reqwest_error(&err))?;
            Ok(TransportResponse {
                kind: self.kind,
                status,
                headers,
                body: ResponseBody::Bytes(bytes.to_vec()),
            })
        }
    }
}

/// Iroh adapter — HTTP/1.1 over QUIC to the workshop gateway. Generalized
/// beyond mobile pairing: any client with a ticket (desktop included) can use
/// it as a first-class route, not just a phone-pairing special case.
#[cfg(feature = "iroh-transport")]
pub struct IrohAdapter {
    ticket: String,
}

#[cfg(feature = "iroh-transport")]
impl IrohAdapter {
    pub fn new(ticket: impl Into<String>) -> Self {
        Self {
            ticket: ticket.into(),
        }
    }
}

#[cfg(feature = "iroh-transport")]
#[async_trait]
impl Transport for IrohAdapter {
    fn kind(&self) -> TransportKind {
        TransportKind::Iroh
    }

    async fn health(&self) -> bool {
        crate::iroh_transport::iroh_http_get_text(&self.ticket, "/health")
            .await
            .is_ok()
    }

    async fn execute(&self, req: &TransportRequest) -> Result<TransportResponse, TransportError> {
        let header_refs: Vec<(&str, &str)> = req
            .headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let mut response = crate::iroh_transport::iroh_http_request(
            &self.ticket,
            req.method.as_str(),
            &req.path,
            &header_refs,
            req.body.as_deref(),
        )
        .await
        .map_err(|err| TransportError::connect(err.to_string()))?;

        let headers = response.headers.clone();
        if req.stream {
            let (tx, rx) = mpsc::channel::<Result<Vec<u8>, TransportError>>(STREAM_CHUNK_CAPACITY);
            tokio::spawn(async move {
                loop {
                    match response.body.read_chunk().await {
                        Ok(Some(chunk)) => {
                            if tx.send(Ok(chunk)).await.is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Err(TransportError::protocol(err.to_string()))).await;
                            break;
                        }
                    }
                }
            });
            Ok(TransportResponse {
                kind: TransportKind::Iroh,
                status: response.status,
                headers,
                body: ResponseBody::Stream(rx),
            })
        } else {
            let mut out = Vec::new();
            while let Some(chunk) = response
                .body
                .read_chunk()
                .await
                .map_err(|err| TransportError::protocol(err.to_string()))?
            {
                out.extend_from_slice(&chunk);
            }
            Ok(TransportResponse {
                kind: TransportKind::Iroh,
                status: response.status,
                headers,
                body: ResponseBody::Bytes(out),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_adapter_normalizes_base_and_path() {
        let pool = Arc::new(ConnectionPool::new(Default::default()).expect("pool"));
        let adapter = HttpAdapter::lan("http://127.0.0.1:7419/", pool);
        assert_eq!(adapter.url("/health"), "http://127.0.0.1:7419/health");
        assert_eq!(adapter.url("v1/turn"), "http://127.0.0.1:7419/v1/turn");
        assert_eq!(adapter.kind(), TransportKind::Lan);
    }
}
