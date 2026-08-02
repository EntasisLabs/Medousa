use medousa_types::local::{
    CatalogModelEntry, GpuBackend, HardwareProbe, HardwareTier, LocalDeviceTelemetrySnapshot,
    LocalResourceAdmission,
};

pub const SAFE_MAX_SEQ_LEN: usize = 4 * 1024;
pub const SAFE_MAX_BATCH_SIZE: usize = 1;
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 5 * 60;
const MIN_SYSTEM_RESERVE_MB: u64 = 4 * 1024;
const MIB: u64 = 1024 * 1024;
const MIN_DEVICE_RESERVE_MB: u64 = 512;

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
    evaluate_model_admission_with_devices(entry, probe, &[])
}

pub fn evaluate_model_admission_with_devices(
    entry: &CatalogModelEntry,
    probe: &HardwareProbe,
    devices: &[LocalDeviceTelemetrySnapshot],
) -> LocalResourceAdmission {
    evaluate_model_admission_with_calibration(entry, probe, devices, None)
}

pub fn evaluate_model_admission_with_calibration(
    entry: &CatalogModelEntry,
    probe: &HardwareProbe,
    devices: &[LocalDeviceTelemetrySnapshot],
    calibration: Option<&super::calibration::PeakCalibration>,
) -> LocalResourceAdmission {
    let tier = super::hardware::score_tier(probe);
    let reserve_mb = system_reserve_mb(probe.total_ram_mb);
    let host_headroom_mb = probe.available_ram_mb.saturating_sub(reserve_mb);
    let tier_cap_mb = tier_recipe_cap_mb(tier);
    let host_admissible_mb = host_headroom_mb.min(tier_cap_mb);
    let artifact_mb = entry.size_bytes.div_ceil(MIB);
    let estimated_steady_mb = entry.ram_estimate_mb.max(artifact_mb);
    let estimated_conversion_mb = if requires_in_situ_conversion(entry) {
        artifact_mb
    } else {
        0
    };
    let allocator_slack_mb = estimated_steady_mb / 8 + 512;
    let static_estimated_peak_mb = estimated_steady_mb
        .saturating_add(estimated_conversion_mb)
        .saturating_add(allocator_slack_mb);
    let estimated_peak_mb = calibration
        .map(super::calibration::PeakCalibration::padded_host_peak_mb)
        .unwrap_or_default()
        .max(static_estimated_peak_mb);
    let device_estimated_peak_mb = calibration
        .and_then(super::calibration::PeakCalibration::padded_device_peak_mb)
        .unwrap_or(static_estimated_peak_mb)
        .max(static_estimated_peak_mb);
    let device = device_envelope(probe.gpu_backend, devices, device_estimated_peak_mb);
    let admissible_mb = device
        .as_ref()
        .map(|device| host_admissible_mb.min(device.admissible_mb))
        .unwrap_or(host_admissible_mb);
    let admitted = estimated_peak_mb <= host_admissible_mb
        && device
            .as_ref()
            .is_none_or(|device| device.estimated_peak_mb <= device.admissible_mb);
    let critical_available_mb = critical_available_mb(probe.total_ram_mb);
    let device_rationale = device.as_ref().map(DeviceEnvelope::rationale);
    let rationale = if admitted {
        if let Some(device) = device.as_ref() {
            format!(
                "Estimated host peak {estimated_peak_mb} MiB fits the {host_admissible_mb} MiB host envelope, and estimated device peak {} MiB fits the {} MiB {} device envelope",
                device.estimated_peak_mb,
                device.admissible_mb,
                device.name.as_deref().unwrap_or("selected")
            )
        } else {
            format!(
                "Estimated peak {estimated_peak_mb} MiB fits the {host_admissible_mb} MiB host envelope (reserve {reserve_mb} MiB); device counters unavailable, so admission remains host-only"
            )
        }
    } else {
        let (limiting_envelope, refused_peak_mb) = if estimated_peak_mb > host_admissible_mb {
            (
                format!("{host_admissible_mb} MiB host envelope"),
                estimated_peak_mb,
            )
        } else if let Some(device) = device.as_ref() {
            (
                format!(
                    "{} MiB {} device envelope",
                    device.admissible_mb,
                    device.name.as_deref().unwrap_or("selected")
                ),
                device.estimated_peak_mb,
            )
        } else {
            (
                format!("{admissible_mb} MiB safe envelope"),
                estimated_peak_mb,
            )
        };
        format!(
            "Refusing to load {}: estimated peak {} MiB exceeds the {}; {} MiB remains reserved for the OS and other apps",
            entry.id, refused_peak_mb, limiting_envelope, reserve_mb
        )
    };
    let rationale = if let Some(calibration) = calibration {
        format!(
            "{rationale}; calibration uses {} content-free sample(s), a {}% margin, and fixed allocator slack without lowering the static {} MiB estimate",
            calibration.sample_count,
            super::calibration::CALIBRATION_MARGIN_PERCENT,
            static_estimated_peak_mb
        )
    } else {
        rationale
    };

    LocalResourceAdmission {
        admitted,
        model_id: entry.id.clone(),
        hardware_tier: tier,
        total_ram_mb: probe.total_ram_mb,
        available_ram_mb: probe.available_ram_mb,
        system_reserve_mb: reserve_mb,
        tier_cap_mb,
        host_admissible_mb,
        admissible_mb,
        estimated_steady_mb,
        estimated_conversion_mb,
        static_estimated_peak_mb,
        estimated_peak_mb,
        calibration_applied: calibration.is_some(),
        calibration_sample_count: calibration.map(|value| value.sample_count).unwrap_or(0),
        calibration_observed_host_peak_mb: calibration.map(|value| value.observed_host_peak_mb),
        calibration_observed_device_peak_mb: calibration
            .and_then(|value| value.observed_device_peak_mb),
        calibration_margin_percent: calibration
            .map(|_| super::calibration::CALIBRATION_MARGIN_PERCENT),
        critical_available_mb,
        max_seq_len: SAFE_MAX_SEQ_LEN,
        max_batch_size: SAFE_MAX_BATCH_SIZE,
        device_enforced: device.is_some(),
        device_source: device.as_ref().map(|device| device.source),
        device_backend: device.as_ref().map(|device| device.backend),
        device_index: device.as_ref().and_then(|device| device.index),
        device_uuid: device.as_ref().and_then(|device| device.uuid.clone()),
        device_name: device.as_ref().and_then(|device| device.name.clone()),
        device_total_mb: device.as_ref().and_then(|device| device.total_mb),
        device_budget_mb: device.as_ref().and_then(|device| device.budget_mb),
        device_available_mb: device.as_ref().and_then(|device| device.available_mb),
        device_reserve_mb: device.as_ref().map(|device| device.reserve_mb),
        device_admissible_mb: device.as_ref().map(|device| device.admissible_mb),
        device_estimated_peak_mb: device.as_ref().map(|device| device.estimated_peak_mb),
        device_rationale,
        rationale,
    }
}

struct DeviceEnvelope {
    source: medousa_types::local::LocalDeviceTelemetrySource,
    backend: GpuBackend,
    index: Option<u32>,
    uuid: Option<String>,
    name: Option<String>,
    total_mb: Option<u64>,
    budget_mb: Option<u64>,
    available_mb: Option<u64>,
    reserve_mb: u64,
    admissible_mb: u64,
    estimated_peak_mb: u64,
}

impl DeviceEnvelope {
    fn rationale(&self) -> String {
        format!(
            "{} via {:?} reports {} available; reserving {} MiB leaves {} MiB for an estimated {} MiB peak",
            self.name.as_deref().unwrap_or("Selected device"),
            self.source,
            self.available_mb
                .map(|value| format!("{value} MiB"))
                .unwrap_or_else(|| "no trustworthy remaining budget".to_string()),
            self.reserve_mb,
            self.admissible_mb,
            self.estimated_peak_mb
        )
    }
}

fn device_envelope(
    backend: GpuBackend,
    devices: &[LocalDeviceTelemetrySnapshot],
    estimated_peak_mb: u64,
) -> Option<DeviceEnvelope> {
    if backend == GpuBackend::None {
        return None;
    }
    let device = select_device_snapshot(backend, devices)?;
    let total_mb = device.memory_total_mb;
    let budget_mb = device
        .recommended_working_set_mb
        .or(device.memory_budget_mb);
    let capacity_mb = budget_mb.or(total_mb);
    let available_mb = if let Some(budget_mb) = budget_mb {
        device
            .process_memory_used_mb
            .map(|used_mb| budget_mb.saturating_sub(used_mb))
    } else {
        device.memory_free_mb.or_else(|| {
            Some(total_mb?.saturating_sub(device.memory_used_mb.or(device.process_memory_used_mb)?))
        })
    };
    if budget_mb.is_none() && available_mb.is_none() {
        return None;
    }
    let reserve_mb = capacity_mb
        .map(|total| MIN_DEVICE_RESERVE_MB.max(total / 10))
        .unwrap_or(MIN_DEVICE_RESERVE_MB);
    Some(DeviceEnvelope {
        source: device.source,
        backend: device.backend,
        index: device.device_index,
        uuid: device.device_uuid.clone(),
        name: device.device_name.clone(),
        total_mb,
        budget_mb,
        available_mb,
        reserve_mb,
        admissible_mb: available_mb.unwrap_or_default().saturating_sub(reserve_mb),
        estimated_peak_mb,
    })
}

fn select_device_snapshot(
    backend: GpuBackend,
    devices: &[LocalDeviceTelemetrySnapshot],
) -> Option<&LocalDeviceTelemetrySnapshot> {
    devices
        .iter()
        .filter(|device| device.backend == backend && device.collector_error.is_none())
        .min_by_key(|device| {
            (
                device.device_index.unwrap_or(u32::MAX),
                telemetry_source_rank(device.source),
            )
        })
}

fn telemetry_source_rank(source: medousa_types::local::LocalDeviceTelemetrySource) -> u8 {
    use medousa_types::local::LocalDeviceTelemetrySource;
    match source {
        LocalDeviceTelemetrySource::MetalApi
        | LocalDeviceTelemetrySource::Nvml
        | LocalDeviceTelemetrySource::AmdSmiLibrary
        | LocalDeviceTelemetrySource::Wddm
        | LocalDeviceTelemetrySource::VulkanBudget => 0,
        LocalDeviceTelemetrySource::NvidiaSmi | LocalDeviceTelemetrySource::AmdSmi => 1,
    }
}

pub fn admission_for_model_id(model_id: &str) -> Result<LocalResourceAdmission, String> {
    let catalog = super::catalog::builtin_catalog();
    let entry = catalog
        .models
        .iter()
        .find(|entry| entry.id.eq_ignore_ascii_case(model_id.trim()))
        .ok_or_else(|| format!("unknown catalog model id: {}", model_id.trim()))?;
    let probe = super::hardware::probe_hardware();
    let devices = super::telemetry::collect_device_telemetry();
    let calibration = super::calibration::calibration_for(entry, &probe, &devices)?;
    Ok(evaluate_model_admission_with_calibration(
        entry,
        &probe,
        &devices,
        calibration.as_ref(),
    ))
}

pub fn recommended_model_admission() -> Result<LocalResourceAdmission, String> {
    let probe = super::hardware::probe_hardware();
    let devices = super::telemetry::collect_device_telemetry();
    let tier = super::hardware::score_tier(&probe);
    let mut candidates =
        super::catalog::filter_catalog_for_tier(&super::catalog::builtin_catalog(), tier);
    candidates.sort_by_key(|entry| std::cmp::Reverse(entry.ram_estimate_mb));
    let mut selected = None;
    for entry in candidates {
        let calibration = super::calibration::calibration_for(&entry, &probe, &devices)?;
        let admission = evaluate_model_admission_with_calibration(
            &entry,
            &probe,
            &devices,
            calibration.as_ref(),
        );
        if admission.admitted {
            selected = Some(admission);
            break;
        }
    }
    selected.ok_or_else(|| {
        format!(
            "no local model fits the safe envelope for hardware tier {}",
            tier.as_str()
        )
    })
}

pub fn recommended_admitted_model(probe: &HardwareProbe) -> Option<CatalogModelEntry> {
    recommended_admitted_model_with_devices(probe, &[])
}

pub fn recommended_admitted_model_with_devices(
    probe: &HardwareProbe,
    devices: &[LocalDeviceTelemetrySnapshot],
) -> Option<CatalogModelEntry> {
    let tier = super::hardware::score_tier(probe);
    super::catalog::filter_catalog_for_tier(&super::catalog::builtin_catalog(), tier)
        .into_iter()
        .filter(|entry| evaluate_model_admission_with_devices(entry, probe, devices).admitted)
        .max_by_key(|entry| entry.ram_estimate_mb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medousa_types::local::{LocalDeviceTelemetryAvailability, LocalDeviceTelemetrySource};

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

    fn device(
        backend: GpuBackend,
        index: u32,
        total_mb: u64,
        free_mb: Option<u64>,
    ) -> LocalDeviceTelemetrySnapshot {
        LocalDeviceTelemetrySnapshot {
            captured_at: Utc::now(),
            source: match backend {
                GpuBackend::Metal => LocalDeviceTelemetrySource::MetalApi,
                GpuBackend::Cuda => LocalDeviceTelemetrySource::NvidiaSmi,
                _ => LocalDeviceTelemetrySource::AmdSmi,
            },
            availability: LocalDeviceTelemetryAvailability::Partial,
            backend,
            device_index: Some(index),
            device_uuid: None,
            device_name: Some(format!("test-{index}")),
            driver_version: None,
            runtime_version: None,
            unified_memory: Some(backend == GpuBackend::Metal),
            memory_total_mb: Some(total_mb),
            memory_budget_mb: None,
            memory_used_mb: free_mb.map(|free| total_mb.saturating_sub(free)),
            memory_free_mb: free_mb,
            process_memory_used_mb: (backend == GpuBackend::Metal)
                .then(|| free_mb.map(|free| total_mb.saturating_sub(free)))
                .flatten(),
            recommended_working_set_mb: (backend == GpuBackend::Metal).then_some(total_mb),
            utilization_percent: None,
            power_watts: None,
            temperature_c: None,
            graphics_clock_mhz: None,
            memory_clock_mhz: None,
            throttle_reasons: None,
            unavailable_fields: Vec::new(),
            collector_error: None,
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

    #[test]
    fn metal_working_set_can_reject_a_host_safe_model() {
        let admission = evaluate_model_admission_with_devices(
            &entry("small", 1, 2, true),
            &probe(16, 13),
            &[device(GpuBackend::Metal, 0, 8 * 1024, Some(2 * 1024))],
        );
        assert!(!admission.admitted);
        assert!(admission.device_enforced);
        assert_eq!(admission.device_admissible_mb, Some(1229));
        assert!(admission.rationale.contains("device envelope"));
    }

    #[test]
    fn missing_device_memory_keeps_admission_explicitly_host_only() {
        let mut incomplete = device(GpuBackend::Metal, 0, 8 * 1024, None);
        incomplete.recommended_working_set_mb = None;
        let admission = evaluate_model_admission_with_devices(
            &entry("small", 1, 2, true),
            &probe(16, 13),
            &[incomplete],
        );
        assert!(admission.admitted);
        assert!(!admission.device_enforced);
        assert!(admission.rationale.contains("host-only"));
    }

    #[test]
    fn dynamic_budget_without_process_usage_fails_closed() {
        let mut directml_probe = probe(32, 24);
        directml_probe.gpu_backend = GpuBackend::DirectMl;
        let mut incomplete = device(GpuBackend::DirectMl, 0, 24 * 1024, Some(20 * 1024));
        incomplete.source = LocalDeviceTelemetrySource::Wddm;
        incomplete.memory_budget_mb = Some(8 * 1024);
        incomplete.process_memory_used_mb = None;
        let admission = evaluate_model_admission_with_devices(
            &entry("small", 1, 2, true),
            &directml_probe,
            &[incomplete],
        );
        assert!(!admission.admitted);
        assert_eq!(admission.device_available_mb, None);
    }

    #[test]
    fn discrete_admission_uses_device_zero_instead_of_a_larger_secondary_gpu() {
        let mut cuda_probe = probe(32, 24);
        cuda_probe.gpu_backend = GpuBackend::Cuda;
        let admission = evaluate_model_admission_with_devices(
            &entry("small", 1, 2, true),
            &cuda_probe,
            &[
                device(GpuBackend::Cuda, 1, 24 * 1024, Some(20 * 1024)),
                device(GpuBackend::Cuda, 0, 4 * 1024, Some(2 * 1024)),
            ],
        );
        assert!(!admission.admitted);
        assert_eq!(admission.device_name.as_deref(), Some("test-0"));
    }

    #[test]
    fn calibration_never_lowers_the_static_safety_estimate() {
        let model = entry("small", 1, 2, true);
        let baseline = evaluate_model_admission(&model, &probe(16, 13));
        let calibration = super::super::calibration::PeakCalibration {
            sample_count: 3,
            observed_host_peak_mb: 128,
            observed_device_peak_mb: None,
        };
        let calibrated = evaluate_model_admission_with_calibration(
            &model,
            &probe(16, 13),
            &[],
            Some(&calibration),
        );
        assert_eq!(calibrated.estimated_peak_mb, baseline.estimated_peak_mb);
        assert!(calibrated.calibration_applied);
        assert_eq!(calibrated.calibration_sample_count, 3);
    }

    #[test]
    fn observed_high_water_can_reject_a_static_fit() {
        let model = entry("small", 1, 2, true);
        let calibration = super::super::calibration::PeakCalibration {
            sample_count: 2,
            observed_host_peak_mb: 9 * 1024,
            observed_device_peak_mb: None,
        };
        let admission = evaluate_model_admission_with_calibration(
            &model,
            &probe(16, 13),
            &[],
            Some(&calibration),
        );
        assert!(!admission.admitted);
        assert!(admission.estimated_peak_mb > admission.static_estimated_peak_mb);
        assert_eq!(admission.calibration_margin_percent, Some(15));
    }

    #[test]
    fn dynamic_process_budget_wins_over_physical_vram_free() {
        let mut directml_probe = probe(32, 24);
        directml_probe.gpu_backend = GpuBackend::DirectMl;
        let mut budget = device(GpuBackend::DirectMl, 0, 24 * 1024, Some(20 * 1024));
        budget.source = LocalDeviceTelemetrySource::Wddm;
        budget.memory_budget_mb = Some(4 * 1024);
        budget.process_memory_used_mb = Some(1024);
        let admission = evaluate_model_admission_with_devices(
            &entry("small", 1, 2, true),
            &directml_probe,
            &[budget],
        );
        assert!(!admission.admitted);
        assert_eq!(admission.device_budget_mb, Some(4 * 1024));
        assert_eq!(admission.device_available_mb, Some(3 * 1024));
        assert_eq!(
            admission.device_source,
            Some(LocalDeviceTelemetrySource::Wddm)
        );
    }

    #[test]
    fn native_collector_precedes_cli_for_the_same_device() {
        let mut cuda_probe = probe(32, 24);
        cuda_probe.gpu_backend = GpuBackend::Cuda;
        let cli = device(GpuBackend::Cuda, 0, 24 * 1024, Some(20 * 1024));
        let mut native = device(GpuBackend::Cuda, 0, 24 * 1024, Some(2 * 1024));
        native.source = LocalDeviceTelemetrySource::Nvml;
        let admission = evaluate_model_admission_with_devices(
            &entry("small", 1, 2, true),
            &cuda_probe,
            &[cli, native],
        );
        assert!(!admission.admitted);
        assert_eq!(
            admission.device_source,
            Some(LocalDeviceTelemetrySource::Nvml)
        );
    }

    #[cfg(all(target_os = "macos", feature = "telemetry-metal"))]
    #[test]
    fn live_metal_admission_records_the_device_envelope_without_loading_a_model() {
        let probe = super::super::hardware::probe_hardware();
        let devices = super::super::telemetry::collect_device_telemetry();
        let model = super::super::catalog::builtin_catalog()
            .models
            .into_iter()
            .find(|entry| entry.id == "gemma-4-e2b-it-qat")
            .expect("small catalog model");
        let admission = evaluate_model_admission_with_devices(&model, &probe, &devices);
        assert!(admission.device_enforced);
        assert_eq!(
            admission.device_source,
            Some(LocalDeviceTelemetrySource::MetalApi)
        );
        assert!(admission.device_admissible_mb.is_some());
        assert!(admission.device_rationale.is_some());
    }
}
