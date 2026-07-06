//! Route selection.
//!
//! Generalizes the old mobile-only "LAN else Iroh" branch into an ordered
//! preference list (`LAN -> Iroh -> HTTP-fallback`) with a cached decision and
//! TTL. The selection is the *only* place routing is decided; call sites just
//! issue [`super::transport::TransportRequest`]s.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::transport::TransportKind;

#[derive(Debug, Clone)]
pub struct RoutePolicy {
    /// Preference order; the first reachable transport wins.
    pub order: Vec<TransportKind>,
    /// How long a successful LAN decision is trusted before re-probing.
    pub primary_ttl: Duration,
    /// How long a fallback (Iroh / HTTP) decision is trusted (longer, since
    /// re-probing LAN every call when it's down is wasteful).
    pub fallback_ttl: Duration,
}

impl Default for RoutePolicy {
    fn default() -> Self {
        Self {
            order: vec![
                TransportKind::Lan,
                TransportKind::Iroh,
                TransportKind::HttpFallback,
            ],
            primary_ttl: Duration::from_secs(15),
            fallback_ttl: Duration::from_secs(45),
        }
    }
}

impl RoutePolicy {
    fn ttl_for(&self, kind: TransportKind) -> Duration {
        match kind {
            TransportKind::Lan => self.primary_ttl,
            _ => self.fallback_ttl,
        }
    }
}

/// Pure selection kernel: first transport in `order` that is in `healthy`.
/// Extracted so route preference is unit-testable without any network.
pub fn first_available(
    order: &[TransportKind],
    healthy: &HashSet<TransportKind>,
) -> Option<TransportKind> {
    order.iter().copied().find(|kind| healthy.contains(kind))
}

#[derive(Debug, Clone, Copy)]
struct CachedDecision {
    kind: TransportKind,
    expires_at: Instant,
}

/// Caches the chosen route with a per-kind TTL.
pub struct RouteSelector {
    policy: RoutePolicy,
    cached: Option<CachedDecision>,
}

impl RouteSelector {
    pub fn new(policy: RoutePolicy) -> Self {
        Self {
            policy,
            cached: None,
        }
    }

    pub fn policy(&self) -> &RoutePolicy {
        &self.policy
    }

    /// Returns the cached decision if still fresh at `now`.
    pub fn cached(&self, now: Instant) -> Option<TransportKind> {
        self.cached.and_then(|decision| {
            (decision.expires_at > now).then_some(decision.kind)
        })
    }

    /// Record a freshly probed decision, stamping its TTL from `now`.
    pub fn record(&mut self, kind: TransportKind, now: Instant) {
        self.cached = Some(CachedDecision {
            kind,
            expires_at: now + self.policy.ttl_for(kind),
        });
    }

    /// Invalidate the cache so the next request re-probes (e.g. after a
    /// connectivity failure on the current route).
    pub fn invalidate(&mut self) {
        self.cached = None;
    }

    /// Choose a route given a freshly probed `healthy` set, updating the cache.
    pub fn choose(
        &mut self,
        healthy: &HashSet<TransportKind>,
        now: Instant,
    ) -> Option<TransportKind> {
        let chosen = first_available(&self.policy.order, healthy)?;
        self.record(chosen, now);
        Some(chosen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy(kinds: &[TransportKind]) -> HashSet<TransportKind> {
        kinds.iter().copied().collect()
    }

    #[test]
    fn prefers_earliest_available_in_order() {
        let order = vec![
            TransportKind::Lan,
            TransportKind::Iroh,
            TransportKind::HttpFallback,
        ];
        assert_eq!(
            first_available(&order, &healthy(&[TransportKind::Lan, TransportKind::Iroh])),
            Some(TransportKind::Lan)
        );
        assert_eq!(
            first_available(&order, &healthy(&[TransportKind::Iroh, TransportKind::HttpFallback])),
            Some(TransportKind::Iroh)
        );
        assert_eq!(
            first_available(&order, &healthy(&[TransportKind::HttpFallback])),
            Some(TransportKind::HttpFallback)
        );
        assert_eq!(first_available(&order, &healthy(&[])), None);
    }

    #[test]
    fn cache_expires_per_kind_ttl() {
        let mut selector = RouteSelector::new(RoutePolicy {
            primary_ttl: Duration::from_secs(10),
            fallback_ttl: Duration::from_secs(40),
            ..RoutePolicy::default()
        });
        let t0 = Instant::now();
        selector.record(TransportKind::Lan, t0);
        assert_eq!(selector.cached(t0 + Duration::from_secs(9)), Some(TransportKind::Lan));
        assert_eq!(selector.cached(t0 + Duration::from_secs(11)), None);

        selector.record(TransportKind::Iroh, t0);
        assert_eq!(selector.cached(t0 + Duration::from_secs(39)), Some(TransportKind::Iroh));
        assert_eq!(selector.cached(t0 + Duration::from_secs(41)), None);
    }

    #[test]
    fn invalidate_clears_cache() {
        let mut selector = RouteSelector::new(RoutePolicy::default());
        let t0 = Instant::now();
        selector.record(TransportKind::Lan, t0);
        assert!(selector.cached(t0).is_some());
        selector.invalidate();
        assert!(selector.cached(t0).is_none());
    }

    #[test]
    fn choose_records_decision() {
        let mut selector = RouteSelector::new(RoutePolicy::default());
        let t0 = Instant::now();
        let chosen = selector.choose(&healthy(&[TransportKind::Iroh]), t0);
        assert_eq!(chosen, Some(TransportKind::Iroh));
        assert_eq!(selector.cached(t0), Some(TransportKind::Iroh));
    }
}
