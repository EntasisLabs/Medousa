//! Iroh transport layer — HTTP/1.1 over QUIC (`medousa-http/1` ALPN).
//!
//! Phase 0 spike: proxy axum daemon API to paired phones without LAN reachability.

#[cfg(feature = "iroh-transport")]
pub mod client;
#[cfg(feature = "iroh-transport")]
pub mod gateway;
#[cfg(feature = "iroh-transport")]
pub mod identity;

#[cfg(feature = "iroh-transport")]
pub use client::fetch_http_path;
#[cfg(feature = "iroh-transport")]
pub use gateway::{
    IrohWorkshopInfo, WorkshopGateway, spawn_workshop_gateway, spawn_workshop_gateway_with_secret,
    workshop_ticket_from_router,
};
#[cfg(feature = "iroh-transport")]
pub use medousa_iroh_http::{
    iroh_http_get_text, iroh_http_request, IrohHttpBody, IrohHttpResponse,
};
#[cfg(feature = "iroh-transport")]
pub use identity::secret_key_from_pairing_identity;

#[cfg(feature = "iroh-transport")]
pub use medousa_iroh_http::ALPN;

#[cfg(feature = "iroh-transport")]
pub fn iroh_enabled_from_env() -> bool {
    match std::env::var("MEDOUSA_IROH")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
    {
        Some(value) if matches!(value.as_str(), "0" | "false" | "no" | "off") => false,
        Some(value) if matches!(value.as_str(), "1" | "true" | "yes" | "on") => true,
        Some(_) => true,
        None => true,
    }
}

#[cfg(not(feature = "iroh-transport"))]
pub fn iroh_enabled_from_env() -> bool {
    false
}
