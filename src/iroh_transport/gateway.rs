use std::sync::Arc;

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use httparse::{Request, Status, EMPTY_HEADER};
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, SecretKey, endpoint::presets};
use iroh_tickets::endpoint::EndpointTicket;
use tokio::io::AsyncWriteExt;

use super::ALPN;

const MAX_REQUEST_BYTES: usize = 256 * 1024;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// Snapshot of a live workshop Iroh endpoint for QR v2 and `/pair/iroh-ticket`.
#[derive(Debug, Clone)]
pub struct IrohWorkshopInfo {
    pub ticket: String,
    pub endpoint_id: String,
}

/// Keeps the Iroh router alive for the process lifetime.
pub struct WorkshopGateway {
    router: Router,
    info: IrohWorkshopInfo,
}

impl WorkshopGateway {
    pub fn info(&self) -> &IrohWorkshopInfo {
        &self.info
    }

    pub async fn shutdown(self) -> Result<()> {
        self.router.shutdown().await.context("shutdown iroh router")
    }
}

#[derive(Debug, Clone)]
struct HttpProxy {
    upstream: Arc<String>,
}

impl ProtocolHandler for HttpProxy {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        loop {
            let (mut send, mut recv) = connection
                .accept_bi()
                .await
                .map_err(AcceptError::from_err)?;
            let upstream = Arc::clone(&self.upstream);
            tokio::spawn(async move {
                if let Err(err) = proxy_stream(&upstream, &mut send, &mut recv).await {
                    eprintln!("medousa-iroh: proxy stream failed: {err:#}");
                }
            });
        }
    }
}

/// Bind an ephemeral Iroh endpoint + HTTP proxy router.
pub async fn spawn_workshop_gateway(upstream: &str) -> Result<WorkshopGateway> {
    spawn_workshop_gateway_with_secret(upstream, SecretKey::generate()).await
}

/// Bind a stable Iroh endpoint derived from the pairing identity seed.
pub async fn spawn_workshop_gateway_with_secret(
    upstream: &str,
    secret_key: SecretKey,
) -> Result<WorkshopGateway> {
    let upstream = normalize_upstream(upstream)?;
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .bind()
        .await
        .context("bind iroh endpoint")?;
    endpoint.online().await;
    let ticket = EndpointTicket::new(endpoint.addr());
    let endpoint_id = endpoint.addr().id.to_string();
    let info = IrohWorkshopInfo {
        ticket: ticket.to_string(),
        endpoint_id,
    };
    let proxy = HttpProxy {
        upstream: Arc::new(upstream),
    };
    let router = Router::builder(endpoint)
        .accept(ALPN, proxy)
        .spawn();
    Ok(WorkshopGateway { router, info })
}

pub fn workshop_ticket_from_router(router: &Router) -> Result<IrohWorkshopInfo> {
    let ticket = EndpointTicket::new(router.endpoint().addr());
    Ok(IrohWorkshopInfo {
        ticket: ticket.to_string(),
        endpoint_id: router.endpoint().addr().id.to_string(),
    })
}

async fn proxy_stream(
    upstream: &str,
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<()> {
    let raw = read_http_request(recv).await?;
    forward_request_stream(upstream, &raw, send).await
}

async fn read_http_request(recv: &mut iroh::endpoint::RecvStream) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if buf.len() >= MAX_REQUEST_BYTES {
            bail!("HTTP request exceeds {MAX_REQUEST_BYTES} bytes");
        }
        if request_complete(&buf)? {
            return Ok(buf);
        }
        let read = recv
            .read(&mut chunk)
            .await
            .context("read HTTP request bytes")?;
        let Some(read) = read else {
            break;
        };
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
    }
    if buf.is_empty() {
        bail!("empty HTTP request");
    }
    if request_complete(&buf)? {
        Ok(buf)
    } else {
        bail!("truncated HTTP body");
    }
}

fn request_complete(raw: &[u8]) -> Result<bool> {
    let mut headers = [EMPTY_HEADER; 32];
    let mut request = Request::new(&mut headers);
    match request.parse(raw).context("parse HTTP request")? {
        Status::Complete(header_end) => {
            let content_length = request
                .headers
                .iter()
                .find(|header| header.name.eq_ignore_ascii_case("Content-Length"))
                .and_then(|header| std::str::from_utf8(header.value).ok())
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            Ok(raw.len() >= header_end + content_length)
        }
        Status::Partial => Ok(false),
    }
}

async fn forward_request_stream(
    upstream: &str,
    raw: &[u8],
    send: &mut iroh::endpoint::SendStream,
) -> Result<()> {
    let (method, path, header_end) = parse_request_line(raw)?;
    if method != "GET" && method != "HEAD" && method != "POST" && method != "PUT" && method != "DELETE"
    {
        bail!("unsupported HTTP method {method}");
    }

    let headers = parse_headers(raw, header_end)?;
    let content_length = header_value(&headers, "content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body = if content_length > 0 {
        if raw.len() < header_end + content_length {
            bail!("truncated HTTP body");
        }
        &raw[header_end..header_end + content_length]
    } else {
        &[]
    };

    let url = format!("{upstream}{path}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .context("build upstream HTTP client")?;
    let mut builder = client.request(method.parse().context("invalid method")?, &url);
    for (name, value) in &headers {
        if name.eq_ignore_ascii_case("host")
            || name.eq_ignore_ascii_case("connection")
            || name.eq_ignore_ascii_case("content-length")
        {
            continue;
        }
        builder = builder.header(name.as_str(), value.as_str());
    }
    if !body.is_empty() {
        builder = builder.body(body.to_vec());
    }

    let response = builder.send().await.context("upstream HTTP request")?;
    let status = response.status();
    let response_headers = response.headers().clone();
    let is_event_stream = response_headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/event-stream"));

    let mut out = format!(
        "HTTP/1.1 {} {}\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    );
    for (name, value) in response_headers.iter() {
        if name == reqwest::header::CONNECTION || name == reqwest::header::TRANSFER_ENCODING {
            continue;
        }
        let Ok(value) = value.to_str() else {
            continue;
        };
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    if is_event_stream {
        out.push_str("\r\n");
        send.write_all(out.as_bytes())
            .await
            .context("write HTTP response headers")?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("read upstream SSE chunk")?;
            send.write_all(&chunk).await.context("write SSE chunk")?;
        }
    } else {
        let response_body = response
            .bytes()
            .await
            .context("read upstream response body")?;
        if response_body.len() > MAX_RESPONSE_BYTES {
            bail!("upstream response exceeds {MAX_RESPONSE_BYTES} bytes");
        }
        if !response_headers.contains_key(reqwest::header::CONTENT_LENGTH) {
            out.push_str(&format!("Content-Length: {}\r\n", response_body.len()));
        }
        out.push_str("\r\n");
        send.write_all(out.as_bytes())
            .await
            .context("write HTTP response headers")?;
        send.write_all(&response_body)
            .await
            .context("write HTTP response body")?;
    }

    send.finish().context("finish send stream")?;
    Ok(())
}

fn parse_request_line(raw: &[u8]) -> Result<(String, String, usize)> {
    let mut headers = [EMPTY_HEADER; 32];
    let mut request = Request::new(&mut headers);
    let status = request
        .parse(raw)
        .context("parse HTTP request")?;
    let header_end = match status {
        Status::Complete(offset) => offset,
        Status::Partial => bail!("incomplete HTTP request"),
    };
    let method = request
        .method
        .context("missing HTTP method")?
        .to_string();
    let path = request.path.context("missing HTTP path")?.to_string();
    Ok((method, path, header_end))
}

fn parse_headers(raw: &[u8], header_end: usize) -> Result<Vec<(String, String)>> {
    let header_bytes = &raw[..header_end];
    let mut headers = [EMPTY_HEADER; 32];
    let mut request = Request::new(&mut headers);
    let status = request
        .parse(header_bytes)
        .context("parse HTTP headers")?;
    if !matches!(status, Status::Complete(_)) {
        bail!("incomplete HTTP headers");
    }
    Ok(request
        .headers
        .iter()
        .map(|header| {
            (
                header.name.to_string(),
                String::from_utf8_lossy(header.value).to_string(),
            )
        })
        .collect())
}

fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

fn normalize_upstream(raw: &str) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        bail!("upstream URL is required");
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        bail!("upstream must be http:// or https://");
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_complete_waits_for_post_body() {
        let headers = b"POST /v1/interactive/turn HTTP/1.1\r\nHost: medousa-workshop\r\nContent-Length: 11\r\nConnection: close\r\n\r\n";
        assert!(!request_complete(headers).expect("headers only"));
        let mut full = headers.to_vec();
        full.extend_from_slice(b"{\"msg\":\"hi\"}");
        assert!(request_complete(&full).expect("headers + body"));
    }

    #[test]
    fn request_complete_for_get_without_body() {
        let get = b"GET /health HTTP/1.1\r\nHost: medousa-workshop\r\nConnection: close\r\n\r\n";
        assert!(request_complete(get).expect("GET"));
    }
}
