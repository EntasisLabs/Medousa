use serde::{Deserialize, Serialize};
use serde_json::Value;

const BUILTIN_CATALOG_JSON: &str = include_str!("catalog/v2.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFile {
    pub catalog_version: String,
    pub family_default: String,
    pub models: Vec<CatalogModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModelEntry {
    pub id: String,
    pub display_name: String,
    pub family: String,
    pub variant: String,
    pub tier_min: String,
    pub tier_max: String,
    #[serde(default)]
    pub tier_recommended: bool,
    pub format: String,
    pub source: String,
    pub repo: String,
    pub engine: String,
    #[serde(default)]
    pub engine_args: Value,
    #[serde(default)]
    pub fallback: Option<Value>,
    pub size_bytes: u64,
    pub context_length: u64,
    pub ram_estimate_mb: u64,
    pub modalities: Vec<String>,
    pub license: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

pub fn builtin_catalog() -> CatalogFile {
    serde_json::from_str(BUILTIN_CATALOG_JSON).expect("builtin catalog v2.json must parse")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_parses() {
        let catalog = builtin_catalog();
        assert_eq!(catalog.catalog_version, "2");
        assert_eq!(catalog.family_default, "gemma-4");
        assert!(catalog.models.iter().any(|entry| entry.id == "gemma-4-12b-it"));
    }
}
