//! Workshop storage accounting and regenerable Forge cache governance.
//!
//! Only repository-group `.cache` directories under Forge worktrees are
//! eviction candidates. Worktrees, Forge custody, Detamu, artifacts, and Coder
//! evidence are measured but never deleted here.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use medousa_forge::forge::Forge;
use medousa_forge::model::WorkState;
use serde::{Deserialize, Serialize};

use crate::daemon::state::AppState;

const GIB: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_REPOSITORY_CACHE_MAX_BYTES: u64 = 10 * GIB;
pub const DEFAULT_GLOBAL_CACHE_MAX_BYTES: u64 = 30 * GIB;
pub const DEFAULT_FREE_DISK_FLOOR_BYTES: u64 = 10 * GIB;
pub const DEFAULT_MIN_INACTIVE_AGE_HOURS: u64 = 24;
static MAINTENANCE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageGovernorSettings {
    pub enabled: bool,
    pub repository_cache_max_bytes: u64,
    pub global_cache_max_bytes: u64,
    pub free_disk_floor_bytes: u64,
    pub min_inactive_age_hours: u64,
}

impl Default for StorageGovernorSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            repository_cache_max_bytes: DEFAULT_REPOSITORY_CACHE_MAX_BYTES,
            global_cache_max_bytes: DEFAULT_GLOBAL_CACHE_MAX_BYTES,
            free_disk_floor_bytes: DEFAULT_FREE_DISK_FLOOR_BYTES,
            min_inactive_age_hours: DEFAULT_MIN_INACTIVE_AGE_HOURS,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageCategoryUsage {
    pub physical_bytes: u64,
    pub file_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeCacheUsage {
    pub repository_key: String,
    pub physical_bytes: u64,
    pub file_count: u64,
    pub last_used_unix_seconds: u64,
    pub protected: bool,
    pub protection_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageUsageReport {
    pub settings: StorageGovernorSettings,
    pub data_root: String,
    pub available_disk_bytes: Option<u64>,
    pub total_managed_bytes: u64,
    pub forge_metadata: StorageCategoryUsage,
    pub forge_worktrees: StorageCategoryUsage,
    pub build_caches: StorageCategoryUsage,
    pub detamu: StorageCategoryUsage,
    pub artifacts: StorageCategoryUsage,
    pub coder_evidence: StorageCategoryUsage,
    pub forge_caches: Vec<ForgeCacheUsage>,
    pub scan_warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StorageMaintenanceRequest {
    /// Preview is the safe default; pass `false` to delete selected regenerable
    /// cache roots.
    pub dry_run: bool,
}

impl Default for StorageMaintenanceRequest {
    fn default() -> Self {
        Self { dry_run: true }
    }
}

impl StorageMaintenanceRequest {
    fn normalized_dry_run(&self) -> bool {
        self.dry_run
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageEvictionAction {
    pub repository_key: String,
    pub physical_bytes: u64,
    pub reason: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageMaintenanceReport {
    pub enabled: bool,
    pub dry_run: bool,
    pub before: StorageUsageReport,
    pub after: StorageUsageReport,
    pub selected_bytes: u64,
    pub reclaimed_bytes: u64,
    pub actions: Vec<StorageEvictionAction>,
    pub pressure_remaining: bool,
}

#[derive(Debug, Clone, Default)]
struct TreeUsage {
    physical_bytes: u64,
    file_count: u64,
    newest_modified_unix_seconds: u64,
}

#[derive(Debug, Clone)]
struct CacheCandidate {
    usage: ForgeCacheUsage,
    path: PathBuf,
}

pub fn settings_path(data_root: &Path) -> PathBuf {
    data_root.join("storage_governor.json")
}

pub fn load_settings(data_root: &Path) -> StorageGovernorSettings {
    let path = settings_path(data_root);
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_settings(data_root: &Path, settings: &StorageGovernorSettings) -> Result<(), String> {
    validate_settings(settings)?;
    fs::create_dir_all(data_root).map_err(|err| err.to_string())?;
    let path = settings_path(data_root);
    let bytes = serde_json::to_vec_pretty(settings).map_err(|err| err.to_string())?;
    crate::session::atomic_write(&path, &bytes).map_err(|err| err.to_string())?;
    Ok(())
}

fn validate_settings(settings: &StorageGovernorSettings) -> Result<(), String> {
    if settings.global_cache_max_bytes > 0
        && settings.repository_cache_max_bytes > settings.global_cache_max_bytes
    {
        return Err(
            "repository_cache_max_bytes cannot exceed global_cache_max_bytes when both caps are enabled"
                .to_string(),
        );
    }
    Ok(())
}

pub fn storage_usage_report(
    data_root: &Path,
    forge: &Forge,
    settings: StorageGovernorSettings,
) -> Result<StorageUsageReport, String> {
    validate_settings(&settings)?;
    let forge_root = forge.store().root();
    let worktrees_root = forge_root.join("worktrees");
    let protections = repository_protections(forge)?;
    let mut warnings = Vec::new();
    let candidates = scan_cache_candidates(&worktrees_root, &protections, &mut warnings);
    let cache_paths = candidates
        .iter()
        .map(|candidate| candidate.path.clone())
        .collect::<HashSet<_>>();

    let forge_metadata_exclusions = HashSet::from([worktrees_root.clone()]);
    let forge_metadata = scan_tree_excluding(forge_root, &forge_metadata_exclusions, &mut warnings);
    let forge_worktrees = scan_tree_excluding(&worktrees_root, &cache_paths, &mut warnings);
    let build_caches = candidates
        .iter()
        .fold(TreeUsage::default(), |mut total, candidate| {
            total.physical_bytes = total
                .physical_bytes
                .saturating_add(candidate.usage.physical_bytes);
            total.file_count = total.file_count.saturating_add(candidate.usage.file_count);
            total.newest_modified_unix_seconds = total
                .newest_modified_unix_seconds
                .max(candidate.usage.last_used_unix_seconds);
            total
        });
    let detamu = scan_tree(&data_root.join("detamu"), &mut warnings);
    let artifacts = scan_tree(&data_root.join("artifacts"), &mut warnings);
    let coder_evidence = scan_tree(&data_root.join("coder-evidence"), &mut warnings);
    let categories = [
        &forge_metadata,
        &forge_worktrees,
        &build_caches,
        &detamu,
        &artifacts,
        &coder_evidence,
    ];
    let total_managed_bytes = categories.iter().fold(0u64, |total, usage| {
        total.saturating_add(usage.physical_bytes)
    });
    let available_disk_bytes = fs2::available_space(data_root)
        .or_else(|_| {
            data_root
                .parent()
                .ok_or_else(|| std::io::Error::other("data root has no parent"))
                .and_then(fs2::available_space)
        })
        .ok();

    Ok(StorageUsageReport {
        settings,
        data_root: data_root.display().to_string(),
        available_disk_bytes,
        total_managed_bytes,
        forge_metadata: public_usage(forge_metadata),
        forge_worktrees: public_usage(forge_worktrees),
        build_caches: public_usage(build_caches),
        detamu: public_usage(detamu),
        artifacts: public_usage(artifacts),
        coder_evidence: public_usage(coder_evidence),
        forge_caches: candidates
            .into_iter()
            .map(|candidate| candidate.usage)
            .collect(),
        scan_warnings: warnings,
    })
}

pub fn maintain_storage(
    data_root: &Path,
    forge: &Forge,
    settings: StorageGovernorSettings,
    dry_run: bool,
) -> Result<StorageMaintenanceReport, String> {
    let _maintenance_guard = MAINTENANCE_LOCK
        .lock()
        .map_err(|_| "storage maintenance lock is poisoned".to_string())?;
    let before = storage_usage_report(data_root, forge, settings.clone())?;
    let candidate_paths = cache_paths_by_key(forge.store().root());
    let now = unix_seconds(SystemTime::now());
    let min_age_seconds = settings.min_inactive_age_hours.saturating_mul(60 * 60);
    let mut eligible = before
        .forge_caches
        .iter()
        .filter(|cache| !cache.protected)
        .filter(|cache| {
            now.saturating_sub(cache.last_used_unix_seconds) >= min_age_seconds
                || free_floor_deficit(&before, &settings) > 0
        })
        .cloned()
        .collect::<Vec<_>>();
    eligible.sort_by_key(|cache| cache.last_used_unix_seconds);

    let mut selected = HashMap::<String, String>::new();
    if settings.repository_cache_max_bytes > 0 {
        for cache in &eligible {
            if cache.physical_bytes > settings.repository_cache_max_bytes {
                selected.insert(
                    cache.repository_key.clone(),
                    "repository_cache_cap".to_string(),
                );
            }
        }
    }
    let mut selected_bytes = selected_bytes(&before.forge_caches, &selected);
    let global_excess = if settings.global_cache_max_bytes == 0 {
        0
    } else {
        before
            .build_caches
            .physical_bytes
            .saturating_sub(selected_bytes)
            .saturating_sub(settings.global_cache_max_bytes)
    };
    let floor_deficit = free_floor_deficit(&before, &settings).saturating_sub(selected_bytes);
    let mut additional_needed = global_excess.max(floor_deficit);
    for cache in &eligible {
        if additional_needed == 0 {
            break;
        }
        if selected.contains_key(&cache.repository_key) {
            continue;
        }
        let reason = if floor_deficit > global_excess {
            "free_disk_floor"
        } else {
            "global_cache_cap"
        };
        selected.insert(cache.repository_key.clone(), reason.to_string());
        selected_bytes = selected_bytes.saturating_add(cache.physical_bytes);
        additional_needed = additional_needed.saturating_sub(cache.physical_bytes);
    }

    let mut actions = Vec::new();
    let mut reclaimed_bytes = 0u64;
    for cache in &before.forge_caches {
        let Some(reason) = selected.get(&cache.repository_key) else {
            continue;
        };
        let mut status = "planned".to_string();
        if !dry_run {
            let protections = repository_protections(forge)?;
            if protections.contains_key(&cache.repository_key) {
                status = "skipped_became_protected".to_string();
            } else if let Some(path) = candidate_paths.get(&cache.repository_key) {
                match fs::remove_dir_all(path) {
                    Ok(()) => {
                        status = "evicted".to_string();
                        reclaimed_bytes = reclaimed_bytes.saturating_add(cache.physical_bytes);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                        status = "already_absent".to_string();
                    }
                    Err(err) => status = format!("failed:{err}"),
                }
            } else {
                status = "already_absent".to_string();
            }
        }
        actions.push(StorageEvictionAction {
            repository_key: cache.repository_key.clone(),
            physical_bytes: cache.physical_bytes,
            reason: reason.clone(),
            status,
        });
    }

    let after = if dry_run {
        before.clone()
    } else {
        storage_usage_report(data_root, forge, settings.clone())?
    };
    let pressure_remaining = cache_pressure(&after, &settings);
    Ok(StorageMaintenanceReport {
        enabled: settings.enabled,
        dry_run,
        before,
        after,
        selected_bytes,
        reclaimed_bytes,
        actions,
        pressure_remaining,
    })
}

fn free_floor_deficit(report: &StorageUsageReport, settings: &StorageGovernorSettings) -> u64 {
    report
        .available_disk_bytes
        .map(|available| settings.free_disk_floor_bytes.saturating_sub(available))
        .unwrap_or(0)
}

fn cache_pressure(report: &StorageUsageReport, settings: &StorageGovernorSettings) -> bool {
    (settings.global_cache_max_bytes > 0
        && report.build_caches.physical_bytes > settings.global_cache_max_bytes)
        || (settings.repository_cache_max_bytes > 0
            && report
                .forge_caches
                .iter()
                .any(|cache| cache.physical_bytes > settings.repository_cache_max_bytes))
        || free_floor_deficit(report, settings) > 0
}

fn selected_bytes(caches: &[ForgeCacheUsage], selected: &HashMap<String, String>) -> u64 {
    caches
        .iter()
        .filter(|cache| selected.contains_key(&cache.repository_key))
        .fold(0u64, |total, cache| {
            total.saturating_add(cache.physical_bytes)
        })
}

fn repository_protections(forge: &Forge) -> Result<HashMap<String, String>, String> {
    let mut protections = HashMap::new();
    for item in forge.list().map_err(|err| err.to_string())? {
        if item.state.is_terminal() {
            continue;
        }
        let Some(environment) = item.environment.as_ref() else {
            continue;
        };
        let Some(group) = environment
            .worktree
            .parent()
            .and_then(Path::file_name)
            .map(|value| value.to_string_lossy().into_owned())
        else {
            continue;
        };
        let reason = if item.has_active_attempts() || item.state == WorkState::Executing {
            format!("active_attempt:{}", item.id)
        } else {
            format!("non_terminal_work:{}:{}", item.id, item.state)
        };
        protections.insert(group, reason);
    }
    Ok(protections)
}

fn scan_cache_candidates(
    worktrees_root: &Path,
    protections: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Vec<CacheCandidate> {
    let mut candidates = Vec::new();
    let Ok(groups) = fs::read_dir(worktrees_root) else {
        return candidates;
    };
    for group in groups.flatten() {
        let Ok(file_type) = group.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let repository_key = group.file_name().to_string_lossy().into_owned();
        let path = group.path().join(".cache");
        if !is_real_directory(&path) {
            continue;
        }
        let usage = scan_tree(&path, warnings);
        let protection_reason = protections.get(&repository_key).cloned();
        candidates.push(CacheCandidate {
            usage: ForgeCacheUsage {
                repository_key,
                physical_bytes: usage.physical_bytes,
                file_count: usage.file_count,
                last_used_unix_seconds: usage.newest_modified_unix_seconds,
                protected: protection_reason.is_some(),
                protection_reason,
            },
            path,
        });
    }
    candidates.sort_by(|a, b| a.usage.repository_key.cmp(&b.usage.repository_key));
    candidates
}

fn cache_paths_by_key(forge_root: &Path) -> HashMap<String, PathBuf> {
    let mut paths = HashMap::new();
    let worktrees = forge_root.join("worktrees");
    let Ok(groups) = fs::read_dir(worktrees) else {
        return paths;
    };
    for group in groups.flatten() {
        let path = group.path().join(".cache");
        if is_real_directory(&path) {
            paths.insert(group.file_name().to_string_lossy().into_owned(), path);
        }
    }
    paths
}

fn is_real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn scan_tree(path: &Path, warnings: &mut Vec<String>) -> TreeUsage {
    scan_tree_excluding(path, &HashSet::new(), warnings)
}

fn scan_tree_excluding(
    path: &Path,
    excluded: &HashSet<PathBuf>,
    warnings: &mut Vec<String>,
) -> TreeUsage {
    if excluded.contains(path) || !path.exists() {
        return TreeUsage::default();
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) => {
            warnings.push(format!("cannot inspect {}: {err}", path.display()));
            return TreeUsage::default();
        }
    };
    let mut usage = TreeUsage {
        physical_bytes: allocated_bytes(&metadata),
        file_count: u64::from(metadata.is_file() || metadata.file_type().is_symlink()),
        newest_modified_unix_seconds: metadata.modified().map(unix_seconds).unwrap_or(0),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return usage;
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) => {
            warnings.push(format!("cannot read {}: {err}", path.display()));
            return usage;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let child = scan_tree_excluding(&entry.path(), excluded, warnings);
                usage.physical_bytes = usage.physical_bytes.saturating_add(child.physical_bytes);
                usage.file_count = usage.file_count.saturating_add(child.file_count);
                usage.newest_modified_unix_seconds = usage
                    .newest_modified_unix_seconds
                    .max(child.newest_modified_unix_seconds);
            }
            Err(err) => warnings.push(format!("cannot enumerate {}: {err}", path.display())),
        }
    }
    usage
}

#[cfg(unix)]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn public_usage(usage: TreeUsage) -> StorageCategoryUsage {
    StorageCategoryUsage {
        physical_bytes: usage.physical_bytes,
        file_count: usage.file_count,
    }
}

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(serde_json::json!({ "ok": false, "error": message.into() })),
    )
}

pub async fn get_storage_status(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let forge = state.forge.clone();
    let data_root = crate::paths::medousa_data_dir();
    let settings = load_settings(&data_root);
    let report =
        tokio::task::spawn_blocking(move || storage_usage_report(&data_root, &forge, settings))
            .await
            .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
            .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err))?;
    Ok(Json(report))
}

pub async fn put_storage_settings(
    State(state): State<AppState>,
    Json(settings): Json<StorageGovernorSettings>,
) -> Result<impl IntoResponse, ApiError> {
    let forge = state.forge.clone();
    let data_root = crate::paths::medousa_data_dir();
    let report = tokio::task::spawn_blocking(move || {
        save_settings(&data_root, &settings)?;
        storage_usage_report(&data_root, &forge, settings)
    })
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    .map_err(|err| api_error(StatusCode::BAD_REQUEST, err))?;
    *state.last_storage_maintenance_at.write().await = None;
    Ok(Json(report))
}

pub async fn post_storage_maintenance(
    State(state): State<AppState>,
    Json(request): Json<StorageMaintenanceRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let forge = state.forge.clone();
    let data_root = crate::paths::medousa_data_dir();
    let settings = load_settings(&data_root);
    let dry_run = request.normalized_dry_run();
    let report = tokio::task::spawn_blocking(move || {
        maintain_storage(&data_root, &forge, settings, dry_run)
    })
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err))?;
    Ok(Json(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_forge::forge::Forge;
    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use medousa_forge::model::{ActorKind, ActorRef};

    fn write_sized(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    fn actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "storage-test".to_string(),
        }
    }

    fn init_repo(path: &Path) {
        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(path)
            .status()
            .unwrap();
        fs::write(path.join("README.md"), "# fixture\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(path)
            .status()
            .unwrap();
        GitEngine::detect()
            .unwrap()
            .commit_checkpoint(path, "initial", &CheckpointAuthor::default())
            .unwrap();
    }

    fn set_tree_modified(path: &Path, modified: SystemTime) {
        let times = fs::FileTimes::new().set_modified(modified);
        fs::File::open(path).unwrap().set_times(times).unwrap();
        if path.is_dir() {
            for entry in fs::read_dir(path).unwrap().flatten() {
                set_tree_modified(&entry.path(), modified);
            }
        }
    }

    #[test]
    fn accounting_separates_cache_from_worktree_and_other_authorities() {
        let temp = tempfile::TempDir::new().unwrap();
        let data = temp.path();
        let forge_root = data.join("forge");
        let forge = Forge::open(&forge_root).unwrap();
        write_sized(
            &forge_root.join("worktrees/repo-a/work-1/src/lib.rs"),
            4_096,
        );
        write_sized(
            &forge_root.join("worktrees/repo-a/.cache/cargo-target/object.o"),
            16_384,
        );
        write_sized(&data.join("detamu/store.surrealkv/data"), 8_192);
        write_sized(&data.join("artifacts/session/output.json"), 4_096);

        let report =
            storage_usage_report(data, &forge, StorageGovernorSettings::default()).expect("report");
        assert_eq!(report.forge_caches.len(), 1);
        assert!(report.build_caches.physical_bytes > 0);
        assert!(report.forge_worktrees.physical_bytes > 0);
        assert!(report.detamu.physical_bytes > 0);
        assert!(report.artifacts.physical_bytes > 0);
        assert_eq!(report.coder_evidence.physical_bytes, 0);
        assert!(report.scan_warnings.is_empty());
    }

    #[test]
    fn maintenance_dry_run_plans_without_deleting() {
        let temp = tempfile::TempDir::new().unwrap();
        let data = temp.path();
        let forge = Forge::open(data.join("forge")).unwrap();
        let cache = data.join("forge/worktrees/repo-old/.cache/cargo-target/object.o");
        write_sized(&cache, 16_384);
        let settings = StorageGovernorSettings {
            enabled: false,
            repository_cache_max_bytes: 1,
            global_cache_max_bytes: 1,
            free_disk_floor_bytes: 0,
            min_inactive_age_hours: 0,
        };

        let report = maintain_storage(data, &forge, settings, true).expect("dry run");
        assert!(cache.exists());
        assert!(!report.enabled);
        assert_eq!(report.actions.len(), 1);
        assert_eq!(report.actions[0].status, "planned");
        assert_eq!(report.reclaimed_bytes, 0);
    }

    #[test]
    fn maintenance_evicts_only_explicit_inactive_cache_roots() {
        let temp = tempfile::TempDir::new().unwrap();
        let data = temp.path();
        let forge = Forge::open(data.join("forge")).unwrap();
        let worktree_file = data.join("forge/worktrees/repo-old/work-1/src/lib.rs");
        let cache_file = data.join("forge/worktrees/repo-old/.cache/cargo-target/object.o");
        write_sized(&worktree_file, 4_096);
        write_sized(&cache_file, 16_384);
        let settings = StorageGovernorSettings {
            repository_cache_max_bytes: 1,
            global_cache_max_bytes: 1,
            free_disk_floor_bytes: 0,
            min_inactive_age_hours: 0,
            ..StorageGovernorSettings::default()
        };

        let report = maintain_storage(data, &forge, settings, false).expect("maintenance");
        assert!(worktree_file.exists());
        assert!(!cache_file.exists());
        assert_eq!(report.actions[0].status, "evicted");
        assert!(report.reclaimed_bytes > 0);
    }

    #[test]
    fn non_terminal_forge_work_protects_its_repository_cache() {
        let temp = tempfile::TempDir::new().unwrap();
        let data = temp.path();
        let repo = data.join("repo");
        fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let forge = Forge::open(data.join("forge")).unwrap();
        let item = forge
            .register("active", "protect cache", &repo, "main", "user", &actor())
            .unwrap();
        let item = forge.provision(&item.id, &actor()).unwrap();
        let environment = item.environment.expect("environment");
        let group = environment.worktree.parent().unwrap();
        let cache_file = group.join(".cache/cargo-target/object.o");
        write_sized(&cache_file, 16_384);
        let settings = StorageGovernorSettings {
            repository_cache_max_bytes: 1,
            global_cache_max_bytes: 1,
            free_disk_floor_bytes: 0,
            min_inactive_age_hours: 0,
            ..StorageGovernorSettings::default()
        };

        let report = maintain_storage(data, &forge, settings, false).expect("maintenance");
        assert!(cache_file.exists());
        assert!(report.actions.is_empty());
        assert!(report.before.forge_caches[0].protected);
        assert!(
            report.before.forge_caches[0]
                .protection_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("non_terminal_work"))
        );
        assert!(report.pressure_remaining);
    }

    #[test]
    fn global_pressure_selects_oldest_inactive_cache_first() {
        let temp = tempfile::TempDir::new().unwrap();
        let data = temp.path();
        let forge = Forge::open(data.join("forge")).unwrap();
        let old_root = data.join("forge/worktrees/repo-old/.cache");
        let new_root = data.join("forge/worktrees/repo-new/.cache");
        write_sized(&old_root.join("target/old.o"), 16_384);
        write_sized(&new_root.join("target/new.o"), 16_384);
        set_tree_modified(
            &old_root,
            SystemTime::now() - std::time::Duration::from_secs(7 * 24 * 60 * 60),
        );
        let initial = storage_usage_report(data, &forge, StorageGovernorSettings::default())
            .expect("initial report");
        let one_cache = initial
            .forge_caches
            .iter()
            .map(|cache| cache.physical_bytes)
            .max()
            .unwrap();
        let settings = StorageGovernorSettings {
            repository_cache_max_bytes: 0,
            global_cache_max_bytes: one_cache.saturating_add(1),
            free_disk_floor_bytes: 0,
            min_inactive_age_hours: 0,
            ..StorageGovernorSettings::default()
        };

        let report = maintain_storage(data, &forge, settings, true).expect("maintenance");
        assert_eq!(report.actions.len(), 1);
        assert_eq!(report.actions[0].repository_key, "repo-old");
        assert_eq!(report.actions[0].reason, "global_cache_cap");
    }

    #[test]
    fn settings_round_trip_and_reject_inverted_caps() {
        let temp = tempfile::TempDir::new().unwrap();
        let settings = StorageGovernorSettings {
            repository_cache_max_bytes: 2 * GIB,
            global_cache_max_bytes: 4 * GIB,
            free_disk_floor_bytes: GIB,
            min_inactive_age_hours: 12,
            ..StorageGovernorSettings::default()
        };
        save_settings(temp.path(), &settings).unwrap();
        assert_eq!(load_settings(temp.path()), settings);

        let invalid = StorageGovernorSettings {
            repository_cache_max_bytes: 5,
            global_cache_max_bytes: 4,
            ..StorageGovernorSettings::default()
        };
        assert!(save_settings(temp.path(), &invalid).is_err());
    }
}
