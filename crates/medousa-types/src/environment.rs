//! Environment spec — daemon-held layout, components, and chrome configuration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ENVIRONMENT_SPEC_VERSION: u32 = 1;

pub const SAFETY_SURFACE_SETTINGS: &str = "settings";
pub const SAFETY_SURFACE_RUNTIME: &str = "runtime";

pub const COMPONENT_SLOT_MAIN: &str = "main";
pub const COMPONENT_SLOT_HEADER: &str = "header";
pub const COMPONENT_SLOT_FAB: &str = "fab";
pub const COMPONENT_SLOT_SIDEBAR: &str = "sidebar";

pub const CHROME_ACTION_OPEN_ASK: &str = "open-ask";
pub const CHROME_ACTION_OPEN_ACTIVITY: &str = "open-activity";

pub const POINTER_KIND_SESSION: &str = "session";
pub const POINTER_KIND_COMPONENT: &str = "component";
pub const POINTER_KIND_WORK_CARD: &str = "work_card";

pub const STALENESS_FRESH: &str = "fresh";
pub const STALENESS_RECENT: &str = "recent";
pub const STALENESS_STALE: &str = "stale";
pub const STALENESS_ARCHIVED: &str = "archived";

pub const DIGEST_MIN_CONFIDENCE: f32 = 0.35;
pub const DIGEST_MAX_POINTERS: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ComponentType {
    Artifact,
    MedousaView,
    BuiltinPanel,
    Presentation,
    MediaEmbed,
    ChromeAction,
    /// Native Liquid scene pinned to a custom surface. `config.scene` holds an
    /// opaque scene payload (`{ ops: [...] }`) the daemon never interprets — the
    /// client decodes + renders it via the Liquid SceneRenderer.
    Scene,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    Builtin,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SurfaceLayout {
    Single,
    Split,
    Dashboard,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiPresentation {
    Inline,
    Panel,
    Fullscreen,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MobileAskEntry {
    Inline,
    Fab,
    TabOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MobileTabBar {
    Full,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DesktopNavStyle {
    Rail,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ActivityRailMode {
    Visible,
    Collapsed,
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentTheme {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_theme_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ShellChromeMobile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_home: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_entry: Option<MobileAskEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_bar: Option<MobileTabBar>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ShellChromeDesktop {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nav_style: Option<DesktopNavStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_rail: Option<ActivityRailMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ShellChromeDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mobile: Option<ShellChromeMobile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop: Option<ShellChromeDesktop>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct SlotDef {
    pub id: String,
    pub zone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct SurfaceDef {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub kind: SurfaceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin_id: Option<String>,
    pub layout: SurfaceLayout,
    #[serde(default)]
    pub slots: Vec<SlotDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mobile_tab: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_root: Option<crate::layout::LayoutNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ComponentDef {
    pub id: String,
    #[serde(rename = "type")]
    pub component_type: ComponentType,
    pub surface_id: String,
    pub slot: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<UiPresentation>,
    #[serde(default)]
    pub feeds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LayoutPreset {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub active: bool,
    pub surfaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_chrome: Option<ShellChromeDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSpec {
    pub version: u32,
    pub profile_id: String,
    pub surfaces: Vec<SurfaceDef>,
    pub components: Vec<ComponentDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_presets: Option<Vec<LayoutPreset>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_chrome: Option<ShellChromeDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<EnvironmentTheme>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSpecResponse {
    pub spec: EnvironmentSpec,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSpecPutRequest {
    pub spec: EnvironmentSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentValidateRequest {
    pub spec: EnvironmentSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentValidateResponse {
    pub valid: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStreamEvent {
    pub revision: u64,
    pub event_type: String,
    pub emitted_at_utc: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<EnvironmentSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_patches: Option<Vec<crate::feed::ComponentFeedPatch>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_event: Option<crate::feed::FeedEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_probe: Option<crate::component_runtime::ComponentRuntimeProbeRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStreamQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ContextPointer {
    pub id: String,
    pub kind: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    pub last_active_at: DateTime<Utc>,
    pub confidence: f32,
    pub staleness: String,
    pub age_human: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ContextPointerDigest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_gap_human: Option<String>,
    pub pointers: Vec<ContextPointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ContextFollowPointerRequest {
    pub pointer_id: String,
    #[serde(default = "default_pointer_scope")]
    pub scope: String,
}

fn default_pointer_scope() -> String {
    "last_5_turns".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ContextFollowPointerResponse {
    pub pointer_id: String,
    pub kind: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ComponentListResponse {
    pub components: Vec<ComponentDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentProposeResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub diff_summary: String,
    pub proposed_spec: EnvironmentSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPendingProposal {
    pub proposed_spec: EnvironmentSpec,
    pub diff_summary: String,
    pub errors: Vec<String>,
    pub proposed_at: DateTime<Utc>,
    pub proposed_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPendingResponse {
    pub pending: Option<EnvironmentPendingProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct CustomViewComponentStatus {
    pub component_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub feeds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<crate::component_runtime::ComponentRuntimeDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct CustomViewFeedStatus {
    pub feed_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_emitted_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct CustomViewRecurringBindingStatus {
    pub recurring_id: String,
    #[serde(default)]
    pub feed_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct CustomViewSurfaceStatus {
    pub surface_id: String,
    pub label: String,
    pub nav_visible: bool,
    #[serde(default)]
    pub components: Vec<CustomViewComponentStatus>,
    #[serde(default)]
    pub subscribed_feed_ids: Vec<String>,
    #[serde(default)]
    pub feed_status: Vec<CustomViewFeedStatus>,
    #[serde(default)]
    pub feed_mismatches: Vec<String>,
    #[serde(default)]
    pub recurring_bindings: Vec<CustomViewRecurringBindingStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_root: Option<crate::layout::LayoutNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStatusResponse {
    pub profile_id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_preset_id: Option<String>,
    pub pending_proposal: bool,
    #[serde(default)]
    pub custom_surfaces: Vec<CustomViewSurfaceStatus>,
    pub feed_mismatch_count: usize,
    pub nav_orphan_count: usize,
    #[serde(default)]
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPatchRequest {
    #[serde(default)]
    pub profile_id: Option<String>,
    pub ops: Vec<EnvironmentPatchOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EnvironmentPatchOp {
    AddCustomSurface {
        id: String,
        label: String,
        #[serde(default = "default_surface_icon")]
        icon: String,
        #[serde(default)]
        layout: Option<SurfaceLayout>,
        #[serde(default = "default_true")]
        add_to_active_preset: bool,
    },
    AddToActivePreset {
        surface_id: String,
    },
    AddComponent {
        component: ComponentDef,
    },
    SetComponentFeeds {
        component_id: String,
        feed_ids: Vec<String>,
    },
    RewriteActivePresetSurfaces {
        surfaces: Vec<String>,
    },
    UpdateSurface {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
    },
    SetEnvironmentTheme {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color_theme_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        brand_color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tagline: Option<String>,
    },
    RemoveCustomSurface {
        id: String,
    },
    RemoveComponent {
        component_id: String,
    },
}

fn default_surface_icon() -> String {
    "layout-grid".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPatchResponse {
    pub ok: bool,
    pub live: bool,
    #[serde(default)]
    pub pending_operator_approval: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub applied_ops: Vec<String>,
}

/// Activate one layout preset (marks active flag + active_preset_id).
pub fn activate_layout_preset(spec: &mut EnvironmentSpec, preset_id: &str) -> Result<(), String> {
    let preset_id = preset_id.trim();
    if preset_id.is_empty() {
        return Err("preset_id is required".to_string());
    }
    let Some(presets) = spec.layout_presets.as_mut() else {
        return Err("spec has no layout presets".to_string());
    };
    if !presets.iter().any(|preset| preset.id == preset_id) {
        return Err(format!("unknown layout preset '{preset_id}'"));
    }
    for preset in presets.iter_mut() {
        preset.active = preset.id == preset_id;
        if preset.active {
            if let Some(chrome) = preset.shell_chrome.clone() {
                spec.shell_chrome = Some(chrome);
            }
        }
    }
    spec.active_preset_id = Some(preset_id.to_string());
    Ok(())
}
