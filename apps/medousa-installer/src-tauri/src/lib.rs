mod install;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use medousa::install::manifest::{
    package_installed, read_install_manifest, write_install_manifest, InstallManifest,
    PackageInstallRecord, ReleaseManifest,
};
use medousa::install::packages::{
    catalog_entry, default_install_profiles, expand_package_dependencies, package_catalog,
    sort_for_install, visible_catalog,
};
use medousa::install::release_config::{host_target, release_base_url, release_channel, release_manifest_url};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileSummary {
    id: String,
    display_name: String,
    description: String,
    packages: Vec<String>,
    size_label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageSummary {
    id: String,
    display_name: String,
    category: String,
    depends: Vec<String>,
    binaries: Vec<String>,
    size_label: String,
    size_bytes: u64,
    optional: bool,
    selected: bool,
    installed: bool,
    update_available: bool,
    installed_version: Option<String>,
    remote_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapResponse {
    install_root: String,
    data_dir: String,
    model_cache_dir: String,
    release_manifest_url: String,
    release_base_url: Option<String>,
    release_channel: String,
    profiles: Vec<ProfileSummary>,
    packages: Vec<PackageSummary>,
    modify_mode: bool,
    installed_version: Option<String>,
    remote_version: Option<String>,
    version_mismatch: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogResponse {
    profiles: Vec<ProfileSummary>,
    packages: Vec<PackageSummary>,
    remote_version: Option<String>,
    installed_version: Option<String>,
    version_mismatch: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SidebarNode {
    id: String,
    label: String,
    included: bool,
    optional: bool,
    children: Vec<SidebarNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolveSelectionResponse {
    expanded_package_ids: Vec<String>,
    total_bytes: u64,
    size_label: String,
    tree: Vec<SidebarNode>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DetectExistingResponse {
    modify_mode: bool,
    installed_package_ids: Vec<String>,
    install_root: String,
    installed_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallRequest {
    install_root: String,
    package_ids: Vec<String>,
    modify_mode: bool,
    remove_package_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgressEvent {
    package_id: String,
    phase: String,
    percent: f32,
    message: String,
}

fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= GB as u64 {
        format!("{:.1} GB", bytes as f64 / GB)
    } else {
        format!("{:.0} MB", bytes as f64 / MB)
    }
}

fn default_install_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Applications/Medousa.app")
    } else if cfg!(windows) {
        PathBuf::from(r"C:\Program Files\Medousa")
    } else {
        PathBuf::from("/opt/medousa")
    }
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

fn model_cache_dir() -> PathBuf {
    data_dir().join("models")
}

fn modify_mode_from_args() -> bool {
    std::env::args().any(|arg| arg == "--modify" || arg == "--repair" || arg == "--update")
}

fn profile_description(profile_id: &str) -> &'static str {
    match profile_id {
        "express" => "Desktop app and engine — recommended for most people.",
        "offline-workstation" => "Desktop, engine, offline brain, and a balanced Gemma model.",
        "developer" => "Desktop, engine, CLI tools, and MCP gateway.",
        "headless-server" => "Engine only — no desktop UI.",
        _ => "",
    }
}

fn build_profile_summaries(remote: &Option<ReleaseManifest>) -> Vec<ProfileSummary> {
    default_install_profiles()
        .into_iter()
        .map(|profile| {
            let packages: Vec<String> = profile
                .packages
                .iter()
                .map(|id| id.to_string())
                .collect();
            let bytes = estimate_bytes(&packages, remote);
            ProfileSummary {
                id: profile.id.to_string(),
                display_name: profile.display_name.to_string(),
                description: profile_description(profile.id).to_string(),
                packages,
                size_label: format_bytes(bytes),
            }
        })
        .collect()
}

fn remote_package_ids(remote: &Option<ReleaseManifest>) -> Vec<String> {
    remote
        .as_ref()
        .map(|manifest| {
            manifest
                .packages
                .values()
                .map(|pkg| pkg.id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect()
        })
        .unwrap_or_default()
}

fn estimate_bytes(package_ids: &[String], remote: &Option<ReleaseManifest>) -> u64 {
    let mut total = 0u64;
    for id in package_ids {
        if let Some(manifest) = remote {
            if let Some(pkg) = manifest.packages.values().find(|p| p.id == *id) {
                total += pkg.size_bytes;
                continue;
            }
        }
        total += package_catalog()
            .into_iter()
            .find(|entry| entry.id == id.as_str())
            .map(|entry| entry.default_size_bytes)
            .unwrap_or(0);
    }
    total
}

fn installed_records(install_root: &PathBuf) -> HashMap<String, PackageInstallRecord> {
    let path = install::install_manifest_path(install_root);
    if !path.exists() {
        return HashMap::new();
    }
    read_install_manifest(&path)
        .ok()
        .map(|manifest| {
            manifest
                .packages
                .into_iter()
                .map(|record| (record.id.clone(), record))
                .collect()
        })
        .unwrap_or_default()
}

fn build_package_summaries(
    selected: &HashSet<String>,
    remote: &Option<ReleaseManifest>,
    installed: &HashMap<String, PackageInstallRecord>,
) -> Vec<PackageSummary> {
    let remote_ids = remote_package_ids(remote);
    let catalog = visible_catalog(&remote_ids);
    let remote_version = remote.as_ref().map(|m| m.version.clone());

    catalog
        .into_iter()
        .map(|entry| {
            let installed_record = installed.get(entry.id);
            let installed = installed_record.is_some()
                || package_installed(&data_dir(), entry.id);
            let remote_pkg = remote.as_ref().and_then(|manifest| {
                manifest
                    .packages
                    .values()
                    .find(|pkg| pkg.id == entry.id)
            });
            let remote_pkg_version = remote_pkg.map(|pkg| pkg.version.clone());
            let installed_version = installed_record.map(|r| r.version.clone());
            let update_available = installed
                && remote_pkg_version
                    .as_ref()
                    .zip(installed_version.as_ref())
                    .is_some_and(|(remote_v, local_v)| remote_v != local_v);
            let size_bytes = remote_pkg
                .map(|pkg| pkg.size_bytes)
                .unwrap_or(entry.default_size_bytes);

            PackageSummary {
                id: entry.id.to_string(),
                display_name: entry.display_name.to_string(),
                category: entry.category.as_str().to_string(),
                depends: entry.depends.iter().map(|d| d.to_string()).collect(),
                binaries: entry.binaries.iter().map(|b| b.to_string()).collect(),
                size_label: format_bytes(size_bytes),
                size_bytes,
                optional: entry.optional,
                selected: selected.contains(entry.id)
                    || (!entry.optional && matches!(entry.id, "desktop" | "engine")),
                installed,
                update_available,
                installed_version,
                remote_version: remote_pkg_version,
            }
        })
        .collect()
}

async fn load_remote_manifest() -> Option<ReleaseManifest> {
    install::fetch_release_manifest().await.ok()
}

#[tauri::command]
async fn installer_bootstrap() -> Result<BootstrapResponse, String> {
    let install_root = default_install_root();
    let remote = load_remote_manifest().await;
    let installed = installed_records(&install_root);
    let selected: HashSet<String> = if installed.is_empty() {
        ["desktop", "engine"].into_iter().map(str::to_string).collect()
    } else {
        installed.keys().cloned().collect()
    };

    let installed_version = install::read_local_install_manifest(&install_root)
        .map(|m| m.version);
    let remote_version = remote.as_ref().map(|m| m.version.clone());
    let version_mismatch = installed_version
        .as_ref()
        .zip(remote_version.as_ref())
        .is_some_and(|(local, remote)| local != remote);

    Ok(BootstrapResponse {
        install_root: install_root.display().to_string(),
        data_dir: data_dir().display().to_string(),
        model_cache_dir: model_cache_dir().display().to_string(),
        release_manifest_url: release_manifest_url(),
        release_base_url: release_base_url(),
        release_channel: release_channel(),
        profiles: build_profile_summaries(&remote),
        packages: build_package_summaries(&selected, &remote, &installed),
        modify_mode: modify_mode_from_args()
            || install::install_manifest_path(&install_root).exists(),
        installed_version,
        remote_version,
        version_mismatch,
    })
}

#[tauri::command]
async fn installer_catalog(selected_ids: Vec<String>) -> Result<CatalogResponse, String> {
    let install_root = default_install_root();
    let remote = load_remote_manifest().await;
    let installed = installed_records(&install_root);
    let selected: HashSet<String> = selected_ids.into_iter().collect();
    let installed_version = install::read_local_install_manifest(&install_root)
        .map(|m| m.version);
    let remote_version = remote.as_ref().map(|m| m.version.clone());

    Ok(CatalogResponse {
        profiles: build_profile_summaries(&remote),
        packages: build_package_summaries(&selected, &remote, &installed),
        installed_version: installed_version.clone(),
        remote_version: remote_version.clone(),
        version_mismatch: installed_version
            .as_ref()
            .zip(remote_version.as_ref())
            .is_some_and(|(a, b)| a != b),
    })
}

#[tauri::command]
async fn installer_resolve_selection(
    package_ids: Vec<String>,
) -> Result<ResolveSelectionResponse, String> {
    let remote = load_remote_manifest().await;
    let expanded: Vec<String> = expand_package_dependencies(
        &package_ids.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let total_bytes = estimate_bytes(&expanded, &remote);
    let mut warnings = Vec::new();

    if let Some(manifest) = &remote {
        let target = host_target();
        for id in &expanded {
            let found = install::resolve_release_package(manifest, id).is_ok();
            if !found && id != "desktop" {
                warnings.push(format!(
                    "{id} is not available for {target} in the current release manifest"
                ));
            }
        }
        if let Some(local) = install::read_local_install_manifest(&default_install_root()) {
            if local.version != manifest.version {
                warnings.push(format!(
                    "Installed version {} differs from release {}",
                    local.version, manifest.version
                ));
            }
        }
    }

    let tree = build_sidebar_tree(&expanded);

    Ok(ResolveSelectionResponse {
        expanded_package_ids: expanded,
        total_bytes,
        size_label: format_bytes(total_bytes),
        tree,
        warnings,
    })
}

fn build_sidebar_tree(expanded: &[String]) -> Vec<SidebarNode> {
    let expanded_set: HashSet<_> = expanded.iter().cloned().collect();
    let mut roots = Vec::new();

    for profile in default_install_profiles() {
        let profile_packages: Vec<_> = profile
            .packages
            .iter()
            .filter(|id| expanded_set.contains(**id))
            .collect();
        if profile_packages.is_empty() {
            continue;
        }
        let children: Vec<SidebarNode> = profile_packages
            .into_iter()
            .map(|id| {
                let entry = catalog_entry(id);
                let label = entry
                    .as_ref()
                    .map(|e| e.display_name.to_string())
                    .unwrap_or_else(|| id.to_string());
                SidebarNode {
                    id: id.to_string(),
                    label,
                    included: entry.as_ref().is_some_and(|e| !e.optional),
                    optional: entry.as_ref().is_some_and(|e| e.optional),
                    children: vec![],
                }
            })
            .collect();
        roots.push(SidebarNode {
            id: profile.id.to_string(),
            label: profile.display_name.to_string(),
            included: false,
            optional: false,
            children,
        });
    }

    let profile_ids: HashSet<_> = default_install_profiles()
        .iter()
        .flat_map(|p| p.packages.iter().copied())
        .collect();
    let orphans: Vec<SidebarNode> = expanded
        .iter()
        .filter(|id| !profile_ids.contains(id.as_str()))
        .map(|id| {
            let entry = catalog_entry(id);
            let label = entry
                .as_ref()
                .map(|e| e.display_name.to_string())
                .unwrap_or_else(|| id.clone());
            SidebarNode {
                id: id.clone(),
                label,
                included: entry.as_ref().is_some_and(|e| !e.optional),
                optional: entry.as_ref().is_some_and(|e| e.optional),
                children: vec![],
            }
        })
        .collect();
    if !orphans.is_empty() {
        roots.push(SidebarNode {
            id: "individual".to_string(),
            label: "Individual components".to_string(),
            included: false,
            optional: false,
            children: orphans,
        });
    }

    roots
}

#[tauri::command]
fn installer_detect_existing() -> Result<DetectExistingResponse, String> {
    let install_root = default_install_root();
    let manifest_path = install::install_manifest_path(&install_root);
    if !manifest_path.exists() {
        return Ok(DetectExistingResponse {
            modify_mode: false,
            installed_package_ids: Vec::new(),
            install_root: install_root.display().to_string(),
            installed_version: None,
        });
    }
    let manifest = read_install_manifest(&manifest_path)?;
    Ok(DetectExistingResponse {
        modify_mode: true,
        installed_package_ids: manifest.packages.iter().map(|p| p.id.clone()).collect(),
        install_root: install_root.display().to_string(),
        installed_version: Some(manifest.version),
    })
}

#[tauri::command]
async fn installer_pick_install_root(app: AppHandle) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose install location")
        .blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
async fn installer_run(app: AppHandle, request: InstallRequest) -> Result<(), String> {
    let install_root = PathBuf::from(request.install_root.trim());
    let data = data_dir();
    let manifest_path = install::install_manifest_path(&install_root);
    let existing_manifest = if request.modify_mode && manifest_path.exists() {
        read_install_manifest(&manifest_path).ok()
    } else {
        None
    };

    for package_id in &request.remove_package_ids {
        emit_progress(&app, package_id, "removing", 0.0, "Removing…");
        if let Err(err) = install::remove_package(&data, package_id).await {
            emit_progress(&app, package_id, "failed", 0.0, &err);
            return Err(err);
        }
        emit_progress(&app, package_id, "removed", 100.0, "Removed");
    }

    let mut expanded: Vec<String> = expand_package_dependencies(
        &request
            .package_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    sort_for_install(&mut expanded);

    let remote = install::fetch_release_manifest().await.ok();

    for package_id in expanded {
        let already_installed = existing_manifest
            .as_ref()
            .is_some_and(|manifest| manifest.packages.iter().any(|entry| entry.id == package_id))
            || package_installed(&data, &package_id);

        let needs_update = already_installed
            && remote.as_ref().is_some_and(|manifest| {
                let local = existing_manifest
                    .as_ref()
                    .and_then(|m| m.packages.iter().find(|p| p.id == package_id));
                let remote_pkg = install::resolve_release_package(manifest, &package_id).ok();
                match (local, remote_pkg) {
                    (Some(local), Some(remote_pkg)) => local.version != remote_pkg.version,
                    (None, Some(_)) => request.package_ids.contains(&package_id),
                    _ => false,
                }
            });

        if already_installed && !needs_update {
            emit_progress(&app, &package_id, "ready", 100.0, "Already installed");
            continue;
        }

        emit_progress(
            &app,
            &package_id,
            "downloading",
            0.0,
            "Starting download…",
        );
        let result = install::install_package(
            &install_root,
            &data,
            &package_id,
            |percent, message| {
                emit_progress(&app, &package_id, "downloading", percent, message);
            },
        )
        .await;
        match result {
            Ok(()) => emit_progress(&app, &package_id, "ready", 100.0, "Installed"),
            Err(err) => {
                emit_progress(&app, &package_id, "failed", 0.0, &err);
                return Err(err);
            }
        }
    }

    let mut manifest = if manifest_path.exists() {
        read_install_manifest(&manifest_path)?
    } else {
        InstallManifest {
            schema_version: 2,
            product: "medousa".to_string(),
            version: remote
                .as_ref()
                .map(|m| m.version.clone())
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
            target: host_target(),
            built_at: chrono::Utc::now().to_rfc3339(),
            binaries: Vec::new(),
            component_set_id: String::new(),
            install_root: Some(install_root.display().to_string()),
            data_dir: Some(data.display().to_string()),
            packages: Vec::new(),
        }
    };
    manifest.install_root = Some(install_root.display().to_string());
    manifest.data_dir = Some(data.display().to_string());
    if let Some(remote) = &remote {
        manifest.version = remote.version.clone();
    }

    for package_id in &request.package_ids {
        let record = PackageInstallRecord {
            id: package_id.clone(),
            version: manifest.version.clone(),
            install_path: Some(data.join("packages").join(package_id).display().to_string()),
            sha256: remote
                .as_ref()
                .and_then(|m| install::resolve_release_package(m, package_id).ok())
                .map(|p| p.sha256.clone()),
            binaries: catalog_entry(package_id)
                .map(|e| e.binaries.iter().map(|b| b.to_string()).collect())
                .unwrap_or_default(),
        };
        if let Some(existing) = manifest.packages.iter_mut().find(|p| p.id == *package_id) {
            *existing = record;
        } else {
            manifest.packages.push(record);
        }
    }

    manifest.packages.retain(|p| !request.remove_package_ids.contains(&p.id));
    write_install_manifest(&manifest_path, &manifest)?;
    Ok(())
}

fn emit_progress(app: &AppHandle, package_id: &str, phase: &str, percent: f32, message: &str) {
    let _ = app.emit(
        "install-progress",
        DownloadProgressEvent {
            package_id: package_id.to_string(),
            phase: phase.to_string(),
            percent,
            message: message.to_string(),
        },
    );
}

#[tauri::command]
fn installer_launch_medousa(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let path = default_install_root();
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|err| err.to_string())?;
        let _ = app;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let path = default_install_root().join("Medousa.exe");
        std::process::Command::new(path)
            .spawn()
            .map_err(|err| err.to_string())?;
        let _ = app;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            default_install_root().join("medousa-home"),
            PathBuf::from("/usr/bin/medousa-home"),
            default_install_root().join("Medousa.AppImage"),
        ];
        for path in candidates {
            if path.exists() {
                std::process::Command::new(path)
                    .spawn()
                    .map_err(|err| err.to_string())?;
                let _ = app;
                return Ok(());
            }
        }
        Err("Medousa desktop app not found — install the desktop package first".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            installer_bootstrap,
            installer_catalog,
            installer_resolve_selection,
            installer_detect_existing,
            installer_pick_install_root,
            installer_run,
            installer_launch_medousa
        ])
        .run(tauri::generate_context!())
        .expect("error while running Medousa Installer");
}
