//! Settings → Packages without a renderer: list, install and remove the
//! optional Medousa binaries in `{dataDir}/bin`. The same steps as
//! medousa-home's Tauri commands, with progress through a callback so any
//! shell can show it.

use std::fs;
use std::path::{Path, PathBuf};

use medousa_install_support::manifest::{
    InstallManifest, PackageInstallRecord, package_dir, package_installed, read_install_manifest,
    resolve_release_package, write_install_manifest,
};
use medousa_install_support::packages::{
    PackageCatalogEntry, catalog_entry, expand_home_package_dependencies, home_packages_catalog,
    is_home_packages_package, package_short_hint, phase_label, sort_for_install,
};
use medousa_install_support::tarball_install::{
    fetch_release_manifest, install_tarball_package, remove_tarball_package,
};
use medousa_install_support::{host_target, release_base_url};

/// One package as Settings lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct PackageRow {
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

#[derive(Clone, Debug, PartialEq)]
pub struct Catalog {
    pub packages: Vec<PackageRow>,
    pub installer_available: bool,
    pub release_version: Option<String>,
}

/// Install or removal progress for one package.
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub package_id: String,
    pub display_name: String,
    pub phase: String,
    pub phase_label: String,
    pub percent: f32,
    pub message: String,
}

fn data_dir() -> PathBuf {
    crate::workshop_registry::medousa_data_dir()
}

fn platform_bin_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// A package binary where the daemon looks for it: dataDir/bin, the usual
/// install roots, PATH, and beside `medousa_daemon`.
fn find_package_binary(name: &str) -> Option<PathBuf> {
    let file = platform_bin_name(name);
    let mut candidates: Vec<PathBuf> = vec![data_dir().join("bin").join(&file)];
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from("/usr/local/bin").join(&file));
        candidates.push(PathBuf::from("/opt/homebrew/bin").join(&file));
    }
    if cfg!(target_os = "linux") {
        candidates.push(PathBuf::from("/usr/local/bin").join(&file));
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            candidates.push(dir.join(&file));
        }
    }
    for daemon_name in ["medousa_daemon", "medousa_daemon.exe"] {
        if let Some(dir) =
            find_named_binary(daemon_name).and_then(|d| d.parent().map(Path::to_path_buf))
        {
            candidates.push(dir.join(&file));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn find_named_binary(name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from("/usr/local/bin").join(name));
        candidates.push(PathBuf::from("/opt/homebrew/bin").join(name));
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            candidates.push(dir.join(name));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn package_is_present(data: &Path, entry: &PackageCatalogEntry) -> bool {
    package_installed(data, entry.id)
        || (!entry.binaries.is_empty()
            && entry
                .binaries
                .iter()
                .any(|bin| find_package_binary(bin).is_some()))
        || (entry.id == "local-brain" && crate::workshop_runtime::local_brain_installed())
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

/// Medousa Installer, for repairs and the engine itself.
pub fn installer_app() -> Option<PathBuf> {
    [
        PathBuf::from("/Applications/Medousa Installer.app"),
        PathBuf::from(r"C:\Program Files\Medousa\MedousaInstaller.exe"),
        PathBuf::from("/opt/medousa/MedousaInstaller"),
        dirs::data_local_dir()
            .unwrap_or_default()
            .join("medousa")
            .join("MedousaInstaller"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn read_local_manifest() -> Option<InstallManifest> {
    read_install_manifest(&default_install_root().join("install-manifest.json"))
        .ok()
        .or_else(|| read_install_manifest(&data_dir().join("install-manifest.json")).ok())
}

/// What is installed here and what the release offers. The release lookup is
/// a network read; without it the list shows only what is present.
pub async fn catalog() -> Catalog {
    let data = data_dir();
    let local = read_local_manifest();
    let remote = fetch_release_manifest().await.ok();
    let release_version = remote.as_ref().map(|m| m.version.clone());
    let packages = home_packages_catalog()
        .into_iter()
        // MCP has its own section.
        .filter(|entry| entry.id != "mcp-gateway")
        .filter(|entry| {
            !entry.remote_only
                || package_is_present(&data, entry)
                || local
                    .as_ref()
                    .is_some_and(|m| m.packages.iter().any(|p| p.id == entry.id))
                || remote
                    .as_ref()
                    .is_some_and(|m| resolve_release_package(m, entry.id).is_ok())
        })
        .map(|entry| {
            let installed = package_is_present(&data, &entry)
                || local
                    .as_ref()
                    .is_some_and(|m| m.packages.iter().any(|p| p.id == entry.id));
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
            // A binary found only on PATH has no version stamp: don't nag.
            let update_available = match (&installed_version, &available_version) {
                (Some(local_v), Some(remote_v)) => local_v != remote_v && installed,
                _ => false,
            };
            PackageRow {
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
    Catalog {
        packages,
        installer_available: installer_app().is_some(),
        release_version,
    }
}

fn progress(package_id: &str, phase: &str, percent: f32, message: &str) -> Progress {
    Progress {
        package_id: package_id.to_string(),
        display_name: catalog_entry(package_id)
            .map(|entry| entry.display_name.to_string())
            .unwrap_or_else(|| package_id.to_string()),
        phase: phase.to_string(),
        phase_label: phase_label(phase).to_string(),
        percent,
        message: message.to_string(),
    }
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
            version: remote_version.unwrap_or(version).to_string(),
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
        install_path: Some(package_dir(&data, package_id)?.display().to_string()),
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

/// Installs `package_id` and what it depends on, reporting each step.
pub async fn install(package_id: &str, mut report: impl FnMut(Progress)) -> Result<(), String> {
    if !is_home_packages_package(package_id) {
        return Err(format!(
            "{package_id} isn’t installed from Settings — use Medousa Installer for the app and engine."
        ));
    }
    let data = data_dir();
    fs::create_dir_all(&data).map_err(|err| err.to_string())?;
    let mut expanded = expand_home_package_dependencies(&[package_id]);
    sort_for_install(&mut expanded);
    let remote_version = fetch_release_manifest().await.ok().map(|m| m.version);
    for id in expanded {
        report(progress(&id, "downloading", 0.0, "Starting download…"));
        let result = install_tarball_package(&data, &id, None, |percent, message| {
            report(progress(&id, "downloading", percent, message));
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
                report(progress(&id, "ready", 100.0, "Installed"));
            }
            Err(err) => {
                report(progress(&id, "failed", 0.0, &err));
                return Err(err);
            }
        }
    }
    Ok(())
}

/// Removes an optional package. Blocking file IO.
pub fn remove(package_id: &str) -> Result<(), String> {
    if !is_home_packages_package(package_id) {
        return Err(format!("{package_id} can’t be removed from Settings"));
    }
    remove_tarball_package(&data_dir(), package_id)?;
    remove_manifest_record(package_id)
}

/// Opens Medousa Installer in modify mode.
pub fn open_installer() -> Result<(), String> {
    use std::process::Command;
    let Some(path) = installer_app() else {
        return Err(match release_base_url() {
            Some(base) => format!(
                "Medousa Installer isn’t on this computer. Download it from {base}/stable/installer-bootstrap.json"
            ),
            None => "Medousa Installer isn’t on this computer.".to_string(),
        });
    };
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-a")
            .arg(&path)
            .arg("--args")
            .arg("--modify")
            .spawn()
            .or_else(|_| Command::new("open").arg(&path).spawn())
            .map_err(|err| err.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        Command::new(&path)
            .arg("--modify")
            .spawn()
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// medousa-home's `formatPackageBytes`: 1024-based, one decimal under 10.
pub fn format_bytes(bytes: u64) -> Option<String> {
    if bytes == 0 {
        return None;
    }
    let units = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024. && unit < units.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    let digits = if value >= 10. || unit == 0 { 0 } else { 1 };
    Some(format!("{value:.digits$} {}", units[unit]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_medousa_home() {
        assert_eq!(format_bytes(0), None);
        assert_eq!(format_bytes(512).as_deref(), Some("512 B"));
        assert_eq!(format_bytes(1536).as_deref(), Some("1.5 KB"));
        assert_eq!(format_bytes(48 * 1024 * 1024).as_deref(), Some("48 MB"));
    }

    #[test]
    fn only_settings_packages_install_or_remove_here() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let err = rt.block_on(install("medousa-desktop", |_| {})).unwrap_err();
        assert!(err.contains("Medousa Installer"), "{err}");
        assert!(remove("medousa-desktop").is_err());
    }
}
