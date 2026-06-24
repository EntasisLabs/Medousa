mod backends;
mod catalog;
mod engine;
mod hardware;
mod process;
mod store;

pub use catalog::{
    builtin_catalog, filter_catalog_for_tier, recommended_model_for_tier, CatalogFile,
    CatalogModelEntry,
};
pub use engine::{
    config_from_catalog_entry, config_from_catalog_entry_with_probe, load_recommended_engine,
    LocalEngineConfig, LocalEngineManager, LocalEngineStatus, LOCAL_ENGINE,
    DEFAULT_LOCAL_ENGINE_BASE_URL, DEFAULT_LOCAL_ENGINE_BIND,
};
pub use store::{
    local_repo_if_installed, DownloadPhase, InstalledModelRecord, ModelDownloadProgress,
    ModelStore, MODEL_STORE,
};
pub use backends::{
    compiled_backends, cuda_device_present, detect_gpu_backend, resolve_cpu_only,
    resolve_inference_device, InferenceDevice,
};
pub use hardware::{
    build_hardware_profile, hardware_profile_path, probe_hardware, read_hardware_profile,
    score_tier, write_hardware_profile, GpuBackend, HardwareProfile, HardwareProbe, HardwareTier,
};
pub use process::{
    external_engine_status, is_bind_reachable, load_external_engine, medousa_local_binary_available,
    resolve_medousa_local_binary, spawn_external_local_engine, spawn_external_recommended,
    stop_external_local_engine,
};
