//! Base URL for in-process calls back into this daemon's own HTTP API.
//!
//! Several cognition tools (coding, code intelligence, detamu) reach daemon
//! endpoints over loopback rather than calling the services directly. They run
//! inside the daemon, so the authoritative address is whatever it actually
//! bound — not a compiled-in default that silently rots when the port changes.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

use crate::daemon_api::DEFAULT_DAEMON_URL;

static SELF_BASE_URL: OnceLock<String> = OnceLock::new();

/// Record the address the listener actually bound, so in-process tool proxies
/// dial the live port. Wildcard binds normalize to the matching loopback family.
pub fn init_daemon_self_base_url(bound_addr: SocketAddr) {
    let _ = SELF_BASE_URL.set(daemon_self_url_for_bound_addr(bound_addr));
}

fn daemon_self_url_for_bound_addr(bound_addr: SocketAddr) -> String {
    let dial_ip = match bound_addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("http://{}", SocketAddr::new(dial_ip, bound_addr.port()))
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

/// Build an authenticated client for calls back into this daemon.
///
/// A live bound address is process-owned authority even when the daemon binds
/// a specific LAN interface, so it may carry the provisioned local credential.
/// Before daemon initialization, the normal local-client rules still apply and
/// refuse to send that credential to a non-loopback URL from the environment.
pub fn authenticated_http_client() -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    if let Some(value) = self_authorization_header()? {
        headers.insert(AUTHORIZATION, value);
    }
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("build authenticated daemon self client")
}

/// Authorization value for an HTTP upgrade back into this daemon.
pub fn self_authorization_header() -> Result<Option<HeaderValue>> {
    if SELF_BASE_URL.get().is_some() {
        return crate::local_daemon_auth::trusted_self_authorization_header(
            medousa_local_credential::CLI_LOCAL_NAME,
        )
        .map(Some);
    }
    crate::local_daemon_auth::authorization_header(
        &daemon_self_base_url(),
        medousa_local_credential::CLI_LOCAL_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A daemon on a wildcard bind still has to call itself over loopback.
    #[test]
    fn wildcard_binds_resolve_to_loopback_on_the_real_port() {
        assert_eq!(
            daemon_self_url_for_bound_addr("0.0.0.0:7419".parse().expect("v4 bind")),
            "http://127.0.0.1:7419"
        );
        assert_eq!(
            daemon_self_url_for_bound_addr("[::]:9000".parse().expect("v6 bind")),
            "http://[::1]:9000"
        );
    }

    #[test]
    fn explicit_bind_address_and_actual_port_are_preserved() {
        assert_eq!(
            daemon_self_url_for_bound_addr("192.0.2.10:49152".parse().expect("explicit bind")),
            "http://192.0.2.10:49152"
        );
    }

    /// Without a bound server the default must match the documented port.
    #[test]
    fn default_matches_the_documented_daemon_port() {
        assert_eq!(DEFAULT_DAEMON_URL, "http://127.0.0.1:7419");
    }
}
