//! Base URL for in-process calls back into this daemon's own HTTP API.
//!
//! Several cognition tools (coding, code intelligence, detamu) reach daemon
//! endpoints over loopback rather than calling the services directly. They run
//! inside the daemon, so the authoritative address is whatever it actually
//! bound — not a compiled-in default that silently rots when the port changes.

use std::sync::OnceLock;

use crate::daemon_api::{DEFAULT_DAEMON_URL, resolve_local_daemon_health_url};

static SELF_BASE_URL: OnceLock<String> = OnceLock::new();

/// Record the address this daemon bound, so in-process tool proxies dial the
/// live port. Wildcard binds (`0.0.0.0`, `[::]`) normalize to loopback.
pub fn init_daemon_self_base_url(bind: &str) {
    let _ = SELF_BASE_URL.set(resolve_local_daemon_health_url(bind));
}

/// Base URL for in-process HTTP calls back into this daemon.
///
/// The live bind wins: a self-call must reach *this* process, even when
/// `MEDOUSA_DAEMON_URL` points somewhere else. The env vars only apply when the
/// tools run outside a daemon (CLI/TUI), where no server was bound.
pub fn daemon_self_base_url() -> String {
    if let Some(url) = SELF_BASE_URL.get() {
        return url.clone();
    }
    std::env::var("MEDOUSA_DAEMON_URL")
        .ok()
        .or_else(|| std::env::var("STASIS_DAEMON_URL").ok())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A daemon on a wildcard bind still has to call itself over loopback.
    #[test]
    fn wildcard_binds_resolve_to_loopback_on_the_real_port() {
        assert_eq!(
            resolve_local_daemon_health_url("0.0.0.0:7419"),
            "http://127.0.0.1:7419"
        );
        assert_eq!(
            resolve_local_daemon_health_url("[::]:9000"),
            "http://127.0.0.1:9000"
        );
        assert_eq!(
            resolve_local_daemon_health_url("127.0.0.1:9000"),
            "http://127.0.0.1:9000"
        );
    }

    /// Without a bound server the default must match the documented port.
    #[test]
    fn default_matches_the_documented_daemon_port() {
        assert_eq!(DEFAULT_DAEMON_URL, "http://127.0.0.1:7419");
    }
}
