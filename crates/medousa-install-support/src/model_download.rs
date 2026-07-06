use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::model_catalog::CatalogModelEntry;
use crate::paths::medousa_data_dir;

const MODELS_INDEX_FILE: &str = "models-index.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadPhase {
    Queued,
    Downloading,
    Verifying,
    Ready,
    Failed,
}

impl DownloadPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Verifying => "verifying",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadFileRecord {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelsIndex {
    #[serde(default)]
    models: HashMap<String, InstalledModelRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl ModelDownloadProgress {
    fn new(job_id: String, model_id: String) -> Self {
        Self {
            job_id,
            model_id,
            phase: DownloadPhase::Queued.as_str().to_string(),
            bytes_done: 0,
            bytes_total: 0,
            percent: 0.0,
            current_file: None,
            message: "Queued".to_string(),
            error: None,
        }
    }

    fn recompute_percent(&mut self) {
        self.percent = if self.bytes_total == 0 {
            0.0
        } else {
            ((self.bytes_done as f64 / self.bytes_total as f64) * 100.0) as f32
        };
    }
}

struct DownloadJobState {
    progress: ModelDownloadProgress,
    tx: broadcast::Sender<ModelDownloadProgress>,
}

pub struct ModelStore {
    index: Arc<RwLock<ModelsIndex>>,
    jobs: Arc<RwLock<HashMap<String, DownloadJobState>>>,
}

impl ModelStore {
    pub fn new() -> Self {
        let index = read_models_index().unwrap_or_default();
        Self {
            index: Arc::new(RwLock::new(index)),
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn models_dir() -> PathBuf {
        medousa_data_dir().join("models")
    }

    pub fn model_dir(model_id: &str) -> PathBuf {
        Self::models_dir().join(model_id)
    }

    pub async fn list_installed(&self) -> Vec<InstalledModelRecord> {
        self.index
            .read()
            .await
            .models
            .values()
            .cloned()
            .collect()
    }

    pub async fn get_installed(&self, model_id: &str) -> Option<InstalledModelRecord> {
        self.index.read().await.models.get(model_id).cloned()
    }

    pub async fn is_installed(&self, model_id: &str) -> bool {
        self.index.read().await.models.contains_key(model_id)
    }

    pub async fn local_repo_path(&self, model_id: &str) -> Option<String> {
        self.get_installed(model_id)
            .await
            .map(|record| record.local_path)
    }

    pub async fn get_job_progress(&self, job_id: &str) -> Option<ModelDownloadProgress> {
        self.jobs
            .read()
            .await
            .get(job_id)
            .map(|job| job.progress.clone())
    }

    pub async fn list_active_downloads(&self) -> Vec<ModelDownloadProgress> {
        self.jobs
            .read()
            .await
            .values()
            .map(|job| job.progress.clone())
            .filter(|progress| progress.phase != "ready" && progress.phase != "failed")
            .collect()
    }

    pub async fn subscribe_job_async(
        &self,
        job_id: &str,
    ) -> Option<broadcast::Receiver<ModelDownloadProgress>> {
        self.jobs
            .read()
            .await
            .get(job_id)
            .map(|job| job.tx.subscribe())
    }

    pub async fn start_download(
        &self,
        entry: CatalogModelEntry,
    ) -> Result<ModelDownloadProgress, String> {
        if self.is_installed(&entry.id).await {
            return Err(format!("model {} is already installed", entry.id));
        }

        for job in self.jobs.read().await.values() {
            if job.progress.model_id == entry.id
                && job.progress.phase != DownloadPhase::Failed.as_str()
                && job.progress.phase != DownloadPhase::Ready.as_str()
            {
                return Ok(job.progress.clone());
            }
        }

        let job_id = Uuid::new_v4().to_string();
        let (tx, _) = broadcast::channel(128);
        let progress = ModelDownloadProgress::new(job_id.clone(), entry.id.clone());
        self.jobs.write().await.insert(
            job_id.clone(),
            DownloadJobState {
                progress: progress.clone(),
                tx: tx.clone(),
            },
        );

        let store = self.clone_handle();
        tokio::spawn(async move {
            if let Err(err) = run_download_job(store.clone_handle(), entry, job_id.clone(), tx).await
            {
                store.fail_job(&job_id, err).await;
            }
        });

        Ok(progress)
    }

    pub async fn remove_model(&self, model_id: &str) -> Result<(), String> {
        let dir = Self::model_dir(model_id);
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .await
                .map_err(|err| format!("failed to remove model dir: {err}"))?;
        }
        self.index.write().await.models.remove(model_id);
        write_models_index(&self.index.read().await.clone())?;
        Ok(())
    }

    fn clone_handle(&self) -> Self {
        Self {
            index: self.index.clone(),
            jobs: self.jobs.clone(),
        }
    }

    async fn update_job<F>(&self, job_id: &str, update: F)
    where
        F: FnOnce(&mut ModelDownloadProgress),
    {
        let mut jobs = self.jobs.write().await;
        let Some(job) = jobs.get_mut(job_id) else {
            return;
        };
        update(&mut job.progress);
        job.progress.recompute_percent();
        let _ = job.tx.send(job.progress.clone());
    }

    async fn fail_job(&self, job_id: &str, message: String) {
        self.update_job(job_id, |progress| {
            progress.phase = DownloadPhase::Failed.as_str().to_string();
            progress.message = message.clone();
            progress.error = Some(message);
        })
        .await;
    }
}

impl Default for ModelStore {
    fn default() -> Self {
        Self::new()
    }
}

pub static MODEL_STORE: Lazy<Arc<ModelStore>> = Lazy::new(|| Arc::new(ModelStore::new()));

pub fn local_repo_if_installed(model_id: &str) -> Option<String> {
    read_models_index()
        .ok()
        .and_then(|index| {
            index
                .models
                .get(model_id)
                .map(|record| record.local_path.clone())
        })
}

fn read_models_index() -> Result<ModelsIndex, String> {
    let path = medousa_data_dir().join(MODELS_INDEX_FILE);
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(ModelsIndex::default()),
        Err(err) => return Err(err.to_string()),
    };
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

fn write_models_index(index: &ModelsIndex) -> Result<(), String> {
    let path = medousa_data_dir().join(MODELS_INDEX_FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string_pretty(index).map_err(|err| err.to_string())?;
    std::fs::write(path, json).map_err(|err| err.to_string())
}

#[derive(Debug, Deserialize)]
struct HfTreeEntry {
    path: String,
    size: Option<u64>,
    #[serde(rename = "type")]
    entry_type: String,
}

pub fn include_hf_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".md") || lower.contains("readme") {
        return false;
    }
    if lower.ends_with(".gitattributes") || lower.ends_with(".gitignore") {
        return false;
    }
    lower.ends_with(".json")
        || lower.ends_with(".safetensors")
        || lower.ends_with(".uqff")
        || lower.ends_with(".model")
        || lower.ends_with(".tiktoken")
        || (lower.ends_with(".txt") && lower.contains("merges"))
}

fn hf_auth_header() -> Option<String> {
    std::env::var("HF_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("Bearer {}", value.trim()))
}

async fn list_hf_files(repo: &str) -> Result<Vec<(String, u64)>, String> {
    let url = format!("https://huggingface.co/api/models/{repo}/tree/main?recursive=1");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|err| err.to_string())?;
    let mut request = client.get(url);
    if let Some(auth) = hf_auth_header() {
        request = request.header(reqwest::header::AUTHORIZATION, auth);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HF tree listing failed ({status}): {body}"));
    }
    let entries: Vec<HfTreeEntry> = response.json().await.map_err(|err| err.to_string())?;
    Ok(entries
        .into_iter()
        .filter(|entry| entry.entry_type == "file" && include_hf_file(&entry.path))
        .map(|entry| (entry.path, entry.size.unwrap_or(0)))
        .collect())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

async fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).await.map_err(|err| err.to_string())?;
    Ok(sha256_hex(&bytes))
}

async fn download_hf_file(
    repo: &str,
    relative_path: &str,
    destination: &Path,
) -> Result<u64, String> {
    fs::create_dir_all(
        destination
            .parent()
            .ok_or_else(|| "invalid destination path".to_string())?,
    )
    .await
    .map_err(|err| err.to_string())?;

    let existing = fs::metadata(destination)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);
    let url = format!(
        "https://huggingface.co/{repo}/resolve/main/{}",
        relative_path.replace('\\', "/")
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|err| err.to_string())?;

    let mut request = client.get(url);
    if let Some(auth) = hf_auth_header() {
        request = request.header(reqwest::header::AUTHORIZATION, auth);
    }
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }

    let response = request.send().await.map_err(|err| err.to_string())?;
    if !(response.status().is_success()
        || response.status() == reqwest::StatusCode::PARTIAL_CONTENT)
    {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("download failed for {relative_path} ({status}): {body}"));
    }

    let mut file = if existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        OpenOptions::new()
            .append(true)
            .open(destination)
            .await
            .map_err(|err| err.to_string())?
    } else {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(destination)
            .await
            .map_err(|err| err.to_string())?
    };

    let mut downloaded = existing;
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        file.write_all(&chunk).await.map_err(|err| err.to_string())?;
        downloaded += chunk.len() as u64;
    }
    file.flush().await.map_err(|err| err.to_string())?;
    Ok(downloaded)
}

async fn run_download_job(
    store: ModelStore,
    entry: CatalogModelEntry,
    job_id: String,
    _tx: broadcast::Sender<ModelDownloadProgress>,
) -> Result<(), String> {
    store
        .update_job(&job_id, |progress| {
            progress.phase = DownloadPhase::Downloading.as_str().to_string();
            progress.message = format!("Listing files for {}", entry.repo);
        })
        .await;

    let files = list_hf_files(&entry.repo).await?;
    if files.is_empty() {
        return Err(format!("no downloadable files found for {}", entry.repo));
    }

    let bytes_total: u64 = files.iter().map(|(_, size)| *size).sum();
    store
        .update_job(&job_id, |progress| {
            progress.bytes_total = bytes_total.max(entry.size_bytes);
            progress.message = format!("Downloading {} files", files.len());
        })
        .await;

    let model_dir = ModelStore::model_dir(&entry.id);
    fs::create_dir_all(&model_dir)
        .await
        .map_err(|err| err.to_string())?;

    let mut file_records = Vec::new();
    let mut bytes_done = 0u64;
    for (relative_path, _expected_size) in files {
        let destination = model_dir.join(&relative_path);
        store
            .update_job(&job_id, |progress| {
                progress.current_file = Some(relative_path.clone());
                progress.message = format!("Downloading {relative_path}");
                progress.bytes_done = bytes_done;
            })
            .await;

        let file_bytes = download_hf_file(&entry.repo, &relative_path, &destination).await?;
        bytes_done = bytes_done.saturating_add(file_bytes);
        store
            .update_job(&job_id, |progress| {
                progress.bytes_done = bytes_done;
            })
            .await;

        store
            .update_job(&job_id, |progress| {
                progress.phase = DownloadPhase::Verifying.as_str().to_string();
                progress.message = format!("Verifying {relative_path}");
            })
            .await;

        let digest = sha256_file(&destination).await?;
        let bytes = fs::metadata(&destination)
            .await
            .map_err(|err| err.to_string())?
            .len();
        file_records.push(DownloadFileRecord {
            path: relative_path,
            bytes,
            sha256: digest,
        });
    }

    let bytes_on_disk: u64 = file_records.iter().map(|file| file.bytes).sum();
    let record = InstalledModelRecord {
        model_id: entry.id.clone(),
        repo: entry.repo.clone(),
        local_path: model_dir.to_string_lossy().to_string(),
        installed_at: Utc::now(),
        bytes_on_disk,
        verified: true,
        files: file_records,
    };

    store
        .index
        .write()
        .await
        .models
        .insert(entry.id.clone(), record);
    write_models_index(&store.index.read().await.clone())?;

    store
        .update_job(&job_id, |progress| {
            progress.phase = DownloadPhase::Ready.as_str().to_string();
            progress.bytes_done = progress.bytes_total.max(bytes_on_disk);
            progress.percent = 100.0;
            progress.current_file = None;
            progress.message = "Download complete".to_string();
            progress.error = None;
        })
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_hf_file_filters_docs() {
        assert!(include_hf_file("config.json"));
        assert!(include_hf_file("model-00001-of-00002.safetensors"));
        assert!(!include_hf_file("README.md"));
    }
}
