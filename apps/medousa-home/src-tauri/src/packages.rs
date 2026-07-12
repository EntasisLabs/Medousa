//! Settings → Packages: install optional Medousa binaries into `{dataDir}/bin`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use medousa_install_support::manifest::{
    package_installed, read_install_manifest, write_install_manifest, InstallManifest,
    PackageInstallRecord,
};
use medousa_install_support::packages::{
    catalog_entry, expand_home_package_dependencies, home_packages_catalog,
    is_home_packages_package, package_short_hint, phase_label, sort_for_install,
};
use medousa_install_support::tarball_install::{
    fetch_release_manifest, install_tarball_package, remove_tarball_package,
    resolve_release_package,
};
use medousa_install_support::{host_target, release_base_url};
use tauri::{AppHandle, Emitter};

use crate::workshop_runtime;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageStatusSummary {
    pub local_brain_installed: bool,
    pub installer_available: bool,
    pub installed_packages: Vec<String>,
    pub installed_version: Option<String>,
    pub release_base_url: Option<String>,
    pub update_available: bool,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HomePackageRow {
    pub id: String,
    pub display_name: String,
    pub hint: String,
    pub category_label: String,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub available_version: Option<String>,
    pub update_available: bool,
    pub size_bytes: u64,
    pub optional: bool,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HomePackagesCatalog {
    pub packages: Vec<HomePackageRow>,
    pub installer_available: bool,
    pub release_version: Option<String>,
    pub release_base_url: Option<String>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageProgressEvent {
    pub package_id: String,
    pub display_name: String,
    pub phase: String,
    pub phase_label: String,
    pub percent: f32,
    pub message: String,
}

fn data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
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

fn resolve_installer_app() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("/Applications/Medousa Installer.app"),
        PathBuf::from(r"C:\Program Files\Medousa\MedousaInstaller.exe"),
        PathBuf::from("/opt/medousa/MedousaInstaller"),
        dirs::data_local_dir()
            .unwrap_or_default()
            .join("medousa")
            .join("MedousaInstaller"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn read_local_manifest() -> Option<InstallManifest> {
    let data = data_dir();
    let install_root = default_install_root();
    read_install_manifest(&install_root.join("install-manifest.json"))
        .ok()
        .or_else(|| read_install_manifest(&data.join("install-manifest.json")).ok())
}

fn local_brain_present() -> bool {
    workshop_runtime::local_brain_installed() || package_installed(&data_dir(), "local-brain")
}

#[tauri::command]
pub fn packages_status() -> PackageStatusSummary {
    let manifest = read_local_manifest();
    let installed_packages: Vec<String> = manifest
        .as_ref()
        .map(|m| m.packages.iter().map(|p| p.id.clone()).collect())
        .unwrap_or_default();

    let update_available = installed_packages.iter().any(|id| {
        if !is_home_packages_package(id) {
            return false;
        }
        // Cheap status flag — detailed rows come from packages_catalog.
        false
    });

    PackageStatusSummary {
        local_brain_installed: local_brain_present(),
        installer_available: resolve_installer_app().is_some(),
        installed_packages,
        installed_version: manifest.as_ref().map(|m| m.version.clone()),
        release_base_url: release_base_url(),
        update_available,
    }
}

#[tauri::command]
pub async fn packages_catalog() -> Result<HomePackagesCatalog, String> {
    let data = data_dir();
    let local = read_local_manifest();
    let remote = fetch_release_manifest().await.ok();
    let release_version = remote.as_ref().map(|m| m.version.clone());

    let packages = home_packages_catalog()
        .into_iter()
        .map(|entry| {
            let installed = package_installed(&data, entry.id)
                || local
                    .as_ref()
                    .is_some_and(|m| m.packages.iter().any(|p| p.id == entry.id))
                || (entry.id == "local-brain" && workshop_runtime::local_brain_installed());

            let installed_version = local.as_ref().and_then(|m| {
                m.packages
                    .iter()
                    .find(|p| p.id == entry.id)
                    .map(|p| p.version.clone())
            });

            let available = remote
                .as_ref()
                .and_then(|m| resolve_release_package(m, entry.id).ok())
                .cloned();
            let available_version = available.as_ref().map(|p| p.version.clone());
            let size_bytes = available
                .as_ref()
                .map(|p| p.size_bytes)
                .unwrap_or(entry.default_size_bytes);
            let update_available = match (&installed_version, &available_version) {
                (Some(local_v), Some(remote_v)) => local_v != remote_v && installed,
                (None, Some(_)) => installed,
                _ => false,
            };

            HomePackageRow {
                id: entry.id.to_string(),
                display_name: entry.display_name.to_string(),
                hint: package_short_hint(entry.id).to_string(),
                category_label: entry.category_label.to_string(),
                installed,
                installed_version,
                available_version,
                update_available,
                size_bytes,
                optional: entry.optional,
            }
        })
        .collect();

    Ok(HomePackagesCatalog {
        packages,
        installer_available: resolve_installer_app().is_some(),
        release_version,
        release_base_url: release_base_url(),
    })
}

fn emit_progress(
    app: &AppHandle,
    package_id: &str,
    phase: &str,
    percent: f32,
    message: &str,
) {
    let display_name = catalog_entry(package_id)
        .map(|entry| entry.display_name.to_string())
        .unwrap_or_else(|| package_id.to_string());
    let _ = app.emit(
        "packages-progress",
        PackageProgressEvent {
            package_id: package_id.to_string(),
            display_name,
            phase: phase.to_string(),
            phase_label: phase_label(phase).to_string(),
            percent,
            message: message.to_string(),
        },
    );
}

fn upsert_manifest_record(
    package_id: &str,
    version: &str,
    sha256: Option<String>,
    remote_version: Option<&str>,
) -> Result<(), String> {
    let data = data_dir();
    let path = data.join("install-manifest.json");
    let mut manifest = if path.exists() {
        read_install_manifest(&path)?
    } else {
        InstallManifest {
            schema_version: 2,
            product: "medousa".to_string(),
            version: remote_version
                .unwrap_or(version)
                .to_string(),
            target: host_target(),
            built_at: chrono::Utc::now().to_rfc3339(),
            binaries: Vec::new(),
            component_set_id: String::new(),
            install_root: None,
            data_dir: Some(data.display().to_string()),
            packages: Vec::new(),
        }
    };
    manifest.data_dir = Some(data.display().to_string());
    if let Some(v) = remote_version {
        manifest.version = v.to_string();
    }

    let record = PackageInstallRecord {
        id: package_id.to_string(),
        version: version.to_string(),
        install_path: Some(data.join("packages").join(package_id).display().to_string()),
        sha256,
        binaries: catalog_entry(package_id)
            .map(|e| e.binaries.iter().map(|b| b.to_string()).collect())
            .unwrap_or_default(),
    };
    if let Some(existing) = manifest.packages.iter_mut().find(|p| p.id == *package_id) {
        *existing = record;
    } else {
        manifest.packages.push(record);
    }
    write_install_manifest(&path, &manifest)
}

fn remove_manifest_record(package_id: &str) -> Result<(), String> {
    let path = data_dir().join("install-manifest.json");
    if !path.exists() {
        return Ok(());
    }
    let mut manifest = read_install_manifest(&path)?;
    manifest.packages.retain(|p| p.id != package_id);
    write_install_manifest(&path, &manifest)
}

#[tauri::command]
pub async fn packages_install(app: AppHandle, package_id: String) -> Result<(), String> {
    if !is_home_packages_package(&package_id) {
        return Err(format!(
            "package {package_id} can’t be installed from Home — use Medousa Installer for desktop/engine."
        ));
    }

    let data = data_dir();
    fs::create_dir_all(&data).map_err(|err| err.to_string())?;

    let mut expanded = expand_home_package_dependencies(&[&package_id]);
    sort_for_install(&mut expanded);

    let remote = fetch_release_manifest().await.ok();
    let remote_version = remote.as_ref().map(|m| m.version.clone());

    for id in expanded {
        emit_progress(&app, &id, "downloading", 0.0, "Starting download…");
        let result = install_tarball_package(&data, &id, None, |percent, message| {
            emit_progress(&app, &id, "downloading", percent, message);
        })
        .await;

        match result {
            Ok(pkg) => {
                upsert_manifest_record(
                    &id,
                    &pkg.version,
                    Some(pkg.sha256),
                    remote_version.as_deref(),
                )?;
                emit_progress(&app, &id, "ready", 100.0, "Installed");
            }
            Err(err) => {
                emit_progress(&app, &id, "failed", 0.0, &err);
                return Err(err);
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn packages_remove(app: AppHandle, package_id: String) -> Result<(), String> {
    if !is_home_packages_package(&package_id) {
        return Err(format!("cannot remove {package_id} from Home Packages"));
    }
    emit_progress(&app, &package_id, "removing", 0.0, "Removing…");
    remove_tarball_package(&data_dir(), &package_id)?;
    remove_manifest_record(&package_id)?;
    emit_progress(&app, &package_id, "removed", 100.0, "Removed");
    Ok(())
}

#[tauri::command]
pub fn packages_open_installer() -> Result<(), String> {
    let Some(path) = resolve_installer_app() else {
        if let Some(base) = release_base_url() {
            let bootstrap = format!("{base}/stable/installer-bootstrap.json");
            #[cfg(target_os = "windows")]
            {
                return Err(format!(
                    "Medousa Installer not found locally. For add-ons, download it from {bootstrap} (see installerUrl on Windows)."
                ));
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err(format!(
                    "Medousa Installer not found locally. Download it from {bootstrap}"
                ));
            }
        }
        return Err(
            "Medousa Installer not found. Set MEDOUSA_RELEASE_BASE_URL or install the installer app."
                .to_string(),
        );
    };

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-a")
            .arg(&path)
            .arg("--args")
            .arg("--modify")
            .spawn()
            .or_else(|_| Command::new("open").arg(path).spawn())
            .map_err(|err| err.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new(&path)
            .arg("--modify")
            .spawn()
            .map_err(|err| err.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new(&path)
            .arg("--modify")
            .spawn()
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}
