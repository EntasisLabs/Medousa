//! Bounded browser presentation DTOs shared by the daemon contract and Home.

use serde::{Deserialize, Serialize};

pub const BROWSER_PRESENTATION_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BrowserPresentationViewport {
    pub width: u32,
    pub height: u32,
    pub scroll_x: i64,
    pub scroll_y: i64,
    pub device_scale_factor: f64,
}

/// The action fence Home needs from the daemon's semantic mirror. Raw DOM and
/// accessibility nodes remain on the owning workshop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BrowserPresentationObservation {
    pub schema_version: u16,
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub document_id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<u64>,
    pub full: bool,
    pub viewport: BrowserPresentationViewport,
    pub truncated: bool,
    pub captured_at_ms: u64,
    pub untrusted_content: bool,
}

/// One redacted, size-capped viewport artifact paired to its observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BrowserPresentationScreenshot {
    pub schema_version: u16,
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub document_id: String,
    pub observation_revision: u64,
    pub viewport: BrowserPresentationViewport,
    pub coordinate_frame: String,
    pub mime: String,
    pub image_width: u32,
    pub image_height: u32,
    pub byte_size: usize,
    pub sha256: String,
    pub sensitive_regions_redacted: usize,
    pub captured_at_ms: u64,
    pub untrusted_content: bool,
    pub image_base64: String,
}

/// Atomic presentation update. Keeping the semantic fence and pixels in one
/// event prevents Home from pairing an observation with a different frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BrowserPresentationFrame {
    pub schema_version: u16,
    pub world_id: String,
    pub observation: BrowserPresentationObservation,
    pub screenshot: BrowserPresentationScreenshot,
}
