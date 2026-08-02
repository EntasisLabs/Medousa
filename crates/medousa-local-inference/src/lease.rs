use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use medousa_types::local::{GpuBackend, LocalResourceAdmission};
use serde::{Deserialize, Serialize};

const LEASE_REGISTRY_FILE: &str = "local-inference-activation-leases.json";
const LEASE_LOCK_FILE: &str = "local-inference-activation-leases.lock";
const LEASE_MAX_AGE_SECS: u64 = 30 * 60;
static LEASE_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivationLeaseRecord {
    lease_id: String,
    pid: u32,
    created_at_unix_secs: u64,
    model_id: String,
    host_reserved_mb: u64,
    device_key: Option<String>,
    device_reserved_mb: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivationLeaseRegistry {
    #[serde(default)]
    leases: Vec<ActivationLeaseRecord>,
}

/// A short-lived, cross-process reservation held only while model allocation is
/// in flight. Once loading finishes, live host/device counters become the source
/// of truth and this reservation is released.
#[derive(Debug)]
pub struct LocalResourceActivationLease {
    lease_id: String,
    runtime_dir: PathBuf,
}

impl Drop for LocalResourceActivationLease {
    fn drop(&mut self) {
        let _ = update_registry(&self.runtime_dir, |registry| {
            registry
                .leases
                .retain(|lease| lease.lease_id != self.lease_id);
            Ok(())
        });
    }
}

pub fn acquire_activation_lease(
    admission: &LocalResourceAdmission,
) -> Result<LocalResourceActivationLease, String> {
    acquire_activation_lease_in(admission, &super::paths::local_inference_coordination_dir())
}

fn acquire_activation_lease_in(
    admission: &LocalResourceAdmission,
    runtime_dir: &Path,
) -> Result<LocalResourceActivationLease, String> {
    if !admission.admitted {
        return Err(admission.rationale.clone());
    }
    let lease_id = format!(
        "{}-{}-{}",
        std::process::id(),
        now_unix_secs(),
        LEASE_NONCE.fetch_add(1, Ordering::Relaxed)
    );
    let device_key = admission.device_enforced.then(|| device_key(admission));
    let requested_host_mb = admission.estimated_peak_mb;
    let requested_device_mb = admission.device_enforced.then_some(
        admission
            .device_estimated_peak_mb
            .unwrap_or(requested_host_mb),
    );
    let runtime_dir = runtime_dir.to_path_buf();

    update_registry(&runtime_dir, |registry| {
        let now = now_unix_secs();
        registry.leases.retain(|lease| {
            now.saturating_sub(lease.created_at_unix_secs) <= LEASE_MAX_AGE_SECS
                && medousa_host::is_process_alive(lease.pid)
        });

        let host_already_reserved_mb = registry
            .leases
            .iter()
            .map(|lease| lease.host_reserved_mb)
            .sum::<u64>();
        if host_already_reserved_mb.saturating_add(requested_host_mb) > admission.host_admissible_mb
        {
            return Err(format!(
                "Refusing to start {}: concurrent local model activation has reserved {} MiB of the {} MiB host envelope; this load needs another {} MiB",
                admission.model_id,
                host_already_reserved_mb,
                admission.host_admissible_mb,
                requested_host_mb
            ));
        }

        if let (Some(key), Some(requested_mb), Some(device_admissible_mb)) = (
            device_key.as_deref(),
            requested_device_mb,
            admission.device_admissible_mb,
        ) {
            let device_already_reserved_mb = registry
                .leases
                .iter()
                .filter(|lease| lease.device_key.as_deref() == Some(key))
                .filter_map(|lease| lease.device_reserved_mb)
                .sum::<u64>();
            if device_already_reserved_mb.saturating_add(requested_mb) > device_admissible_mb {
                return Err(format!(
                    "Refusing to start {}: concurrent local model activation has reserved {} MiB of the {} MiB device envelope; this load needs another {} MiB",
                    admission.model_id,
                    device_already_reserved_mb,
                    device_admissible_mb,
                    requested_mb
                ));
            }
        }

        registry.leases.push(ActivationLeaseRecord {
            lease_id: lease_id.clone(),
            pid: std::process::id(),
            created_at_unix_secs: now,
            model_id: admission.model_id.clone(),
            host_reserved_mb: requested_host_mb,
            device_key: device_key.clone(),
            device_reserved_mb: requested_device_mb,
        });
        Ok(())
    })?;

    Ok(LocalResourceActivationLease {
        lease_id,
        runtime_dir,
    })
}

fn device_key(admission: &LocalResourceAdmission) -> String {
    let backend = admission
        .device_backend
        .unwrap_or(GpuBackend::None)
        .as_str();
    if let Some(uuid) = admission.device_uuid.as_deref() {
        return format!("{backend}:uuid:{uuid}");
    }
    if let Some(index) = admission.device_index {
        return format!("{backend}:index:{index}");
    }
    format!(
        "{backend}:name:{}",
        admission.device_name.as_deref().unwrap_or("selected")
    )
}

fn update_registry(
    runtime_dir: &Path,
    update: impl FnOnce(&mut ActivationLeaseRegistry) -> Result<(), String>,
) -> Result<(), String> {
    fs::create_dir_all(runtime_dir).map_err(|error| {
        format!(
            "failed to create local inference runtime directory {}: {error}",
            runtime_dir.display()
        )
    })?;
    let lock_path = runtime_dir.join(LEASE_LOCK_FILE);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| format!("failed to open resource lease lock: {error}"))?;
    lock.lock_exclusive()
        .map_err(|error| format!("failed to lock resource lease registry: {error}"))?;

    let result = (|| {
        let registry_path = runtime_dir.join(LEASE_REGISTRY_FILE);
        let mut registry = read_registry(&registry_path)?;
        update(&mut registry)?;
        write_registry(&registry_path, &registry)
    })();
    let _ = FileExt::unlock(&lock);
    result
}

fn read_registry(path: &Path) -> Result<ActivationLeaseRegistry, String> {
    if !path.exists() {
        return Ok(ActivationLeaseRegistry::default());
    }
    let mut raw = String::new();
    File::open(path)
        .and_then(|mut file| file.read_to_string(&mut raw))
        .map_err(|error| format!("failed to read resource lease registry: {error}"))?;
    if raw.trim().is_empty() {
        return Ok(ActivationLeaseRegistry::default());
    }
    serde_json::from_str(&raw).map_err(|error| {
        format!("resource lease registry is invalid; refusing activation: {error}")
    })
}

fn write_registry(path: &Path, registry: &ActivationLeaseRegistry) -> Result<(), String> {
    let encoded = serde_json::to_vec_pretty(registry)
        .map_err(|error| format!("failed to encode resource lease registry: {error}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("failed to open resource lease registry: {error}"))?;
    file.set_len(0)
        .map_err(|error| format!("failed to truncate resource lease registry: {error}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("failed to seek resource lease registry: {error}"))?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist resource lease registry: {error}"))
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::local::{HardwareTier, LocalDeviceTelemetrySource};

    fn admission(host_envelope_mb: u64, peak_mb: u64) -> LocalResourceAdmission {
        LocalResourceAdmission {
            admitted: true,
            model_id: "test-model".to_string(),
            hardware_tier: HardwareTier::C,
            total_ram_mb: 16 * 1024,
            available_ram_mb: 12 * 1024,
            system_reserve_mb: 4 * 1024,
            tier_cap_mb: 8 * 1024,
            host_admissible_mb: host_envelope_mb,
            admissible_mb: host_envelope_mb,
            estimated_steady_mb: peak_mb,
            estimated_conversion_mb: 0,
            estimated_peak_mb: peak_mb,
            critical_available_mb: 1024,
            max_seq_len: 4096,
            max_batch_size: 1,
            device_enforced: false,
            device_source: None,
            device_backend: None,
            device_index: None,
            device_uuid: None,
            device_name: None,
            device_total_mb: None,
            device_available_mb: None,
            device_reserve_mb: None,
            device_admissible_mb: None,
            device_estimated_peak_mb: None,
            device_rationale: None,
            rationale: "test admission".to_string(),
        }
    }

    fn test_runtime_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "medousa-lease-test-{label}-{}-{}",
            std::process::id(),
            LEASE_NONCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn concurrent_activation_cannot_spend_the_same_host_headroom_twice() {
        let dir = test_runtime_dir("host");
        let first = acquire_activation_lease_in(&admission(6_000, 4_000), &dir)
            .expect("first activation fits");
        let rejection = acquire_activation_lease_in(&admission(6_000, 4_000), &dir)
            .expect_err("second activation must not reuse reserved headroom");
        assert!(rejection.contains("concurrent local model activation"));
        drop(first);
        acquire_activation_lease_in(&admission(6_000, 4_000), &dir)
            .expect("released reservation can be reused");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn simultaneous_callers_serialize_lease_accounting() {
        const CALLERS: usize = 8;
        let dir = test_runtime_dir("race");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(CALLERS + 1));
        let successes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut threads = Vec::new();
        for _ in 0..CALLERS {
            let dir = dir.clone();
            let barrier = barrier.clone();
            let successes = successes.clone();
            threads.push(std::thread::spawn(move || {
                let lease = acquire_activation_lease_in(&admission(6_000, 4_000), &dir).ok();
                if lease.is_some() {
                    successes.fetch_add(1, Ordering::SeqCst);
                }
                barrier.wait();
                drop(lease);
            }));
        }
        barrier.wait();
        assert_eq!(successes.load(Ordering::SeqCst), 1);
        for thread in threads {
            thread.join().expect("lease caller");
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn device_reservations_are_isolated_by_stable_device_identity() {
        let dir = test_runtime_dir("device");
        let mut first_admission = admission(16_000, 3_000);
        first_admission.device_enforced = true;
        first_admission.device_source = Some(LocalDeviceTelemetrySource::NvidiaSmi);
        first_admission.device_backend = Some(GpuBackend::Cuda);
        first_admission.device_index = Some(0);
        first_admission.device_uuid = Some("GPU-a".to_string());
        first_admission.device_admissible_mb = Some(5_000);
        first_admission.device_estimated_peak_mb = Some(3_000);
        let first =
            acquire_activation_lease_in(&first_admission, &dir).expect("first device lease");

        let mut same_device = first_admission.clone();
        same_device.device_index = Some(1);
        let rejection = acquire_activation_lease_in(&same_device, &dir)
            .expect_err("UUID identity must prevent duplicate spend");
        assert!(rejection.contains("device envelope"));

        let mut other_device = first_admission;
        other_device.device_uuid = Some("GPU-b".to_string());
        acquire_activation_lease_in(&other_device, &dir)
            .expect("a separate device has a separate envelope");
        drop(first);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dead_process_reservations_are_reclaimed_before_admission() {
        let dir = test_runtime_dir("stale");
        fs::create_dir_all(&dir).expect("test runtime dir");
        write_registry(
            &dir.join(LEASE_REGISTRY_FILE),
            &ActivationLeaseRegistry {
                leases: vec![ActivationLeaseRecord {
                    lease_id: "dead".to_string(),
                    pid: u32::MAX,
                    created_at_unix_secs: now_unix_secs(),
                    model_id: "dead-model".to_string(),
                    host_reserved_mb: 6_000,
                    device_key: None,
                    device_reserved_mb: None,
                }],
            },
        )
        .expect("stale registry");

        acquire_activation_lease_in(&admission(6_000, 4_000), &dir)
            .expect("dead process must not strand its reservation");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_registry_fails_closed_instead_of_discarding_reservations() {
        let dir = test_runtime_dir("corrupt");
        fs::create_dir_all(&dir).expect("test runtime dir");
        fs::write(dir.join(LEASE_REGISTRY_FILE), b"not-json").expect("corrupt registry");
        let error = acquire_activation_lease_in(&admission(6_000, 4_000), &dir)
            .expect_err("unknown reservations must fail closed");
        assert!(error.contains("registry is invalid"));
        let _ = fs::remove_dir_all(dir);
    }
}
