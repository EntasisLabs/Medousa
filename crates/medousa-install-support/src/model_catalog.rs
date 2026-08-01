use serde::{Deserialize, Serialize};
pub use medousa_types::local::CatalogModelEntry;

const BUILTIN_CATALOG_JSON: &str = include_str!("catalog/v2.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFile {
    pub catalog_version: String,
    pub family_default: String,
    pub models: Vec<CatalogModelEntry>,
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
