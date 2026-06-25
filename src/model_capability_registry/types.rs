use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const DEFAULT_CATALOG_TTL_SECS: u64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Audio,
    File,
    Video,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelPricing {
    pub prompt_per_token_usd: Option<f64>,
    pub completion_per_token_usd: Option<f64>,
    pub image_per_unit_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCapabilityRecord {
    pub provider: String,
    pub model_id: String,
    pub display_name: Option<String>,
    pub input_modalities: Vec<Modality>,
    pub output_modalities: Vec<Modality>,
    pub max_input_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub supports_tool_calling: Option<bool>,
    pub supports_vision: bool,
    pub pricing: Option<ModelPricing>,
    pub source: String,
    pub fetched_at: DateTime<Utc>,
}

impl ModelCapabilityRecord {
    pub fn model_key(provider: &str, model_id: &str) -> String {
        format!(
            "{}:{}",
            provider.trim().to_ascii_lowercase(),
            model_id.trim()
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogIndexEntry {
    pub fetched_at: Option<DateTime<Utc>>,
    pub model_count: usize,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogIndex {
    pub ttl_secs: u64,
    pub providers: std::collections::HashMap<String, CatalogIndexEntry>,
}

impl Default for CatalogIndex {
    fn default() -> Self {
        Self {
            ttl_secs: DEFAULT_CATALOG_TTL_SECS,
            providers: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCatalogSnapshot {
    pub provider: String,
    pub fetched_at: DateTime<Utc>,
    pub source: String,
    pub models: Vec<ModelCapabilityRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub use medousa_types::model_catalog::{CatalogFreshnessResponse, CatalogProviderFreshness};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogListResponse {
    pub freshness: CatalogFreshnessResponse,
    pub models: Vec<ModelCapabilityRecord>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogListQuery {
    pub provider: Option<String>,
    pub capability: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilitiesLookupQuery {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilitiesLookupResponse {
    pub found: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelCapabilityRecord>,
    pub heuristic: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogRefreshRequest {
    pub providers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogRefreshFailure {
    pub provider: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogRefreshResponse {
    pub refreshed: Vec<String>,
    pub failures: Vec<ModelCatalogRefreshFailure>,
    pub freshness: CatalogFreshnessResponse,
}
