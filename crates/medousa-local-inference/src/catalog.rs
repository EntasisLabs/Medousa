use super::hardware::HardwareTier;
pub use medousa_install_support::model_catalog::{
    builtin_catalog, CatalogFile, CatalogModelEntry,
};

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
    use crate::hardware::HardwareTier;

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
