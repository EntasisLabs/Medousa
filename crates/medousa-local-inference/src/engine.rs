use once_cell::sync::Lazy;
use std::sync::Arc;

use medousa_host::is_bind_reachable;

pub use medousa_types::local::{
    DEFAULT_LOCAL_ENGINE_BASE_URL, DEFAULT_LOCAL_ENGINE_BIND, LocalEngineStatus, LocalRuntimePhase,
};

#[derive(Debug, Clone)]
pub struct LocalEngineConfig {
    pub bind: String,
    pub model_repo: String,
    pub model_alias: String,
    pub from_uqff: Option<String>,
    pub in_situ_quant: Option<String>,
    pub cpu_only: bool,
    pub max_seq_len: usize,
    pub max_batch_size: usize,
    pub idle_timeout_secs: u64,
    pub critical_available_mb: u64,
}

impl Default for LocalEngineConfig {
    fn default() -> Self {
        Self {
            bind: DEFAULT_LOCAL_ENGINE_BIND.to_string(),
            model_repo: "google/gemma-4-E4B-it".to_string(),
            model_alias: "gemma-4-e4b-it".to_string(),
            from_uqff: None,
            in_situ_quant: Some("4".to_string()),
            cpu_only: false,
            max_seq_len: super::governor::SAFE_MAX_SEQ_LEN,
            max_batch_size: super::governor::SAFE_MAX_BATCH_SIZE,
            idle_timeout_secs: super::governor::DEFAULT_IDLE_TIMEOUT_SECS,
            critical_available_mb: 1024,
        }
    }
}

pub struct LocalEngineManager;

impl LocalEngineManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn status(&self) -> LocalEngineStatus {
        probe_local_engine_status().await
    }
}

impl Default for LocalEngineManager {
    fn default() -> Self {
        Self::new()
    }
}

pub static LOCAL_ENGINE: Lazy<Arc<LocalEngineManager>> =
    Lazy::new(|| Arc::new(LocalEngineManager::new()));

pub async fn probe_local_engine_status() -> LocalEngineStatus {
    let bind = std::env::var("MEDOUSA_LOCAL_ENGINE_BIND")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_LOCAL_ENGINE_BIND.to_string());
    let feature_enabled = medousa_host::resolve_medousa_local_binary().is_ok();

    if is_bind_reachable(&bind) {
        return LocalEngineStatus {
            feature_enabled,
            loaded: true,
            phase: LocalRuntimePhase::Ready,
            base_url: format!("http://{bind}/v1"),
            bind: Some(bind),
            model_repo: None,
            model_alias: None,
            inference_backend: None,
            message: "Local engine running (medousa_local)".to_string(),
        };
    }

    LocalEngineStatus::idle(feature_enabled)
}

pub fn recommended_engine_config(bind: Option<String>) -> Result<LocalEngineConfig, String> {
    let probe = super::hardware::probe_hardware();
    let tier = super::hardware::score_tier(&probe);
    let devices = super::telemetry::collect_device_telemetry();
    let entry = super::governor::recommended_admitted_model_with_devices(&probe, &devices)
        .ok_or_else(|| {
            format!(
                "no local model fits the safe envelope for hardware tier {}",
                tier.as_str()
            )
        })?;
    Ok(config_from_catalog_entry(&entry, bind))
}

pub fn config_from_catalog_entry(
    entry: &super::catalog::CatalogModelEntry,
    bind: Option<String>,
) -> LocalEngineConfig {
    let probe = super::hardware::probe_hardware();
    config_from_catalog_entry_with_probe(entry, bind, &probe)
}

pub fn config_from_catalog_entry_with_probe(
    entry: &super::catalog::CatalogModelEntry,
    bind: Option<String>,
    probe: &super::hardware::HardwareProbe,
) -> LocalEngineConfig {
    let model_repo =
        super::store::local_repo_if_installed(&entry.id).unwrap_or_else(|| entry.repo.clone());
    let uqff_file = entry
        .engine_args
        .get("uqffFile")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let in_situ_quant = entry
        .engine_args
        .get("fromUqff")
        .and_then(|value| value.as_u64())
        .map(|level| level.to_string());
    let use_uqff = uqff_file.is_some();
    let admission = super::governor::evaluate_model_admission(entry, probe);
    LocalEngineConfig {
        bind: bind.unwrap_or_else(|| DEFAULT_LOCAL_ENGINE_BIND.to_string()),
        model_repo,
        model_alias: entry.id.clone(),
        from_uqff: uqff_file,
        in_situ_quant: if use_uqff {
            None
        } else {
            in_situ_quant.or_else(|| Some("4".to_string()))
        },
        cpu_only: super::backends::resolve_cpu_only(probe),
        max_seq_len: super::governor::SAFE_MAX_SEQ_LEN,
        max_batch_size: super::governor::SAFE_MAX_BATCH_SIZE,
        idle_timeout_secs: super::governor::DEFAULT_IDLE_TIMEOUT_SECS,
        critical_available_mb: admission.critical_available_mb,
    }
}
