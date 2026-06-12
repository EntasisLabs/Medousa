use super::hardware::HardwareTier;
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

fn parse_tier(value: &str) -> Option<HardwareTier> {
    match value.trim().to_ascii_uppercase().as_str() {
        "A" => Some(HardwareTier::A),
        "B" => Some(HardwareTier::B),
        "C" => Some(HardwareTier::C),
        "D" => Some(HardwareTier::D),
        "E" => Some(HardwareTier::E),
        _ => None,
    }
}

fn tier_in_range(entry: &CatalogModelEntry, tier: HardwareTier) -> bool {
    let min = parse_tier(&entry.tier_min).unwrap_or(HardwareTier::A);
    let max = parse_tier(&entry.tier_max).unwrap_or(HardwareTier::E);
    tier >= min && tier <= max
}

pub fn filter_catalog_for_tier(catalog: &CatalogFile, tier: HardwareTier) -> Vec<CatalogModelEntry> {
    catalog
        .models
        .iter()
        .filter(|entry| tier_in_range(entry, tier))
        .cloned()
        .collect()
}

pub fn recommended_model_for_tier(tier: HardwareTier) -> Option<CatalogModelEntry> {
    let catalog = builtin_catalog();
    let eligible = filter_catalog_for_tier(&catalog, tier);

    eligible
        .iter()
        .find(|entry| entry.tier_recommended)
        .or_else(|| {
            eligible
                .iter()
                .max_by_key(|entry| parse_tier(&entry.tier_max).unwrap_or(HardwareTier::A))
        })
        .cloned()
        .or_else(|| catalog.models.first().cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_inference::hardware::HardwareTier;

    #[test]
    fn builtin_catalog_parses() {
        let catalog = builtin_catalog();
        assert_eq!(catalog.catalog_version, "2");
        assert_eq!(catalog.family_default, "gemma-4");
        assert!(catalog.models.iter().any(|entry| entry.id == "gemma-4-12b-it"));
    }

    #[test]
    fn tier_c_gets_12b_recommended() {
        let recommended = recommended_model_for_tier(HardwareTier::C).expect("recommended");
        assert_eq!(recommended.id, "gemma-4-12b-it");
    }

    #[test]
    fn tier_a_only_small_models() {
        let models = filter_catalog_for_tier(&builtin_catalog(), HardwareTier::A);
        assert!(models.iter().all(|entry| entry.id.contains("e2b")));
    }
}
