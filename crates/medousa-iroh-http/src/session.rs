//! Retain QUIC connections and serialize dials per peer, rather than per request.

use super::*;
use iroh::{EndpointId, endpoint::Connection};
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::Mutex;

type ConnectionSlot = Arc<Mutex<Option<Connection>>>;
static CLIENTS: OnceLock<Mutex<HashMap<String, Arc<ClientSession>>>> = OnceLock::new();
const MAX_CLIENTS: usize = 32;
const MAX_PEERS: usize = 256;

#[derive(Default)]
pub(super) struct ClientSession {
    endpoint: Mutex<Option<Endpoint>>,
    connections: Mutex<HashMap<EndpointId, ConnectionSlot>>,
}

impl ClientSession {
    pub(super) async fn endpoint(&self, relays: &[RelayUrl]) -> Result<Endpoint> {
        let mut guard = self.endpoint.lock().await;
        if let Some(endpoint) = guard.as_ref().filter(|endpoint| !endpoint.is_closed()) {
            return Ok(endpoint.clone());
        }
        let endpoint = bind_client(relays).await?;
        *guard = Some(endpoint.clone());
        Ok(endpoint)
    }

    pub(super) async fn invalidate(&self, peer: EndpointId, connection_id: usize) {
        let slot = self.connections.lock().await.get(&peer).cloned();
        if let Some(slot) = slot {
            let mut guard = slot.lock().await;
            if guard
                .as_ref()
                .is_some_and(|connection| connection.stable_id() == connection_id)
            {
                *guard = None;
            }
        }
    }

    pub(super) async fn connection(
        &self,
        endpoint: &Endpoint,
        addr: EndpointAddr,
    ) -> Result<Connection> {
        let slot = {
            let mut connections = self.connections.lock().await;
            if !connections.contains_key(&addr.id) && connections.len() >= MAX_PEERS {
                connections.retain(|_, slot| {
                    Arc::strong_count(slot) > 1
                        || slot.try_lock().map_or(true, |guard| {
                            guard
                                .as_ref()
                                .is_some_and(|connection| connection.close_reason().is_none())
                        })
                });
                if connections.len() >= MAX_PEERS {
                    bail!("iroh peer capacity reached");
                }
            }
            connections.entry(addr.id).or_default().clone()
        };
        let mut guard = slot.lock().await;
        let started = Instant::now();
        if let Some(connection) = guard
            .as_ref()
            .filter(|connection| connection.close_reason().is_none())
        {
            diagnostics::record("connection", "reused", started, Some(connection));
            return Ok(connection.clone());
        }
        if let Some(connection) = guard.as_ref() {
            diagnostics::record("connection", "closed", started, Some(connection));
        }
        // Only retry before an HTTP stream has been written. Never replay a mutation.
        diagnostics::record("dial", "started", started, None);
        let connection = match connect_workshop(endpoint, addr.clone()).await {
            Ok(connection) => connection,
            Err(_) => {
                endpoint.network_change().await;
                match connect_workshop(endpoint, addr).await {
                    Ok(connection) => connection,
                    Err(error) => {
                        diagnostics::record("dial", "failed", started, None);
                        return Err(error);
                    }
                }
            }
        };
        diagnostics::record("dial", "connected", started, Some(&connection));
        *guard = Some(connection.clone());
        Ok(connection)
    }
}

fn clients() -> &'static Mutex<HashMap<String, Arc<ClientSession>>> {
    CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) async fn client_for_relays(relays: &[RelayUrl]) -> Result<Arc<ClientSession>> {
    let mut urls: Vec<_> = relays.iter().map(|url| url.as_str()).collect();
    urls.sort_unstable();
    urls.dedup();
    let key = urls.join("\n");
    let mut clients = clients().lock().await;
    if let Some(client) = clients.get(&key) {
        return Ok(client.clone());
    }
    if clients.len() >= MAX_CLIENTS {
        // Active response bodies hold an Arc too. Never close another workshop's
        // endpoint just because a request arrived with a different relay list.
        clients.retain(|_, client| Arc::strong_count(client) > 1);
        if clients.len() >= MAX_CLIENTS {
            bail!("iroh client capacity reached");
        }
    }
    let client = Arc::new(ClientSession::default());
    clients.insert(key, client.clone());
    Ok(client)
}

pub(super) async fn notify_network_change() {
    diagnostics::record("network_change", "requested", Instant::now(), None);
    let sessions: Vec<_> = clients().lock().await.values().cloned().collect();
    for session in sessions {
        let endpoint = session.endpoint.lock().await.clone();
        if let Some(endpoint) = endpoint {
            endpoint.network_change().await;
        }
    }
}
