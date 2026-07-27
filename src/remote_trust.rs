//! Remote vs local trust for daemon HTTP handlers.
//!
//! Iroh's gateway proxies into the daemon on loopback. Handlers that skip auth when
//! `ConnectInfo` is loopback would then treat every Iroh client as local. The gateway
//! injects [`TRANSPORT_HEADER`] = [`TRANSPORT_IROH`]; callers must use
//! [`is_trusted_local`] instead of raw `ip.is_loopback()`.

use std::net::IpAddr;

use axum::http::HeaderMap;

/// Injected by the Iroh HTTP gateway on every proxied upstream request.
pub const TRANSPORT_HEADER: &str = "x-medousa-transport";
/// Value of [`TRANSPORT_HEADER`] for Iroh-proxied traffic.
pub const TRANSPORT_IROH: &str = "iroh";

/// True when the request arrived over the Iroh gateway (header present).
pub fn transport_is_iroh(headers: &HeaderMap) -> bool {
    headers
        .get(TRANSPORT_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(TRANSPORT_IROH))
}

/// True only for genuine local loopback that is **not** Iroh-proxied.
///
/// Use this wherever handlers previously short-circuited auth on `ip.is_loopback()`.
pub fn is_trusted_local(ip: IpAddr, headers: &HeaderMap) -> bool {
    if transport_is_iroh(headers) {
        return false;
    }
    ip.is_loopback()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use std::net::Ipv4Addr;

    #[test]
    fn loopback_without_header_is_trusted() {
        let headers = HeaderMap::new();
        assert!(is_trusted_local(IpAddr::V4(Ipv4Addr::LOCALHOST), &headers));
    }

    #[test]
    fn loopback_with_iroh_header_is_not_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert(TRANSPORT_HEADER, HeaderValue::from_static(TRANSPORT_IROH));
        assert!(!is_trusted_local(IpAddr::V4(Ipv4Addr::LOCALHOST), &headers));
    }

    #[test]
    fn remote_ip_is_never_trusted_local() {
        let headers = HeaderMap::new();
        assert!(!is_trusted_local(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            &headers
        ));
    }
}
