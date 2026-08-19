//! Local inference support for Medousa: hardware tiering, model catalog, engine
//! config, and backend detection — plus the standalone `medousa_local` binary.
//!
//! This crate is intentionally lean: it depends only on `medousa-types`,
//! `medousa-host`, and `medousa-install-support` (and, for the binary, the
//! `medousa-local-engine` mistralrs wrapper behind `embedded-inference`). It does
//! NOT pull in the main `medousa` application crate, so building the offline
//! brain no longer compiles the entire runtime dependency tree.

mod backends;
mod calibration;
mod catalog;
mod engine;
mod governor;
mod hardware;
mod lease;
mod paths;
mod process;
mod store;
mod telemetry;

pub use backends::{
    InferenceDevice, compiled_backends, cuda_device_present, detect_gpu_backend, resolve_cpu_only,
    resolve_inference_device,
};
pub use calibration::{PeakCalibration, record_benchmark_calibration};
pub use catalog::{
    CatalogFile, CatalogModelEntry, builtin_catalog, filter_catalog_for_tier,
    recommended_model_for_tier,
};
pub use engine::{
    DEFAULT_LOCAL_ENGINE_BASE_URL, DEFAULT_LOCAL_ENGINE_BIND, LOCAL_ENGINE, LocalEngineConfig,
    LocalEngineManager, LocalEngineStatus, config_from_catalog_entry,
    config_from_catalog_entry_with_probe, probe_local_engine_status, recommended_engine_config,
    worker_status_for_config,
};
pub use governor::{
    DEFAULT_IDLE_TIMEOUT_SECS, SAFE_MAX_BATCH_SIZE, SAFE_MAX_SEQ_LEN, admission_for_model_id,
    critical_available_mb, device_pressure_requires_eviction, evaluate_model_admission,
    evaluate_model_admission_with_calibration, evaluate_model_admission_with_devices,
    recommended_admitted_model, recommended_admitted_model_with_devices,
    recommended_model_admission, system_reserve_mb, tier_recipe_cap_mb,
};
pub use hardware::{
    GpuBackend, HardwareProbe, HardwareProfile, HardwareTier, build_hardware_profile,
    hardware_profile_path, probe_hardware, read_hardware_profile, score_tier,
    write_hardware_profile,
};
pub use lease::{LocalResourceActivationLease, acquire_activation_lease};
pub use process::{
    external_engine_status, is_bind_reachable, load_external_engine,
    medousa_local_binary_available, resolve_medousa_local_binary, spawn_external_local_engine,
    spawn_external_recommended, stop_external_local_engine,
};
pub use store::{
    DownloadPhase, InstalledModelRecord, MODEL_STORE, ModelDownloadProgress, ModelStore,
    local_repo_if_installed,
};
pub use telemetry::collect_device_telemetry;
