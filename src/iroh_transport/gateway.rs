use std::sync::Arc;

use anyhow::{Context, Result, bail};
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
    let response = forward_request(upstream, &raw).await?;
    send.write_all(&response).await.context("write HTTP response")?;
    send.finish().context("finish send stream")?;
    Ok(())
}

async fn read_http_request(recv: &mut iroh::endpoint::RecvStream) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if buf.len() >= MAX_REQUEST_BYTES {
            bail!("HTTP request exceeds {MAX_REQUEST_BYTES} bytes");
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
        if buf.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    if buf.is_empty() {
        bail!("empty HTTP request");
    }
    Ok(buf)
}

async fn forward_request(upstream: &str, raw: &[u8]) -> Result<Vec<u8>> {
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
        if raw.len() < header_end + 4 + content_length {
            bail!("truncated HTTP body");
        }
        &raw[header_end + 4..header_end + 4 + content_length]
    } else {
        &[]
    };

    let url = format!("{upstream}{path}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
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
    let response_body = response
        .bytes()
        .await
        .context("read upstream response body")?;
    if response_body.len() > MAX_RESPONSE_BYTES {
        bail!("upstream response exceeds {MAX_RESPONSE_BYTES} bytes");
    }

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
    out.push_str(&format!("Content-Length: {}\r\n\r\n", response_body.len()));
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(&response_body);
    Ok(bytes)
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
