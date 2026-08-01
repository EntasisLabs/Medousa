use medousa_types::local::{
    CatalogModelEntry, HardwareProbe, HardwareTier, LocalResourceAdmission,
};

pub const SAFE_MAX_SEQ_LEN: usize = 4 * 1024;
pub const SAFE_MAX_BATCH_SIZE: usize = 1;
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 5 * 60;
const MIN_SYSTEM_RESERVE_MB: u64 = 4 * 1024;
const MIB: u64 = 1024 * 1024;

pub fn critical_available_mb(total_ram_mb: u64) -> u64 {
    1024.max(total_ram_mb / 20)
}

pub fn system_reserve_mb(total_ram_mb: u64) -> u64 {
    MIN_SYSTEM_RESERVE_MB.max(total_ram_mb / 4)
}

pub fn tier_recipe_cap_mb(tier: HardwareTier) -> u64 {
    match tier {
        HardwareTier::A => 3 * 1024,
        HardwareTier::B => 7 * 1024,
        HardwareTier::C => 12 * 1024,
        HardwareTier::D => 20 * 1024,
        HardwareTier::E => 32 * 1024,
    }
}

fn requires_in_situ_conversion(entry: &CatalogModelEntry) -> bool {
    entry
        .engine_args
        .get("uqffFile")
        .and_then(|value| value.as_str())
        .is_none()
}

pub fn evaluate_model_admission(
    entry: &CatalogModelEntry,
    probe: &HardwareProbe,
) -> LocalResourceAdmission {
    let tier = super::hardware::score_tier(probe);
    let reserve_mb = system_reserve_mb(probe.total_ram_mb);
    let host_headroom_mb = probe.available_ram_mb.saturating_sub(reserve_mb);
    let tier_cap_mb = tier_recipe_cap_mb(tier);
    let admissible_mb = host_headroom_mb.min(tier_cap_mb);
    let artifact_mb = entry.size_bytes.div_ceil(MIB);
    let estimated_steady_mb = entry.ram_estimate_mb.max(artifact_mb);
    let estimated_conversion_mb = if requires_in_situ_conversion(entry) {
        artifact_mb
    } else {
        0
    };
    let allocator_slack_mb = estimated_steady_mb / 8 + 512;
    let estimated_peak_mb = estimated_steady_mb
        .saturating_add(estimated_conversion_mb)
        .saturating_add(allocator_slack_mb);
    let admitted = estimated_peak_mb <= admissible_mb;
    let critical_available_mb = critical_available_mb(probe.total_ram_mb);
    let rationale = if admitted {
        format!(
            "Estimated peak {estimated_peak_mb} MiB fits the {admissible_mb} MiB safe envelope (reserve {reserve_mb} MiB)"
        )
    } else {
        format!(
            "Refusing to load {}: estimated peak {} MiB exceeds the {} MiB safe envelope; {} MiB remains reserved for the OS and other apps",
            entry.id, estimated_peak_mb, admissible_mb, reserve_mb
        )
    };

    LocalResourceAdmission {
        admitted,
        model_id: entry.id.clone(),
        hardware_tier: tier,
        total_ram_mb: probe.total_ram_mb,
        available_ram_mb: probe.available_ram_mb,
        system_reserve_mb: reserve_mb,
        tier_cap_mb,
        admissible_mb,
        estimated_steady_mb,
        estimated_conversion_mb,
        estimated_peak_mb,
        critical_available_mb,
        max_seq_len: SAFE_MAX_SEQ_LEN,
        max_batch_size: SAFE_MAX_BATCH_SIZE,
        rationale,
    }
}

pub fn admission_for_model_id(model_id: &str) -> Result<LocalResourceAdmission, String> {
    let catalog = super::catalog::builtin_catalog();
    let entry = catalog
        .models
        .iter()
        .find(|entry| entry.id.eq_ignore_ascii_case(model_id.trim()))
        .ok_or_else(|| format!("unknown catalog model id: {}", model_id.trim()))?;
    Ok(evaluate_model_admission(
        entry,
        &super::hardware::probe_hardware(),
    ))
}

pub fn recommended_admitted_model(probe: &HardwareProbe) -> Option<CatalogModelEntry> {
    let tier = super::hardware::score_tier(probe);
    super::catalog::filter_catalog_for_tier(&super::catalog::builtin_catalog(), tier)
        .into_iter()
        .filter(|entry| evaluate_model_admission(entry, probe).admitted)
        .max_by_key(|entry| entry.ram_estimate_mb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::local::GpuBackend;

    fn probe(total_gb: u64, available_gb: u64) -> HardwareProbe {
        HardwareProbe {
            total_ram_mb: total_gb * 1024,
            available_ram_mb: available_gb * 1024,
            cpu_cores: 8,
            cpu_arch: "aarch64".to_string(),
            gpu_backend: GpuBackend::Metal,
            free_disk_gb: 100,
        }
    }

    fn entry(id: &str, size_gb: u64, steady_gb: u64, prequantized: bool) -> CatalogModelEntry {
        CatalogModelEntry {
            id: id.to_string(),
            display_name: id.to_string(),
            family: "test".to_string(),
            variant: "test".to_string(),
            tier_min: "A".to_string(),
            tier_max: "E".to_string(),
            tier_recommended: false,
            format: "uqff".to_string(),
            source: "test".to_string(),
            repo: "test/model".to_string(),
            engine: "mistralrs".to_string(),
            engine_args: if prequantized {
                serde_json::json!({ "uqffFile": "model.uqff" })
            } else {
                serde_json::json!({ "fromUqff": 4 })
            },
            fallback: None,
            size_bytes: size_gb * 1024 * 1024 * 1024,
            context_length: 128 * 1024,
            ram_estimate_mb: steady_gb * 1024,
            modalities: vec!["text".to_string()],
            license: "test".to_string(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn reserves_at_least_four_gib_for_the_system() {
        assert_eq!(system_reserve_mb(8 * 1024), 4 * 1024);
        assert_eq!(system_reserve_mb(64 * 1024), 16 * 1024);
        assert_eq!(critical_available_mb(16 * 1024), 1024);
    }

    #[test]
    fn rejects_in_situ_conversion_that_would_overcommit_unified_memory() {
        let admission = evaluate_model_admission(&entry("large", 7, 14, false), &probe(16, 13));
        assert!(!admission.admitted);
        assert!(admission.estimated_conversion_mb > 0);
    }

    #[test]
    fn admits_small_prequantized_model_inside_envelope() {
        let admission = evaluate_model_admission(&entry("small", 1, 2, true), &probe(16, 13));
        assert!(admission.admitted);
        assert_eq!(admission.estimated_conversion_mb, 0);
        assert_eq!(admission.max_seq_len, 4096);
        assert_eq!(admission.max_batch_size, 1);
    }

    #[test]
    fn recommendation_falls_back_before_overcommitting_machine() {
        let selected = recommended_admitted_model(&probe(16, 13)).expect("safe fallback");
        assert_eq!(selected.id, "gemma-4-e2b-it-qat");
    }
}
