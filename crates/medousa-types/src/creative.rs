//! Renderer-independent Create contract. Scene coordinates are world coordinates;
//! camera, selection and inspector state never belong in a saved document.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CREATIVE_SCHEMA: &str = "medousa-create";
pub const CREATIVE_VERSION: u32 = 3;
pub const MAX_CREATIVE_SCENE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CREATIVE_ASSET_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeTransform {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativePoint {
    pub x: f64,
    pub y: f64,
    pub pressure: Option<f64>,
    pub elapsed_ms: Option<f64>,
    pub tilt_x: Option<f64>,
    pub tilt_y: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeBrush {
    pub kind: CreativeBrushKind,
    pub size: f64,
    pub thinning: f64,
    pub smoothing: f64,
    pub streamline: f64,
    pub opacity: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CreativeBrushKind {
    Pen,
    Pencil,
    Marker,
    Highlighter,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CreativeInputKind {
    Pen,
    Touch,
    Mouse,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CreativeShapeKind {
    Rectangle,
    Ellipse,
    Line,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeCrop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CreativeFontStyle {
    Normal,
    Italic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeLinePoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeLine {
    pub start: CreativeLinePoint,
    pub end: CreativeLinePoint,
    #[serde(default)]
    pub start_arrow: bool,
    #[serde(default)]
    pub end_arrow: bool,
    #[serde(default)]
    pub dashed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreativeContent {
    Ink {
        color: String,
        input: CreativeInputKind,
        brush: CreativeBrush,
        points: Vec<CreativePoint>,
    },
    Text {
        text: String,
        color: String,
        font_family: String,
        font_size: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font_weight: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font_style: Option<CreativeFontStyle>,
    },
    Shape {
        shape: CreativeShapeKind,
        fill: String,
        stroke: String,
        stroke_width: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<CreativeLine>,
    },
    Image {
        asset_id: String,
        crop: Option<CreativeCrop>,
    },
    Group,
    Frame {
        name: String,
        background: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeNode {
    pub id: String,
    /// Optional layer label; omitted in existing version-3 documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub parent_id: Option<String>,
    pub transform: CreativeTransform,
    pub opacity: f64,
    pub visible: bool,
    pub locked: bool,
    pub content: CreativeContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeScene {
    pub schema: String,
    pub version: u32,
    pub background: String,
    /// Back-to-front paint order. Parent relationships define containment.
    pub nodes: Vec<CreativeNode>,
}

impl Default for CreativeScene {
    fn default() -> Self {
        Self {
            schema: CREATIVE_SCHEMA.into(),
            version: CREATIVE_VERSION,
            background: "transparent".into(),
            nodes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeAsset {
    pub id: String,
    pub mime: String,
    pub byte_size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeDocument {
    pub id: String,
    pub revision: u64,
    pub title: String,
    pub pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    pub scene: CreativeScene,
    pub assets: BTreeMap<String, CreativeAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeCreateRequest {
    /// Client-generated stable id makes creation retries safe.
    pub id: String,
    pub title: String,
    pub scene: CreativeScene,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreativeOperation {
    PutNode {
        node: CreativeNode,
    },
    RemoveNode {
        id: String,
    },
    /// Must be a permutation of all remaining node IDs.
    Reorder {
        ids: Vec<String>,
    },
    SetBackground {
        color: String,
    },
    Rename {
        title: String,
    },
    Pin {
        pinned: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeTransaction {
    pub id: String,
    pub expected_revision: u64,
    pub label: String,
    pub operations: Vec<CreativeOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeHistoryRequest {
    pub id: String,
    pub expected_revision: u64,
    pub redo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeMutationResponse {
    pub document: CreativeDocument,
    pub applied_revision: u64,
    pub duplicate: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeProjectSummary {
    pub id: String,
    pub revision: u64,
    pub title: String,
    pub pinned: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeListQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeListResponse {
    pub projects: Vec<CreativeProjectSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativePromoteMediaRequest {
    pub session_id: String,
    pub media_id: String,
}

/// Resolves within the currently addressed workshop; never contains a local path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeDocumentRef {
    pub document_id: String,
    pub frame_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeProposalRequest {
    pub id: String,
    pub expected_revision: u64,
    pub selected_ids: Vec<String>,
    pub label: String,
    pub operations: Vec<CreativeOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CreativeProposalStatus {
    Ready,
    Accepted,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeProposal {
    pub request: CreativeProposalRequest,
    pub status: CreativeProposalStatus,
    pub created_at: String,
    pub applied_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeProposalDecision {
    pub expected_revision: u64,
    pub accept: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeProposalListResponse {
    pub proposals: Vec<CreativeProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeAssetImportRequest {
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeAssetData {
    pub asset: CreativeAsset,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeDeleteRequest {
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeDeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeContextRequest {
    pub selected_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeSelectionContext {
    pub document_id: String,
    pub revision: u64,
    pub selected_ids: Vec<String>,
    pub nodes: Vec<CreativeNode>,
    pub assets: Vec<CreativeAsset>,
    pub fingerprint: String,
}

/// Stable admission identity. Retrying an ID must repeat the exact input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreativeJobRequest {
    pub id: String,
    pub expected_revision: u64,
    pub selected_ids: Vec<String>,
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CreativeJobStatus {
    Queued,
    Dispatching,
    Running,
    Ready,
    Stale,
    Accepted,
    Failed,
    Interrupted,
    Cancelled,
}
impl CreativeJobStatus {
    pub fn executing(self) -> bool {
        matches!(self, Self::Dispatching | Self::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeJob {
    pub request: CreativeJobRequest,
    pub document_id: String,
    pub context: CreativeSelectionContext,
    pub status: CreativeJobStatus,
    pub session_id: String,
    pub turn_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub result: Option<CreativeProposalRequest>,
    pub assets: Vec<CreativeAsset>,
    pub error: Option<String>,
    pub applied_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreativeJobListResponse {
    pub jobs: Vec<CreativeJob>,
}
