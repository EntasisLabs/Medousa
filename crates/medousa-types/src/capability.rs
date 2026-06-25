use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBindingSummary {
    pub source: String,
    pub reference: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoke_via: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityListEntry {
    pub id: String,
    pub title: String,
    pub binding_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub domain: String,
    pub has_grapheme: bool,
    pub has_mcp: bool,
    #[serde(default)]
    pub bindings_summary: Vec<CapabilityBindingSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityListResponse {
    pub capabilities: Vec<CapabilityListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub source: String,
    pub reference: String,
    pub priority: u16,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoke_via: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityImplementations {
    #[serde(default)]
    pub grapheme: Vec<CapabilityBinding>,
    #[serde(default)]
    pub mcp: Vec<CapabilityBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRecommendation {
    pub source: String,
    pub reference: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResolveResponse {
    pub capability: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub implementations: CapabilityImplementations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<CapabilityRecommendation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_unreachable: Option<bool>,
}
