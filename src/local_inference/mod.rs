mod catalog;
mod engine;
mod hardware;

pub use catalog::{
    builtin_catalog, filter_catalog_for_tier, recommended_model_for_tier, CatalogFile,
    CatalogModelEntry,
};
pub use engine::{
    config_from_catalog_entry, load_recommended_engine, LocalEngineConfig, LocalEngineManager,
    LocalEngineStatus, LOCAL_ENGINE, DEFAULT_LOCAL_ENGINE_BASE_URL, DEFAULT_LOCAL_ENGINE_BIND,
};
pub use hardware::{
    build_hardware_profile, hardware_profile_path, probe_hardware, read_hardware_profile,
    score_tier, write_hardware_profile, GpuBackend, HardwareProfile, HardwareProbe, HardwareTier,
};
