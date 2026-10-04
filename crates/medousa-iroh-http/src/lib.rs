//! HTTP/1.1 client tunneled over Iroh (`medousa-http/1` ALPN).

use std::str::FromStr;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use httparse::{EMPTY_HEADER, Response, Status};
use iroh::{Endpoint, EndpointAddr, RelayMode, RelayUrl, TransportAddr, endpoint::presets};
use iroh_tickets::endpoint::EndpointTicket;
use tokio::sync::Mutex;

/// Application-layer protocol identifier for Medousa HTTP tunneling.
pub const ALPN: &[u8] = b"medousa-http/1";

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BODY_CHUNK: usize = 64 * 1024;

struct CachedEndpoint {
    key: String,
    endpoint: Endpoint,
}

static WORKSHOP_CLIENT: OnceLock<Mutex<Option<CachedEndpoint>>> = OnceLock::new();

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

fn client_slot() -> &'static Mutex<Option<CachedEndpoint>> {
    WORKSHOP_CLIENT.get_or_init(|| Mutex::new(None))
}

/// Bind one client for this ticket's relays.
///
/// `Endpoint::bind(presets::N0)` probes every public n0 relay. In a browser
/// those hostnames end in `.`, and `GET /ping` then fails closed and keeps
/// retrying. Urspace dials only the relays named in the ticket, with that
/// trailing dot removed, so the probe hits a host the browser can open.
async fn endpoint_for_relays(relays: &[RelayUrl]) -> Result<Endpoint> {
    let key = if relays.is_empty() {
        "default".to_string()
    } else {
        relays
            .iter()
            .map(|url| url.as_str().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut guard = client_slot().lock().await;
    if let Some(cached) = guard.as_ref()
        && cached.key == key
        && !cached.endpoint.is_closed()
    {
        return Ok(cached.endpoint.clone());
    }
    let endpoint = bind_client(relays).await?;
    if let Some(previous) = guard.take() {
        previous.endpoint.close().await;
    }
    *guard = Some(CachedEndpoint {
        key,
        endpoint: endpoint.clone(),
    });
    Ok(endpoint)
}

/// Re-probe sockets/relays after a foreground resume or network handoff without
/// closing streams or replacing the client's identity underneath other requests.
pub async fn notify_network_change() {
    let endpoint = client_slot()
        .lock()
        .await
        .as_ref()
        .map(|cached| cached.endpoint.clone());
    if let Some(endpoint) = endpoint {
        endpoint.network_change().await;
    }
}

async fn bind_client(relays: &[RelayUrl]) -> Result<Endpoint> {
    let mut builder = Endpoint::builder(presets::N0);
    if !relays.is_empty() {
        builder = builder.relay_mode(RelayMode::custom(relays.iter().cloned()));
    }
    builder.bind().await.context("bind iroh client endpoint")
}

fn dial_target(ticket: &str) -> Result<(EndpointAddr, Vec<RelayUrl>)> {
    let ticket = EndpointTicket::from_str(ticket).map_err(|err| anyhow::anyhow!("{err}"))?;
    let addr = normalize_relay_hosts(ticket.endpoint_addr())?;
    let relays: Vec<RelayUrl> = addr.relay_urls().cloned().collect();
    Ok((addr, relays))
}

fn normalize_relay_hosts(endpoint_addr: &EndpointAddr) -> Result<EndpointAddr> {
    let addrs = endpoint_addr
        .addrs
        .iter()
        .map(|addr| match addr {
            TransportAddr::Relay(relay_url) => {
                normalize_relay_host(relay_url).map(TransportAddr::Relay)
            }
            addr => Ok(addr.clone()),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(EndpointAddr::from_parts(endpoint_addr.id, addrs))
}

fn normalize_relay_host(relay_url: &RelayUrl) -> Result<RelayUrl> {
    let mut url: url::Url = relay_url.clone().into();
    let Some(host) = url.host_str().map(ToOwned::to_owned) else {
        return Ok(relay_url.clone());
    };
    let normalized = host.trim_end_matches('.');
    if normalized == host {
        return Ok(relay_url.clone());
    }
    if normalized.is_empty() {
        bail!("relay hostname is empty after normalization");
    }
    url.set_host(Some(normalized))
        .map_err(|_| anyhow::anyhow!("relay hostname could not be normalized"))?;
    Ok(RelayUrl::from(url))
}

/// `Endpoint::online` waits until a relay socket is up, and if that watcher
/// drops it parks forever. A browser relay that closes (`ERR_CONNECTION_CLOSED`)
/// would leave the portal join on "Joining…" with no error.
#[cfg(target_arch = "wasm32")]
async fn wait_for_relay(endpoint: &Endpoint) -> Result<()> {
    let online = endpoint.online();
    n0_future::time::timeout(std::time::Duration::from_secs(8), online)
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for an iroh relay"))?;
    Ok(())
}

async fn connect_workshop(
    endpoint: &Endpoint,
    addr: EndpointAddr,
) -> Result<iroh::endpoint::Connection> {
    let dial = endpoint.connect(addr, ALPN);
    #[cfg(target_arch = "wasm32")]
    let result = n0_future::time::timeout(std::time::Duration::from_secs(12), dial).await;
    #[cfg(not(target_arch = "wasm32"))]
    let result = tokio::time::timeout(std::time::Duration::from_secs(12), dial).await;
    result
        .map_err(|_| anyhow::anyhow!("timed out connecting to workshop over iroh"))?
        .context("connect to workshop over iroh")
}

pub async fn iroh_http_request(
    ticket: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Result<IrohHttpResponse> {
    let (addr, relays) = dial_target(ticket)?;
    #[cfg(target_arch = "wasm32")]
    if relays.is_empty() {
        bail!("invitation does not contain a browser relay");
    }
    let endpoint = endpoint_for_relays(&relays).await?;
    #[cfg(target_arch = "wasm32")]
    wait_for_relay(&endpoint).await?;
    // Native iroh can dial directly while its relay is recovering. Waiting for
    // `online()` first incorrectly makes a working direct path depend on a relay.
    // Retry only the handshake: no HTTP bytes have been sent at this point.
    let conn = match connect_workshop(&endpoint, addr.clone()).await {
        Ok(conn) => conn,
        Err(_) => {
            endpoint.network_change().await;
            connect_workshop(&endpoint, addr).await?
        }
    };
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
        let read = recv.read(&mut chunk).await.context("read HTTP response")?;
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

fn parse_response_headers(raw: &[u8], header_end: usize) -> Result<ParsedResponseHeaders> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn native_handshake_connects_without_an_online_relay() {
        let server = Endpoint::builder(presets::Minimal)
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0")
            .unwrap()
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .unwrap();
        let client = Endpoint::builder(presets::Minimal)
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0")
            .unwrap()
            .bind()
            .await
            .unwrap();
        let addr = EndpointAddr::new(server.id()).with_ip_addr(server.bound_sockets()[0]);
        let (connected, accepted) = tokio::join!(connect_workshop(&client, addr), async {
            server.accept().await.unwrap().await.unwrap()
        },);
        let conn = connected.unwrap();
        assert_eq!(conn.remote_id(), server.id());
        assert_eq!(accepted.remote_id(), client.id());
        client.close().await;
        server.close().await;
    }

    #[test]
    fn strips_terminal_dot_from_relay_hosts_for_the_browser() {
        let relay: RelayUrl = "https://usw1-1.relay.n0.iroh.link./".parse().unwrap();
        let endpoint = EndpointAddr::new(iroh::SecretKey::from_bytes(&[7_u8; 32]).public())
            .with_relay_url(relay);

        let normalized = normalize_relay_hosts(&endpoint).unwrap();

        assert_eq!(
            normalized.relay_urls().next().unwrap().to_string(),
            "https://usw1-1.relay.n0.iroh.link/"
        );
    }

    #[test]
    fn leaves_already_normalized_relay_hosts_unchanged() {
        let relay: RelayUrl = "https://relay.example.com/".parse().unwrap();
        assert_eq!(normalize_relay_host(&relay).unwrap(), relay);
    }
}
