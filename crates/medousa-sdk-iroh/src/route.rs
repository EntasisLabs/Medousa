//! LAN / Iroh route selection with TTL cache.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkshopRoute {
    Lan,
    Iroh,
}

struct CacheEntry {
    lan_base: String,
    route: WorkshopRoute,
    expires_at: Instant,
}

static ROUTE_CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);

const LAN_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
const LAN_TTL: Duration = Duration::from_secs(15);
const IROH_TTL: Duration = Duration::from_secs(45);

pub fn invalidate_route_cache() {
    if let Ok(mut guard) = ROUTE_CACHE.lock() {
        *guard = None;
    }
}

fn read_cache(lan_base: &str) -> Option<WorkshopRoute> {
    let guard = ROUTE_CACHE.lock().ok()?;
    let entry = guard.as_ref()?;
    if entry.lan_base == lan_base && entry.expires_at > Instant::now() {
        Some(entry.route)
    } else {
        None
    }
}

fn write_cache(lan_base: &str, route: WorkshopRoute) {
    let ttl = match route {
        WorkshopRoute::Lan => LAN_TTL,
        WorkshopRoute::Iroh => IROH_TTL,
    };
    if let Ok(mut guard) = ROUTE_CACHE.lock() {
        *guard = Some(CacheEntry {
            lan_base: lan_base.to_string(),
            route,
            expires_at: Instant::now() + ttl,
        });
    }
}

pub async fn pick_route(lan_base: &str, iroh_available: bool) -> WorkshopRoute {
    if !iroh_available {
        return WorkshopRoute::Lan;
    }
    if let Some(route) = read_cache(lan_base) {
        return route;
    }
    let route = if lan_reachable(lan_base).await {
        WorkshopRoute::Lan
    } else {
        WorkshopRoute::Iroh
    };
    write_cache(lan_base, route);
    route
}

async fn lan_reachable(lan_base: &str) -> bool {
    let client = super::pool::standard_client();
    client
        .get(format!("{lan_base}/health"))
        .timeout(LAN_PROBE_TIMEOUT)
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub fn is_connect_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("connect")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("unreachable")
        || lower.contains("network")
        || lower.contains("dns")
        || lower.contains("connection refused")
        || lower.contains("failed to lookup")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_error_heuristic() {
        assert!(is_connect_error("connection refused"));
        assert!(!is_connect_error("HTTP 404"));
    }
}
