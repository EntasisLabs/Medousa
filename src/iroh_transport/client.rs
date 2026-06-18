use std::str::FromStr;

use anyhow::{Context, Result, bail};
use iroh::{Endpoint, endpoint::presets};
use iroh_tickets::endpoint::EndpointTicket;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::ALPN;

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// Dial a workshop ticket and perform a single HTTP GET (Phase 0 spike client).
pub async fn fetch_http_path(ticket: &str, path: &str) -> Result<String> {
    let ticket = EndpointTicket::from_str(ticket).map_err(|err| anyhow::anyhow!("{err}"))?;
    let endpoint = Endpoint::bind(presets::N0)
        .await
        .context("bind iroh client endpoint")?;
    endpoint.online().await;

    let conn = endpoint
        .connect(ticket.endpoint_addr().clone(), ALPN)
        .await
        .context("connect to workshop over iroh")?;
    let (mut send, mut recv) = conn.open_bi().await.context("open bi stream")?;

    let normalized = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let request = format!(
        "GET {normalized} HTTP/1.1\r\nHost: medousa-workshop\r\nConnection: close\r\n\r\n"
    );
    send.write_all(request.as_bytes())
        .await
        .context("write HTTP request")?;
    send.finish().context("finish HTTP request stream")?;

    let mut body = Vec::new();
    loop {
        let mut chunk = [0u8; 8192];
        let read = recv
            .read(&mut chunk)
            .await
            .context("read HTTP response")?;
        let Some(read) = read else {
            break;
        };
        if read == 0 {
            break;
        }
        if body.len() + read > MAX_RESPONSE_BYTES {
            bail!("response exceeds {MAX_RESPONSE_BYTES} bytes");
        }
        body.extend_from_slice(&chunk[..read]);
    }

    let response = String::from_utf8_lossy(&body).to_string();
    Ok(response)
}
