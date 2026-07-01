use anyhow::Result;

pub use medousa_iroh_http::{iroh_http_get_text, iroh_http_request};

/// Dial a workshop ticket and perform a single HTTP GET (Phase 0 spike client).
pub async fn fetch_http_path(ticket: &str, path: &str) -> Result<String> {
    iroh_http_get_text(ticket, path).await
}
