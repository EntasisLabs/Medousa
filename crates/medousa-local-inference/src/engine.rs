use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::Arc;

pub use medousa_types::local::{
    DEFAULT_LOCAL_ENGINE_BASE_URL, DEFAULT_LOCAL_ENGINE_BIND, LOCAL_WORKER_PROTOCOL_VERSION,
    LocalEngineStatus, LocalRuntimePhase, LocalWorkerStatus,
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

pub async fn worker_status_for_config(config: &LocalEngineConfig) -> LocalWorkerStatus {
    LocalWorkerStatus {
        protocol_version: LOCAL_WORKER_PROTOCOL_VERSION,
        generation_id: uuid::Uuid::new_v4().to_string(),
        pid: std::process::id(),
        started_at: chrono::Utc::now(),
        phase: LocalRuntimePhase::Loading,
        model_repo: config.model_repo.clone(),
        model_alias: config.model_alias.clone(),
        artifact_digest: installed_artifact_digest(&config.model_alias).await,
        recipe_revision: recipe_revision(config),
        binary_digest: current_binary_digest(),
        runtime_name: "mistral.rs".to_string(),
        runtime_version: "0.8.1".to_string(),
        compiled_backends: super::backends::compiled_backends()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

async fn installed_artifact_digest(model_id: &str) -> Option<String> {
    let mut record = super::store::MODEL_STORE.get_installed(model_id).await?;
    if !record.verified || record.files.is_empty() {
        return None;
    }
    record
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    let mut hasher = Sha256::new();
    for file in record.files {
        hasher.update(file.path.as_bytes());
        hasher.update([0]);
        hasher.update(file.bytes.to_le_bytes());
        hasher.update(file.sha256.as_bytes());
        hasher.update([0xff]);
    }
    Some(format!("sha256:{:x}", hasher.finalize()))
}

fn recipe_revision(config: &LocalEngineConfig) -> String {
    let mut hasher = Sha256::new();
    for value in [
        config.model_repo.as_str(),
        config.model_alias.as_str(),
        config.from_uqff.as_deref().unwrap_or(""),
        config.in_situ_quant.as_deref().unwrap_or(""),
    ] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    hasher.update([u8::from(config.cpu_only)]);
    hasher.update((config.max_seq_len as u64).to_le_bytes());
    hasher.update((config.max_batch_size as u64).to_le_bytes());
    format!("mir-recipe-v1:{:x}", hasher.finalize())
}

fn current_binary_digest() -> Option<String> {
    let path = std::env::current_exe().ok()?;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(format!("sha256:{:x}", hasher.finalize()))
}

pub async fn probe_local_engine_status() -> LocalEngineStatus {
    let bind = std::env::var("MEDOUSA_LOCAL_ENGINE_BIND")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_LOCAL_ENGINE_BIND.to_string());
    let feature_enabled = medousa_host::resolve_medousa_local_binary().is_ok();

    match medousa_host::probe_local_worker(&bind) {
        Ok(worker) => {
            return LocalEngineStatus {
                feature_enabled,
                loaded: true,
                phase: worker.phase.clone(),
                base_url: format!("http://{bind}/v1"),
                bind: Some(bind),
                model_repo: Some(worker.model_repo.clone()),
                model_alias: Some(worker.model_alias.clone()),
                inference_backend: None,
                worker: Some(worker),
                message: "Compatible local worker running".to_string(),
            };
        }
        Err(error) if medousa_host::is_bind_reachable(&bind) => {
            return LocalEngineStatus {
                feature_enabled,
                loaded: false,
                phase: LocalRuntimePhase::Failed,
                base_url: format!("http://{bind}/v1"),
                bind: Some(bind),
                model_repo: None,
                model_alias: None,
                inference_backend: None,
                worker: None,
                message: format!("Local worker handshake failed: {error}"),
            };
        }
        Err(_) => {}
    }

    LocalEngineStatus::idle(feature_enabled)
}

pub fn recommended_engine_config(bind: Option<String>) -> Result<LocalEngineConfig, String> {
    let admission = super::governor::recommended_model_admission()?;
    let entry = super::catalog::builtin_catalog()
        .models
        .into_iter()
        .find(|entry| entry.id == admission.model_id)
        .ok_or_else(|| format!("recommended model {} left the catalog", admission.model_id))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_revision_ignores_bind_but_tracks_resource_recipe() {
        let base = LocalEngineConfig::default();
        let mut other_bind = base.clone();
        other_bind.bind = "127.0.0.1:9999".to_string();
        assert_eq!(recipe_revision(&base), recipe_revision(&other_bind));

        let mut other_context = base.clone();
        other_context.max_seq_len *= 2;
        assert_ne!(recipe_revision(&base), recipe_revision(&other_context));
    }
}
