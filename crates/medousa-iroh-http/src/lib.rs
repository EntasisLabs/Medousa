//! HTTP/1.1 client tunneled over Iroh (`medousa-http/1` ALPN).

use std::str::FromStr;

use anyhow::{Context, Result, bail};
use httparse::{Response, Status, EMPTY_HEADER};
use iroh::{Endpoint, endpoint::presets};
use iroh_tickets::endpoint::EndpointTicket;
use tokio::sync::OnceCell;

/// Application-layer protocol identifier for Medousa HTTP tunneling.
pub const ALPN: &[u8] = b"medousa-http/1";

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BODY_CHUNK: usize = 64 * 1024;

static WORKSHOP_CLIENT: OnceCell<Endpoint> = OnceCell::const_new();

pub struct IrohHttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: IrohHttpBody,
}

pub struct IrohHttpBody {
    recv: iroh::endpoint::RecvStream,
    buffer: Vec<u8>,
    finished: bool,
}

impl IrohHttpBody {
    pub async fn read_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        if self.finished && self.buffer.is_empty() {
            return Ok(None);
        }
        if !self.buffer.is_empty() {
            let chunk = self.buffer.split_off(0);
            return Ok(Some(chunk));
        }
        let mut chunk = vec![0u8; MAX_BODY_CHUNK];
        let read = self
            .recv
            .read(&mut chunk)
            .await
            .context("read iroh HTTP body")?;
        let Some(read) = read else {
            self.finished = true;
            return Ok(None);
        };
        if read == 0 {
            self.finished = true;
            return Ok(None);
        }
        chunk.truncate(read);
        Ok(Some(chunk))
    }
}

async fn shared_client_endpoint() -> Result<&'static Endpoint> {
    WORKSHOP_CLIENT
        .get_or_try_init(|| async {
            let endpoint = Endpoint::bind(presets::N0)
                .await
                .context("bind iroh client endpoint")?;
            endpoint.online().await;
            Ok(endpoint)
        })
        .await
}

pub async fn iroh_http_request(
    ticket: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Result<IrohHttpResponse> {
    let ticket = EndpointTicket::from_str(ticket).map_err(|err| anyhow::anyhow!("{err}"))?;
    let endpoint = shared_client_endpoint().await?;

    let conn = endpoint
        .connect(ticket.endpoint_addr().clone(), ALPN)
        .await
        .context("connect to workshop over iroh")?;
    let (mut send, mut recv) = conn.open_bi().await.context("open bi stream")?;

    let normalized = normalize_path(path);
    let mut request = format!("{method} {normalized} HTTP/1.1\r\nHost: medousa-workshop\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("Connection: close\r\n\r\n");
    send.write_all(request.as_bytes())
        .await
        .context("write HTTP request")?;
    if let Some(body) = body {
        send.write_all(body).await.context("write HTTP body")?;
    }
    send.finish().context("finish HTTP request stream")?;

    let (status, response_headers, header_end, mut raw) =
        read_http_response_headers(&mut recv).await?;
    raw.drain(..header_end.saturating_add(4));

    Ok(IrohHttpResponse {
        status,
        headers: response_headers,
        body: IrohHttpBody {
            recv,
            buffer: raw,
            finished: false,
        },
    })
}

fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

async fn read_http_response_headers(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<(u16, Vec<(String, String)>, usize, Vec<u8>)> {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if raw.len() >= MAX_HEADER_BYTES {
            bail!("HTTP response headers exceed {MAX_HEADER_BYTES} bytes");
        }
        let read = recv
            .read(&mut chunk)
            .await
            .context("read HTTP response")?;
        let Some(read) = read else {
            bail!("truncated HTTP response before headers");
        };
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        if let Some(header_end) = find_header_end(&raw) {
            return parse_response_headers(&raw, header_end);
        }
    }
    bail!("incomplete HTTP response headers")
}

fn find_header_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|window| window == b"\r\n\r\n")
}

type ParsedResponseHeaders = (u16, Vec<(String, String)>, usize, Vec<u8>);

fn parse_response_headers(
    raw: &[u8],
    header_end: usize,
) -> Result<ParsedResponseHeaders> {
    let mut headers = [EMPTY_HEADER; 32];
    let mut response = Response::new(&mut headers);
    let status = response
        .parse(&raw[..header_end + 4])
        .context("parse HTTP response")?;
    if !matches!(status, Status::Complete(_)) {
        bail!("incomplete HTTP response");
    }
    let code = response.code.context("missing HTTP status code")?;
    let parsed_headers = response
        .headers
        .iter()
        .map(|header| {
            (
                header.name.to_string(),
                String::from_utf8_lossy(header.value).to_string(),
            )
        })
        .collect();
    Ok((code, parsed_headers, header_end, raw.to_vec()))
}

pub async fn iroh_http_get_text(ticket: &str, path: &str) -> Result<String> {
    let mut response = iroh_http_request(ticket, "GET", path, &[], None).await?;
    let mut body = Vec::new();
    while let Some(chunk) = response.body.read_chunk().await? {
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).to_string())
}
