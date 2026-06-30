//! Common transport interface shared by every comms adapter (LAN, Iroh,
//! HTTP-fallback).
//!
//! The engine never sees these types directly — it talks to the
//! [`crate::comms::service::CommsService`] over channels, and the service
//! drives whichever adapter the route selector picked. Modeling every wire as
//! the same [`Transport`] trait is what lets route selection live *inside* the
//! service instead of being branched at every call site.

use async_trait::async_trait;
use tokio::sync::mpsc;

/// Which physical transport an adapter speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportKind {
    /// Direct LAN HTTP to the daemon.
    Lan,
    /// HTTP/1.1 over QUIC via the Iroh gateway (NAT traversal).
    Iroh,
    /// Relay / public HTTP fallback when neither LAN nor Iroh is reachable.
    HttpFallback,
}

impl TransportKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransportKind::Lan => "lan",
            TransportKind::Iroh => "iroh",
            TransportKind::HttpFallback => "http-fallback",
        }
    }
}

/// HTTP verb, kept transport-agnostic so adapters that aren't reqwest (Iroh)
/// don't have to depend on `reqwest::Method`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
        }
    }
}

/// One request the engine wants delivered to the workshop, independent of route.
#[derive(Debug, Clone)]
pub struct TransportRequest {
    pub method: HttpMethod,
    /// Path (and query) relative to the route base, e.g. `/v1/interactive/turn`.
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    /// True for SSE / long-lived byte streams; selects streaming timeouts and a
    /// chunked [`ResponseBody::Stream`] instead of a buffered body.
    pub stream: bool,
}

impl TransportRequest {
    pub fn get(path: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            path: path.into(),
            headers: Vec::new(),
            body: None,
            stream: false,
        }
    }

    pub fn post_json(path: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            method: HttpMethod::Post,
            path: path.into(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: Some(body),
            stream: false,
        }
    }

    pub fn streaming(path: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            path: path.into(),
            headers: Vec::new(),
            body: None,
            stream: true,
        }
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

/// Response body: either fully buffered or a bounded chunk stream for SSE.
pub enum ResponseBody {
    Bytes(Vec<u8>),
    /// Bounded receiver of byte chunks. Errors mid-stream (idle timeout, reset)
    /// are surfaced as a [`TransportError`] item so the consumer can react.
    Stream(mpsc::Receiver<Result<Vec<u8>, TransportError>>),
}

impl std::fmt::Debug for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseBody::Bytes(bytes) => f.debug_tuple("Bytes").field(&bytes.len()).finish(),
            ResponseBody::Stream(_) => f.write_str("Stream(..)"),
        }
    }
}

impl ResponseBody {
    /// Collect a (possibly streamed) body into a single buffer.
    pub async fn collect(self) -> Result<Vec<u8>, TransportError> {
        match self {
            ResponseBody::Bytes(bytes) => Ok(bytes),
            ResponseBody::Stream(mut rx) => {
                let mut out = Vec::new();
                while let Some(chunk) = rx.recv().await {
                    out.extend_from_slice(&chunk?);
                }
                Ok(out)
            }
        }
    }
}

#[derive(Debug)]
pub struct TransportResponse {
    pub kind: TransportKind,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: ResponseBody,
}

impl TransportResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Why a transport request failed. The taxonomy is deliberately small so the
/// circuit breaker / route selector can reason about *retryability* without
/// string-matching error text the way the legacy `is_connect_error` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportErrorKind {
    /// Could not establish a connection (DNS, refused, unreachable).
    Connect,
    /// Connection established but timed out.
    Timeout,
    /// A non-2xx HTTP status was returned.
    Http(u16),
    /// Malformed response / protocol violation.
    Protocol,
    /// Adapter cannot serve this request (e.g. Iroh on a non-mobile build).
    Unsupported,
    /// Anything else.
    Other,
}

#[derive(Debug, Clone)]
pub struct TransportError {
    pub kind: TransportErrorKind,
    pub message: String,
}

impl TransportError {
    pub fn new(kind: TransportErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn connect(message: impl Into<String>) -> Self {
        Self::new(TransportErrorKind::Connect, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(TransportErrorKind::Timeout, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(TransportErrorKind::Unsupported, message)
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(TransportErrorKind::Protocol, message)
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::new(TransportErrorKind::Other, message)
    }

    /// True when retrying (possibly on a different route) is worthwhile.
    /// HTTP failures and `Unsupported` are *not* retryable on the same route —
    /// only connectivity-class failures are.
    pub fn is_retryable(&self) -> bool {
        matches!(self.kind, TransportErrorKind::Connect | TransportErrorKind::Timeout)
    }

    /// True when the failure indicates the *route itself* is down, so the
    /// selector should fail over to the next transport.
    pub fn is_connectivity(&self) -> bool {
        matches!(self.kind, TransportErrorKind::Connect | TransportErrorKind::Timeout)
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind_label(), self.message)
    }
}

impl TransportError {
    fn kind_label(&self) -> String {
        match self.kind {
            TransportErrorKind::Connect => "connect".to_string(),
            TransportErrorKind::Timeout => "timeout".to_string(),
            TransportErrorKind::Http(code) => format!("http {code}"),
            TransportErrorKind::Protocol => "protocol".to_string(),
            TransportErrorKind::Unsupported => "unsupported".to_string(),
            TransportErrorKind::Other => "other".to_string(),
        }
    }
}

impl std::error::Error for TransportError {}

/// A single wire. Every adapter (LAN/Iroh/HTTP-fallback) implements this; the
/// comms service owns a set of them and selects between them.
#[async_trait]
pub trait Transport: Send + Sync {
    fn kind(&self) -> TransportKind;

    /// Cheap reachability probe used by the route selector.
    async fn health(&self) -> bool;

    /// Execute one request. Adapters must honor `req.stream`: when set, return a
    /// [`ResponseBody::Stream`]; otherwise a buffered [`ResponseBody::Bytes`].
    async fn execute(&self, req: &TransportRequest) -> Result<TransportResponse, TransportError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryability_only_covers_connectivity() {
        assert!(TransportError::connect("x").is_retryable());
        assert!(TransportError::timeout("x").is_retryable());
        assert!(!TransportError::new(TransportErrorKind::Http(500), "x").is_retryable());
        assert!(!TransportError::unsupported("x").is_retryable());
        assert!(!TransportError::protocol("x").is_retryable());
    }

    #[tokio::test]
    async fn collect_drains_stream_body() {
        let (tx, rx) = mpsc::channel(4);
        tx.send(Ok(b"hello ".to_vec())).await.unwrap();
        tx.send(Ok(b"world".to_vec())).await.unwrap();
        drop(tx);
        let body = ResponseBody::Stream(rx).collect().await.unwrap();
        assert_eq!(body, b"hello world");
    }

    #[tokio::test]
    async fn collect_propagates_stream_error() {
        let (tx, rx) = mpsc::channel(4);
        tx.send(Ok(b"partial".to_vec())).await.unwrap();
        tx.send(Err(TransportError::timeout("idle"))).await.unwrap();
        drop(tx);
        let err = ResponseBody::Stream(rx).collect().await.unwrap_err();
        assert_eq!(err.kind, TransportErrorKind::Timeout);
    }
}
