//! Iroh transport layer — HTTP/1.1 over QUIC (`medousa-http/1` ALPN).
//!
//! Phase 0 spike: proxy axum daemon API to paired phones without LAN reachability.

#[cfg(feature = "iroh-transport")]
pub mod client;
#[cfg(feature = "iroh-transport")]
pub mod gateway;

#[cfg(feature = "iroh-transport")]
pub use client::fetch_http_path;
#[cfg(feature = "iroh-transport")]
pub use gateway::{spawn_workshop_gateway, workshop_ticket_from_router};

/// Application-layer protocol identifier for Medousa HTTP tunneling.
pub const ALPN: &[u8] = b"medousa-http/1";

#[cfg(feature = "iroh-transport")]
pub fn iroh_enabled_from_env() -> bool {
    std::env::var("MEDOUSA_IROH")
        .ok()
        .map(|value| {
            let trimmed = value.trim().to_ascii_lowercase();
            trimmed == "1" || trimmed == "true" || trimmed == "yes"
        })
        .unwrap_or(false)
}

#[cfg(not(feature = "iroh-transport"))]
pub fn iroh_enabled_from_env() -> bool {
    false
}

#[cfg(not(feature = "iroh-transport"))]
pub fn spawn_workshop_gateway(_upstream: &str) -> anyhow::Result<()> {
    anyhow::bail!("iroh transport disabled — rebuild with --features iroh-transport")
}
