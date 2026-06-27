use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_LOCAL_ENGINE_BIND: &str = "127.0.0.1:7421";
pub const DEFAULT_LOCAL_ENGINE_BASE_URL: &str = "http://127.0.0.1:7421/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    Other,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LocalEngineStatus {
    pub feature_enabled: bool,
    pub loaded: bool,
    pub base_url: String,
    pub bind: Option<String>,
    pub model_repo: Option<String>,
    pub model_alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_backend: Option<String>,
    pub message: String,
}

impl LocalEngineStatus {
    pub fn idle(feature_enabled: bool) -> Self {
        Self {
            feature_enabled,
            loaded: false,
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
