use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use medousa_types::local::{
    CatalogModelEntry, GpuBackend, HardwareProbe, LocalBenchmarkArtifactMode,
    LocalBenchmarkManifest, LocalBenchmarkOutcome, LocalBenchmarkPhase,
    LocalDeviceTelemetrySnapshot,
};
use serde::{Deserialize, Serialize};

pub const CALIBRATION_MARGIN_PERCENT: u8 = 15;
const CALIBRATION_FIXED_SLACK_MB: u64 = 256;
const CALIBRATION_SCHEMA_VERSION: u32 = 1;
const CALIBRATION_FILE: &str = "local-inference-peak-calibrations.json";
const CALIBRATION_LOCK_FILE: &str = "local-inference-peak-calibrations.lock";
const RUNTIME_NAME: &str = "mistral.rs";
const RUNTIME_VERSION: &str = "0.8.1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalibrationIdentity {
    model_id: String,
    runtime_name: String,
    runtime_version: String,
    artifact_mode: LocalBenchmarkArtifactMode,
    quantization: Option<String>,
    cpu_only: bool,
    max_seq_len: usize,
    max_batch_size: usize,
    cpu_arch: String,
    gpu_backend: GpuBackend,
    device_uuid: Option<String>,
    device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalibrationRecord {
    identity: CalibrationIdentity,
    sample_count: u32,
    observed_host_peak_mb: u64,
    observed_device_peak_mb: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalibrationStore {
    schema_version: u32,
    #[serde(default)]
    records: Vec<CalibrationRecord>,
}

impl Default for CalibrationStore {
    fn default() -> Self {
        Self {
            schema_version: CALIBRATION_SCHEMA_VERSION,
            records: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PeakCalibration {
    pub sample_count: u32,
    pub observed_host_peak_mb: u64,
    pub observed_device_peak_mb: Option<u64>,
}

impl PeakCalibration {
    pub fn padded_host_peak_mb(&self) -> u64 {
        padded_peak(self.observed_host_peak_mb)
    }

    pub fn padded_device_peak_mb(&self) -> Option<u64> {
        self.observed_device_peak_mb.map(padded_peak)
    }
}

pub fn calibration_for(
    entry: &CatalogModelEntry,
    probe: &HardwareProbe,
    devices: &[LocalDeviceTelemetrySnapshot],
) -> Result<Option<PeakCalibration>, String> {
    let identity = identity_for_entry(entry, probe, devices);
    let store = read_store(&calibration_path())?;
    Ok(store
        .records
        .iter()
        .find(|record| record.identity == identity)
        .map(|record| PeakCalibration {
            sample_count: record.sample_count,
            observed_host_peak_mb: record.observed_host_peak_mb,
            observed_device_peak_mb: record.observed_device_peak_mb,
        }))
}

pub fn record_benchmark_calibration(
    manifest: &LocalBenchmarkManifest,
) -> Result<Option<PeakCalibration>, String> {
    if manifest.schema_version != 1
        || manifest.result.outcome != LocalBenchmarkOutcome::Completed
        || manifest.result.error.is_some()
    {
        return Ok(None);
    }
    let Some((host_peak, device_peak)) = observed_peaks(manifest) else {
        return Ok(None);
    };
    let identity = identity_from_manifest(manifest);
    let path = calibration_path();
    let result = update_store(&path, |store| {
        let record = if let Some(record) = store
            .records
            .iter_mut()
            .find(|record| record.identity == identity)
        {
            record.sample_count = record.sample_count.saturating_add(1);
            record.observed_host_peak_mb = record.observed_host_peak_mb.max(host_peak);
            record.observed_device_peak_mb = match (record.observed_device_peak_mb, device_peak) {
                (Some(previous), Some(current)) => Some(previous.max(current)),
                (previous, current) => previous.or(current),
            };
            record.clone()
        } else {
            let record = CalibrationRecord {
                identity,
                sample_count: 1,
                observed_host_peak_mb: host_peak,
                observed_device_peak_mb: device_peak,
            };
            store.records.push(record.clone());
            record
        };
        Ok(PeakCalibration {
            sample_count: record.sample_count,
            observed_host_peak_mb: record.observed_host_peak_mb,
            observed_device_peak_mb: record.observed_device_peak_mb,
        })
    })?;
    Ok(Some(result))
}

fn observed_peaks(manifest: &LocalBenchmarkManifest) -> Option<(u64, Option<u64>)> {
    let host_peak = observed_host_peak(&manifest.samples)?;
    let device_peak = observed_device_peak(&manifest.samples, &manifest.admission);
    Some((host_peak, device_peak))
}

fn active_samples(
    samples: &[medousa_types::local::LocalBenchmarkMemorySample],
) -> Vec<&medousa_types::local::LocalBenchmarkMemorySample> {
    samples
        .iter()
        .filter(|sample| {
            matches!(
                sample.phase,
                LocalBenchmarkPhase::BeforeLoad
                    | LocalBenchmarkPhase::AfterLoad
                    | LocalBenchmarkPhase::AfterStream
            )
        })
        .collect()
}

fn observed_host_peak(samples: &[medousa_types::local::LocalBenchmarkMemorySample]) -> Option<u64> {
    let baseline = samples
        .iter()
        .find(|sample| sample.phase == LocalBenchmarkPhase::BeforeLoad)?;
    let active = active_samples(samples);
    if !active
        .iter()
        .any(|sample| sample.phase == LocalBenchmarkPhase::AfterLoad)
    {
        return None;
    }
    let rss_growth = active
        .iter()
        .map(|sample| {
            sample
                .process_rss_mb
                .saturating_sub(baseline.process_rss_mb)
        })
        .max()
        .unwrap_or(0);
    let availability_drop = baseline.host_available_mb.saturating_sub(
        active
            .iter()
            .map(|sample| sample.host_available_mb)
            .min()
            .unwrap_or(baseline.host_available_mb),
    );
    let swap_growth = active
        .iter()
        .map(|sample| sample.host_used_swap_mb)
        .max()
        .unwrap_or(baseline.host_used_swap_mb)
        .saturating_sub(baseline.host_used_swap_mb);
    let host_peak = rss_growth.max(availability_drop.saturating_add(swap_growth));
    if host_peak == 0 {
        return None;
    }

    Some(host_peak)
}

fn observed_device_peak(
    samples: &[medousa_types::local::LocalBenchmarkMemorySample],
    admission: &medousa_types::local::LocalResourceAdmission,
) -> Option<u64> {
    let baseline = samples
        .iter()
        .find(|sample| sample.phase == LocalBenchmarkPhase::BeforeLoad)?;
    let active = active_samples(samples);
    let baseline_device = selected_process_memory(baseline.devices.as_slice(), admission);
    baseline_device.and_then(|baseline_mb| {
        active
            .iter()
            .filter_map(|sample| selected_process_memory(&sample.devices, admission))
            .map(|used_mb| used_mb.saturating_sub(baseline_mb))
            .max()
            .filter(|peak| *peak > 0)
    })
}

fn selected_process_memory(
    devices: &[LocalDeviceTelemetrySnapshot],
    admission: &medousa_types::local::LocalResourceAdmission,
) -> Option<u64> {
    devices
        .iter()
        .find(|device| {
            admission
                .device_uuid
                .as_ref()
                .is_some_and(|uuid| device.device_uuid.as_ref() == Some(uuid))
                || (admission.device_uuid.is_none()
                    && device.backend == admission.device_backend.unwrap_or(GpuBackend::None)
                    && device.device_index == admission.device_index)
        })
        .and_then(|device| device.process_memory_used_mb)
}

fn identity_for_entry(
    entry: &CatalogModelEntry,
    probe: &HardwareProbe,
    devices: &[LocalDeviceTelemetrySnapshot],
) -> CalibrationIdentity {
    let device = devices
        .iter()
        .filter(|device| device.backend == probe.gpu_backend && device.collector_error.is_none())
        .min_by_key(|device| device.device_index.unwrap_or(u32::MAX));
    let uqff = entry
        .engine_args
        .get("uqffFile")
        .and_then(|value| value.as_str());
    CalibrationIdentity {
        model_id: entry.id.clone(),
        runtime_name: RUNTIME_NAME.to_string(),
        runtime_version: RUNTIME_VERSION.to_string(),
        artifact_mode: if uqff.is_some() {
            LocalBenchmarkArtifactMode::PrequantizedUqff
        } else {
            LocalBenchmarkArtifactMode::InSituQuantization
        },
        quantization: uqff.map(|_| "uqff".to_string()).or_else(|| {
            entry
                .engine_args
                .get("inSituQuant")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        }),
        cpu_only: cpu_only(probe),
        max_seq_len: super::governor::SAFE_MAX_SEQ_LEN,
        max_batch_size: super::governor::SAFE_MAX_BATCH_SIZE,
        cpu_arch: probe.cpu_arch.clone(),
        gpu_backend: if cpu_only(probe) {
            GpuBackend::None
        } else {
            probe.gpu_backend
        },
        device_uuid: device.and_then(|device| device.device_uuid.clone()),
        device_name: device.and_then(|device| device.device_name.clone()),
    }
}

fn identity_from_manifest(manifest: &LocalBenchmarkManifest) -> CalibrationIdentity {
    CalibrationIdentity {
        model_id: manifest.recipe.model_id.clone(),
        runtime_name: manifest.engine.runtime_name.clone(),
        runtime_version: manifest.engine.runtime_version.clone(),
        artifact_mode: manifest.recipe.artifact_mode,
        quantization: manifest.recipe.quantization.clone(),
        cpu_only: manifest.recipe.cpu_only,
        max_seq_len: manifest.recipe.max_seq_len,
        max_batch_size: manifest.recipe.max_batch_size,
        cpu_arch: manifest.hardware.cpu_arch.clone(),
        gpu_backend: manifest
            .admission
            .device_backend
            .unwrap_or(GpuBackend::None),
        device_uuid: manifest.admission.device_uuid.clone(),
        device_name: manifest.admission.device_name.clone(),
    }
}

fn cpu_only(probe: &HardwareProbe) -> bool {
    std::env::var("MEDOUSA_LOCAL_ENGINE_CPU")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
        || probe.gpu_backend == GpuBackend::None
}

fn padded_peak(observed_mb: u64) -> u64 {
    observed_mb
        .saturating_mul(100 + u64::from(CALIBRATION_MARGIN_PERCENT))
        .div_ceil(100)
        .saturating_add(CALIBRATION_FIXED_SLACK_MB)
}

fn calibration_path() -> PathBuf {
    super::paths::medousa_data_dir().join(CALIBRATION_FILE)
}

fn update_store<T>(
    path: &Path,
    update: impl FnOnce(&mut CalibrationStore) -> Result<T, String>,
) -> Result<T, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let lock_path = path.with_file_name(CALIBRATION_LOCK_FILE);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|error| format!("failed to open calibration lock: {error}"))?;
    lock.lock_exclusive()
        .map_err(|error| format!("failed to lock calibration store: {error}"))?;
    let result = (|| {
        let mut store = read_store(path)?;
        let value = update(&mut store)?;
        write_store(path, &store)?;
        Ok(value)
    })();
    let _ = FileExt::unlock(&lock);
    result
}

fn read_store(path: &Path) -> Result<CalibrationStore, String> {
    if !path.exists() {
        return Ok(CalibrationStore::default());
    }
    let mut raw = String::new();
    File::open(path)
        .and_then(|mut file| file.read_to_string(&mut raw))
        .map_err(|error| format!("failed to read peak calibration store: {error}"))?;
    let store: CalibrationStore = serde_json::from_str(&raw)
        .map_err(|error| format!("peak calibration store is invalid: {error}"))?;
    if store.schema_version != CALIBRATION_SCHEMA_VERSION {
        return Err(format!(
            "unsupported peak calibration schema {}",
            store.schema_version
        ));
    }
    Ok(store)
}

fn write_store(path: &Path, store: &CalibrationStore) -> Result<(), String> {
    let encoded = serde_json::to_vec_pretty(store).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("failed to open peak calibration store: {error}"))?;
    file.set_len(0).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist peak calibration store: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::local::LocalBenchmarkMemorySample;

    fn sample(
        phase: LocalBenchmarkPhase,
        rss_mb: u64,
        available_mb: u64,
        swap_mb: u64,
    ) -> LocalBenchmarkMemorySample {
        LocalBenchmarkMemorySample {
            phase,
            elapsed_ms: 0,
            process_rss_mb: rss_mb,
            host_available_mb: available_mb,
            host_used_swap_mb: swap_mb,
            devices: Vec::new(),
        }
    }

    #[test]
    fn safety_padding_is_ceil_fifteen_percent_plus_fixed_slack() {
        assert_eq!(padded_peak(1_000), 1_406);
        assert_eq!(padded_peak(0), CALIBRATION_FIXED_SLACK_MB);
    }

    #[test]
    fn newer_store_schema_fails_closed() {
        let path = std::env::temp_dir().join(format!(
            "medousa-calibration-schema-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::write(&path, r#"{"schemaVersion":999,"records":[]}"#).expect("test store");
        let error = read_store(&path).expect_err("future schema must not be ignored");
        assert!(error.contains("unsupported peak calibration schema"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn observed_host_peak_uses_the_worst_rss_or_pressure_growth() {
        let samples = vec![
            sample(LocalBenchmarkPhase::BeforeLoad, 100, 8_000, 0),
            sample(LocalBenchmarkPhase::AfterLoad, 2_100, 5_500, 100),
            sample(LocalBenchmarkPhase::AfterStream, 2_000, 5_000, 200),
            sample(LocalBenchmarkPhase::AfterUnload, 120, 7_900, 200),
        ];
        assert_eq!(observed_host_peak(&samples), Some(3_200));
    }

    #[test]
    fn incomplete_load_samples_are_not_promoted() {
        let samples = vec![sample(LocalBenchmarkPhase::BeforeLoad, 100, 8_000, 0)];
        assert_eq!(observed_host_peak(&samples), None);
    }
}
