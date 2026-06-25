use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProviderFreshness {
    pub provider: String,
    pub fetched_at: Option<DateTime<Utc>>,
    pub model_count: usize,
    pub source: String,
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFreshnessResponse {
    pub ttl_secs: u64,
    pub providers: Vec<CatalogProviderFreshness>,
}
