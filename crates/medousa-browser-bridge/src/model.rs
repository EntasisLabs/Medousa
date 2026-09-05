use serde::{Deserialize, Serialize};

pub const BROWSER_OBSERVATION_SCHEMA_VERSION: u16 = 1;
pub const BROWSER_SCREENSHOT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabOpenedBy {
    Agent,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserControl {
    Agent,
    User,
    AwaitingOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTab {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub favicon: Option<String>,
    pub opened_by: TabOpenedBy,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: String,
    #[serde(default)]
    pub chat_session_id: Option<String>,
    #[serde(default)]
    pub work_card_id: Option<String>,
    pub tabs: Vec<BrowserTab>,
    pub control: BrowserControl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroupState {
    pub tab_group: TabGroup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSnapshot {
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub markdown: String,
    #[serde(default)]
    pub links: Vec<String>,
}

/// Viewport and coordinate frame for a semantic browser observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserObservationViewport {
    pub width: u32,
    pub height: u32,
    pub scroll_x: i64,
    pub scroll_y: i64,
    pub device_scale_factor: f64,
}

/// Viewport-relative bounds for an observed semantic node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserObservationBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A bounded, untrusted accessibility/DOM projection produced by the driver.
///
/// `element_ref` is opaque outside the driver and is valid only for the
/// observation's `document_id`. It deliberately is not a CSS selector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserSemanticNode {
    pub element_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_ref: Option<String>,
    pub role: String,
    pub name: String,
    pub tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<BrowserObservationBounds>,
    #[serde(default)]
    pub sensitive: bool,
}

/// Full capture reported by the colocated browser driver. Captures are folded
/// into the mirror before callers receive a full projection or bounded delta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserObservationCapture {
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub document_id: String,
    pub viewport: BrowserObservationViewport,
    pub nodes: Vec<BrowserSemanticNode>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub unchanged: bool,
    pub captured_at_ms: u64,
}

/// Current mirror identity used to fence actions that target opaque refs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserObservationState {
    pub tab_id: String,
    pub url: String,
    pub document_id: String,
    pub revision: u64,
}

/// A full semantic projection or a delta after `base_revision`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserObservation {
    pub schema_version: u16,
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub document_id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<u64>,
    pub full: bool,
    pub viewport: BrowserObservationViewport,
    pub nodes: Vec<BrowserSemanticNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_refs: Vec<String>,
    pub truncated: bool,
    pub captured_at_ms: u64,
    /// Browser-derived text and attributes are data, never runtime policy.
    pub untrusted_content: bool,
}

/// A bounded, redacted viewport capture returned only across the local
/// BrowserHost transport. Consumers persist the bytes out of band and expose
/// only the resulting artifact receipt to a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserScreenshotCapture {
    pub schema_version: u16,
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub document_id: String,
    pub observation_revision: u64,
    pub viewport: BrowserObservationViewport,
    pub coordinate_frame: String,
    pub mime: String,
    pub image_width: u32,
    pub image_height: u32,
    pub byte_size: usize,
    pub sha256: String,
    pub sensitive_regions_redacted: usize,
    pub captured_at_ms: u64,
    /// Pixels are page-derived data, never runtime policy.
    pub untrusted_content: bool,
    /// Kept on the colocated host transport only; callers must not echo this
    /// field into tool output or a turn transcript.
    pub image_base64: String,
}
