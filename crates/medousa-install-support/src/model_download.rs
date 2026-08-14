use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use chrono::{DateTime, Utc};
use medousa_types::authority_id::ModelId;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{RwLock, broadcast};
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

    pub fn model_dir(model_id: &str) -> Result<PathBuf, String> {
        let model_id = ModelId::parse(model_id).map_err(|error| error.to_string())?;
        let models = Self::models_dir();
        let opaque = models.join(model_id.storage_key().as_str());
        if opaque.exists() {
            return Ok(opaque);
        }
        let legacy = models.join(model_id.as_str());
        if legacy
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        {
            return Ok(legacy);
        }
        Ok(opaque)
    }

    pub async fn list_installed(&self) -> Vec<InstalledModelRecord> {
        self.index.read().await.models.values().cloned().collect()
    }

    pub async fn get_installed(&self, model_id: &str) -> Option<InstalledModelRecord> {
        self.index.read().await.models.get(model_id).cloned()
    }

    pub async fn is_installed(&self, model_id: &str) -> bool {
        self.index.read().await.models.contains_key(model_id)
    }

    pub async fn local_repo_path(&self, model_id: &str) -> Option<String> {
        self.get_installed(model_id).await?;
        Self::model_dir(model_id)
            .ok()
            .map(|path| path.to_string_lossy().to_string())
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
        ModelId::parse(&entry.id).map_err(|error| error.to_string())?;
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
            if let Err(err) =
                run_download_job(store.clone_handle(), entry, job_id.clone(), tx).await
            {
                store.fail_job(&job_id, err).await;
            }
        });

        Ok(progress)
    }

    pub async fn remove_model(&self, model_id: &str) -> Result<(), String> {
        let dir = Self::model_dir(model_id)?;
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
        .and_then(|index| index.models.contains_key(model_id).then_some(index))
        .and_then(|_| ModelStore::model_dir(model_id).ok())
        .map(|path| path.to_string_lossy().to_string())
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
    if validated_model_relative_path(path).is_err() {
        return false;
    }
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

fn validated_model_relative_path(path: &str) -> Result<PathBuf, String> {
    if path.is_empty() || path.len() > 1024 || !path.is_ascii() {
        return Err("invalid model file path".to_string());
    }
    if path.starts_with(['/', '\\']) || path.contains('\\') {
        return Err("invalid model file path".to_string());
    }
    let segments = path.split('/').collect::<Vec<_>>();
    if segments.len() > 32 {
        return Err("invalid model file path".to_string());
    }
    for segment in &segments {
        if segment.is_empty()
            || matches!(*segment, "." | "..")
            || segment.ends_with(['.', ' '])
            || !segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        {
            return Err("invalid model file path".to_string());
        }
        let base = segment.split_once('.').map_or(*segment, |(base, _)| base);
        let upper = base.to_ascii_uppercase();
        if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || upper
                .strip_prefix("COM")
                .or_else(|| upper.strip_prefix("LPT"))
                .is_some_and(|suffix| {
                    suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
                })
        {
            return Err("invalid model file path".to_string());
        }
    }
    Ok(segments.iter().collect())
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

struct ModelPayloadStore {
    ambient_root: PathBuf,
    root: Dir,
}

impl ModelPayloadStore {
    fn open() -> Result<Self, String> {
        Self::open_at(ModelStore::models_dir())
    }

    fn open_at(ambient_root: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&ambient_root).map_err(|error| error.to_string())?;
        let root = Dir::open_ambient_dir(&ambient_root, ambient_authority())
            .map_err(|error| error.to_string())?;
        Ok(Self { ambient_root, root })
    }

    fn open_model(&self, model_id: &ModelId) -> Result<Dir, String> {
        let storage_key = model_id.storage_key();
        let name = storage_key.as_str();
        match self.root.create_dir(name) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
        self.root
            .open_dir_nofollow(name)
            .map_err(|error| error.to_string())
    }

    fn ambient_model_dir(&self, model_id: &ModelId) -> PathBuf {
        self.ambient_root.join(model_id.storage_key().as_str())
    }
}

fn open_model_file(
    model_dir: &Dir,
    relative_path: &Path,
    existing: u64,
) -> Result<cap_std::fs::File, String> {
    let mut components = relative_path.components().peekable();
    let mut current = model_dir.try_clone().map_err(|error| error.to_string())?;
    while let Some(component) = components.next() {
        let segment = component.as_os_str();
        if components.peek().is_none() {
            let mut options = OpenOptions::new();
            options
                .write(true)
                .create(true)
                .append(existing > 0)
                .truncate(existing == 0)
                .follow(FollowSymlinks::No);
            return current
                .open_with(segment, &options)
                .map_err(|error| error.to_string());
        }
        match current.create_dir(segment) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
        current = current
            .open_dir_nofollow(segment)
            .map_err(|error| error.to_string())?;
    }
    Err("invalid empty model file path".to_string())
}

fn model_file_len(model_dir: &Dir, relative_path: &Path) -> Result<u64, String> {
    match model_dir.symlink_metadata(relative_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("model file path is a symbolic link".to_string())
        }
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => Err("model file path is not a regular file".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.to_string()),
    }
}

async fn sha256_model_file(model_dir: &Dir, relative_path: &Path) -> Result<String, String> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = model_dir
        .open_with(relative_path, &options)
        .map_err(|error| error.to_string())?;
    let mut file = tokio::fs::File::from_std(file.into_std());
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn download_hf_file(
    repo: &str,
    relative_path: &str,
    model_dir: &Dir,
    destination: &Path,
) -> Result<u64, String> {
    let existing = model_file_len(model_dir, destination)?;
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
        return Err(format!(
            "download failed for {relative_path} ({status}): {body}"
        ));
    }

    let resumed = existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let file = if resumed {
        open_model_file(model_dir, destination, existing)?
    } else {
        open_model_file(model_dir, destination, 0)?
    };
    let mut file = tokio::fs::File::from_std(file.into_std());

    let mut downloaded = if resumed { existing } else { 0 };
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        file.write_all(&chunk)
            .await
            .map_err(|err| err.to_string())?;
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

    let model_id = ModelId::parse(&entry.id).map_err(|error| error.to_string())?;
    let payload_store = ModelPayloadStore::open()?;
    let model_dir = payload_store.open_model(&model_id)?;
    let ambient_model_dir = payload_store.ambient_model_dir(&model_id);

    let mut file_records = Vec::new();
    let mut bytes_done = 0u64;
    for (relative_path, _expected_size) in files {
        let destination = validated_model_relative_path(&relative_path)?;
        store
            .update_job(&job_id, |progress| {
                progress.current_file = Some(relative_path.clone());
                progress.message = format!("Downloading {relative_path}");
                progress.bytes_done = bytes_done;
            })
            .await;

        let file_bytes =
            download_hf_file(&entry.repo, &relative_path, &model_dir, &destination).await?;
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

        let digest = sha256_model_file(&model_dir, &destination).await?;
        let bytes = model_file_len(&model_dir, &destination)?;
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
        local_path: ambient_model_dir.to_string_lossy().to_string(),
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
    use std::io::Write as _;

    #[test]
    fn include_hf_file_filters_docs() {
        assert!(include_hf_file("config.json"));
        assert!(include_hf_file("model-00001-of-00002.safetensors"));
        assert!(!include_hf_file("README.md"));
        assert!(!include_hf_file("../../outside.json"));
        assert!(!include_hf_file("C:\\outside.json"));
        assert!(!include_hf_file("nested/CON.json"));
    }

    #[test]
    fn model_ids_map_to_opaque_directories() {
        let path = ModelStore::model_dir("gemma-4-12b-it").unwrap();
        assert!(
            !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("gemma")
        );
        assert!(ModelStore::model_dir("../../outside").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn payload_writes_survive_ambient_root_replacement() {
        let root = std::env::temp_dir().join(format!(
            "medousa-model-capability-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let held = root.with_extension("held");
        let store = ModelPayloadStore::open_at(root.clone()).unwrap();
        let model_id = ModelId::parse("replacement-model").unwrap();
        let model_dir = store.open_model(&model_id).unwrap();

        std::fs::rename(&root, &held).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let relative = Path::new("nested/config.json");
        let mut file = open_model_file(&model_dir, relative, 0).unwrap();
        file.write_all(b"held authority").unwrap();
        file.sync_data().unwrap();

        let storage_key = model_id.storage_key();
        assert_eq!(
            std::fs::read(held.join(storage_key.as_str()).join(relative)).unwrap(),
            b"held authority"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&held).ok();
    }

    #[cfg(unix)]
    #[test]
    fn payload_writes_reject_link_backed_ancestors() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "medousa-model-link-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let outside = root.with_extension("outside");
        let store = ModelPayloadStore::open_at(root.clone()).unwrap();
        let model_id = ModelId::parse("link-model").unwrap();
        let model_dir = store.open_model(&model_id).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("canary"), b"outside").unwrap();
        let storage_key = model_id.storage_key();
        symlink(&outside, root.join(storage_key.as_str()).join("nested")).unwrap();

        assert!(open_model_file(&model_dir, Path::new("nested/config.json"), 0).is_err());
        assert_eq!(std::fs::read(outside.join("canary")).unwrap(), b"outside");

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }
}
