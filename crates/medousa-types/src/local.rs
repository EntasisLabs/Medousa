use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_LOCAL_ENGINE_BIND: &str = "127.0.0.1:7421";
pub const DEFAULT_LOCAL_ENGINE_BASE_URL: &str = "http://127.0.0.1:7421/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "UPPERCASE")]
pub enum HardwareTier {
    A,
    B,
    C,
    D,
    E,
}

impl HardwareTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::A => "Minimal",
            Self::B => "Everyday",
            Self::C => "Comfortable",
            Self::D => "Enthusiast",
            Self::E => "Workstation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum GpuBackend {
    None,
    Metal,
    Cuda,
    Rocm,
    Vulkan,
    DirectMl,
    Other,
}

impl GpuBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Metal => "metal",
            Self::Cuda => "cuda",
            Self::Rocm => "rocm",
            Self::Vulkan => "vulkan",
            Self::DirectMl => "directml",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct HardwareProbe {
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub cpu_cores: usize,
    pub cpu_arch: String,
    pub gpu_backend: GpuBackend,
    pub free_disk_gb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct HardwareProfile {
    pub probed_at: DateTime<Utc>,
    pub tier: HardwareTier,
    pub tier_label: String,
    pub probe: HardwareProbe,
    pub recommended_model_id: String,
    pub recommended_display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DownloadFileRecord {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct InstalledModelRecord {
    pub model_id: String,
    pub repo: String,
    pub local_path: String,
    pub installed_at: DateTime<Utc>,
    pub bytes_on_disk: u64,
    pub verified: bool,
    #[serde(default)]
    pub files: Vec<DownloadFileRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadProgress {
    pub job_id: String,
    pub model_id: String,
    pub phase: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub percent: f32,
    pub current_file: Option<String>,
    pub message: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LocalRuntimePhase {
    Unavailable,
    Cold,
    StartingWorker,
    Loading,
    Ready,
    Busy,
    Draining,
    Unloading,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalEngineStatus {
    pub feature_enabled: bool,
    pub loaded: bool,
    pub phase: LocalRuntimePhase,
    pub base_url: String,
    pub bind: Option<String>,
    pub model_repo: Option<String>,
    pub model_alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_backend: Option<String>,
    pub message: String,
}

impl<'de> Deserialize<'de> for LocalEngineStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct WireStatus {
            feature_enabled: bool,
            loaded: bool,
            #[serde(default)]
            phase: Option<LocalRuntimePhase>,
            base_url: String,
            bind: Option<String>,
            model_repo: Option<String>,
            model_alias: Option<String>,
            #[serde(default)]
            inference_backend: Option<String>,
            message: String,
        }

        let wire = WireStatus::deserialize(deserializer)?;
        let phase = wire.phase.unwrap_or({
            if wire.loaded {
                LocalRuntimePhase::Ready
            } else if wire.feature_enabled {
                LocalRuntimePhase::Cold
            } else {
                LocalRuntimePhase::Unavailable
            }
        });
        Ok(Self {
            feature_enabled: wire.feature_enabled,
            loaded: wire.loaded,
            phase,
            base_url: wire.base_url,
            bind: wire.bind,
            model_repo: wire.model_repo,
            model_alias: wire.model_alias,
            inference_backend: wire.inference_backend,
            message: wire.message,
        })
    }
}

impl LocalEngineStatus {
    pub fn idle(feature_enabled: bool) -> Self {
        Self {
            feature_enabled,
            loaded: false,
            phase: if feature_enabled {
                LocalRuntimePhase::Cold
            } else {
                LocalRuntimePhase::Unavailable
            },
            base_url: DEFAULT_LOCAL_ENGINE_BASE_URL.to_string(),
            bind: None,
            model_repo: None,
            model_alias: None,
            inference_backend: None,
            message: if feature_enabled {
                "Local engine not loaded".to_string()
            } else {
                "Offline brain package not installed".to_string()
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalHardwareResponse {
    pub profile: HardwareProfile,
    pub engine_available: bool,
    pub compiled_backends: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalResourceAdmission {
    pub admitted: bool,
    pub model_id: String,
    pub hardware_tier: HardwareTier,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub system_reserve_mb: u64,
    pub tier_cap_mb: u64,
    #[serde(default)]
    pub host_admissible_mb: u64,
    pub admissible_mb: u64,
    pub estimated_steady_mb: u64,
    pub estimated_conversion_mb: u64,
    #[serde(default)]
    pub static_estimated_peak_mb: u64,
    pub estimated_peak_mb: u64,
    #[serde(default)]
    pub calibration_applied: bool,
    #[serde(default)]
    pub calibration_sample_count: u32,
    #[serde(default)]
    pub calibration_observed_host_peak_mb: Option<u64>,
    #[serde(default)]
    pub calibration_observed_device_peak_mb: Option<u64>,
    #[serde(default)]
    pub calibration_margin_percent: Option<u8>,
    pub critical_available_mb: u64,
    pub max_seq_len: usize,
    pub max_batch_size: usize,
    #[serde(default)]
    pub device_enforced: bool,
    #[serde(default)]
    pub device_source: Option<LocalDeviceTelemetrySource>,
    #[serde(default)]
    pub device_backend: Option<GpuBackend>,
    #[serde(default)]
    pub device_index: Option<u32>,
    #[serde(default)]
    pub device_uuid: Option<String>,
    #[serde(default)]
    pub device_name: Option<String>,
    #[serde(default)]
    pub device_total_mb: Option<u64>,
    #[serde(default)]
    pub device_budget_mb: Option<u64>,
    #[serde(default)]
    pub device_available_mb: Option<u64>,
    #[serde(default)]
    pub device_reserve_mb: Option<u64>,
    #[serde(default)]
    pub device_admissible_mb: Option<u64>,
    #[serde(default)]
    pub device_estimated_peak_mb: Option<u64>,
    #[serde(default)]
    pub device_rationale: Option<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LocalBenchmarkOutcome {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LocalBenchmarkArtifactMode {
    PrequantizedUqff,
    InSituQuantization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LocalBenchmarkPhase {
    BeforeLoad,
    AfterLoad,
    AfterStream,
    AfterUnload,
    Reclaimed1s,
    Reclaimed5s,
    Reclaimed10s,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalBenchmarkGitState {
    pub revision: Option<String>,
    pub dirty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalBenchmarkEngineIdentity {
    pub control_plane_version: String,
    pub runtime_name: String,
    pub runtime_version: String,
    pub compiled_backends: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalBenchmarkHostIdentity {
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub cpu_brand: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalBenchmarkRecipe {
    pub model_id: String,
    pub model_repo: String,
    pub artifact_mode: LocalBenchmarkArtifactMode,
    pub quantization: Option<String>,
    pub cpu_only: bool,
    pub max_seq_len: usize,
    pub max_batch_size: usize,
    pub synthetic_prompt_tokens: usize,
    pub max_output_tokens: usize,
    pub sampling_seed: u64,
    pub bind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalBenchmarkMemorySample {
    pub phase: LocalBenchmarkPhase,
    pub elapsed_ms: u64,
    pub process_rss_mb: u64,
    pub host_available_mb: u64,
    pub host_used_swap_mb: u64,
    #[serde(default)]
    pub devices: Vec<LocalDeviceTelemetrySnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LocalDeviceTelemetrySource {
    MetalApi,
    Nvml,
    NvidiaSmi,
    AmdSmiLibrary,
    AmdSmi,
    Wddm,
    VulkanBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LocalDeviceTelemetryAvailability {
    Available,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalDeviceTelemetrySnapshot {
    pub captured_at: DateTime<Utc>,
    pub source: LocalDeviceTelemetrySource,
    pub availability: LocalDeviceTelemetryAvailability,
    pub backend: GpuBackend,
    pub device_index: Option<u32>,
    pub device_uuid: Option<String>,
    pub device_name: Option<String>,
    pub driver_version: Option<String>,
    pub runtime_version: Option<String>,
    pub unified_memory: Option<bool>,
    pub memory_total_mb: Option<u64>,
    #[serde(default)]
    pub memory_budget_mb: Option<u64>,
    pub memory_used_mb: Option<u64>,
    pub memory_free_mb: Option<u64>,
    pub process_memory_used_mb: Option<u64>,
    pub recommended_working_set_mb: Option<u64>,
    pub utilization_percent: Option<f64>,
    pub power_watts: Option<f64>,
    pub temperature_c: Option<f64>,
    pub graphics_clock_mhz: Option<u64>,
    pub memory_clock_mhz: Option<u64>,
    pub throttle_reasons: Option<Vec<String>>,
    #[serde(default)]
    pub unavailable_fields: Vec<String>,
    pub collector_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalBenchmarkResult {
    pub outcome: LocalBenchmarkOutcome,
    pub error: Option<String>,
    pub load_ms: Option<u64>,
    pub ttft_ms: Option<u64>,
    pub stream_ms: Option<u64>,
    pub response_chunks: u64,
    pub response_bytes: u64,
    pub generated_content_bytes: u64,
    pub reported_completion_tokens: Option<u64>,
    pub unload_ms: Option<u64>,
    pub rss_reclaimed_mb_1s: Option<u64>,
    pub rss_reclaimed_mb_5s: Option<u64>,
    pub rss_reclaimed_mb_10s: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalBenchmarkManifest {
    pub schema_version: u32,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub git: LocalBenchmarkGitState,
    pub engine: LocalBenchmarkEngineIdentity,
    pub host: LocalBenchmarkHostIdentity,
    pub hardware: HardwareProbe,
    pub admission: LocalResourceAdmission,
    pub recipe: LocalBenchmarkRecipe,
    pub samples: Vec<LocalBenchmarkMemorySample>,
    pub result: LocalBenchmarkResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalCatalogResponse {
    pub tier: HardwareTier,
    pub tier_label: String,
    pub family_default: String,
    pub recommended_model_id: String,
    pub models: Vec<CatalogModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalModelsResponse {
    pub installed: Vec<InstalledModelRecord>,
    pub active_downloads: Vec<ModelDownloadProgress>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEngineLoadRequest {
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub bind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalModelDownloadRequest {
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalModelDownloadResponse {
    pub job: ModelDownloadProgress,
}

#[cfg(test)]
mod tests {
    use super::{LocalEngineStatus, LocalRuntimePhase};

    #[test]
    fn idle_status_distinguishes_cold_from_unavailable() {
        assert_eq!(LocalEngineStatus::idle(true).phase, LocalRuntimePhase::Cold);
        assert_eq!(
            LocalEngineStatus::idle(false).phase,
            LocalRuntimePhase::Unavailable
        );
    }

    #[test]
    fn lifecycle_phase_serializes_for_desktop_clients() {
        assert_eq!(
            serde_json::to_string(&LocalRuntimePhase::StartingWorker).unwrap(),
            "\"startingWorker\""
        );
    }

    #[test]
    fn status_infers_phase_from_legacy_payloads() {
        let status: LocalEngineStatus = serde_json::from_str(
            r#"{"featureEnabled":true,"loaded":true,"baseUrl":"http://127.0.0.1:7421/v1","bind":null,"modelRepo":null,"modelAlias":null,"message":"ready"}"#,
        )
        .unwrap();
        assert_eq!(status.phase, LocalRuntimePhase::Ready);
    }
}
