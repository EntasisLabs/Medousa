//! The Comms/Transport service.
//!
//! One service owns the connection pool, the set of adapters, route selection,
//! and a per-route circuit breaker. The engine never touches a transport
//! directly — it holds a [`CommsHandle`] and sends [`CommsCommand`]s over a
//! **bounded** mpsc channel (backpressure, no unbounded queue). The service is
//! a single-consumer actor, so requests are naturally serialized and route
//! probing can never overlap.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, oneshot};

use super::backoff::{CircuitBreaker, CircuitBreakerConfig};
use super::route::{RoutePolicy, RouteSelector};
use super::transport::{
    Transport, TransportError, TransportKind, TransportRequest, TransportResponse,
};

#[derive(Debug, Clone)]
pub struct CommsConfig {
    /// Bounded command-queue depth. Caps in-flight requests the engine can
    /// enqueue before `request()` awaits (backpressure).
    pub queue_capacity: usize,
    pub route: RoutePolicy,
    pub breaker: CircuitBreakerConfig,
}

impl Default for CommsConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 256,
            route: RoutePolicy::default(),
            breaker: CircuitBreakerConfig::default(),
        }
    }
}

/// Commands the engine sends to the service over the channel.
pub enum CommsCommand {
    Request {
        req: TransportRequest,
        reply: oneshot::Sender<Result<TransportResponse, TransportError>>,
    },
    /// Resolve (and cache) the currently preferred route.
    ActiveRoute {
        reply: oneshot::Sender<Option<TransportKind>>,
    },
    /// Drop the cached route so the next request re-probes.
    Refresh {
        reply: oneshot::Sender<()>,
    },
}

/// Cheap-to-clone handle the engine uses to talk to the service.
#[derive(Clone)]
pub struct CommsHandle {
    tx: mpsc::Sender<CommsCommand>,
}

impl CommsHandle {
    pub async fn request(
        &self,
        req: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommsCommand::Request { req, reply })
            .await
            .map_err(|_| TransportError::other("comms service stopped"))?;
        rx.await
            .map_err(|_| TransportError::other("comms service dropped reply"))?
    }

    pub async fn active_route(&self) -> Option<TransportKind> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommsCommand::ActiveRoute { reply })
            .await
            .ok()?;
        rx.await.ok().flatten()
    }

    pub async fn refresh(&self) {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(CommsCommand::Refresh { reply }).await.is_ok() {
            let _ = rx.await;
        }
    }
}

pub struct CommsService {
    adapters: HashMap<TransportKind, Arc<dyn Transport>>,
    selector: RouteSelector,
    breakers: HashMap<TransportKind, CircuitBreaker>,
}

impl CommsService {
    /// Build and spawn the service, returning the engine-facing handle.
    pub fn spawn(adapters: Vec<Arc<dyn Transport>>, config: CommsConfig) -> CommsHandle {
        let mut adapter_map: HashMap<TransportKind, Arc<dyn Transport>> = HashMap::new();
        let mut breakers = HashMap::new();
        for adapter in adapters {
            breakers.insert(adapter.kind(), CircuitBreaker::new(config.breaker.clone()));
            adapter_map.insert(adapter.kind(), adapter);
        }
        let service = CommsService {
            adapters: adapter_map,
            selector: RouteSelector::new(config.route),
            breakers,
        };
        let (tx, rx) = mpsc::channel(config.queue_capacity.max(1));
        tokio::spawn(service.run(rx));
        CommsHandle { tx }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<CommsCommand>) {
        while let Some(command) = rx.recv().await {
            match command {
                CommsCommand::Request { req, reply } => {
                    let result = self.handle_request(&req).await;
                    let _ = reply.send(result);
                }
                CommsCommand::ActiveRoute { reply } => {
                    let route = self.resolve_route().await;
                    let _ = reply.send(route);
                }
                CommsCommand::Refresh { reply } => {
                    self.selector.invalidate();
                    let _ = reply.send(());
                }
            }
        }
    }

    /// Candidate routes in the order we should try them: the cached route first
    /// (if its breaker still allows it), then the policy preference order.
    fn candidate_order(&mut self, now: Instant) -> Vec<TransportKind> {
        let mut order: Vec<TransportKind> = Vec::new();
        if let Some(cached) = self.selector.cached(now) {
            order.push(cached);
        }
        for kind in &self.selector.policy().order {
            if !order.contains(kind) {
                order.push(*kind);
            }
        }
        order
    }

    async fn handle_request(
        &mut self,
        req: &TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let now = Instant::now();
        let order = self.candidate_order(now);
        let mut last_error =
            TransportError::connect("no reachable transport route");

        for kind in order {
            let Some(adapter) = self.adapters.get(&kind).cloned() else {
                continue;
            };
            // Respect the breaker: skip routes that are tripped open.
            let allowed = self
                .breakers
                .get_mut(&kind)
                .map(|breaker| breaker.allow(now))
                .unwrap_or(true);
            if !allowed {
                continue;
            }

            match adapter.execute(req).await {
                Ok(response) => {
                    if let Some(breaker) = self.breakers.get_mut(&kind) {
                        breaker.on_success();
                    }
                    self.selector.record(kind, Instant::now());
                    return Ok(response);
                }
                Err(err) if err.is_connectivity() => {
                    // Route is down — count it against the breaker and fail over.
                    if let Some(breaker) = self.breakers.get_mut(&kind) {
                        breaker.on_failure(Instant::now());
                    }
                    self.selector.invalidate();
                    last_error = err;
                }
                Err(err) => {
                    // The route reached the server (HTTP / protocol error) or
                    // simply can't serve this request (Unsupported). Don't
                    // fail over on a server-side status; surface it. For
                    // Unsupported, try the next route.
                    if err.kind == super::transport::TransportErrorKind::Unsupported {
                        last_error = err;
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        Err(last_error)
    }

    /// Probe adapters whose breaker allows and choose the preferred reachable
    /// route, caching it.
    async fn resolve_route(&mut self) -> Option<TransportKind> {
        let now = Instant::now();
        if let Some(cached) = self.selector.cached(now) {
            return Some(cached);
        }
        let mut healthy: HashSet<TransportKind> = HashSet::new();
        let candidates: Vec<TransportKind> = self.selector.policy().order.clone();
        for kind in candidates {
            let Some(adapter) = self.adapters.get(&kind).cloned() else {
                continue;
            };
            let allowed = self
                .breakers
                .get_mut(&kind)
                .map(|breaker| breaker.allow(now))
                .unwrap_or(true);
            if allowed && adapter.health().await {
                healthy.insert(kind);
            }
        }
        self.selector.choose(&healthy, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comms::transport::{HttpMethod, ResponseBody};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeAdapter {
        kind: TransportKind,
        healthy: bool,
        /// None = connectivity failure; Some(status) = respond with that status.
        respond: Option<u16>,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Transport for FakeAdapter {
        fn kind(&self) -> TransportKind {
            self.kind
        }
        async fn health(&self) -> bool {
            self.healthy
        }
        async fn execute(
            &self,
            _req: &TransportRequest,
        ) -> Result<TransportResponse, TransportError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.respond {
                Some(status) => Ok(TransportResponse {
                    kind: self.kind,
                    status,
                    headers: Vec::new(),
                    body: ResponseBody::Bytes(Vec::new()),
                }),
                None => Err(TransportError::connect("down")),
            }
        }
    }

    fn req() -> TransportRequest {
        TransportRequest {
            method: HttpMethod::Get,
            path: "/health".to_string(),
            headers: Vec::new(),
            body: None,
            stream: false,
        }
    }

    #[tokio::test]
    async fn fails_over_from_dead_lan_to_iroh() {
        let lan_calls = Arc::new(AtomicUsize::new(0));
        let iroh_calls = Arc::new(AtomicUsize::new(0));
        let adapters: Vec<Arc<dyn Transport>> = vec![
            Arc::new(FakeAdapter {
                kind: TransportKind::Lan,
                healthy: false,
                respond: None,
                calls: lan_calls.clone(),
            }),
            Arc::new(FakeAdapter {
                kind: TransportKind::Iroh,
                healthy: true,
                respond: Some(200),
                calls: iroh_calls.clone(),
            }),
        ];
        let handle = CommsService::spawn(adapters, CommsConfig::default());
        let response = handle.request(req()).await.expect("iroh serves");
        assert_eq!(response.kind, TransportKind::Iroh);
        assert_eq!(lan_calls.load(Ordering::SeqCst), 1);
        assert_eq!(iroh_calls.load(Ordering::SeqCst), 1);
        // Subsequent request prefers the now-cached Iroh route, skipping LAN.
        let _ = handle.request(req()).await.expect("iroh again");
        assert_eq!(lan_calls.load(Ordering::SeqCst), 1, "dead LAN not retried while cached");
        assert_eq!(iroh_calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn http_error_does_not_failover() {
        let lan_calls = Arc::new(AtomicUsize::new(0));
        let iroh_calls = Arc::new(AtomicUsize::new(0));
        let adapters: Vec<Arc<dyn Transport>> = vec![
            Arc::new(FakeAdapter {
                kind: TransportKind::Lan,
                healthy: true,
                respond: Some(500),
                calls: lan_calls.clone(),
            }),
            Arc::new(FakeAdapter {
                kind: TransportKind::Iroh,
                healthy: true,
                respond: Some(200),
                calls: iroh_calls.clone(),
            }),
        ];
        let handle = CommsService::spawn(adapters, CommsConfig::default());
        let response = handle.request(req()).await.expect("lan reached server");
        assert_eq!(response.status, 500);
        assert_eq!(lan_calls.load(Ordering::SeqCst), 1);
        assert_eq!(iroh_calls.load(Ordering::SeqCst), 0, "5xx must not silently reroute");
    }

    #[tokio::test]
    async fn active_route_probes_health() {
        let adapters: Vec<Arc<dyn Transport>> = vec![
            Arc::new(FakeAdapter {
                kind: TransportKind::Lan,
                healthy: false,
                respond: None,
                calls: Arc::new(AtomicUsize::new(0)),
            }),
            Arc::new(FakeAdapter {
                kind: TransportKind::HttpFallback,
                healthy: true,
                respond: Some(200),
                calls: Arc::new(AtomicUsize::new(0)),
            }),
        ];
        let handle = CommsService::spawn(adapters, CommsConfig::default());
        assert_eq!(handle.active_route().await, Some(TransportKind::HttpFallback));
    }

    #[tokio::test]
    async fn all_routes_down_returns_connect_error() {
        let adapters: Vec<Arc<dyn Transport>> = vec![Arc::new(FakeAdapter {
            kind: TransportKind::Lan,
            healthy: false,
            respond: None,
            calls: Arc::new(AtomicUsize::new(0)),
        })];
        let handle = CommsService::spawn(adapters, CommsConfig::default());
        let err = handle.request(req()).await.expect_err("no route");
        assert!(err.is_connectivity());
    }
}
